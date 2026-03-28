//! ESRI ASCII Grid (.asc) export.
//!
//! The ESRI ASCII raster format is widely supported by GIS software (QGIS,
//! ArcGIS, Surfer, NoiseMap, etc.) and is the de-facto interchange format
//! for noise mapping results.
//!
//! ## File structure
//! ```text
//! ncols        100
//! nrows        100
//! xllcorner    300000.0
//! yllcorner    5700000.0
//! cellsize     5.0
//! NODATA_value -9999
//! <row data, north-to-south, space-separated floats>
//! ```

use crate::GridView;

/// Sentinel value written for cells with no valid noise level.
pub const NODATA: f32 = -9999.0;

/// Serialise a noise grid to ESRI ASCII Grid format.
///
/// Rows are written **north-to-south** (row `ny-1` first) to comply with the
/// ASC convention where the first data row corresponds to the northernmost
/// row.  Values are rounded to 2 decimal places.
pub fn export_asc(view: &GridView) -> String {
    let mut out = format!(
        "ncols        {ncols}\n\
         nrows        {nrows}\n\
         xllcorner    {xll}\n\
         yllcorner    {yll}\n\
         cellsize     {cs}\n\
         NODATA_value {nodata}\n",
        ncols  = view.nx,
        nrows  = view.ny,
        xll    = view.xllcorner,
        yll    = view.yllcorner,
        cs     = view.cellsize,
        nodata = NODATA,
    );

    // ASC: north → south means row index ny-1 first.
    for row in (0..view.ny).rev() {
        let mut row_parts = Vec::with_capacity(view.nx);
        for col in 0..view.nx {
            let idx = row * view.nx + col;
            let v = view.levels.get(idx).copied().unwrap_or(NODATA);
            if v.is_finite() && v > 0.0 {
                row_parts.push(format!("{v:.2}"));
            } else {
                row_parts.push(format!("{NODATA}"));
            }
        }
        out.push_str(&row_parts.join(" "));
        out.push('\n');
    }

    out
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn two_by_two() -> GridView {
        GridView {
            levels: vec![60.0, 65.0, 55.0, 70.0],
            nx: 2, ny: 2,
            xllcorner: 0.0, yllcorner: 0.0,
            cellsize: 5.0,
        }
    }

    #[test]
    fn header_present() {
        let asc = export_asc(&two_by_two());
        assert!(asc.contains("ncols        2"));
        assert!(asc.contains("nrows        2"));
        assert!(asc.contains("cellsize     5"));
        assert!(asc.contains("NODATA_value"));
    }

    #[test]
    fn row_count_correct() {
        let asc = export_asc(&two_by_two());
        // 6 header lines + 2 data rows = 8 lines total (last may be empty)
        let data_lines: Vec<&str> = asc.lines().skip(6).collect();
        assert_eq!(data_lines.len(), 2, "expected 2 data rows");
    }

    #[test]
    fn north_south_order() {
        // Row 0 (south) = [60.0, 65.0], Row 1 (north) = [55.0, 70.0]
        // First data row should be row 1 (north).
        let asc = export_asc(&two_by_two());
        let data_lines: Vec<&str> = asc.lines().skip(6).collect();
        assert!(data_lines[0].contains("55.00"), "first data line should be northern row");
        assert!(data_lines[1].contains("60.00"), "second data line should be southern row");
    }

    #[test]
    fn nodata_for_non_finite() {
        let view = GridView {
            levels: vec![f32::NEG_INFINITY, 60.0],
            nx: 2, ny: 1,
            xllcorner: 0.0, yllcorner: 0.0,
            cellsize: 1.0,
        };
        let asc = export_asc(&view);
        let data_line = asc.lines().nth(6).unwrap();
        assert!(data_line.contains("-9999"), "non-finite should become NODATA");
    }

    #[test]
    fn three_by_three_grid() {
        let view = GridView {
            levels: (0..9).map(|i| 50.0 + i as f32 * 2.5).collect(),
            nx: 3, ny: 3,
            xllcorner: 1000.0, yllcorner: 2000.0,
            cellsize: 10.0,
        };
        let asc = export_asc(&view);
        assert!(asc.contains("ncols        3"));
        assert!(asc.contains("xllcorner    1000"));
        assert!(asc.contains("yllcorner    2000"));
        let data_rows: Vec<&str> = asc.lines().skip(6).collect();
        assert_eq!(data_rows.len(), 3);
    }
}
