//! Iso-contour extraction from 2D noise grids using the Marching Squares algorithm.
//!
//! # Algorithm
//! For each 2×2 cell of grid values, a 4-bit case index (0–15) is computed from
//! which corners are above the iso-level.  Edge crossings are linearly interpolated
//! and connected into line segments.

/// A single line segment between two 2D points (world coordinates).
pub type Segment = ([f32; 2], [f32; 2]);

/// Collection of line segments forming one iso-contour line at a fixed level.
#[derive(Debug, Clone)]
pub struct IsoContourLine {
    /// Iso-level (dBA) this contour represents.
    pub level_db: f32,
    /// Unordered set of line segments.
    pub segments: Vec<Segment>,
}

impl IsoContourLine {
    /// Total number of line segments.
    pub fn segment_count(&self) -> usize { self.segments.len() }

    /// Total cumulative length of all segments (m).
    pub fn total_length(&self) -> f32 {
        self.segments.iter().map(|(a, b)| {
            let dx = b[0] - a[0];
            let dy = b[1] - a[1];
            (dx * dx + dy * dy).sqrt()
        }).sum()
    }
}

/// Extract iso-contour lines from a flat row-major grid at multiple levels.
///
/// # Parameters
/// - `grid` — noise levels (dBA) in row-major order (row 0 = south)
/// - `nx`, `ny` — grid dimensions (columns, rows)
/// - `dx`, `dy` — cell spacing (m)
/// - `origin` — south-west corner world coordinates
/// - `levels` — iso-levels to extract (dBA)
pub fn extract_isolines(
    grid: &[f32],
    nx: usize,
    ny: usize,
    dx: f32,
    dy: f32,
    origin: [f32; 2],
    levels: &[f32],
) -> Vec<IsoContourLine> {
    levels.iter().map(|&level| {
        IsoContourLine {
            level_db: level,
            segments: marching_squares(grid, nx, ny, dx, dy, origin, level),
        }
    }).collect()
}

/// Run Marching Squares for a single iso-level and return the set of segments.
fn marching_squares(
    grid: &[f32],
    nx: usize,
    ny: usize,
    dx: f32,
    dy: f32,
    origin: [f32; 2],
    level: f32,
) -> Vec<Segment> {
    let mut segments = Vec::new();
    if nx < 2 || ny < 2 { return segments; }

    for row in 0..ny - 1 {
        for col in 0..nx - 1 {
            // Cell corners (counter-clockwise from bottom-left):
            // bl=0, br=1, tr=2, tl=3
            let bl = grid_val(grid, nx, col,     row    );
            let br = grid_val(grid, nx, col + 1, row    );
            let tr = grid_val(grid, nx, col + 1, row + 1);
            let tl = grid_val(grid, nx, col,     row + 1);

            let case = (above(bl, level) as u8)
                | ((above(br, level) as u8) << 1)
                | ((above(tr, level) as u8) << 2)
                | ((above(tl, level) as u8) << 3);

            // World positions of the four corners.
            let x0 = origin[0] + col as f32 * dx;
            let y0 = origin[1] + row as f32 * dy;
            let x1 = x0 + dx;
            let y1 = y0 + dy;

            // Edge interpolation helpers.
            // Edge B (bottom): bl → br
            let e_b = || interp([x0, y0], [x1, y0], bl, br, level);
            // Edge R (right): br → tr
            let e_r = || interp([x1, y0], [x1, y1], br, tr, level);
            // Edge T (top): tl → tr
            let e_t = || interp([x0, y1], [x1, y1], tl, tr, level);
            // Edge L (left): bl → tl
            let e_l = || interp([x0, y0], [x0, y1], bl, tl, level);

            // 16 Marching Squares cases → edge pairs that form segments.
            match case {
                0 | 15 => {} // fully inside or outside
                1 | 14 => segments.push((e_b(), e_l())),
                2 | 13 => segments.push((e_b(), e_r())),
                3 | 12 => segments.push((e_l(), e_r())),
                4 | 11 => segments.push((e_t(), e_r())),
                5  => { segments.push((e_b(), e_r())); segments.push((e_t(), e_l())); }
                6 | 9  => segments.push((e_b(), e_t())),
                7 | 8  => segments.push((e_t(), e_l())),
                10 => { segments.push((e_b(), e_l())); segments.push((e_t(), e_r())); }
                _  => {} // unreachable for u8 & 0xF
            }
        }
    }
    segments
}

