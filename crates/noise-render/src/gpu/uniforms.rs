//! GPU uniform buffer structs.
//!
//! All types implement `bytemuck::Pod + Zeroable` for direct `wgpu::Buffer` upload.

use bytemuck::{Pod, Zeroable};

/// Camera uniform — view-projection matrix uploaded once per frame.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct CameraUniform {
    /// Column-major 4×4 view-projection matrix.
    pub view_proj: [[f32; 4]; 4],
}

impl CameraUniform {
    pub fn identity() -> Self {
        Self { view_proj: glam::Mat4::IDENTITY.to_cols_array_2d() }
    }

    pub fn from_matrix(m: glam::Mat4) -> Self {
        Self { view_proj: m.to_cols_array_2d() }
    }
}

/// Heatmap uniform — colour scale bounds.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct HeatmapUniform {
    /// Minimum noise level for colour scale (dBA).
    pub min_db: f32,
    /// Maximum noise level for colour scale (dBA).
    pub max_db: f32,
    /// Alpha multiplier (0–1).
    pub alpha: f32,
    /// Padding to 16-byte alignment.
    pub _pad: f32,
}

impl HeatmapUniform {
    pub fn new(min_db: f32, max_db: f32, alpha: f32) -> Self {
        Self { min_db, max_db, alpha, _pad: 0.0 }
    }
}

impl Default for HeatmapUniform {
    fn default() -> Self {
        Self::new(35.0, 75.0, 0.8)
    }
}

/// Light uniform for 3D terrain rendering.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct LightUniform {
    /// Normalised light direction (world space).
    pub direction: [f32; 3],
    /// Intensity multiplier.
    pub intensity: f32,
    /// Ambient intensity.
    pub ambient: f32,
    /// Padding to 32-byte alignment.
    pub _pad: [f32; 3],
}

impl Default for LightUniform {
    fn default() -> Self {
        Self {
            direction: [0.5_f32.sqrt(), 1.0, 0.5_f32.sqrt()],
            intensity: 1.0,
            ambient: 0.15,
            _pad: [0.0; 3],
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_uniform_identity() {
        let u = CameraUniform::identity();
        // Diagonal of identity matrix.
        assert_eq!(u.view_proj[0][0], 1.0);
        assert_eq!(u.view_proj[1][1], 1.0);
        assert_eq!(u.view_proj[2][2], 1.0);
        assert_eq!(u.view_proj[3][3], 1.0);
    }

    #[test]
    fn camera_uniform_pod_size() {
        use std::mem::size_of;
        assert_eq!(size_of::<CameraUniform>(), 64);
    }

    #[test]
    fn heatmap_uniform_pod_size() {
        use std::mem::size_of;
        assert_eq!(size_of::<HeatmapUniform>(), 16);
    }

    #[test]
    fn heatmap_uniform_values() {
        let u = HeatmapUniform::new(40.0, 80.0, 0.9);
        assert_eq!(u.min_db, 40.0);
        assert_eq!(u.max_db, 80.0);
        assert_eq!(u.alpha, 0.9);
    }

    #[test]
    fn zeroable_is_all_zeros() {
        let u: CameraUniform = Zeroable::zeroed();
        for row in &u.view_proj {
            for &v in row { assert_eq!(v, 0.0); }
        }
    }
}
