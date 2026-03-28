//! wgpu-based GPU grid calculator.
//!
//! Implements a simplified ISO 9613-2 propagation model (geometric spreading +
//! atmospheric absorption only) as a WGSL compute shader dispatched over all
//! receiver points in parallel on the GPU.

use nalgebra::Point3;

use crate::engine::propagation::{AtmosphericConditions, PropagationConfig};
use crate::grid::calculator::SourceSpec;

// ─── WGSL compute shader ─────────────────────────────────────────────────────

/// WGSL compute shader source.  Each invocation computes the combined SPL
/// from all sources at one receiver using geometric spreading + atmospheric
/// absorption (A_div + A_atm).  Ground effect and barriers are excluded
/// for throughput in this fast-preview path.
const SHADER_SRC: &str = r#"
// Receiver layout: (x, y, z, _pad)
struct Receiver { x: f32, y: f32, z: f32, pad: f32 }

// Source layout: (x, y, z, g_source, lw[0..8])
struct Source { x: f32, y: f32, z: f32, g_source: f32, lw: array<f32, 8> }

// Uniform parameters
struct Params {
    a_atm: array<f32, 8>,     // atmospheric absorption coeff × distance (dB/m * per band pre-multiplied)
    a_weights: array<f32, 8>, // A-weighting corrections (dB)
    n_sources: u32,
    n_receivers: u32,
}

@group(0) @binding(0) var<storage, read>       receivers: array<Receiver>;
@group(0) @binding(1) var<storage, read>       sources:   array<Source>;
@group(0) @binding(2) var<uniform>             params:    Params;
@group(0) @binding(3) var<storage, read_write> results:   array<f32>;

/// Compute 10^x using native log2/exp2 (WGSL has no pow(10, x) natively).
fn pow10(x: f32) -> f32 {
    return exp2(x * 3.321928f);   // log2(10) ≈ 3.321928
}

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let rcv_idx = id.x;
    if rcv_idx >= params.n_receivers { return; }

    let rcv = receivers[rcv_idx];
    var total: f32 = 0.0;

    for (var s = 0u; s < params.n_sources; s++) {
        let src = sources[s];
        let dx  = rcv.x - src.x;
        let dy  = rcv.y - src.y;
        let dz  = rcv.z - src.z;
        let d   = max(sqrt(dx * dx + dy * dy + dz * dz), 1.0);

        // A_div = 20·log10(d) + 11  (geometric spreading)
        let a_div = 20.0 * log2(d) / log2(10.0) + 11.0;

        // Sum over 8 octave bands.
        var band_sum: f32 = 0.0;
        for (var b = 0u; b < 8u; b++) {
            // A_atm[b] is (alpha_b × 1m) pre-computed; scale by d.
            let a_atm   = params.a_atm[b] * d;
            let a_total = a_div + a_atm;
            let lp_band = src.lw[b] - a_total + params.a_weights[b];
            band_sum   += pow10(lp_band * 0.1);
        }
        total += band_sum;
    }

    if total > 0.0 {
        results[rcv_idx] = 10.0 * log2(total) / log2(10.0);
    } else {
        results[rcv_idx] = -999.0;
    }
}
"#;

// ─── Host types mirroring WGSL structs ───────────────────────────────────────

/// Must match `Receiver` layout in WGSL (16 bytes, `align(16)`).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuReceiver {
    x: f32, y: f32, z: f32, _pad: f32,
}

/// Must match `Source` layout in WGSL (48 bytes).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuSource {
    x: f32, y: f32, z: f32, g_source: f32,
    lw: [f32; 8],
}

/// Uniform params (must be 16-byte aligned; total = 80 bytes).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuParams {
    a_atm:      [f32; 8],
    a_weights:  [f32; 8],
    n_sources:  u32,
    n_receivers: u32,
    _pad:       [u32; 2],
}

// ─── GpuGridCalculator ───────────────────────────────────────────────────────

/// GPU-accelerated grid calculator (geometric spreading + atmospheric
/// absorption only — no ground effect, no barriers).
///
/// Constructed asynchronously via [`GpuGridCalculator::new`].
pub struct GpuGridCalculator {
    device:   wgpu::Device,
    queue:    wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    bg_layout: wgpu::BindGroupLayout,
    config:   PropagationConfig,
}

