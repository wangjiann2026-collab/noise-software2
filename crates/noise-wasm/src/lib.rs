//! # noise-wasm
//!
//! WebAssembly bindings for the 3D Environmental Noise Mapping platform.
//!
//! Exposes key computation functions to JavaScript/TypeScript via
//! `wasm-bindgen` (when compiled with `--features wasm`), and provides
//! pure-Rust structs that are independently unit-testable without a browser.
//!
//! ## Exported functions (wasm feature)
//! | JS name                  | Description                              |
//! |--------------------------|------------------------------------------|
//! | `calculate_spl`          | SPL at a single receiver from one source |
//! | `combine_levels`         | Incoherent energy sum of dBA values      |
//! | `lden_from_ld_le_ln`     | EU Lden from day/evening/night levels    |
//! | `ldn_from_ld_ln`         | US Ldn from day/night levels             |
//! | `grid_stats`             | Min/max/mean of a flat f32 grid array    |
//! | `iso9613_atmospheric`    | ISO 9613-1 atmospheric absorption (dB/m) |

pub mod calc;
pub mod metrics;
pub mod stats;

pub use calc::{calculate_spl, SplInput};
pub use metrics::{combine_levels, lden_from_ld_le_ln, ldn_from_ld_ln};
pub use stats::{grid_stats, GridStats};
