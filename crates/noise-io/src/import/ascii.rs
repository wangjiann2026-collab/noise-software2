//! ESRI ASCII Grid import.
//!
//! Parses the standard ESRI ASCII raster format:
//! ```text
//! ncols        10
//! nrows        8
//! xllcorner    500000.0
//! yllcorner    100000.0
//! cellsize     5.0
//! NODATA_value -9999
//! <row 0 (top/north)> ...
//! ```
//!
//! Data rows run north-to-south (first row = northernmost).
//! Output is stored south-to-north (row 0 = southernmost) to match
//! `HorizontalGrid` convention.

use std::path::Path;
use super::ImportError;

/// Parsed ESRI ASCII grid.
#[derive(Debug, Clone)]
pub struct AsciiGrid {
    /// Number of columns.
    pub ncols: usize,
    /// Number of rows.
    pub nrows: usize,
    /// X coordinate of lower-left corner.
    pub xllcorner: f64,
    /// Y coordinate of lower-left corner.
    pub yllcorner: f64,
    /// Cell size (m).
    pub cellsize: f64,
    /// NODATA sentinel value.
    pub nodata_value: f64,
    /// Grid values in row-major order (row 0 = south/bottom).
    /// NODATA replaced with `f32::NEG_INFINITY`.
    pub data: Vec<f32>,
}

impl AsciiGrid {
    /// Value at column `col`, row `row` (row 0 = south).
    pub fn get(&self, col: usize, row: usize) -> f32 {
        self.data.get(row * self.ncols + col).copied().unwrap_or(f32::NEG_INFINITY)
    }

    /// World X coordinate of cell centre at column `col`.
    pub fn x_at(&self, col: usize) -> f64 {
        self.xllcorner + (col as f64 + 0.5) * self.cellsize
    }

    /// World Y coordinate of cell centre at row `row` (row 0 = south).
    pub fn y_at(&self, row: usize) -> f64 {
        self.yllcorner + (row as f64 + 0.5) * self.cellsize
    }

    /// Total number of cells.
    pub fn cell_count(&self) -> usize { self.ncols * self.nrows }
}

/// Import an ESRI ASCII grid from a file.
pub fn import_ascii(path: impl AsRef<Path>) -> Result<AsciiGrid, ImportError> {
    let content = std::fs::read_to_string(path.as_ref()).map_err(ImportError::Io)?;
    import_ascii_str(&content)
}

/// Import from a string (useful for testing).
pub fn import_ascii_str(content: &str) -> Result<AsciiGrid, ImportError> {
    let mut lines = content.lines().peekable();

    // Parse header key-value pairs.
    let mut ncols: Option<usize> = None;
    let mut nrows: Option<usize> = None;
    let mut xllcorner = 0.0f64;
    let mut yllcorner = 0.0f64;
    let mut cellsize  = 1.0f64;
    let mut nodata    = -9999.0f64;

    // Read up to 10 header lines.
    let mut header_count = 0;
    while header_count < 10 {
        let line = match lines.peek() {
            Some(l) => l.trim(),
            None => break,
        };
        // If the line starts with a digit or sign, we've reached data.
        if line.starts_with(|c: char| c.is_ascii_digit() || c == '-' || c == '+') {
            break;
        }
        let line = lines.next().unwrap().trim().to_ascii_lowercase();
        let mut parts = line.split_whitespace();
        let key = parts.next().unwrap_or("");
        let val = parts.next().unwrap_or("");
        match key {
            "ncols"        => ncols    = val.parse().ok(),
            "nrows"        => nrows    = val.parse().ok(),
            "xllcorner" | "xllcenter" => xllcorner = val.parse().unwrap_or(0.0),
            "yllcorner" | "yllcenter" => yllcorner = val.parse().unwrap_or(0.0),
            "cellsize"     => cellsize  = val.parse().unwrap_or(1.0),
            "nodata_value" => nodata    = val.parse().unwrap_or(-9999.0),
            _ => {}
        }
        header_count += 1;
    }

    let ncols = ncols.ok_or_else(|| ImportError::ParseError("missing ncols".into()))?;
    let nrows = nrows.ok_or_else(|| ImportError::ParseError("missing nrows".into()))?;

    // Parse data rows (north-to-south in file, flip to south-to-north).
    let mut north_to_south: Vec<Vec<f32>> = Vec::with_capacity(nrows);
    for line in lines {
        let line = line.trim();
        if line.is_empty() { continue; }
        let row: Vec<f32> = line.split_whitespace()
            .filter_map(|tok| tok.parse::<f64>().ok())
            .map(|v| if (v - nodata).abs() < 1e-6 { f32::NEG_INFINITY } else { v as f32 })
            .collect();
        if !row.is_empty() { north_to_south.push(row); }
    }

    // Flip: row 0 in output = south = last row in file.
    let data: Vec<f32> = north_to_south.into_iter().rev()
        .flat_map(|row| {
            let mut r = row;
            r.resize(ncols, f32::NEG_INFINITY);
            r
        })
        .collect();

    Ok(AsciiGrid { ncols, nrows, xllcorner, yllcorner, cellsize, nodata_value: nodata, data })
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
ncols        4
nrows        3
xllcorner    0.0
yllcorner    0.0
cellsize     5.0
NODATA_value -9999
70.0 68.0 65.0 62.0
65.0 63.0 60.0 57.0
60.0 58.0 55.0 52.0
";

    #[test]
    fn header_parsed_correctly() {
        let g = import_ascii_str(SAMPLE).unwrap();
        assert_eq!(g.ncols, 4);
        assert_eq!(g.nrows, 3);
        assert!((g.cellsize - 5.0).abs() < 1e-9);
        assert!((g.xllcorner).abs() < 1e-9);
    }

    #[test]
    fn data_flipped_south_at_row0() {
        let g = import_ascii_str(SAMPLE).unwrap();
        // File row 2 (last/south) → output row 0.
        // File: "60.0 58.0 55.0 52.0" is southernmost.
        assert!((g.get(0, 0) - 60.0).abs() < 0.01, "row0 col0 = {}", g.get(0,0));
        // File row 0 (first/north) → output row 2.
        assert!((g.get(0, 2) - 70.0).abs() < 0.01, "row2 col0 = {}", g.get(0,2));
    }

    #[test]
    fn cell_count_correct() {
        let g = import_ascii_str(SAMPLE).unwrap();
        assert_eq!(g.cell_count(), 12);
    }

    #[test]
    fn nodata_becomes_neg_infinity() {
        let content = "\
ncols 2\nnrows 2\nxllcorner 0\nyllcorner 0\ncellsize 1\nNODATA_value -9999\n\
-9999 60.0\n55.0 -9999\n";
        let g = import_ascii_str(content).unwrap();
        assert!(g.data.iter().any(|v| v.is_infinite()));
        assert!(g.data.iter().any(|v| v.is_finite()));
    }

    #[test]
    fn x_y_at_correct() {
        let g = import_ascii_str(SAMPLE).unwrap();
        // Cell (col=0, row=0): xll + 0.5*cell = 2.5, yll + 0.5*cell = 2.5
        assert!((g.x_at(0) - 2.5).abs() < 1e-9);
        assert!((g.y_at(0) - 2.5).abs() < 1e-9);
    }

    #[test]
    fn missing_ncols_returns_error() {
        let bad = "nrows 2\ncellsize 1.0\n1.0 2.0\n";
        assert!(import_ascii_str(bad).is_err());
    }

    #[test]
    fn nonexistent_file_returns_error() {
        assert!(import_ascii("/tmp/no_such_file_noise.asc").is_err());
    }
}