impl GpuGridCalculator {
    /// Initialise a wgpu device and compile the compute shader.
    ///
    /// Returns `None` if no compatible GPU adapter is available.
    pub async fn new(config: PropagationConfig) -> Option<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: None,
            })
            .await?;

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("noise-gpu"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: Default::default(),
                },
                None,
            )
            .await
            .ok()?;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("noise_grid_compute"),
            source: wgpu::ShaderSource::Wgsl(SHADER_SRC.into()),
        });

        let bg_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("noise_bg_layout"),
            entries: &[
                // binding 0: receivers (read-only storage)
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // binding 1: sources (read-only storage)
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // binding 2: params (uniform)
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // binding 3: results (read-write storage)
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("noise_pipeline_layout"),
            bind_group_layouts: &[&bg_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("noise_compute_pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: "main",
            compilation_options: Default::default(),
            cache: None,
        });

        Some(Self { device, queue, pipeline, bg_layout, config })
    }

    /// Calculate SPL (dBA) for every receiver using the GPU shader.
    ///
    /// Returns `Vec<f32>` with one value per receiver point.
    /// Results of `-999.0` indicate no contribution from any source.
    pub async fn calculate(
        &self,
        receivers: &[Point3<f64>],
        sources: &[SourceSpec],
    ) -> Vec<f32> {
        use wgpu::util::DeviceExt;

        let n_rcv = receivers.len() as u32;
        let n_src = sources.len() as u32;

        // Build per-band atmospheric absorption coefficients (dB/m).
        let atm = &self.config.atmosphere;
        let a_atm: [f32; 8] = {
            use crate::engine::ground_effect::OCTAVE_BANDS;
            std::array::from_fn(|i| atm.alpha_db_per_m(OCTAVE_BANDS[i]) as f32)
        };

        let params = GpuParams {
            a_atm,
            a_weights: AtmosphericConditions::A_WEIGHTS.map(|v| v as f32),
            n_sources: n_src,
            n_receivers: n_rcv,
            _pad: [0; 2],
        };

        let gpu_receivers: Vec<GpuReceiver> = receivers
            .iter()
            .map(|r| GpuReceiver { x: r.x as f32, y: r.y as f32, z: r.z as f32, _pad: 0.0 })
            .collect();

        let gpu_sources: Vec<GpuSource> = sources
            .iter()
            .map(|s| GpuSource {
                x: s.position.x as f32,
                y: s.position.y as f32,
                z: s.position.z as f32,
                g_source: s.g_source as f32,
                lw: s.lw_db.map(|v| v as f32),
            })
            .collect();

        let buf_receivers = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label:    Some("receivers"),
            contents: bytemuck::cast_slice(&gpu_receivers),
            usage:    wgpu::BufferUsages::STORAGE,
        });
        let buf_sources = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label:    Some("sources"),
            contents: bytemuck::cast_slice(&gpu_sources),
            usage:    wgpu::BufferUsages::STORAGE,
        });
        let buf_params = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label:    Some("params"),
            contents: bytemuck::bytes_of(&params),
            usage:    wgpu::BufferUsages::UNIFORM,
        });

        let result_size = (n_rcv as u64) * std::mem::size_of::<f32>() as u64;
        let buf_results = self.device.create_buffer(&wgpu::BufferDescriptor {
            label:             Some("results"),
            size:              result_size,
            usage:             wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let buf_readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label:             Some("readback"),
            size:              result_size,
            usage:             wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label:  Some("noise_bind_group"),
            layout: &self.bg_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: buf_receivers.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: buf_sources.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: buf_params.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: buf_results.as_entire_binding() },
            ],
        });

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("noise_encoder"),
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("noise_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            // Workgroup size = 64; ceil(n_rcv / 64).
            let groups = (n_rcv + 63) / 64;
            pass.dispatch_workgroups(groups, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&buf_results, 0, &buf_readback, 0, result_size);
        self.queue.submit(std::iter::once(encoder.finish()));

        // Map and read back results.
        let slice = buf_readback.slice(..);
        let (tx, rx) = futures::channel::oneshot::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| { let _ = tx.send(r); });
        self.device.poll(wgpu::Maintain::Wait);
        rx.await.expect("GPU readback failed").expect("GPU map error");

        let data = slice.get_mapped_range();
        bytemuck::cast_slice::<u8, f32>(&data).to_vec()
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Smoke test: shader source is syntactically non-empty.
    #[test]
    fn shader_source_non_empty() {
        assert!(!SHADER_SRC.is_empty());
        assert!(SHADER_SRC.contains("@compute"));
        assert!(SHADER_SRC.contains("@workgroup_size(64)"));
    }

    /// GpuReceiver and GpuSource are Pod (required by bytemuck).
    #[test]
    fn gpu_types_are_pod() {
        let r = GpuReceiver { x: 1.0, y: 2.0, z: 3.0, _pad: 0.0 };
        let _bytes: &[u8] = bytemuck::bytes_of(&r);
        let s = GpuSource {
            x: 0.0, y: 0.0, z: 0.0, g_source: 0.5,
            lw: [90.0; 8],
        };
        let _bytes: &[u8] = bytemuck::bytes_of(&s);
    }

    /// GpuParams is Pod and 16-byte aligned.
    #[test]
    fn gpu_params_is_pod() {
        let p = GpuParams {
            a_atm: [0.0; 8],
            a_weights: [0.0; 8],
            n_sources: 1,
            n_receivers: 1,
            _pad: [0; 2],
        };
        let _bytes: &[u8] = bytemuck::bytes_of(&p);
        assert_eq!(std::mem::size_of::<GpuParams>(), 80);
    }
}
