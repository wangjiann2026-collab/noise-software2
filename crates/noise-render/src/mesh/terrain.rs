//! 3D terrain mesh generation from a heightfield.
//!
//! Generates an indexed triangle mesh with per-vertex normals computed from
//! central differences (smooth shading).

use bytemuck::{Pod, Zeroable};

/// Vertex for the 3D terrain shader.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct TerrainVertex {
    /// World position (X, Y = horizontal, Z = elevation in metres).
    pub position: [f32; 3],
    /// Outward surface normal (normalised).
    pub normal:   [f32; 3],
    /// UV texture coordinate [0, 1]².
    pub uv:       [f32; 2],
}

impl TerrainVertex {
    /// Vertex buffer layout descriptor (for wgpu).
    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute { offset: 0,  shader_location: 0, format: wgpu::VertexFormat::Float32x3 },
                wgpu::VertexAttribute { offset: 12, shader_location: 1, format: wgpu::VertexFormat::Float32x3 },
                wgpu::VertexAttribute { offset: 24, shader_location: 2, format: wgpu::VertexFormat::Float32x2 },
            ],
        }
    }
}

/// Indexed triangle mesh for a terrain.
pub struct TerrainMesh {
    pub vertices: Vec<TerrainVertex>,
    pub indices:  Vec<u32>,
}

impl TerrainMesh {
    /// Build a terrain mesh from a flat row-major heightfield.
    ///
    /// # Parameters
    /// - `heights` — elevation (m) values in row-major order (row 0 = south).
    /// - `nx`, `ny` — number of sample columns and rows.
    /// - `dx`, `dy` — horizontal sample spacing (m).
    /// - `origin`   — south-west corner world XY.
    pub fn from_heightfield(
        heights: &[f32],
        nx: usize,
        ny: usize,
        dx: f32,
        dy: f32,
        origin: [f32; 2],
    ) -> Self {
        let mut vertices = Vec::with_capacity(nx * ny);

        for row in 0..ny {
            for col in 0..nx {
                let x = origin[0] + col as f32 * dx;
                let y = origin[1] + row as f32 * dy;
                let z = heights.get(row * nx + col).copied().unwrap_or(0.0);
                let uv = [col as f32 / (nx - 1).max(1) as f32,
                           row as f32 / (ny - 1).max(1) as f32];
                let normal = compute_normal(heights, nx, ny, col, row, dx, dy);
                vertices.push(TerrainVertex { position: [x, y, z], normal, uv });
            }
        }

        // Two triangles per cell.
        let mut indices = Vec::with_capacity((nx - 1) * (ny - 1) * 6);
        if nx >= 2 && ny >= 2 {
            for row in 0..ny - 1 {
                for col in 0..nx - 1 {
                    let bl = (row * nx + col) as u32;
                    let br = bl + 1;
                    let tl = bl + nx as u32;
                    let tr = tl + 1;
                    indices.extend_from_slice(&[bl, br, tl]);
                    indices.extend_from_slice(&[br, tr, tl]);
                }
            }
        }

        Self { vertices, indices }
    }

    /// Number of triangles.
    pub fn triangle_count(&self) -> usize { self.indices.len() / 3 }

    /// Bounding box [min_x, min_y, min_z, max_x, max_y, max_z].
    pub fn bounding_box(&self) -> [f32; 6] {
        let mut mn = [f32::MAX; 3];
        let mut mx = [f32::MIN; 3];
        for v in &self.vertices {
            for i in 0..3 {
                mn[i] = mn[i].min(v.position[i]);
                mx[i] = mx[i].max(v.position[i]);
            }
        }
        [mn[0], mn[1], mn[2], mx[0], mx[1], mx[2]]
    }
}

