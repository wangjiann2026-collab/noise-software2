//! # noise-render
//!
//! 2D/3D rendering engine built on wgpu.
//!
//! ## Modules
//! | Module | Purpose |
//! |--------|---------|
//! | `color` | dBA→RGBA colour map (WHO standard scale) |
//! | `contour` | Iso-contour extraction (marching squares) |
//! | `camera` | 2D orthographic + 3D perspective cameras |
//! | `mesh` | CPU-side vertex/index mesh generation |
//! | `export` | PNG and SVG export (no GPU required) |
//! | `gpu` | wgpu context, pipelines, uniform buffers |
//! | `map2d` | 2D heatmap renderer |
//! | `scene3d` | 3D terrain renderer |
//! | `shaders` | WGSL shader source constants |
//!
//! ## Backend selection
//! | Platform | wgpu Backend |
//! |----------|-------------|
//! | Windows  | Vulkan → DX12 (fallback) |
//! | Linux    | Vulkan |
//! | macOS    | Metal |
//! | Web/WASM | WebGPU |
//!
//! ## CPU-only usage (no GPU required)
//! ```no_run
//! use noise_render::map2d::Map2DRenderer;
//! use noise_render::export::SvgStyle;
//! use std::path::Path;
//!
//! let renderer = Map2DRenderer::default();
//! // renderer.export_png(&grid, Path::new("output.png")).unwrap();
//! ```

pub mod camera;
pub mod color;
pub mod contour;
pub mod export;
pub mod gpu;
pub mod map2d;
pub mod mesh;
pub mod scene3d;
pub mod shaders;

pub mod prelude {
    pub use crate::camera::{Camera2D, Camera3D};
    pub use crate::color::{ColorMap, NoiseColor};
    pub use crate::contour::{extract_isolines, IsoContourLine};
    pub use crate::export::{export_grid_png, render_to_buffer, svg_to_string, ExportError, SvgStyle};
    pub use crate::map2d::Map2DRenderer;
    pub use crate::mesh::{HeatmapMesh, HeatmapVertex, TerrainMesh, TerrainVertex};
    pub use crate::scene3d::Scene3DRenderer;
}
