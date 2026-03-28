//! Export a noise grid as a PNG image.
//!
//! Each grid cell maps to one pixel coloured by the `ColorMap`.
//! No GPU is required — rendering is entirely CPU-side.

use std::path::Path;

use image::{ImageBuffer, Rgba};

use crate::color::ColorMap;
use super::ExportError;

/// Render a flat row-major noise grid to a PNG file.
///
/// # Parameters
/// - `grid`      — dBA levels (row 0 = south/bottom of image).
/// - `nx`, `ny`  — grid dimensions (columns, rows).
/// - `color_map` — colour scale to apply.
/// - `path`      — output file path (`.png`).
pub fn export_grid_png(
    grid: &[f32],
    nx: usize,
    ny: usize,
    color_map: &ColorMap,
    path: &Path,
) -> Result<(), ExportError> {
    if nx == 0 || ny == 0 {
        return Err(ExportError::EmptyGrid);
    }

    // Build image: flip vertically so row 0 (south) appears at the bottom.
    let mut img: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::new(nx as u32, ny as u32);

    for row in 0..ny {
        for col in 0..nx {
            let idx = row * nx + col;
            let level = grid.get(idx).copied().unwrap_or(f32::NEG_INFINITY);
            let c = color_map.sample(level);
            // Flip row: image row 0 = top = north.
            let img_row = (ny - 1 - row) as u32;
            img.put_pixel(col as u32, img_row, Rgba([c.r, c.g, c.b, c.a]));
        }
    }

    img.save(path).map_err(|e| ExportError::Io(e.to_string()))?;
    Ok(())
}

/// Render a grid to an in-memory RGBA byte buffer (no file I/O).
///
/// Returns `(width, height, pixels)` where pixels are RGBA u8, row-major,
/// top-row = north.
pub fn render_to_buffer(
    grid: &[f32],
    nx: usize,
    ny: usize,
    color_map: &ColorMap,
) -> Result<(u32, u32, Vec<u8>), ExportError> {
    if nx == 0 || ny == 0 {
        return Err(ExportError::EmptyGrid);
    }
    let mut pixels = vec![0u8; nx * ny * 4];
    for row in 0..ny {
        for col in 0..nx {
            let idx = row * nx + col;
            let level = grid.get(idx).copied().unwrap_or(f32::NEG_INFINITY);
            let c = color_map.sample(level);
            let img_row = ny - 1 - row; // flip: row 0 = south = image bottom
            let base = (img_row * nx + col) * 4;
            pixels[base]     = c.r;
            pixels[base + 1] = c.g;
            pixels[base + 2] = c.b;
            pixels[base + 3] = c.a;
        }
    }
    Ok((nx as u32, ny as u32, pixels))
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn cm() -> ColorMap { ColorMap::who_standard() }

    #[test]
    fn buffer_size_correct() {
        let grid = vec![65.0f32; 6];
        let (w, h, buf) = render_to_buffer(&grid, 3, 2, &cm()).unwrap();
        assert_eq!(w, 3);
        assert_eq!(h, 2);
        assert_eq!(buf.len(), 3 * 2 * 4);
    }

    #[test]
    fn empty_grid_returns_error() {
        let result = render_to_buffer(&[], 0, 0, &cm());
        assert!(result.is_err());
    }

    #[test]
    fn high_level_renders_red() {
        let grid = vec![80.0f32]; // above max → dark red
        let (_, _, buf) = render_to_buffer(&grid, 1, 1, &cm()).unwrap();
        // Dark red: r > g and r > b.
        assert!(buf[0] > buf[2], "expected red-dominant pixel");
    }

    #[test]
    fn low_level_renders_green() {
        let grid = vec![35.0f32]; // at min → dark green
        let (_, _, buf) = render_to_buffer(&grid, 1, 1, &cm()).unwrap();
        // Dark green: g > r and g > b.
        assert!(buf[1] > buf[0], "expected green-dominant pixel: {:?}", &buf[..4]);
    }

    #[test]
    fn no_data_renders_transparent() {
        let grid = vec![f32::NEG_INFINITY];
        let (_, _, buf) = render_to_buffer(&grid, 1, 1, &cm()).unwrap();
        assert_eq!(buf[3], 0, "no-data should be transparent");
    }

    #[test]
    fn row_flip_south_at_bottom() {
        // Row 0 (south) = 35 dB (green), row 1 (north) = 80 dB (red).
        let grid = vec![35.0f32, 80.0]; // 1-wide, 2-tall
        let (_, _, buf) = render_to_buffer(&grid, 1, 2, &cm()).unwrap();
        // Top pixel in image = north = row 1 = 80 dB → red channel dominant.
        let top_r = buf[0];
        // Bottom pixel = south = row 0 = 35 dB → green channel dominant.
        let bot_g = buf[4 + 1];
        assert!(top_r > 100, "top pixel should be red-ish (80 dB)");
        assert!(bot_g > 100, "bottom pixel should be green-ish (35 dB)");
    }

    #[test]
    fn export_to_file_creates_valid_png() {
        let grid = vec![60.0f32; 4];
        let dir = std::env::temp_dir();
        let path = dir.join("noise_test_export.png");
        export_grid_png(&grid, 2, 2, &cm(), &path).unwrap();
        assert!(path.exists());
        let img = image::open(&path).unwrap();
        assert_eq!(img.width(), 2);
        assert_eq!(img.height(), 2);
        let _ = std::fs::remove_file(&path);
    }
}
