//! DXF import — stub for Phase 6.
use super::ImportError;

pub fn import_dxf(_path: &str) -> Result<(), ImportError> {
    Err(ImportError::UnsupportedFormat("DXF import not yet implemented (Phase 6)".into()))
}
