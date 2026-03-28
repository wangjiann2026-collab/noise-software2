//! DXF file import.
//!
//! Reads DXF entities and converts them to `ImportedObject` records.
//!
//! # Layer → ObjectKind mapping
//! The DXF layer name is used to infer the semantic type:
//! | Layer keyword | Kind |
//! |---|---|
//! | "build…" | Building |
//! | "barrier", "wall", "fence" | Barrier |
//! | "road", "street", "highway" | Road |
//! | "rail", "train", "tram" | Rail |
//! | "recv…", "receiver", "point" | Receiver |
//!
//! # Supported entity types
//! - `LINE`        → `LineString` with 2 vertices
//! - `LWPOLYLINE`  → `LineString` / `Polygon` (closed flag)
//! - `POINT`       → `Point`

use std::path::Path;
use dxf::{Drawing, entities::EntityType};
use super::{ImportError, types::{ImportedGeometry, ImportedObject, ImportedScene, ObjectKind}};

/// Import a DXF file and return all recognised scene objects.
pub fn import_dxf(path: impl AsRef<Path>) -> Result<ImportedScene, ImportError> {
    let path = path.as_ref();
    let drawing = Drawing::load_file(path)
        .map_err(|e| ImportError::ParseError(format!("DXF load error: {e}")))?;

    let mut scene = ImportedScene::new(path.to_string_lossy());
    let mut next_id = 1u64;

    for entity in drawing.entities() {
        let layer = entity.common.layer.clone();
        let kind = ObjectKind::from_hint(&layer);

        let obj = match &entity.specific {
            EntityType::Line(line) => {
                let pts = vec![
                    [line.p1.x, line.p1.y],
                    [line.p2.x, line.p2.y],
                ];
                ImportedObject::new(next_id, kind, &layer, ImportedGeometry::LineString(pts))
                    .with_property("entity", "LINE")
                    .with_property("z1", line.p1.z.to_string())
                    .with_property("z2", line.p2.z.to_string())
            }
            EntityType::LwPolyline(lw) => {
                let pts: Vec<[f64; 2]> = lw.vertices.iter()
                    .map(|v| [v.x, v.y])
                    .collect();
                let geom = if lw.is_closed() && pts.len() >= 3 {
                    ImportedGeometry::Polygon(pts)
                } else {
                    ImportedGeometry::LineString(pts)
                };
                ImportedObject::new(next_id, kind, &layer, geom)
                    .with_property("entity", "LWPOLYLINE")
            }
            EntityType::ModelPoint(pt) => {
                let geom = ImportedGeometry::Point([pt.location.x, pt.location.y, pt.location.z]);
                ImportedObject::new(next_id, kind, &layer, geom)
                    .with_property("entity", "POINT")
            }
            _ => continue, // skip unsupported entity types
        };

        scene.add(obj);
        next_id += 1;
    }

    Ok(scene)
}

/// Parse DXF content from a string (useful for testing without file I/O).
pub fn import_dxf_str(content: &str) -> Result<ImportedScene, ImportError> {
    use std::io::Cursor;
    let mut cursor = Cursor::new(content.as_bytes());
    let drawing = Drawing::load(&mut cursor)
        .map_err(|e| ImportError::ParseError(format!("DXF parse error: {e}")))?;

    let mut scene = ImportedScene::new("<string>");
    let mut next_id = 1u64;

    for entity in drawing.entities() {
        let layer = entity.common.layer.clone();
        let kind = ObjectKind::from_hint(&layer);

        let obj = match &entity.specific {
            EntityType::Line(line) => {
                let pts = vec![
                    [line.p1.x, line.p1.y],
                    [line.p2.x, line.p2.y],
                ];
                ImportedObject::new(next_id, kind, &layer, ImportedGeometry::LineString(pts))
            }
            EntityType::LwPolyline(lw) => {
                let pts: Vec<[f64; 2]> = lw.vertices.iter().map(|v| [v.x, v.y]).collect();
                let geom = if lw.is_closed() && pts.len() >= 3 {
                    ImportedGeometry::Polygon(pts)
                } else {
                    ImportedGeometry::LineString(pts)
                };
                ImportedObject::new(next_id, kind, &layer, geom)
            }
            EntityType::ModelPoint(pt) => {
                let geom = ImportedGeometry::Point([pt.location.x, pt.location.y, pt.location.z]);
                ImportedObject::new(next_id, kind, &layer, geom)
            }
            _ => continue,
        };

        scene.add(obj);
        next_id += 1;
    }

    Ok(scene)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal valid DXF R12 ASCII with a single LINE entity.
    const DXF_LINE: &str = "\
  0\nSECTION\n  2\nENTITIES\n\
  0\nLINE\n  8\nbarrier\n\
 10\n0.0\n 20\n0.0\n 30\n0.0\n\
 11\n10.0\n 21\n5.0\n 31\n0.0\n\
  0\nENDSEC\n  0\nEOF\n";

    /// DXF with a single POINT on layer "receiver".
    const DXF_POINT: &str = "\
  0\nSECTION\n  2\nENTITIES\n\
  0\nPOINT\n  8\nreceiver\n\
 10\n50.0\n 20\n25.0\n 30\n4.0\n\
  0\nENDSEC\n  0\nEOF\n";

    #[test]
    fn parse_line_entity() {
        let scene = import_dxf_str(DXF_LINE).unwrap();
        assert_eq!(scene.total(), 1);
        let obj = &scene.objects[0];
        assert_eq!(obj.kind, ObjectKind::Barrier);
        assert!(matches!(obj.geometry, ImportedGeometry::LineString(_)));
        if let ImportedGeometry::LineString(pts) = &obj.geometry {
            assert_eq!(pts.len(), 2);
            assert!((pts[0][0]).abs() < 1e-9);
            assert!((pts[1][0] - 10.0).abs() < 1e-9);
        }
    }

    #[test]
    fn parse_point_entity() {
        let scene = import_dxf_str(DXF_POINT).unwrap();
        assert_eq!(scene.total(), 1);
        let obj = &scene.objects[0];
        assert_eq!(obj.kind, ObjectKind::Receiver);
        if let ImportedGeometry::Point(p) = obj.geometry {
            assert!((p[0] - 50.0).abs() < 1e-9);
            assert!((p[1] - 25.0).abs() < 1e-9);
            assert!((p[2] - 4.0).abs() < 1e-9);
        } else {
            panic!("expected Point geometry");
        }
    }

    #[test]
    fn empty_dxf_returns_empty_scene() {
        let dxf_empty = "  0\nSECTION\n  2\nENTITIES\n  0\nENDSEC\n  0\nEOF\n";
        let scene = import_dxf_str(dxf_empty).unwrap();
        assert_eq!(scene.total(), 0);
    }

    #[test]
    fn nonexistent_file_returns_error() {
        let result = import_dxf("/tmp/nonexistent_noise_test_12345.dxf");
        assert!(result.is_err());
    }
}