/// Compute outward normal at `(col, row)` using central differences.
fn compute_normal(
    heights: &[f32],
    nx: usize,
    ny: usize,
    col: usize,
    row: usize,
    dx: f32,
    dy: f32,
) -> [f32; 3] {
    let h = |c: usize, r: usize| heights.get(r * nx + c).copied().unwrap_or(0.0);

    let dz_dx = if col == 0 {
        (h(1, row) - h(0, row)) / dx
    } else if col == nx - 1 {
        (h(nx - 1, row) - h(nx - 2, row)) / dx
    } else {
        (h(col + 1, row) - h(col - 1, row)) / (2.0 * dx)
    };

    let dz_dy = if row == 0 {
        (h(col, 1) - h(col, 0)) / dy
    } else if row == ny - 1 {
        (h(col, ny - 1) - h(col, ny - 2)) / dy
    } else {
        (h(col, row + 1) - h(col, row - 1)) / (2.0 * dy)
    };

    // Normal: cross(tangent_x, tangent_y) = [-dz/dx, -dz/dy, 1], then normalise.
    let nx_ = -dz_dx;
    let ny_ = -dz_dy;
    let nz_ = 1.0_f32;
    let len = (nx_ * nx_ + ny_ * ny_ + nz_ * nz_).sqrt();
    [nx_ / len, ny_ / len, nz_ / len]
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    fn flat_terrain(nx: usize, ny: usize, h: f32) -> Vec<f32> {
        vec![h; nx * ny]
    }

    #[test]
    fn vertex_count_equals_grid_points() {
        let heights = flat_terrain(5, 4, 0.0);
        let mesh = TerrainMesh::from_heightfield(&heights, 5, 4, 1.0, 1.0, [0.0, 0.0]);
        assert_eq!(mesh.vertices.len(), 20);
    }

    #[test]
    fn triangle_count_correct() {
        let heights = flat_terrain(4, 3, 0.0);
        let mesh = TerrainMesh::from_heightfield(&heights, 4, 3, 1.0, 1.0, [0.0, 0.0]);
        assert_eq!(mesh.triangle_count(), 3 * 2 * 2); // (4-1)*(3-1)*2 = 12
    }

    #[test]
    fn flat_terrain_normals_point_up() {
        let heights = flat_terrain(3, 3, 5.0);
        let mesh = TerrainMesh::from_heightfield(&heights, 3, 3, 1.0, 1.0, [0.0, 0.0]);
        for v in &mesh.vertices {
            assert_abs_diff_eq!(v.normal[0], 0.0, epsilon = 1e-5);
            assert_abs_diff_eq!(v.normal[1], 0.0, epsilon = 1e-5);
            assert_abs_diff_eq!(v.normal[2], 1.0, epsilon = 1e-5);
        }
    }

    #[test]
    fn normals_are_unit_length() {
        let heights: Vec<f32> = (0..16).map(|i| i as f32 * 0.5).collect();
        let mesh = TerrainMesh::from_heightfield(&heights, 4, 4, 1.0, 1.0, [0.0, 0.0]);
        for v in &mesh.vertices {
            let len = (v.normal[0].powi(2) + v.normal[1].powi(2) + v.normal[2].powi(2)).sqrt();
            assert_abs_diff_eq!(len, 1.0, epsilon = 1e-5);
        }
    }

    #[test]
    fn uv_at_corners() {
        let heights = flat_terrain(2, 2, 0.0);
        let mesh = TerrainMesh::from_heightfield(&heights, 2, 2, 1.0, 1.0, [0.0, 0.0]);
        // bl (0,0) → (0,0), br (1,0) → (1,0), tl (0,1) → (0,1), tr (1,1) → (1,1)
        assert_abs_diff_eq!(mesh.vertices[0].uv[0], 0.0, epsilon = 1e-5);
        assert_abs_diff_eq!(mesh.vertices[1].uv[0], 1.0, epsilon = 1e-5);
        assert_abs_diff_eq!(mesh.vertices[2].uv[1], 1.0, epsilon = 1e-5);
    }

    #[test]
    fn bounding_box_covers_full_range() {
        let heights = vec![0.0f32, 0.0, 0.0, 10.0];
        let mesh = TerrainMesh::from_heightfield(&heights, 2, 2, 5.0, 5.0, [10.0, 20.0]);
        let bb = mesh.bounding_box();
        assert_abs_diff_eq!(bb[0], 10.0, epsilon = 1e-5); // min_x
        assert_abs_diff_eq!(bb[3], 15.0, epsilon = 1e-5); // max_x
        assert_abs_diff_eq!(bb[2], 0.0,  epsilon = 1e-5); // min_z
        assert_abs_diff_eq!(bb[5], 10.0, epsilon = 1e-5); // max_z
    }

    #[test]
    fn origin_offset_applied() {
        let heights = flat_terrain(2, 2, 0.0);
        let mesh = TerrainMesh::from_heightfield(&heights, 2, 2, 1.0, 1.0, [100.0, 200.0]);
        assert_abs_diff_eq!(mesh.vertices[0].position[0], 100.0, epsilon = 1e-5);
        assert_abs_diff_eq!(mesh.vertices[0].position[1], 200.0, epsilon = 1e-5);
    }
}
