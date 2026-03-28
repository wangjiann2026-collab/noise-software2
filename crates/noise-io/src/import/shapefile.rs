//! Shapefile import — stub for Phase 6.
use super::ImportError;

pub fn import_shapefile(_path: &str) -> Result<(), ImportError> {
    Err(ImportError::UnsupportedFormat("Shapefile import not yet implemented (Phase 6)".into()))
}
