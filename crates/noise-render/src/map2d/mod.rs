//! 2D noise heatmap renderer.
//!
//! Provides CPU-side heatmap rendering (PNG export, SVG iso-contours) and
//! prepares GPU mesh data for the wgpu heatmap pipeline.
//!
//! CPU operations work without any GPU context, making them suitable for
//! batch export, automated testing, and server-side tile generation.

use std::path::Path;

use noise_core::grid::horizontal::HorizontalGrid;

use crate::camera::Camera2D;
use crate::color::ColorMap;
use crate::contour::{extract_isolines, IsoContourLine};
use crate::export::{
    png::{export_grid_png, render_to_buffer},
    svg::{export_svg, SvgStyle, LevelColor},
    ExportError,
};
use crate::mesh::heatmap::HeatmapMesh;

/// 2D noise heatmap renderer.
///
/// In CPU-only mode (default) it operates without a GPU context and supports
/// PNG / SVG export directly from `HorizontalGrid` results.
pub struct Map2DRenderer {
    /// Colour scale used for all rendering operations.
    pub color_map: ColorMap,
    /// 2D orthographic camera.
    pub camera: Camera2D,
    /// Iso-contour levels (dBA) drawn as lines on top of the heatmap.
    pub contour_levels: Vec<f32>,
}

impl Default for Map2DRenderer {
    fn default() -> Self {
        Self {
            color_map: ColorMap::who_standard(),
            camera: Camera2D::new([0.0, 0.0], 1.0, [800, 600]),
            contour_levels: vec![50.0, 55.0, 60.0, 65.0, 70.0],
        }
    }
}

impl Map2DRenderer {
    /// Construct with a custom colour scale.
    pub fn with_color_map(color_map: ColorMap) -> Self {
        Self { color_map, ..Default::default() }
    }

    /// Set the camera to frame a `HorizontalGrid` with some padding.
    pub fn frame_grid(&mut self, grid: &HorizontalGrid, viewport_px: [u32; 2]) {
        let cx = grid.origin.x as f32 + grid.nx as f32 * grid.dx as f32 / 2.0;
        let cy = grid.origin.y as f32 + grid.ny as f32 * grid.dy as f32 / 2.0;
        let world_w = grid.nx as f32 * grid.dx as f32;
        let world_h = grid.ny as f32 * grid.dy as f32;
        let wpp = (world_w / viewport_px[0] as f32)
            .max(world_h / viewport_px[1] as f32)
            * 1.1; // 10 % padding
        self.camera = Camera2D::new([cx, cy], wpp, viewport_px);
    }

    // ── CPU rendering ─────────────────────────────────────────────────────────

    /// Export a computed grid to a PNG file.
    ///
    /// `grid.results` must be populated (e.g. by `GridCalculator::calculate`).
    pub fn export_png(&self, grid: &HorizontalGrid, path: &Path) -> Result<(), ExportError> {
        if grid.results.is_empty() {
            return Err(ExportError::EmptyGrid);
        }
        let levels: Vec<f32> = grid.results.iter().map(|&v| v as f32).collect();
        export_grid_png(&levels, grid.nx, grid.ny, &self.color_map, path)
    }

    /// Render to an in-memory RGBA buffer (width × height × 4 bytes).
    pub fn render_to_buffer(&self, grid: &HorizontalGrid) -> Result<(u32, u32, Vec<u8>), ExportError> {
        if grid.results.is_empty() {
            return Err(ExportError::EmptyGrid);
        }
        let levels: Vec<f32> = grid.results.iter().map(|&v| v as f32).collect();
        render_to_buffer(&levels, grid.nx, grid.ny, &self.color_map)
    }

    /// Extract iso-contour lines from a computed grid.
    pub fn extract_isolines(&self, grid: &HorizontalGrid, levels: &[f32]) -> Vec<IsoContourLine> {
        if grid.results.is_empty() { return Vec::new(); }
        let data: Vec<f32> = grid.results.iter().map(|&v| v as f32).collect();
        extract_isolines(
            &data,
            grid.nx,
            grid.ny,
            grid.dx as f32,
            grid.dy as f32,
            [grid.origin.x as f32, grid.origin.y as f32],
            levels,
        )
    }

    /// Export iso-contour lines to an SVG file.
    pub fn export_svg_isolines(
        &self,
        grid: &HorizontalGrid,
        levels: &[f32],
        path: &Path,
        style: &SvgStyle,
    ) -> Result<(), ExportError> {
        let isolines = self.extract_isolines(grid, levels);
        let bounds = grid_bounds(grid);
        let palette = default_palette(levels);
        export_svg(&isolines, bounds, &palette, style, path)
    }

