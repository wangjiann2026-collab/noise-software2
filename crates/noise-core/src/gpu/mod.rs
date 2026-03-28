//! GPU-accelerated grid calculation (feature = `"gpu"`).
//!
//! Uses `wgpu` to dispatch a WGSL compute shader that evaluates geometric
//! spreading + atmospheric absorption for every receiver point in parallel on
//! the GPU.  Ground effect and barrier diffraction are omitted in this fast
//! path; for full ISO 9613-2 accuracy use the CPU [`GridCalculator`].
//!
//! # Feature gate
//! This module is only compiled when the `gpu` Cargo feature is enabled:
//! ```text
//! cargo build -p noise-core --features gpu
//! ```
//!
//! # Intended use
//! Large grids (> 100 k receivers) where a rapid preview is acceptable before
//! a full CPU run.  The GPU path is approximately 10-50× faster than the
//! single-threaded CPU path for pure geometric spreading.
//!
//! [`GridCalculator`]: crate::grid::GridCalculator

pub mod compute;
pub use compute::GpuGridCalculator;
