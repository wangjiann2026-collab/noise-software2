//! Noise map export — ESRI ASCII Grid, GeoJSON, and CSV formats.
//!
//! # Formats
//! - **ESRI ASCII Grid** (`.asc`) — standard raster format with header + data rows (north-to-south)
//! - **GeoJSON** (`.geojson`) — FeatureCollection of Point features with `noise_db` property
//! - **CSV** (`.csv`) — comma-separated: `x,y,noise_db`

use std::path::Path;
use thiserror::Error;

/// Errors that can occur during export.
#[derive(Debug, Error)]
pub enum ExportError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid grid: {0}")]
    InvalidGrid(String),
}

/// Write a noise level grid to ESRI ASCII format.
///
/// `levels` is in row-major order, row 0 = south (bottom).
/// The file is written north-to-south (first row = northernmost).
///
/// # Arguments
/// * `levels`    — grid values in dBA (row 0 = south).  `f32::NEG_INFINITY` → NODATA.
/// * `nx`, `ny`  — grid dimensions (columns × rows).
/// * `xllcorner`, `yllcorner` — world coordinates of lower-left corner.
/// * `cellsize`  — cell size in metres.
/// * `path`      — output file path.
pub fn export_ascii(
    levels: &[f32],
    nx: usize,
    ny: usize,
    xllcorner: f64,
    yllcorner: f64,
    cellsize: f64,
    path: impl AsRef<Path>,
) -> Result<(), ExportError> {
    let s = ascii_to_string(levels, nx, ny, xllcorner, yllcorner, cellsize)?;
    std::fs::write(path, s).map_err(ExportError::Io)
}

/// Generate ESRI ASCII grid string (useful for testing without file I/O).
pub fn ascii_to_string(
    levels: &[f32],
    nx: usize,
    ny: usize,
    xllcorner: f64,
    yllcorner: f64,
    cellsize: f64,
) -> Result<String, ExportError> {
    if nx == 0 || ny == 0 {
        return Err(ExportError::InvalidGrid("nx and ny must be > 0".into()));
    }
    if levels.len() != nx * ny {
        return Err(ExportError::InvalidGrid(
            format!("levels length {} != nx*ny {}", levels.len(), nx * ny),
        ));
    }

    let nodata = -9999.0f64;
    let mut out = String::with_capacity(256 + levels.len() * 10);
    out.push_str(&format!("ncols        {nx}\n"));
    out.push_str(&format!("nrows        {ny}\n"));
    out.push_str(&format!("xllcorner    {xllcorner}\n"));
    out.push_str(&format!("yllcorner    {yllcorner}\n"));
    out.push_str(&format!("cellsize     {cellsize}\n"));
    out.push_str(&format!("NODATA_value {nodata}\n"));

    // Write rows north-to-south (row ny-1 first).
    for row in (0..ny).rev() {
        let mut line = String::new();
        for col in 0..nx {
            let v = levels[row * nx + col];
            if col > 0 { line.push(' '); }
            if v.is_finite() {
                line.push_str(&format!("{:.2}", v));
            } else {
                line.push_str("-9999");
            }
        }
        out.push_str(&line);
        out.push('\n');
    }
    Ok(out)
}

/// Write a noise level grid to GeoJSON FeatureCollection of Point features.
///
/// Each cell centre becomes a Point feature with property `"noise_db"`.
/// NODATA cells (`f32::NEG_INFINITY`) are omitted.
pub fn export_geojson(
    levels: &[f32],
    nx: usize,
    ny: usize,
    xllcorner: f64,
    yllcorner: f64,
    cellsize: f64,
    path: impl AsRef<Path>,
) -> Result<(), ExportError> {
    let s = geojson_to_string(levels, nx, ny, xllcorner, yllcorner, cellsize)?;
    std::fs::write(path, s).map_err(ExportError::Io)
}

/// Generate GeoJSON string (useful for testing).
pub fn geojson_to_string(
    levels: &[f32],
    nx: usize,
    ny: usize,
    xllcorner: f64,
    yllcorner: f64,
    cellsize: f64,
) -> Result<String, ExportError> {
    if levels.len() != nx * ny {
        return Err(ExportError::InvalidGrid(
            format!("levels length {} != nx*ny {}", levels.len(), nx * ny),
        ));
    }

    let half = cellsize * 0.5;
    let mut features: Vec<serde_json::Value> = Vec::new();

    for row in 0..ny {
        let y = yllcorner + (row as f64 + 0.5) * cellsize;
        for col in 0..nx {
            let v = levels[row * nx + col];
            if !v.is_finite() { continue; }
            let x = xllcorner + (col as f64 + 0.5) * cellsize;
            features.push(serde_json::json!({
                "type": "Feature",
                "geometry": {
                    "type": "Point",
                    "coordinates": [x + half - half, y + half - half]  // centre
                },
                "properties": {
                    "noise_db": (v as f64 * 100.0).round() / 100.0
                }
            }));
        }
    }

    let fc = serde_json::json!({
        "type": "FeatureCollection",
        "features": features
    });
    serde_json::to_string_pretty(&fc).map_err(|e| ExportError::InvalidGrid(e.to_string()))
}

