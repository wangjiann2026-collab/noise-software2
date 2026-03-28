//! GeoJSON file/string import.
//!
//! Reads a GeoJSON `FeatureCollection` and maps each `Feature` to an
//! `ImportedObject`.  The feature `type` property (or geometry type for
//! unmapped features) determines the `ObjectKind`.
//!
//! # Property → ObjectKind mapping
//! | `"type"` property value | Kind |
//! |---|---|
//! | "building" | Building |
//! | "barrier", "wall" | Barrier |
//! | "road" | Road |
//! | "rail" | Rail |
//! | "receiver" | Receiver |
//! | "ground" | GroundZone |
//!
//! Polygons without a `type` property default to `Building`;
//! LineStrings default to `Barrier`.

use std::path::Path;
use geojson::{GeoJson, Geometry, Value as GjValue};
use super::{ImportError, types::{ImportedGeometry, ImportedObject, ImportedScene, ObjectKind}};

/// Import from a GeoJSON file.
pub fn import_geojson(path: impl AsRef<Path>) -> Result<ImportedScene, ImportError> {
    let content = std::fs::read_to_string(path.as_ref())
        .map_err(|e| ImportError::Io(e))?;
    import_geojson_str(&content, path.as_ref().to_string_lossy().as_ref())
}

/// Import from a GeoJSON string.
pub fn import_geojson_str(content: &str, source: &str) -> Result<ImportedScene, ImportError> {
    let gj: GeoJson = content.parse()
        .map_err(|e| ImportError::ParseError(format!("GeoJSON parse error: {e}")))?;

    let mut scene = ImportedScene::new(source);
    let mut next_id = 1u64;

    let fc = match gj {
        GeoJson::FeatureCollection(fc) => fc,
        GeoJson::Feature(f) => geojson::FeatureCollection {
            bbox: None,
            features: vec![f],
            foreign_members: None,
        },
        _ => return Err(ImportError::ParseError(
            "Expected FeatureCollection or Feature at top level".into()
        )),
    };

    for feature in &fc.features {
        let geom = match feature.geometry.as_ref() {
            Some(g) => g,
            None => continue,
        };

        // Determine kind from properties first, then geometry shape.
        let type_hint = feature.property("type")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let label = feature.property("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let kind = if !type_hint.is_empty() {
            ObjectKind::from_hint(type_hint)
        } else {
            match &geom.value {
                GjValue::Polygon(_) | GjValue::MultiPolygon(_) => ObjectKind::Building,
                GjValue::LineString(_) | GjValue::MultiLineString(_) => ObjectKind::Barrier,
                GjValue::Point(_) => ObjectKind::Receiver,
                _ => ObjectKind::Unknown,
            }
        };

        let imported_geom = geometry_to_imported(geom)?;
        let mut obj = ImportedObject::new(next_id, kind,
            if label.is_empty() { format!("feature_{next_id}") } else { label },
            imported_geom);

        // Copy all properties.
        if let Some(props) = &feature.properties {
            for (k, v) in props {
                obj.properties.insert(k.clone(), v.to_string());
            }
        }

        scene.add(obj);
        next_id += 1;
    }

    Ok(scene)
}

