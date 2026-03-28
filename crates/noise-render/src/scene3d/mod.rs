//! 3D scene renderer.
//!
//! Manages 3D camera, terrain mesh generation and the GPU pipeline for
//! rendering terrain + noise heatmap overlay.
//!
//! In CPU-only mode (no GPU context) it generates mesh data for export or
//! for later upload to the GPU.

use noise_core::grid::horizontal::HorizontalGrid;

use crate::camera::Camera3D;
use crate::color::ColorMap;
use crate::mesh::terrain::TerrainMesh;
use crate::mesh::heatmap::HeatmapMesh;

/// A scene object drawn in the 3D view.
#[derive(Debug, Clone)]
pub struct SceneObject {
    /// Human-readable label.
    pub label: String,
    /// World-space bounding box [min_x, min_y, min_z, max_x, max_y, max_z].
    pub bounds: [f32; 6],
    /// Whether this object is currently visible.
    pub visible: bool,
}

/// 3D scene renderer.
///
/// Holds camera state and scene objects.  Mesh data is generated lazily and
/// returned as CPU-side structs for caller to upload to the GPU.
pub struct Scene3DRenderer {
    /// 3D perspective camera.
    pub camera: Camera3D,
    /// Colour scale for the noise heatmap overlay.
    pub color_map: ColorMap,
    /// Scene objects (buildings, barriers, etc.).
    pub objects: Vec<SceneObject>,
}

impl Default for Scene3DRenderer {
    fn default() -> Self {
        Self {
            camera: Camera3D::default(),
            color_map: ColorMap::who_standard(),
            objects: Vec::new(),
        }
    }
}

impl Scene3DRenderer {
    /// Construct with a custom camera.
    pub fn new(camera: Camera3D) -> Self {
        Self { camera, ..Default::default() }
    }

    // ── Camera control ────────────────────────────────────────────────────────

    pub fn set_camera(&mut self, camera: Camera3D) {
        self.camera = camera;
    }

    /// Orbit around the target.
    pub fn orbit(&mut self, delta_yaw_deg: f32, delta_pitch_deg: f32) {
        self.camera.orbit(delta_yaw_deg, delta_pitch_deg);
    }

    /// Zoom towards / away from target.
    pub fn zoom(&mut self, factor: f32) {
        self.camera.zoom(factor);
    }

    /// Frame the camera to encompass the given world extent.
    pub fn frame_extent(&mut self, min_xy: [f32; 2], max_xy: [f32; 2], height: f32) {
        let cx = (min_xy[0] + max_xy[0]) / 2.0;
        let cy = (min_xy[1] + max_xy[1]) / 2.0;
        let extent = ((max_xy[0] - min_xy[0]).powi(2) + (max_xy[1] - min_xy[1]).powi(2)).sqrt();
        self.camera.target = [cx, cy, 0.0];
        self.camera.position = [cx, cy - extent * 0.6, extent * 0.6 + height];
    }

    // ── Mesh generation ───────────────────────────────────────────────────────

    /// Build a terrain mesh from a height grid (uses `grid.results` as elevation).
    ///
    /// If `grid.results` is empty a flat terrain at z=0 is generated.
    pub fn build_terrain_mesh(&self, grid: &HorizontalGrid) -> TerrainMesh {
        let heights: Vec<f32> = if grid.results.is_empty() {
            vec![0.0; grid.nx * grid.ny]
        } else {
            grid.results.iter().map(|&v| v as f32).collect()
        };
        TerrainMesh::from_heightfield(
            &heights,
            grid.nx,
            grid.ny,
            grid.dx as f32,
            grid.dy as f32,
            [grid.origin.x as f32, grid.origin.y as f32],
        )
    }

    /// Build a heatmap mesh for overlay on the 3D terrain.
    pub fn build_heatmap_mesh(&self, grid: &HorizontalGrid) -> Option<HeatmapMesh> {
        if grid.results.is_empty() { return None; }
        let levels: Vec<f32> = grid.results.iter().map(|&v| v as f32).collect();
        Some(HeatmapMesh::from_grid(
            &levels,
            grid.nx,
            grid.ny,
            grid.dx as f32,
            grid.dy as f32,
            [grid.origin.x as f32, grid.origin.y as f32],
        ))
    }

