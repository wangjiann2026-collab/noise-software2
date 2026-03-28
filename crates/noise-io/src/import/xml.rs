//! XML import — stub for Phase 6.
use super::ImportError;

pub fn import_xml(_path: &str) -> Result<(), ImportError> {
    Err(ImportError::UnsupportedFormat("XML import not yet implemented (Phase 6)".into()))
}