#[inline]
fn grid_val(grid: &[f32], nx: usize, col: usize, row: usize) -> f32 {
    let idx = row * nx + col;
    if idx < grid.len() { grid[idx] } else { f32::NEG_INFINITY }
}

#[inline]
fn above(v: f32, level: f32) -> bool { v.is_finite() && v >= level }

/// Linearly interpolate the crossing point on an edge.
fn interp(p0: [f32; 2], p1: [f32; 2], v0: f32, v1: f32, level: f32) -> [f32; 2] {
    let dv = v1 - v0;
    let t = if dv.abs() < 1e-9 { 0.5 } else { (level - v0) / dv };
    let t = t.clamp(0.0, 1.0);
    [p0[0] + t * (p1[0] - p0[0]), p0[1] + t * (p1[1] - p0[1])]
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Uniform grid at a constant level — no crossings for any iso-level not equal to it.
    #[test]
    fn uniform_grid_no_contours() {
        let grid = vec![60.0f32; 9]; // 3×3
        let lines = extract_isolines(&grid, 3, 3, 1.0, 1.0, [0.0, 0.0], &[65.0]);
        assert_eq!(lines[0].segment_count(), 0);
    }

    /// Step function: left half 50 dB, right half 70 dB → vertical iso-contour at x=1.
    #[test]
    fn step_grid_vertical_contour() {
        // 2×2: col0=50, col1=70 (all rows)
        let grid = vec![50.0f32, 70.0, 50.0, 70.0];
        let lines = extract_isolines(&grid, 2, 2, 1.0, 1.0, [0.0, 0.0], &[60.0]);
        // One cell → case 10 or similar → 1 or 2 segments
        assert!(lines[0].segment_count() >= 1, "expected at least 1 segment");
    }

    #[test]
    fn step_grid_produces_segments_with_x_near_half() {
        let grid = vec![50.0f32, 70.0, 50.0, 70.0]; // 2×2
        let lines = extract_isolines(&grid, 2, 2, 2.0, 2.0, [0.0, 0.0], &[60.0]);
        for (a, b) in &lines[0].segments {
            // Crossing at x=1 (linear interpolation between 50 and 70 gives t=0.5 → x=1.0)
            assert!((a[0] - 1.0).abs() < 0.01 || (b[0] - 1.0).abs() < 0.01,
                "crossing should be at x=1.0, got {:?} → {:?}", a, b);
        }
    }

    #[test]
    fn gradient_grid_has_contours() {
        // 3×3 gradient: 40..80 diagonal
        let grid: Vec<f32> = (0..9).map(|i| 40.0 + i as f32 * 5.0).collect();
        let lines = extract_isolines(&grid, 3, 3, 1.0, 1.0, [0.0, 0.0], &[55.0, 65.0]);
        // Both iso-levels should produce at least 1 segment.
        for line in &lines {
            assert!(line.segment_count() >= 1,
                "level {:.0} should produce segments", line.level_db);
        }
    }

    #[test]
    fn total_length_positive() {
        let grid: Vec<f32> = (0..9).map(|i| 40.0 + i as f32 * 5.0).collect();
        let lines = extract_isolines(&grid, 3, 3, 5.0, 5.0, [0.0, 0.0], &[60.0]);
        for line in &lines {
            if line.segment_count() > 0 {
                assert!(line.total_length() > 0.0);
            }
        }
    }

    #[test]
    fn multiple_levels_all_extracted() {
        let grid = vec![60.0f32; 9];
        let levels = [50.0, 60.0, 70.0];
        let lines = extract_isolines(&grid, 3, 3, 1.0, 1.0, [0.0, 0.0], &levels);
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn grid_too_small_returns_empty() {
        let grid = vec![60.0f32; 1];
        let lines = extract_isolines(&grid, 1, 1, 1.0, 1.0, [0.0, 0.0], &[55.0]);
        assert_eq!(lines[0].segment_count(), 0);
    }

    #[test]
    fn interp_midpoint() {
        let p = interp([0.0, 0.0], [2.0, 0.0], 50.0, 70.0, 60.0);
        // t = (60-50)/(70-50) = 0.5 → x = 1.0
        assert!((p[0] - 1.0).abs() < 1e-5);
    }
}
