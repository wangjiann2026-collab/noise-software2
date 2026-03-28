//! Shapefile import (.shp + .dbf).
//!
//! Reads polygon and polyline shapes and maps them to `ImportedObject`
//! records using DBF attribute values to determine the `ObjectKind`.
//!
//! # Attribute → ObjectKind mapping
//! Looks for a field named `TYPE`, `KIND`, `LAYER`, or `CLASS` (case-insensitive)
//! and applies `ObjectKind::from_hint()`.  If no such field exists:
//! - `Polygon` shapes → `Building`
//! - `Polyline` shapes → `Barrier`
//! - `Point` shapes → `Receiver`

use std::path::Path;
use shapefile::{Shape, Reader};
use shapefile::record::PolygonRing;
use shapefile::dbase;
use super::{ImportError, types::{ImportedGeometry, ImportedObject, ImportedScene, ObjectKind}};

/// Import a Shapefile (.shp).  The associated .dbf will be read automatically.
pub fn import_shapefile(path: impl AsRef<Path>) -> Result<ImportedScene, ImportError> {
    let path = path.as_ref();
    let mut reader = Reader::from_path(path)
        .map_err(|e| ImportError::ParseError(format!("Shapefile open error: {e}")))?;

    let mut scene = ImportedScene::new(path.to_string_lossy());
    let mut next_id = 1u64;

    for result in reader.iter_shapes_and_records() {
        let (shape, record) = result
            .map_err(|e| ImportError::ParseError(format!("Shapefile read error: {e}")))?;

        let kind_hint = find_kind_field(&record);
        let label = find_label_field(&record).unwrap_or_else(|| format!("shape_{next_id}"));

        let (kind, geom) = match &shape {
            Shape::Polygon(poly) => {
                let kind = if kind_hint.is_empty() { ObjectKind::Building }
                           else { ObjectKind::from_hint(&kind_hint) };
                let pts: Vec<[f64; 2]> = poly.rings()
                    .iter()
                    .find(|r| matches!(r, PolygonRing::Outer(_)))
                    .map(|ring| ring.points().iter().map(|p| [p.x, p.y]).collect())
                    .unwrap_or_else(|| {
                        // Fall back to first ring regardless of type
                        poly.rings().first()
                            .map(|r| r.points().iter().map(|p| [p.x, p.y]).collect())
                            .unwrap_or_default()
                    });
                (kind, ImportedGeometry::Polygon(pts))
            }
            Shape::Polyline(pl) => {
                let kind = if kind_hint.is_empty() { ObjectKind::Barrier }
                           else { ObjectKind::from_hint(&kind_hint) };
                let pts: Vec<[f64; 2]> = pl.parts()
                    .iter()
                    .flat_map(|part| part.iter().map(|p| [p.x, p.y]))
                    .collect();
                (kind, ImportedGeometry::LineString(pts))
            }
            Shape::Point(pt) => {
                let kind = if kind_hint.is_empty() { ObjectKind::Receiver }
                           else { ObjectKind::from_hint(&kind_hint) };
                (kind, ImportedGeometry::Point([pt.x, pt.y, 0.0]))
            }
            Shape::PointM(pt) => (ObjectKind::Receiver, ImportedGeometry::Point([pt.x, pt.y, pt.m])),
            Shape::PointZ(pt) => (ObjectKind::Receiver, ImportedGeometry::Point([pt.x, pt.y, pt.z])),
            Shape::NullShape => continue,
            _ => continue,
        };

        let mut obj = ImportedObject::new(next_id, kind, label, geom);
        for (field_name, field_value) in record.into_iter() {
            obj.properties.insert(field_name.to_string(), format!("{field_value:?}"));
        }
        scene.add(obj);
        next_id += 1;
    }

    Ok(scene)
}

fn find_kind_field(record: &dbase::Record) -> String {
    let map: &std::collections::HashMap<String, dbase::FieldValue> = record.as_ref();
    for (name, val) in map.iter() {
        let n: String = name.to_ascii_lowercase();
        if matches!(n.as_str(), "type" | "kind" | "layer" | "class" | "category") {
            return format!("{val:?}");
        }
    }
    String::new()
}

fn find_label_field(record: &dbase::Record) -> Option<String> {
    let map: &std::collections::HashMap<String, dbase::FieldValue> = record.as_ref();
    for (name, val) in map.iter() {
        let n: String = name.to_ascii_lowercase();
        if matches!(n.as_str(), "name" | "label" | "id" | "fid" | "gid") {
            let s = format!("{val:?}");
            if !s.is_empty() && s != "\"\"" { return Some(s); }
        }
    }
    None
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonexistent_shapefile_returns_error() {
        let result = import_shapefile("/tmp/no_such_noise_test.shp");
        assert!(result.is_err());
    }

    #[test]
    fn write_and_read_polygon_shapefile() {
        use shapefile::{Polygon, PolygonRing, Point, Writer};
        use shapefile::dbase::TableWriterBuilder;

        let dir = std::env::temp_dir();
        let shp_path = dir.join("test_noise_poly.shp");

        let ring = PolygonRing::Outer(vec![
            Point::new(0.0, 0.0), Point::new(10.0, 0.0),
            Point::new(5.0, 10.0), Point::new(0.0, 0.0),
        ]);
        let poly = Polygon::with_rings(vec![ring]);
        let mut writer = Writer::from_path(&shp_path, TableWriterBuilder::new()).unwrap();
        writer.write_shape_and_record(&poly, &dbase::Record::default()).unwrap();
        drop(writer);

        let scene = import_shapefile(&shp_path).unwrap();
        assert_eq!(scene.total(), 1);
        assert_eq!(scene.objects[0].kind, ObjectKind::Building);
        assert!(matches!(scene.objects[0].geometry, ImportedGeometry::Polygon(_)));

        let _ = std::fs::remove_file(&shp_path);
        let _ = std::fs::remove_file(shp_path.with_extension("shx"));
        let _ = std::fs::remove_file(shp_path.with_extension("dbf"));
    }

    #[test]
    fn write_and_read_point_shapefile() {
        use shapefile::{Point, Writer};
        use shapefile::dbase::TableWriterBuilder;

        let dir = std::env::temp_dir();
        let shp_path = dir.join("test_noise_point.shp");
        let pt = Point::new(100.0, 200.0);
        let mut writer = Writer::from_path(&shp_path, TableWriterBuilder::new()).unwrap();
        writer.write_shape_and_record(&pt, &dbase::Record::default()).unwrap();
        drop(writer);

        let scene = import_shapefile(&shp_path).unwrap();
        assert_eq!(scene.total(), 1);
        assert_eq!(scene.objects[0].kind, ObjectKind::Receiver);
        if let ImportedGeometry::Point(p) = scene.objects[0].geometry {
            assert!((p[0] - 100.0).abs() < 1e-9);
        }
        let _ = std::fs::remove_file(&shp_path);
        let _ = std::fs::remove_file(shp_path.with_extension("shx"));
        let _ = std::fs::remove_file(shp_path.with_extension("dbf"));
    }
}
