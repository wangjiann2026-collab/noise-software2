//! 3D camera — view and projection matrices for the GPU renderer.
//!
//! Uses a right-handed coordinate system with Y-up.
//! The `view_proj_matrix()` output is in the wgpu clip-space convention
//! (depth 0→1, NDC X/Y −1→+1).

use glam::{Mat4, Vec3};
use serde::{Deserialize, Serialize};

/// Perspective 3D camera.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Camera3D {
    /// Camera position in world space.
    pub position: [f32; 3],
    /// Point the camera looks at.
    pub target: [f32; 3],
    /// Up vector (typically [0, 1, 0]).
    pub up: [f32; 3],
    /// Vertical field of view (degrees).
    pub fov_y_deg: f32,
    /// Viewport aspect ratio (width / height).
    pub aspect: f32,
    /// Near clip plane distance (m).
    pub near: f32,
    /// Far clip plane distance (m).
    pub far: f32,
}

impl Default for Camera3D {
    fn default() -> Self {
        Self {
            position: [200.0, 300.0, 200.0],
            target:   [0.0,   0.0,   0.0],
            up:       [0.0,   1.0,   0.0],
            fov_y_deg: 45.0,
            aspect:    16.0 / 9.0,
            near:      0.1,
            far:       10_000.0,
        }
    }
}

impl Camera3D {
    /// Construct from explicit parameters.
    pub fn new(
        position: [f32; 3],
        target: [f32; 3],
        fov_y_deg: f32,
        aspect: f32,
        near: f32,
        far: f32,
    ) -> Self {
        Self { position, target, up: [0.0, 1.0, 0.0], fov_y_deg, aspect, near, far }
    }

    /// Look-at view matrix (right-handed).
    pub fn view_matrix(&self) -> Mat4 {
        Mat4::look_at_rh(
            Vec3::from(self.position),
            Vec3::from(self.target),
            Vec3::from(self.up),
        )
    }

    /// Perspective projection matrix for wgpu clip space (depth 0→1).
    pub fn projection_matrix(&self) -> Mat4 {
        Mat4::perspective_rh(
            self.fov_y_deg.to_radians(),
            self.aspect,
            self.near,
            self.far,
        )
    }

    /// Combined view-projection matrix (projection × view).
    pub fn view_proj_matrix(&self) -> Mat4 {
        self.projection_matrix() * self.view_matrix()
    }

    /// Return the `view_proj` as a column-major `[[f32; 4]; 4]` array
    /// suitable for upload to a GPU uniform buffer.
    pub fn view_proj_array(&self) -> [[f32; 4]; 4] {
        self.view_proj_matrix().to_cols_array_2d()
    }

    /// Return eye-to-target direction (normalised).
    pub fn forward(&self) -> Vec3 {
        (Vec3::from(self.target) - Vec3::from(self.position)).normalize()
    }

    /// Distance from camera to target.
    pub fn distance(&self) -> f32 {
        (Vec3::from(self.target) - Vec3::from(self.position)).length()
    }

    /// Orbit the camera around its target at the same distance.
    ///
    /// `delta_yaw` / `delta_pitch` in degrees.
    pub fn orbit(&mut self, delta_yaw_deg: f32, delta_pitch_deg: f32) {
        let target = Vec3::from(self.target);
        let pos    = Vec3::from(self.position);
        let offset = pos - target;
        let dist   = offset.length();

        // Convert to spherical.
        let yaw   = offset.z.atan2(offset.x) + delta_yaw_deg.to_radians();
        let pitch = (offset.y / dist)
            .asin()
            .clamp(-89.0_f32.to_radians(), 89.0_f32.to_radians())
            + delta_pitch_deg.to_radians();
        let pitch = pitch.clamp(-89.0_f32.to_radians(), 89.0_f32.to_radians());

        let new_offset = Vec3::new(
            dist * pitch.cos() * yaw.cos(),
            dist * pitch.sin(),
            dist * pitch.cos() * yaw.sin(),
        );
        let new_pos = target + new_offset;
        self.position = new_pos.into();
    }

    /// Zoom the camera (multiply distance by `factor`).
    pub fn zoom(&mut self, factor: f32) {
        let target = Vec3::from(self.target);
        let pos    = Vec3::from(self.position);
        let offset = (pos - target) * factor.max(0.01);
        self.position = (target + offset).into();
    }
}

/// Orthographic 2D camera for the flat heatmap view.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Camera2D {
    /// World-space centre of the viewport.
    pub center: [f32; 2],
    /// Zoom level: world units per pixel.
    pub world_per_pixel: f32,
    /// Viewport size (pixels).
    pub viewport_px: [u32; 2],
}

