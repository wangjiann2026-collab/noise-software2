//! GeoJSON import — stub for Phase 6.
use super::ImportError;

pub fn import_geojson(_path: &str) -> Result<(), ImportError> {
    Err(ImportError::UnsupportedFormat("GeoJSON import not yet implemented (Phase 6)".into()))
}
