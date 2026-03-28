//! Spatial hash grid for source-range culling.
//!
//! In large scenes (100+ sources, 10k+ receivers) most sources lie beyond
//! the audible range at most receivers.  This module provides an O(1)-average
//! query structure that returns only the source indices within a configurable
//! `max_range` of a query point, eliminating unnecessary propagation calls.
//!
//! # Algorithm
//! Sources are bucketed into axis-aligned cells of side length `max_range`.
//! A range query checks the 3×3 neighbourhood of cells (9 cells total) and
//! filters by exact Euclidean distance.  False positives from the cell grid
//! never occur; false negatives are impossible when `cell_size == max_range`.
//!
//! # Example
//! ```
//! use nalgebra::Point3;
//! use noise_core::spatial::SourceCuller;
//!
//! let positions = vec![
//!     Point3::new(0.0, 0.0, 0.5),
//!     Point3::new(500.0, 0.0, 0.5),
//! ];
//! let culler = SourceCuller::new(&positions, 200.0);
//!
//! // Only the first source is within 200 m of the origin.
//! let nearby = culler.query(&Point3::new(0.0, 0.0, 4.0));
//! assert_eq!(nearby, vec![0]);
//! ```

use nalgebra::Point3;
use std::collections::HashMap;

/// Spatial hash grid for fast source-range queries.
pub struct SourceCuller {
    /// Grid cell → list of source indices into the original `positions` slice.
    cells: HashMap<(i64, i64), Vec<usize>>,
    /// Flat list of (x, y) for each source (indexed by original index).
    positions: Vec<(f64, f64)>,
    /// Cell size equals `max_range` (sources fit into at most 4 cells each).
    cell_size: f64,
    /// `max_range²` — avoids a sqrt in the query inner loop.
    max_range_sq: f64,
}

impl SourceCuller {
    /// Build the spatial index from `source_positions`.
    ///
    /// `max_range` – only sources within this horizontal distance (m) from a
    /// query point will be returned.
    pub fn new(source_positions: &[Point3<f64>], max_range: f64) -> Self {
        debug_assert!(max_range > 0.0, "max_range must be positive");
        let cell_size = max_range;
        let mut cells: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
        let positions: Vec<(f64, f64)> = source_positions
            .iter()
            .enumerate()
            .map(|(i, pos)| {
                let cx = cell_x(pos.x, cell_size);
                let cy = cell_x(pos.y, cell_size);
                cells.entry((cx, cy)).or_default().push(i);
                (pos.x, pos.y)
            })
            .collect();

        Self {
            cells,
            positions,
            cell_size,
            max_range_sq: max_range * max_range,
        }
    }

    /// Return the indices of all sources within `max_range` of `query`.
    ///
    /// Indices refer to the original `source_positions` slice passed to [`new`].
    /// The result is unsorted.
    pub fn query(&self, query: &Point3<f64>) -> Vec<usize> {
        let cx = cell_x(query.x, self.cell_size);
        let cy = cell_x(query.y, self.cell_size);
        let qx = query.x;
        let qy = query.y;

        let mut result = Vec::new();
        for dx in -1i64..=1 {
            for dy in -1i64..=1 {
                if let Some(indices) = self.cells.get(&(cx + dx, cy + dy)) {
                    for &i in indices {
                        let (sx, sy) = self.positions[i];
                        let ddx = sx - qx;
                        let ddy = sy - qy;
                        if ddx * ddx + ddy * ddy <= self.max_range_sq {
                            result.push(i);
                        }
                    }
                }
            }
        }
        result
    }

    /// Number of sources in the index.
    pub fn len(&self) -> usize { self.positions.len() }

    /// `true` if no sources were indexed.
    pub fn is_empty(&self) -> bool { self.positions.is_empty() }
}

#[inline(always)]
fn cell_x(coord: f64, cell_size: f64) -> i64 {
    (coord / cell_size).floor() as i64
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(x: f64, y: f64) -> Point3<f64> { Point3::new(x, y, 0.5) }

    #[test]
    fn nearby_source_returned() {
        let positions = vec![pt(10.0, 0.0), pt(500.0, 0.0)];
        let culler = SourceCuller::new(&positions, 200.0);
        let result = culler.query(&pt(0.0, 0.0));
        assert!(result.contains(&0), "near source should be returned");
        assert!(!result.contains(&1), "far source should not be returned");
    }

    #[test]
    fn all_sources_within_range() {
        let positions: Vec<Point3<f64>> = (0..10).map(|i| pt(i as f64, 0.0)).collect();
        let culler = SourceCuller::new(&positions, 100.0);
        let result = culler.query(&pt(5.0, 0.0));
        assert_eq!(result.len(), 10);
    }

    #[test]
    fn no_sources_in_range() {
        let positions = vec![pt(1000.0, 1000.0)];
        let culler = SourceCuller::new(&positions, 50.0);
        let result = culler.query(&pt(0.0, 0.0));
        assert!(result.is_empty());
    }

    #[test]
    fn source_exactly_at_boundary_included() {
        let positions = vec![pt(200.0, 0.0)];
        let culler = SourceCuller::new(&positions, 200.0);
        let result = culler.query(&pt(0.0, 0.0));
        assert!(result.contains(&0), "source on boundary should be included");
    }

    #[test]
    fn source_just_beyond_boundary_excluded() {
        let positions = vec![pt(200.001, 0.0)];
        let culler = SourceCuller::new(&positions, 200.0);
        let result = culler.query(&pt(0.0, 0.0));
        assert!(result.is_empty(), "source just outside range should be excluded");
    }

    #[test]
    fn empty_index_returns_empty() {
        let culler = SourceCuller::new(&[], 100.0);
        assert!(culler.is_empty());
        let result = culler.query(&pt(0.0, 0.0));
        assert!(result.is_empty());
    }

    #[test]
    fn len_matches_source_count() {
        let positions: Vec<Point3<f64>> = (0..5).map(|i| pt(i as f64 * 10.0, 0.0)).collect();
        let culler = SourceCuller::new(&positions, 500.0);
        assert_eq!(culler.len(), 5);
    }

    #[test]
    fn query_with_z_ignores_vertical_distance() {
        // The culler only uses horizontal (x, y) distance.
        let positions = vec![pt(0.0, 0.0)];
        let culler = SourceCuller::new(&positions, 50.0);
        let high_receiver = Point3::new(0.0, 0.0, 100.0); // z = 100 m but x,y = 0
        let result = culler.query(&high_receiver);
        assert!(result.contains(&0), "z difference should not affect culling");
    }

    #[test]
    fn multiple_cells_covered() {
        // Sources spread across different cells.
        let positions: Vec<Point3<f64>> = vec![
            pt(-150.0, 0.0),
            pt(0.0,    0.0),
            pt(150.0,  0.0),
            pt(600.0,  0.0),
        ];
        let culler = SourceCuller::new(&positions, 200.0);
        let result = culler.query(&pt(0.0, 0.0));
        // All within 200 m except the last one (600 m away).
        assert!(result.contains(&0));
        assert!(result.contains(&1));
        assert!(result.contains(&2));
        assert!(!result.contains(&3));
    }
}
