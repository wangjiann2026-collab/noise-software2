//! # noise-render
//!
//! 2D/3D rendering engine built on wgpu.
//!
//! ## Backend Selection
//! | Platform | wgpu Backend |
//! |----------|-------------|
//! | Windows  | Vulkan → DX12 (fallback) |
//! | Linux    | Vulkan |
//! | macOS    | Metal |
//! | Web      | WebGPU |
//!
//! Shaders are written in WGSL and compiled by wgpu for each backend.

pub mod map2d;
pub mod scene3d;
pub mod shaders;

pub mod prelude {
    pub use crate::map2d::Map2DRenderer;
    pub use crate::scene3d::Scene3DRenderer;
}