impl Camera2D {
    pub fn new(center: [f32; 2], world_per_pixel: f32, viewport_px: [u32; 2]) -> Self {
        Self { center, world_per_pixel, viewport_px }
    }

    /// Orthographic projection matrix.
    pub fn projection_matrix(&self) -> Mat4 {
        let w = self.viewport_px[0] as f32 * self.world_per_pixel;
        let h = self.viewport_px[1] as f32 * self.world_per_pixel;
        let cx = self.center[0];
        let cy = self.center[1];
        Mat4::orthographic_rh(cx - w / 2.0, cx + w / 2.0, cy - h / 2.0, cy + h / 2.0, -1.0, 1.0)
    }

    /// Convert pixel coordinates to world coordinates.
    pub fn pixel_to_world(&self, px: f32, py: f32) -> [f32; 2] {
        let vw = self.viewport_px[0] as f32;
        let vh = self.viewport_px[1] as f32;
        let w = vw * self.world_per_pixel;
        let h = vh * self.world_per_pixel;
        [
            self.center[0] + (px / vw - 0.5) * w,
            self.center[1] + (0.5 - py / vh) * h,
        ]
    }

    /// Convert world coordinates to pixel coordinates.
    pub fn world_to_pixel(&self, wx: f32, wy: f32) -> [f32; 2] {
        let vw = self.viewport_px[0] as f32;
        let vh = self.viewport_px[1] as f32;
        let w = vw * self.world_per_pixel;
        let h = vh * self.world_per_pixel;
        [
            ((wx - self.center[0]) / w + 0.5) * vw,
            (0.5 - (wy - self.center[1]) / h) * vh,
        ]
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    #[test]
    fn view_matrix_not_identity() {
        let cam = Camera3D::default();
        let view = cam.view_matrix();
        assert_ne!(view, Mat4::IDENTITY);
    }

    #[test]
    fn projection_matrix_not_identity() {
        let cam = Camera3D::default();
        let proj = cam.projection_matrix();
        assert_ne!(proj, Mat4::IDENTITY);
    }

    #[test]
    fn view_proj_is_product() {
        let cam = Camera3D::default();
        let vp   = cam.view_proj_matrix();
        let expected = cam.projection_matrix() * cam.view_matrix();
        for (a, b) in vp.to_cols_array().iter().zip(expected.to_cols_array().iter()) {
            assert_abs_diff_eq!(a, b, epsilon = 1e-5);
        }
    }

    #[test]
    fn forward_is_normalised() {
        let cam = Camera3D::default();
        let f = cam.forward();
        assert_abs_diff_eq!(f.length(), 1.0, epsilon = 1e-5);
    }

    #[test]
    fn orbit_changes_position_not_target() {
        let mut cam = Camera3D::default();
        let orig_target = cam.target;
        cam.orbit(30.0, 15.0);
        assert_eq!(cam.target, orig_target);
        assert_ne!(cam.position, Camera3D::default().position);
    }

    #[test]
    fn orbit_preserves_distance() {
        let mut cam = Camera3D::default();
        let d0 = cam.distance();
        cam.orbit(45.0, -20.0);
        assert_abs_diff_eq!(cam.distance(), d0, epsilon = 0.1);
    }

    #[test]
    fn zoom_changes_distance() {
        let mut cam = Camera3D::default();
        let d0 = cam.distance();
        cam.zoom(0.5);
        assert_abs_diff_eq!(cam.distance(), d0 * 0.5, epsilon = 0.1);
    }

    #[test]
    fn camera2d_pixel_world_roundtrip() {
        let cam = Camera2D::new([500.0, 500.0], 1.0, [1000, 1000]);
        let px = [250.0f32, 300.0];
        let wx = cam.pixel_to_world(px[0], px[1]);
        let back = cam.world_to_pixel(wx[0], wx[1]);
        assert_abs_diff_eq!(back[0], px[0], epsilon = 0.01);
        assert_abs_diff_eq!(back[1], px[1], epsilon = 0.01);
    }

    #[test]
    fn camera2d_center_maps_to_center_pixel() {
        let cam = Camera2D::new([100.0, 200.0], 2.0, [800, 600]);
        let px = cam.world_to_pixel(100.0, 200.0);
        assert_abs_diff_eq!(px[0], 400.0, epsilon = 0.01);
        assert_abs_diff_eq!(px[1], 300.0, epsilon = 0.01);
    }
}
