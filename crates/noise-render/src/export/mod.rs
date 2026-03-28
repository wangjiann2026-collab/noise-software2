pub mod png;
pub mod svg;

pub use png::{export_grid_png, render_to_buffer};
pub use svg::{export_svg, render_to_string as svg_to_string, SvgStyle};

use thiserror::Error;

/// Export-layer error type.
#[derive(Debug, Error)]
pub enum ExportError {
    #[error("empty grid (nx or ny is 0)")]
    EmptyGrid,
    #[error("I/O error: {0}")]
    Io(String),
    #[error("unsupported format: {0}")]
    UnsupportedFormat(String),
}