/// Write a noise level grid to CSV.
///
/// Columns: `x,y,noise_db` (cell centres, world CRS).
/// NODATA cells are omitted.
pub fn export_csv(
    levels: &[f32],
    nx: usize,
    ny: usize,
    xllcorner: f64,
    yllcorner: f64,
    cellsize: f64,
    path: impl AsRef<Path>,
) -> Result<(), ExportError> {
    let s = csv_to_string(levels, nx, ny, xllcorner, yllcorner, cellsize)?;
    std::fs::write(path, s).map_err(ExportError::Io)
}

/// Generate CSV string (useful for testing).
pub fn csv_to_string(
    levels: &[f32],
    nx: usize,
    ny: usize,
    xllcorner: f64,
    yllcorner: f64,
    cellsize: f64,
) -> Result<String, ExportError> {
    if levels.len() != nx * ny {
        return Err(ExportError::InvalidGrid(
            format!("levels length {} != nx*ny {}", levels.len(), nx * ny),
        ));
    }

    let mut out = String::from("x,y,noise_db\n");
    for row in 0..ny {
        let y = yllcorner + (row as f64 + 0.5) * cellsize;
        for col in 0..nx {
            let v = levels[row * nx + col];
            if !v.is_finite() { continue; }
            let x = xllcorner + (col as f64 + 0.5) * cellsize;
            out.push_str(&format!("{:.3},{:.3},{:.2}\n", x, y, v));
        }
    }
    Ok(out)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn small_grid() -> (Vec<f32>, usize, usize) {
        // 2×2 grid, row 0 = south
        let data = vec![55.0f32, 57.0, 60.0, 62.0];
        (data, 2, 2)
    }

    #[test]
    fn ascii_header_correct() {
        let (data, nx, ny) = small_grid();
        let s = ascii_to_string(&data, nx, ny, 0.0, 0.0, 5.0).unwrap();
        assert!(s.starts_with("ncols        2\n"));
        assert!(s.contains("nrows        2\n"));
        assert!(s.contains("cellsize     5\n"));
        assert!(s.contains("NODATA_value -9999\n"));
    }

    #[test]
    fn ascii_north_to_south_order() {
        let (data, nx, ny) = small_grid();
        let s = ascii_to_string(&data, nx, ny, 0.0, 0.0, 5.0).unwrap();
        let lines: Vec<&str> = s.lines().collect();
        // Header = 6 lines; first data row = north (row 1 = index 1 in data)
        assert!(lines[6].contains("60.00") || lines[6].contains("62.00"),
                "north row should be row 1: {}", lines[6]);
        assert!(lines[7].contains("55.00") || lines[7].contains("57.00"),
                "south row should be row 0: {}", lines[7]);
    }

    #[test]
    fn ascii_nodata_rendered() {
        let data = vec![f32::NEG_INFINITY, 55.0f32, 60.0, f32::NEG_INFINITY];
        let s = ascii_to_string(&data, 2, 2, 0.0, 0.0, 1.0).unwrap();
        assert!(s.contains("-9999"));
    }

    #[test]
    fn ascii_invalid_length_returns_error() {
        let result = ascii_to_string(&[55.0f32], 2, 2, 0.0, 0.0, 1.0);
        assert!(result.is_err());
    }

    #[test]
    fn geojson_feature_count() {
        let (data, nx, ny) = small_grid();
        let s = geojson_to_string(&data, nx, ny, 0.0, 0.0, 5.0).unwrap();
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["features"].as_array().unwrap().len(), 4);
    }

    #[test]
    fn geojson_nodata_omitted() {
        let data = vec![f32::NEG_INFINITY, 55.0f32, 60.0, f32::NEG_INFINITY];
        let s = geojson_to_string(&data, 2, 2, 0.0, 0.0, 1.0).unwrap();
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["features"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn csv_header_and_rows() {
        let (data, nx, ny) = small_grid();
        let s = csv_to_string(&data, nx, ny, 0.0, 0.0, 5.0).unwrap();
        assert!(s.starts_with("x,y,noise_db\n"));
        let rows: Vec<&str> = s.lines().collect();
        assert_eq!(rows.len(), 5); // header + 4 cells
    }

    #[test]
    fn csv_nodata_omitted() {
        let data = vec![f32::NEG_INFINITY, 55.0f32, 60.0, f32::NEG_INFINITY];
        let s = csv_to_string(&data, 2, 2, 0.0, 0.0, 1.0).unwrap();
        let rows: Vec<&str> = s.lines().collect();
        assert_eq!(rows.len(), 3); // header + 2 finite cells
    }

    #[test]
    fn ascii_roundtrip_via_import() {
        use crate::import::ascii::import_ascii_str;
        let data: Vec<f32> = (0..9).map(|i| 50.0 + i as f32).collect();
        let s = ascii_to_string(&data, 3, 3, 100.0, 200.0, 10.0).unwrap();
        let grid = import_ascii_str(&s).unwrap();
        assert_eq!(grid.ncols, 3);
        assert_eq!(grid.nrows, 3);
        assert!((grid.xllcorner - 100.0).abs() < 1e-6);
        assert!((grid.yllcorner - 200.0).abs() < 1e-6);
        // Row 0 in data = south = last row in ASCII file = first row after import flip
        assert!((grid.get(0, 0) - 50.0).abs() < 0.01, "got {}", grid.get(0, 0));
    }
}
