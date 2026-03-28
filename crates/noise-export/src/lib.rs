//! # noise-export
//!
//! Export noise-calculation grids to standard GIS and data-exchange formats.
//!
//! ## Supported formats
//!
//! | Format   | Function          | MIME type                      |
//! |----------|-------------------|-------------------------------|
//! | GeoJSON  | [`export_geojson`] | `application/geo+json`        |
//! | ESRI ASC | [`export_asc`]     | `text/plain`                  |
//! | CSV      | [`export_csv`]     | `text/csv`                    |
//!
//! ## Usage
//! ```rust
//! use noise_export::{GridView, export_geojson, export_asc, export_csv};
//!
//! let view = GridView {
//!     levels: vec![65.0, 60.0, 55.0, 50.0],
//!     nx: 2, ny: 2,
//!     xllcorner: 0.0, yllcorner: 0.0,
//!     cellsize: 10.0,
//! };
//!
//! let geojson = export_geojson(&view, &[55.0, 60.0]);
//! let asc     = export_asc(&view);
//! let csv     = export_csv(&view);
//! ```

pub mod asc;
pub mod csv_fmt;
pub mod geojson;

pub use asc::export_asc;
pub use csv_fmt::export_csv;
pub use geojson::export_geojson;

/// A flat, row-major noise-level grid together with its geospatial metadata.
#[derive(Debug, Clone)]
pub struct GridView {
    /// Noise levels (dBA) in row-major order, row 0 = south.
    pub levels: Vec<f32>,
    /// Number of columns (X direction).
    pub nx: usize,
    /// Number of rows (Y direction).
    pub ny: usize,
    /// X coordinate of the south-west corner (metres or map units).
    pub xllcorner: f64,
    /// Y coordinate of the south-west corner.
    pub yllcorner: f64,
    /// Cell size (m).  Assumed equal in X and Y.
    pub cellsize: f64,
}
