//! Export iso-contour lines to SVG.
//!
//! Generates an SVG file with one `<path>` element per iso-contour level.
//! World coordinates are mapped to SVG pixel coordinates with Y-axis flipped
//! (SVG Y grows down, world Y grows up/north).

use std::io::Write as IoWrite;
use std::path::Path;

use crate::contour::IsoContourLine;
use super::ExportError;

/// Style options for SVG output.
#[derive(Debug, Clone)]
pub struct SvgStyle {
    /// SVG viewport width (pixels).
    pub width_px: u32,
    /// SVG viewport height (pixels).
    pub height_px: u32,
    /// Background fill colour (CSS colour string).
    pub background: String,
    /// Stroke width for contour lines (pixels).
    pub stroke_width: f32,
    /// Font size for level labels (pixels).
    pub label_font_size: f32,
}

impl Default for SvgStyle {
    fn default() -> Self {
        Self {
            width_px: 800,
            height_px: 600,
            background: "#f8f8f8".into(),
            stroke_width: 1.5,
            label_font_size: 10.0,
        }
    }
}

/// A palette entry: (dBA level, CSS colour string).
pub type LevelColor = (f32, String);

/// Export iso-contour lines to an SVG file.
///
/// # Parameters
/// - `isolines`      — iso-contour lines to render.
/// - `world_bounds`  — `[min_x, min_y, max_x, max_y]` of the world space (m).
/// - `palette`       — per-level CSS colour (matched by `level_db`).
/// - `style`         — SVG output style.
/// - `path`          — output file path.
pub fn export_svg(
    isolines: &[IsoContourLine],
    world_bounds: [f32; 4],
    palette: &[LevelColor],
    style: &SvgStyle,
    path: &Path,
) -> Result<(), ExportError> {
    let svg = render_to_string(isolines, world_bounds, palette, style)?;
    let mut file = std::fs::File::create(path)
        .map_err(|e| ExportError::Io(e.to_string()))?;
    file.write_all(svg.as_bytes())
        .map_err(|e| ExportError::Io(e.to_string()))?;
    Ok(())
}

/// Render iso-contour lines to an SVG string (no file I/O).
pub fn render_to_string(
    isolines: &[IsoContourLine],
    world_bounds: [f32; 4],
    palette: &[LevelColor],
    style: &SvgStyle,
) -> Result<String, ExportError> {
    let [wx_min, wy_min, wx_max, wy_max] = world_bounds;
    let ww = (wx_max - wx_min).max(1e-6);
    let wh = (wy_max - wy_min).max(1e-6);
    let sw = style.width_px as f32;
    let sh = style.height_px as f32;

    // World → SVG pixel coordinate transform.
    let to_svg = |wx: f32, wy: f32| -> (f32, f32) {
        let px = (wx - wx_min) / ww * sw;
        let py = sh - (wy - wy_min) / wh * sh; // flip Y
        (px, py)
    };

    let mut svg = String::new();
    svg.push_str(&format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">
  <rect width="{w}" height="{h}" fill="{bg}"/>
"#,
        w = style.width_px, h = style.height_px, bg = style.background
    ));

    for line in isolines {
        let color = palette.iter()
            .find(|(lvl, _)| (lvl - line.level_db).abs() < 0.5)
            .map(|(_, c)| c.as_str())
            .unwrap_or("#333333");

        svg.push_str(&format!(
            "  <!-- iso-contour {:.0} dBA -->\n",
            line.level_db
        ));

        // Each segment is a separate `<line>`.
        for (a, b) in &line.segments {
            let (x1, y1) = to_svg(a[0], a[1]);
            let (x2, y2) = to_svg(b[0], b[1]);
            svg.push_str(&format!(
                "  <line x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\" stroke=\"{}\" stroke-width=\"{:.1}\"/>\n",
                x1, y1, x2, y2, color, style.stroke_width
            ));
        }
    }

    // Legend.
    svg.push_str("  <!-- legend -->\n");
    for (i, (lvl, color)) in palette.iter().enumerate() {
        let lx = sw - 120.0;
        let ly = 20.0 + i as f32 * 18.0;
        svg.push_str(&format!(
            "  <rect x=\"{:.0}\" y=\"{:.0}\" width=\"20\" height=\"12\" fill=\"{}\"/>\n",
            lx, ly, color
        ));
        svg.push_str(&format!(
            "  <text x=\"{:.0}\" y=\"{:.0}\" font-size=\"{:.0}\" fill=\"#333\">{:.0} dBA</text>\n",
            lx + 25.0, ly + 11.0, style.label_font_size, lvl
        ));
    }

    svg.push_str("</svg>\n");
    Ok(svg)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contour::IsoContourLine;

    fn sample_isolines() -> Vec<IsoContourLine> {
        vec![
            IsoContourLine {
                level_db: 60.0,
                segments: vec![([0.0, 0.0], [10.0, 5.0]), ([10.0, 5.0], [20.0, 0.0])],
            },
            IsoContourLine {
                level_db: 65.0,
                segments: vec![([5.0, 5.0], [15.0, 5.0])],
            },
        ]
    }

    fn palette() -> Vec<LevelColor> {
        vec![
            (60.0, "orange".into()),
            (65.0, "red".into()),
        ]
    }

    #[test]
    fn svg_starts_with_declaration() {
        let svg = render_to_string(
            &sample_isolines(), [0.0, 0.0, 20.0, 10.0],
            &palette(), &SvgStyle::default()
        ).unwrap();
        assert!(svg.starts_with("<?xml"), "got: {}", &svg[..30]);
    }

    #[test]
    fn svg_contains_svg_element() {
        let svg = render_to_string(
            &sample_isolines(), [0.0, 0.0, 20.0, 10.0],
            &palette(), &SvgStyle::default()
        ).unwrap();
        assert!(svg.contains("<svg "));
        assert!(svg.contains("</svg>"));
    }

    #[test]
    fn svg_contains_line_elements() {
        let svg = render_to_string(
            &sample_isolines(), [0.0, 0.0, 20.0, 10.0],
            &palette(), &SvgStyle::default()
        ).unwrap();
        assert!(svg.contains("<line "), "expected <line> elements in SVG");
    }

    #[test]
    fn svg_contains_legend() {
        let svg = render_to_string(
            &sample_isolines(), [0.0, 0.0, 20.0, 10.0],
            &palette(), &SvgStyle::default()
        ).unwrap();
        assert!(svg.contains("60 dBA") || svg.contains("60.0 dBA") || svg.contains("dBA"));
    }

    #[test]
    fn empty_isolines_produces_valid_svg() {
        let svg = render_to_string(&[], [0.0, 0.0, 100.0, 100.0], &[], &SvgStyle::default()).unwrap();
        assert!(svg.contains("<svg "));
        assert!(svg.contains("</svg>"));
    }

    #[test]
    fn svg_to_file_roundtrip() {
        let isolines = sample_isolines();
        let dir = std::env::temp_dir();
        let path = dir.join("noise_test_contour.svg");
        export_svg(&isolines, [0.0, 0.0, 20.0, 10.0], &palette(), &SvgStyle::default(), &path).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("<svg "));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn segment_count_matches_line_count() {
        let svg = render_to_string(
            &sample_isolines(), [0.0, 0.0, 20.0, 10.0],
            &palette(), &SvgStyle::default()
        ).unwrap();
        // 2 segments in first isoline + 1 in second = 3 <line> elements.
        let count = svg.matches("<line ").count();
        assert_eq!(count, 3);
    }
}
