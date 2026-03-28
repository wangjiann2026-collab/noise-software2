//! Import parsers — full implementations in Phase 6.

pub mod ascii;
pub mod dxf;
pub mod geojson;
pub mod shapefile;
pub mod types;
pub mod xml;

pub use types::{ImportedGeometry, ImportedObject, ImportedScene, ObjectKind};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("File not found: {0}")]
    FileNotFound(String),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Detect file format from extension.
pub fn detect_format(path: &str) -> Option<&'static str> {
    let ext = path.rsplit('.').next()?.to_lowercase();
    match ext.as_str() {
        "dxf" => Some("dxf"),
        "shp" => Some("shapefile"),
        "geojson" | "json" => Some("geojson"),
        "asc" | "txt" => Some("ascii"),
        "xml" => Some("xml"),
        _ => None,
    }
}
