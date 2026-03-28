//! CSV export for noise-calculation grids.
//!
//! Produces a UTF-8 CSV with columns: `x`, `y`, `level_dba`.
//! Each row corresponds to one grid receiver point.  Points with no valid
//! level (non-finite) are omitted.

use crate::GridView;

/// Serialise a noise grid to CSV format.
///
/// Columns: `x,y,level_dba`
///
/// Rows are ordered south-west to north-east (col-major within row, rows
/// south to north).  Only finite, positive level values are included.
pub fn export_csv(view: &GridView) -> String {
    let capacity = view.nx * view.ny * 28; // rough estimate per row
    let mut out = String::with_capacity(capacity);
    out.push_str("x,y,level_dba\n");

    for row in 0..view.ny {
        let y = view.yllcorner + row as f64 * view.cellsize;
        for col in 0..view.nx {
            let x = view.xllcorner + col as f64 * view.cellsize;
            let idx = row * view.nx + col;
            let v = view.levels.get(idx).copied().unwrap_or(f32::NEG_INFINITY);
            if v.is_finite() && v > 0.0 {
                out.push_str(&format!("{x:.2},{y:.2},{v:.2}\n"));
            }
        }
    }

    out
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_row_present() {
        let view = GridView {
            levels: vec![60.0],
            nx: 1, ny: 1,
            xllcorner: 0.0, yllcorner: 0.0,
            cellsize: 5.0,
        };
        let csv = export_csv(&view);
        assert!(csv.starts_with("x,y,level_dba\n"));
    }

    #[test]
    fn single_point() {
        let view = GridView {
            levels: vec![65.3],
            nx: 1, ny: 1,
            xllcorner: 100.0, yllcorner: 200.0,
            cellsize: 5.0,
        };
        let csv = export_csv(&view);
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 2, "header + 1 data row");
        assert_eq!(lines[1], "100.00,200.00,65.30");
    }

    #[test]
    fn non_finite_omitted() {
        let view = GridView {
            levels: vec![f32::NEG_INFINITY, 60.0, 0.0, -5.0],
            nx: 2, ny: 2,
            xllcorner: 0.0, yllcorner: 0.0,
            cellsize: 10.0,
        };
        let csv = export_csv(&view);
        let data_lines: Vec<&str> = csv.lines().skip(1).collect();
        // Only 60.0 is valid (finite + positive)
        assert_eq!(data_lines.len(), 1);
        assert!(data_lines[0].contains("60.00"));
    }

    #[test]
    fn coordinates_are_correct() {
        // 2×2 grid, cellsize=5, origin=(10,20)
        // row0: (10,20)=55.0, (15,20)=60.0
        // row1: (10,25)=65.0, (15,25)=70.0
        let view = GridView {
            levels: vec![55.0, 60.0, 65.0, 70.0],
            nx: 2, ny: 2,
            xllcorner: 10.0, yllcorner: 20.0,
            cellsize: 5.0,
        };
        let csv = export_csv(&view);
        let rows: Vec<&str> = csv.lines().skip(1).collect();
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0], "10.00,20.00,55.00");
        assert_eq!(rows[1], "15.00,20.00,60.00");
        assert_eq!(rows[2], "10.00,25.00,65.00");
        assert_eq!(rows[3], "15.00,25.00,70.00");
    }

    #[test]
    fn empty_grid_returns_header_only() {
        let view = GridView {
            levels: vec![],
            nx: 0, ny: 0,
            xllcorner: 0.0, yllcorner: 0.0,
            cellsize: 1.0,
        };
        let csv = export_csv(&view);
        assert_eq!(csv, "x,y,level_dba\n");
    }
}
