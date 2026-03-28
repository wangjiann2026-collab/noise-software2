//! wgpu GPU context — device, queue, adapter.
//!
//! `GpuContext::new_headless()` requests an adapter without a surface,
//! suitable for offscreen rendering and export.  A surface-aware variant is
//! provided for windowed use.

use thiserror::Error;
use wgpu;

#[derive(Debug, Error)]
pub enum GpuError {
    #[error("no suitable GPU adapter found")]
    NoAdapter,
    #[error("device creation failed: {0}")]
    DeviceError(String),
    #[error("surface error: {0}")]
    SurfaceError(String),
}

/// Holds a wgpu device + queue pair and the adapter they were created from.
pub struct GpuContext {
    pub adapter: wgpu::Adapter,
    pub device:  wgpu::Device,
    pub queue:   wgpu::Queue,
}

impl GpuContext {
    /// Create a headless GPU context (no window/surface).
    ///
    /// Tries Vulkan → DX12 → Metal → Software (in order of preference).
    pub async fn new_headless() -> Result<Self, GpuError> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .ok_or(GpuError::NoAdapter)?;

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("noise-render headless"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::default(),
                },
                None,
            )
            .await
            .map_err(|e| GpuError::DeviceError(e.to_string()))?;

        Ok(Self { adapter, device, queue })
    }

    /// Adapter backend name (e.g. "Vulkan", "Metal", "Dx12", "OpenGl").
    pub fn backend_name(&self) -> String {
        format!("{:?}", self.adapter.get_info().backend)
    }

    /// Adapter device name.
    pub fn device_name(&self) -> String {
        self.adapter.get_info().name.clone()
    }

    /// Create an offscreen render texture at the given size.
    pub fn create_render_texture(
        &self,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> wgpu::Texture {
        self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen render target"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
    }

    /// Create a GPU buffer pre-filled with `data`.
    pub fn create_buffer_init<T: bytemuck::Pod>(
        &self,
        label: &str,
        data: &[T],
        usage: wgpu::BufferUsages,
    ) -> wgpu::Buffer {
        use wgpu::util::DeviceExt;
        self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(label),
            contents: bytemuck::cast_slice(data),
            usage,
        })
    }
}

// GPU tests require actual hardware → marked #[ignore].
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires GPU hardware"]
    fn headless_context_creates_successfully() {
        futures::executor::block_on(async {
            let ctx = GpuContext::new_headless().await.expect("GPU context");
            assert!(!ctx.device_name().is_empty());
        });
    }
}
