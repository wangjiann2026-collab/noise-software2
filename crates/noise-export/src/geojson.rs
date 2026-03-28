//! GeoJSON export — iso-contour lines using the Marching Squares algorithm.
//!
//! Each iso-level is emitted as a GeoJSON `Feature` whose geometry is a
//! `MultiLineString` of the individual Marching-Squares segments.  The
//! segments are produced by [`noise_render::contour::extract_isolines`].

use noise_render::contour::extract_isolines;
use serde_json::{Value, json};

use crate::GridView;

/// WHO-standard noise contour levels (dBA).
pub const DEFAULT_LEVELS: &[f32] = &[35.0, 40.0, 45.0, 50.0, 55.0, 60.0, 65.0, 70.0, 75.0];

/// Export a noise grid as a GeoJSON `FeatureCollection`.
///
/// Each feature represents one iso-contour level.  The geometry is a
/// `MultiLineString` whose coordinates are `[x, y]` pairs in the grid's
/// coordinate system.
///
/// # Parameters
/// - `view`   — grid data and geospatial metadata
/// - `levels` — iso-levels (dBA) to extract; falls back to [`DEFAULT_LEVELS`]
///              when the slice is empty
pub fn export_geojson(view: &GridView, levels: &[f32]) -> Value {
    let effective_levels: &[f32] = if levels.is_empty() { DEFAULT_LEVELS } else { levels };

    let iso_lines = extract_isolines(
        &view.levels,
        view.nx,
        view.ny,
        view.cellsize as f32,
        view.cellsize as f32,
        [view.xllcorner as f32, view.yllcorner as f32],
        effective_levels,
    );

    // Build one Feature per iso-level, skipping empty contours.
    let features: Vec<Value> = iso_lines
        .iter()
        .filter(|line| !line.segments.is_empty())
        .map(|line| {
            // Each segment is a two-point LineString.
            let coordinates: Vec<Value> = line
                .segments
                .iter()
                .map(|(a, b)| {
                    json!([
                        [a[0] as f64, a[1] as f64],
                        [b[0] as f64, b[1] as f64]
                    ])
                })
                .collect();

            json!({
                "type": "Feature",
                "geometry": {
                    "type": "MultiLineString",
                    "coordinates": coordinates
                },
                "properties": {
                    "level_dba": line.level_db,
                    "segment_count": line.segment_count(),
                    "total_length_m": (line.total_length() * 10.0).round() / 10.0
                }
            })
        })
        .collect();

    json!({
        "type": "FeatureCollection",
        "name": "Noise iso-contours",
        "crs": {
            "type": "name",
            "properties": { "name": "urn:ogc:def:crs:OGC:1.3:CRS84" }
        },
        "features": features
    })
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn uniform_view(level: f32) -> GridView {
        GridView {
            levels: vec![level; 9],
            nx: 3, ny: 3,
            xllcorner: 0.0, yllcorner: 0.0,
            cellsize: 10.0,
        }
    }

    #[test]
    fn uniform_grid_produces_feature_collection() {
        let view = uniform_view(60.0);
        let fc = export_geojson(&view, &[55.0, 65.0]);
        assert_eq!(fc["type"], "FeatureCollection");
        // 60 dB grid: 55 dB contour has no crossings (all cells above), 65 dB has no crossings
        // → features may be empty (all uniform → no crossings)
        assert!(fc["features"].is_array());
    }

    #[test]
    fn step_grid_extracts_contour() {
        // Left column 50 dB, right column 70 dB — 2×2 grid
        let view = GridView {
            levels: vec![50.0, 70.0, 50.0, 70.0],
            nx: 2, ny: 2,
            xllcorner: 100.0, yllcorner: 200.0,
            cellsize: 5.0,
        };
        let fc = export_geojson(&view, &[60.0]);
        let features = fc["features"].as_array().unwrap();
        // Should produce at least one contour feature
        assert!(!features.is_empty(), "expected at least one contour feature");
        assert_eq!(features[0]["properties"]["level_dba"], 60.0);
    }

    #[test]
    fn empty_levels_uses_defaults() {
        let view = GridView {
            levels: (0..25).map(|i| 40.0 + i as f32 * 2.0).collect(),
            nx: 5, ny: 5,
            xllcorner: 0.0, yllcorner: 0.0,
            cellsize: 10.0,
        };
        let fc = export_geojson(&view, &[]);
        // With default levels covering 35–75 dB, gradient should produce several features
        let features = fc["features"].as_array().unwrap();
        assert!(!features.is_empty());
    }

    #[test]
    fn feature_has_required_fields() {
        let view = GridView {
            levels: vec![50.0, 70.0, 50.0, 70.0],
            nx: 2, ny: 2,
            xllcorner: 0.0, yllcorner: 0.0,
            cellsize: 1.0,
        };
        let fc = export_geojson(&view, &[60.0]);
        if let Some(feat) = fc["features"].as_array().and_then(|a| a.first()) {
            assert_eq!(feat["type"], "Feature");
            assert!(feat["geometry"].is_object());
            assert_eq!(feat["geometry"]["type"], "MultiLineString");
            assert!(feat["properties"]["level_dba"].is_number());
        }
    }
}