    // ── GPU mesh preparation ──────────────────────────────────────────────────

    /// Build a `HeatmapMesh` ready for GPU upload.
    ///
    /// Returns `None` if the grid has no results.
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

    /// Statistics for the current grid (min/max/mean dBA).
    pub fn grid_stats(&self, grid: &HorizontalGrid) -> Option<GridStats> {
        if grid.results.is_empty() { return None; }
        let mut min = f32::MAX;
        let mut max = f32::MIN;
        let mut sum = 0.0f64;
        let mut count = 0usize;
        for &v in &grid.results {
            if v.is_finite() {
                min = min.min(v);
                max = max.max(v);
                sum += v as f64;
                count += 1;
            }
        }
        if count == 0 { return None; }
        Some(GridStats { min_dba: min, max_dba: max, mean_dba: (sum / count as f64) as f32, count })
    }
}

/// Basic statistics for a computed noise grid.
#[derive(Debug, Clone)]
pub struct GridStats {
    pub min_dba:  f32,
    pub max_dba:  f32,
    pub mean_dba: f32,
    pub count:    usize,
}

fn grid_bounds(grid: &HorizontalGrid) -> [f32; 4] {
    let ox = grid.origin.x as f32;
    let oy = grid.origin.y as f32;
    [ox, oy, ox + grid.nx as f32 * grid.dx as f32, oy + grid.ny as f32 * grid.dy as f32]
}

fn default_palette(levels: &[f32]) -> Vec<LevelColor> {
    let colors = [
        "#007700", "#00bb55", "#aadd33",
        "#ffee00", "#ffaa00", "#ff5500",
        "#cc0000", "#660000",
    ];
    levels.iter().enumerate().map(|(i, &lvl)| {
        let color = colors.get(i % colors.len()).unwrap_or(&"#333333");
        (lvl, color.to_string())
    }).collect()
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use nalgebra::Point3;

    fn make_grid_with_results() -> HorizontalGrid {
        let mut g = HorizontalGrid::new(
            1, "test", Point3::new(0.0, 0.0, 0.0), 5.0, 5.0, 4, 3, 4.0,
        );
        g.results = (0..12).map(|i| 50.0 + i as f32 * 2.0).collect();
        g
    }

    #[test]
    fn render_to_buffer_size_correct() {
        let r = Map2DRenderer::default();
        let g = make_grid_with_results();
        let (w, h, buf) = r.render_to_buffer(&g).unwrap();
        assert_eq!(w, 4);
        assert_eq!(h, 3);
        assert_eq!(buf.len(), 4 * 3 * 4);
    }

    #[test]
    fn empty_results_returns_error() {
        let r = Map2DRenderer::default();
        let g = HorizontalGrid::new(1, "empty", Point3::new(0.0, 0.0, 0.0), 1.0, 1.0, 3, 3, 4.0);
        assert!(r.render_to_buffer(&g).is_err());
    }

    #[test]
    fn extract_isolines_returns_correct_count() {
        let r = Map2DRenderer::default();
        let g = make_grid_with_results();
        let levels = [55.0f32, 60.0, 65.0];
        let lines = r.extract_isolines(&g, &levels);
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn build_heatmap_mesh_vertex_count() {
        let r = Map2DRenderer::default();
        let g = make_grid_with_results();
        let mesh = r.build_heatmap_mesh(&g).unwrap();
        assert_eq!(mesh.vertices.len(), 12);
    }

    #[test]
    fn grid_stats_min_max() {
        let r = Map2DRenderer::default();
        let g = make_grid_with_results();
        let stats = r.grid_stats(&g).unwrap();
        assert!((stats.min_dba - 50.0).abs() < 0.01);
        assert!((stats.max_dba - 72.0).abs() < 0.01);
        assert_eq!(stats.count, 12);
    }

    #[test]
    fn frame_grid_updates_camera() {
        let mut r = Map2DRenderer::default();
        let g = make_grid_with_results();
        r.frame_grid(&g, [800, 600]);
        assert!(r.camera.world_per_pixel > 0.0);
    }

    #[test]
    fn export_png_creates_file() {
        let r = Map2DRenderer::default();
        let g = make_grid_with_results();
        let path = std::env::temp_dir().join("map2d_test.png");
        r.export_png(&g, &path).unwrap();
        assert!(path.exists());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn export_svg_creates_file() {
        let r = Map2DRenderer::default();
        let g = make_grid_with_results();
        let path = std::env::temp_dir().join("map2d_test.svg");
        r.export_svg_isolines(&g, &[55.0, 60.0], &path, &SvgStyle::default()).unwrap();
        assert!(path.exists());
        let _ = std::fs::remove_file(&path);
    }
}
