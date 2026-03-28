//! ASCII grid import — stub for Phase 6.
use super::ImportError;

pub fn import_ascii(_path: &str) -> Result<(), ImportError> {
    Err(ImportError::UnsupportedFormat("ASCII import not yet implemented (Phase 6)".into()))
}