fn geometry_to_imported(geom: &Geometry) -> Result<ImportedGeometry, ImportError> {
    match &geom.value {
        GjValue::Point(c) => {
            let z = c.get(2).copied().unwrap_or(0.0);
            Ok(ImportedGeometry::Point([c[0], c[1], z]))
        }
        GjValue::LineString(coords) => {
            let pts = coords.iter().map(|c| [c[0], c[1]]).collect();
            Ok(ImportedGeometry::LineString(pts))
        }
        GjValue::Polygon(rings) => {
            let outer: Vec<[f64; 2]> = rings.first()
                .map(|r| r.iter().map(|c| [c[0], c[1]]).collect())
                .unwrap_or_default();
            Ok(ImportedGeometry::Polygon(outer))
        }
        GjValue::MultiLineString(parts) => {
            // Flatten all parts into one LineString.
            let pts: Vec<[f64; 2]> = parts.iter()
                .flat_map(|p| p.iter().map(|c| [c[0], c[1]]))
                .collect();
            Ok(ImportedGeometry::LineString(pts))
        }
        GjValue::MultiPolygon(polys) => {
            // Use the first polygon's outer ring.
            let outer: Vec<[f64; 2]> = polys.first()
                .and_then(|p| p.first())
                .map(|r| r.iter().map(|c| [c[0], c[1]]).collect())
                .unwrap_or_default();
            Ok(ImportedGeometry::Polygon(outer))
        }
        other => Err(ImportError::UnsupportedFormat(
            format!("Unsupported geometry type: {other:?}")
        )),
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const BUILDING_FC: &str = r#"{
      "type": "FeatureCollection",
      "features": [
        {
          "type": "Feature",
          "properties": {"type": "building", "name": "Office A", "height": "12"},
          "geometry": {
            "type": "Polygon",
            "coordinates": [[[0,0],[10,0],[10,10],[0,10],[0,0]]]
          }
        },
        {
          "type": "Feature",
          "properties": {"type": "barrier", "name": "Wall 1"},
          "geometry": {
            "type": "LineString",
            "coordinates": [[20,0],[20,50]]
          }
        },
        {
          "type": "Feature",
          "properties": {"type": "receiver"},
          "geometry": {"type": "Point", "coordinates": [100, 50, 4.0]}
        }
      ]
    }"#;

    #[test]
    fn parse_feature_collection() {
        let scene = import_geojson_str(BUILDING_FC, "test.geojson").unwrap();
        assert_eq!(scene.total(), 3);
    }

    #[test]
    fn building_kind_assigned() {
        let scene = import_geojson_str(BUILDING_FC, "test.geojson").unwrap();
        assert_eq!(scene.count_by_kind(ObjectKind::Building), 1);
    }

    #[test]
    fn barrier_kind_assigned() {
        let scene = import_geojson_str(BUILDING_FC, "test.geojson").unwrap();
        assert_eq!(scene.count_by_kind(ObjectKind::Barrier), 1);
    }

    #[test]
    fn receiver_kind_assigned() {
        let scene = import_geojson_str(BUILDING_FC, "test.geojson").unwrap();
        assert_eq!(scene.count_by_kind(ObjectKind::Receiver), 1);
    }

    #[test]
    fn building_is_polygon() {
        let scene = import_geojson_str(BUILDING_FC, "test.geojson").unwrap();
        let b = scene.objects.iter().find(|o| o.kind == ObjectKind::Building).unwrap();
        assert!(matches!(b.geometry, ImportedGeometry::Polygon(_)));
    }

    #[test]
    fn receiver_has_z() {
        let scene = import_geojson_str(BUILDING_FC, "test.geojson").unwrap();
        let r = scene.objects.iter().find(|o| o.kind == ObjectKind::Receiver).unwrap();
        if let ImportedGeometry::Point(p) = r.geometry {
            assert!((p[2] - 4.0).abs() < 1e-9);
        }
    }

    #[test]
    fn properties_carried_through() {
        let scene = import_geojson_str(BUILDING_FC, "test.geojson").unwrap();
        let b = scene.objects.iter().find(|o| o.kind == ObjectKind::Building).unwrap();
        assert_eq!(b.properties.get("height").map(String::as_str), Some("\"12\""));
    }

    #[test]
    fn label_from_name_property() {
        let scene = import_geojson_str(BUILDING_FC, "test.geojson").unwrap();
        let b = scene.objects.iter().find(|o| o.kind == ObjectKind::Building).unwrap();
        assert_eq!(b.label, "Office A");
    }

    #[test]
    fn invalid_json_returns_error() {
        let result = import_geojson_str("not json", "bad.geojson");
        assert!(result.is_err());
    }

    #[test]
    fn polygon_without_type_defaults_to_building() {
        let gj = r#"{
          "type": "FeatureCollection",
          "features": [{
            "type": "Feature",
            "properties": {},
            "geometry": {"type": "Polygon", "coordinates": [[[0,0],[1,0],[1,1],[0,0]]]}
          }]
        }"#;
        let scene = import_geojson_str(gj, "auto.geojson").unwrap();
        assert_eq!(scene.objects[0].kind, ObjectKind::Building);
    }
}