    /// Return the camera view-projection matrix as column-major array.
    pub fn view_proj_array(&self) -> [[f32; 4]; 4] {
        self.camera.view_proj_array()
    }

    // ── Scene objects ─────────────────────────────────────────────────────────

    /// Add a labelled scene object.
    pub fn add_object(&mut self, label: impl Into<String>, bounds: [f32; 6]) {
        self.objects.push(SceneObject { label: label.into(), bounds, visible: true });
    }

    pub fn object_count(&self) -> usize { self.objects.len() }

    pub fn visible_objects(&self) -> impl Iterator<Item = &SceneObject> {
        self.objects.iter().filter(|o| o.visible)
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use nalgebra::Point3;
    use approx::assert_abs_diff_eq;

    fn make_grid() -> HorizontalGrid {
        let mut g = HorizontalGrid::new(
            1, "test", Point3::new(0.0, 0.0, 0.0), 10.0, 10.0, 5, 4, 4.0,
        );
        g.results = vec![60.0; 20];
        g
    }

    #[test]
    fn default_renderer_has_camera() {
        let r = Scene3DRenderer::default();
        assert!(r.camera.distance() > 0.0);
    }

    #[test]
    fn terrain_mesh_vertex_count() {
        let r = Scene3DRenderer::default();
        let g = make_grid();
        let mesh = r.build_terrain_mesh(&g);
        assert_eq!(mesh.vertices.len(), 20);
    }

    #[test]
    fn terrain_mesh_triangle_count() {
        let r = Scene3DRenderer::default();
        let g = make_grid();
        let mesh = r.build_terrain_mesh(&g);
        assert_eq!(mesh.triangle_count(), (5 - 1) * (4 - 1) * 2);
    }

    #[test]
    fn heatmap_mesh_returned_when_results_present() {
        let r = Scene3DRenderer::default();
        let g = make_grid();
        assert!(r.build_heatmap_mesh(&g).is_some());
    }

    #[test]
    fn heatmap_mesh_none_when_empty_results() {
        let r = Scene3DRenderer::default();
        let g = HorizontalGrid::new(1, "e", Point3::new(0.0, 0.0, 0.0), 1.0, 1.0, 3, 3, 4.0);
        assert!(r.build_heatmap_mesh(&g).is_none());
    }

    #[test]
    fn orbit_changes_camera_position() {
        let mut r = Scene3DRenderer::default();
        let orig = r.camera.position;
        r.orbit(30.0, 10.0);
        assert_ne!(r.camera.position, orig);
    }

    #[test]
    fn zoom_changes_distance() {
        let mut r = Scene3DRenderer::default();
        let d0 = r.camera.distance();
        r.zoom(0.5);
        assert_abs_diff_eq!(r.camera.distance(), d0 * 0.5, epsilon = 0.5);
    }

    #[test]
    fn frame_extent_positions_camera_above_center() {
        let mut r = Scene3DRenderer::default();
        r.frame_extent([0.0, 0.0], [200.0, 100.0], 0.0);
        assert_abs_diff_eq!(r.camera.target[0], 100.0, epsilon = 0.1);
        assert_abs_diff_eq!(r.camera.target[1], 50.0, epsilon = 0.1);
        assert!(r.camera.position[2] > 0.0);
    }

    #[test]
    fn add_object_increments_count() {
        let mut r = Scene3DRenderer::default();
        r.add_object("barrier A", [0.0, 0.0, 0.0, 10.0, 0.5, 4.0]);
        r.add_object("barrier B", [20.0, 0.0, 0.0, 30.0, 0.5, 4.0]);
        assert_eq!(r.object_count(), 2);
        assert_eq!(r.visible_objects().count(), 2);
    }

    #[test]
    fn view_proj_array_is_valid_matrix() {
        let r = Scene3DRenderer::default();
        let vp = r.view_proj_array();
        // Check it isn't the zero matrix.
        let sum: f32 = vp.iter().flatten().map(|v| v.abs()).sum();
        assert!(sum > 0.1);
    }
}
