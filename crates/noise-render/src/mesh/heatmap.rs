//! Heatmap mesh — one quad per noise grid cell, coloured by noise level.
//!
//! The mesh is rendered as a flat 2D plane (z = 0) in clip/world space.
//! Each cell is two triangles sharing a diagonal.

use bytemuck::{Pod, Zeroable};

/// Vertex for the 2D noise heatmap.
///
/// `position` is in world space (XY plane), `noise_level` is dBA.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct HeatmapVertex {
    /// World XY position (m).
    pub position:    [f32; 2],
    /// Noise level at this vertex (dBA); interpolated across quads.
    pub noise_level: f32,
    /// Padding to 12-byte alignment.
    pub _pad: f32,
}

impl HeatmapVertex {
    pub fn new(x: f32, y: f32, level: f32) -> Self {
        Self { position: [x, y], noise_level: level, _pad: 0.0 }
    }
    /// Vertex buffer layout descriptor (for wgpu).
    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute { offset: 0,  shader_location: 0, format: wgpu::VertexFormat::Float32x2 },
                wgpu::VertexAttribute { offset: 8,  shader_location: 1, format: wgpu::VertexFormat::Float32 },
                wgpu::VertexAttribute { offset: 12, shader_location: 2, format: wgpu::VertexFormat::Float32 },
            ],
        }
    }
}

/// Indexed triangle mesh for a noise heatmap grid.
pub struct HeatmapMesh {
    pub vertices: Vec<HeatmapVertex>,
    /// Triangle indices (u32).
    pub indices: Vec<u32>,
}

impl HeatmapMesh {
    /// Build the heatmap mesh from a flat row-major `levels` array.
    ///
    /// # Parameters
    /// - `levels` — dBA values (row-major, row 0 = south); `len == nx * ny`.
    /// - `nx`, `ny` — grid dimensions.
    /// - `dx`, `dy` — cell spacing (m).
    /// - `origin` — south-west corner in world XY (m).
    ///
    /// Each grid *cell* becomes a quad; the four corner values are averaged
    /// to produce the per-vertex noise level.
    pub fn from_grid(
        levels: &[f32],
        nx: usize,
        ny: usize,
        dx: f32,
        dy: f32,
        origin: [f32; 2],
    ) -> Self {
        // One vertex per grid point.
        let mut vertices = Vec::with_capacity(nx * ny);
        for row in 0..ny {
            for col in 0..nx {
                let x = origin[0] + col as f32 * dx;
                let y = origin[1] + row as f32 * dy;
                let level = levels.get(row * nx + col).copied().unwrap_or(f32::NEG_INFINITY);
                vertices.push(HeatmapVertex::new(x, y, level));
            }
        }

        // Two triangles per cell.
        let cells = nx.saturating_sub(1) * ny.saturating_sub(1);
        let mut indices = Vec::with_capacity(cells * 6);
        if nx >= 2 && ny >= 2 {
            for row in 0..ny - 1 {
                for col in 0..nx - 1 {
                    let bl = (row * nx + col) as u32;
                    let br = bl + 1;
                    let tl = bl + nx as u32;
                    let tr = tl + 1;
                    // Triangle 1: bl → br → tl
                    indices.extend_from_slice(&[bl, br, tl]);
                    // Triangle 2: br → tr → tl
                    indices.extend_from_slice(&[br, tr, tl]);
                }
            }
        }

        Self { vertices, indices }
    }

    /// Number of triangles.
    pub fn triangle_count(&self) -> usize { self.indices.len() / 3 }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertex_count_matches_grid_points() {
        let levels = vec![65.0f32; 12]; // 4×3
        let mesh = HeatmapMesh::from_grid(&levels, 4, 3, 1.0, 1.0, [0.0, 0.0]);
        assert_eq!(mesh.vertices.len(), 12);
    }

    #[test]
    fn triangle_count_for_2x2_grid() {
        let levels = vec![60.0f32; 4];
        let mesh = HeatmapMesh::from_grid(&levels, 2, 2, 1.0, 1.0, [0.0, 0.0]);
        // 1 cell → 2 triangles
        assert_eq!(mesh.triangle_count(), 2);
    }

    #[test]
    fn triangle_count_for_nxm_grid() {
        let nx = 5; let ny = 4;
        let levels = vec![60.0f32; nx * ny];
        let mesh = HeatmapMesh::from_grid(&levels, nx, ny, 2.0, 2.0, [0.0, 0.0]);
        // (nx-1)*(ny-1) cells, 2 triangles each
        assert_eq!(mesh.triangle_count(), (nx - 1) * (ny - 1) * 2);
    }

    #[test]
    fn origin_offset_applied() {
        let levels = vec![60.0f32; 4];
        let mesh = HeatmapMesh::from_grid(&levels, 2, 2, 1.0, 1.0, [10.0, 20.0]);
        assert!((mesh.vertices[0].position[0] - 10.0).abs() < 1e-5);
        assert!((mesh.vertices[0].position[1] - 20.0).abs() < 1e-5);
    }

    #[test]
    fn noise_level_stored_correctly() {
        let levels = vec![55.0f32, 65.0, 70.0, 75.0];
        let mesh = HeatmapMesh::from_grid(&levels, 2, 2, 1.0, 1.0, [0.0, 0.0]);
        assert!((mesh.vertices[0].noise_level - 55.0).abs() < 1e-5);
        assert!((mesh.vertices[3].noise_level - 75.0).abs() < 1e-5);
    }

    #[test]
    fn empty_grid_produces_no_triangles() {
        let mesh = HeatmapMesh::from_grid(&[], 0, 0, 1.0, 1.0, [0.0, 0.0]);
        assert_eq!(mesh.triangle_count(), 0);
    }

    #[test]
    fn single_row_produces_no_triangles() {
        let levels = vec![60.0f32; 3];
        let mesh = HeatmapMesh::from_grid(&levels, 3, 1, 1.0, 1.0, [0.0, 0.0]);
        assert_eq!(mesh.triangle_count(), 0);
    }
}
