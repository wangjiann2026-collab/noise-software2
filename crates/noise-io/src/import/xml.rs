//! XML import for the NoiseMap project exchange format.
//!
//! ```xml
//! <NoiseMappingProject crs="32650">
//!   <Sources>
//!     <RoadSource id="1" name="Main St" x1="0" y1="0" x2="100" y2="0"
//!                 speed="50" flow="1000" category="Cat1"/>
//!     <PointSource id="2" name="Factory" x="200" y="100" z="5" lwa="95"/>
//!   </Sources>
//!   <Receivers>
//!     <Receiver id="1" name="Flat 1" x="50" y="30" z="4"/>
//!   </Receivers>
//!   <Obstacles>
//!     <Building id="1" name="Block A" points="0,0 20,0 20,10 0,10 0,0" height="12"/>
//!     <Barrier  id="2" name="Wall B"  points="30,0 30,50" height="3"/>
//!   </Obstacles>
//! </NoiseMappingProject>
//! ```

use quick_xml::{Reader, events::Event};
use super::{ImportError, types::{ImportedGeometry, ImportedObject, ImportedScene, ObjectKind}};

/// Import from an XML file.
pub fn import_xml(path: impl AsRef<std::path::Path>) -> Result<ImportedScene, ImportError> {
    let content = std::fs::read_to_string(path.as_ref()).map_err(ImportError::Io)?;
    import_xml_str(&content, &path.as_ref().to_string_lossy())
}

/// Import from an XML string.
pub fn import_xml_str(content: &str, source: &str) -> Result<ImportedScene, ImportError> {
    let mut reader = Reader::from_str(content);
    reader.config_mut().trim_text(true);

    let mut scene = ImportedScene::new(source);
    let mut buf = Vec::new();
    let mut next_id = 1u64;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let tag = std::str::from_utf8(e.name().as_ref())
                    .unwrap_or("").to_ascii_lowercase();

                if tag == "noisemappingproject" {
                    if let Some(crs_attr) = e.attributes().flatten()
                        .find(|a| a.key.as_ref() == b"crs") {
                        let val = String::from_utf8_lossy(&crs_attr.value).to_string();
                        scene.crs_epsg = val.parse().ok();
                    }
                    buf.clear(); continue;
                }

                let obj = match tag.as_str() {
                    "roadsource"                        => parse_road_source(e, next_id),
                    "pointsource" | "industrialsource"  => parse_point_source(e, next_id),
                    "receiver"                          => parse_receiver(e, next_id),
                    "building"                          => parse_obstacle(e, next_id, ObjectKind::Building),
                    "barrier" | "wall"                  => parse_obstacle(e, next_id, ObjectKind::Barrier),
                    _ => { buf.clear(); continue; }
                };

                if let Some(o) = obj { scene.add(o); next_id += 1; }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(ImportError::ParseError(format!("XML error: {e}"))),
            _ => {}
        }
        buf.clear();
    }
    Ok(scene)
}

// ─── Attribute helpers ────────────────────────────────────────────────────────

fn attr(e: &quick_xml::events::BytesStart, key: &[u8]) -> Option<String> {
    e.attributes().flatten()
        .find(|a| a.key.as_ref() == key)
        .map(|a| String::from_utf8_lossy(&a.value).to_string())
}
fn attr_f64(e: &quick_xml::events::BytesStart, key: &[u8]) -> Option<f64> {
    attr(e, key)?.parse().ok()
}

fn parse_road_source(e: &quick_xml::events::BytesStart, id: u64) -> Option<ImportedObject> {
    let name = attr(e, b"name").unwrap_or_else(|| format!("road_{id}"));
    let x1 = attr_f64(e, b"x1").unwrap_or(0.0);
    let y1 = attr_f64(e, b"y1").unwrap_or(0.0);
    let x2 = attr_f64(e, b"x2").unwrap_or(x1);
    let y2 = attr_f64(e, b"y2").unwrap_or(y1);
    let mut obj = ImportedObject::new(id, ObjectKind::Road, name,
        ImportedGeometry::LineString(vec![[x1, y1], [x2, y2]]));
    for k in [b"speed".as_ref(), b"flow", b"category"] {
        if let Some(v) = attr(e, k) {
            obj.properties.insert(String::from_utf8_lossy(k).to_string(), v);
        }
    }
    Some(obj)
}

fn parse_point_source(e: &quick_xml::events::BytesStart, id: u64) -> Option<ImportedObject> {
    let name = attr(e, b"name").unwrap_or_else(|| format!("src_{id}"));
    let x = attr_f64(e, b"x").unwrap_or(0.0);
    let y = attr_f64(e, b"y").unwrap_or(0.0);
    let z = attr_f64(e, b"z").unwrap_or(1.0);
    let mut obj = ImportedObject::new(id, ObjectKind::Road, name,
        ImportedGeometry::Point([x, y, z]));
    if let Some(v) = attr(e, b"lwa") { obj.properties.insert("lwa".into(), v); }
    Some(obj)
}

fn parse_receiver(e: &quick_xml::events::BytesStart, id: u64) -> Option<ImportedObject> {
    let name = attr(e, b"name").unwrap_or_else(|| format!("rcv_{id}"));
    let x = attr_f64(e, b"x")?;
    let y = attr_f64(e, b"y")?;
    let z = attr_f64(e, b"z").unwrap_or(4.0);
    Some(ImportedObject::new(id, ObjectKind::Receiver, name,
        ImportedGeometry::Point([x, y, z])))
}

fn parse_obstacle(e: &quick_xml::events::BytesStart, id: u64, kind: ObjectKind) -> Option<ImportedObject> {
    let name = attr(e, b"name").unwrap_or_else(|| format!("obs_{id}"));
    let pts_str = attr(e, b"points").unwrap_or_default();
    let pts: Vec<[f64; 2]> = pts_str.split_whitespace()
        .filter_map(|pair| {
            let mut it = pair.split(',');
            let x: f64 = it.next()?.parse().ok()?;
            let y: f64 = it.next()?.parse().ok()?;
            Some([x, y])
        })
        .collect();
    let geom = if kind == ObjectKind::Barrier && pts.len() <= 2 {
        ImportedGeometry::LineString(pts)
    } else if pts.len() >= 3 {
        ImportedGeometry::Polygon(pts)
    } else {
        return None;
    };
    let mut obj = ImportedObject::new(id, kind, name, geom);
    if let Some(h) = attr(e, b"height") { obj.properties.insert("height".into(), h); }
    Some(obj)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_XML: &str = r#"<?xml version="1.0"?>
<NoiseMappingProject crs="32650">
  <Sources>
    <RoadSource id="1" name="Main Street" x1="0" y1="0" x2="200" y2="0"
                speed="50" flow="1200" category="Cat1"/>
    <PointSource id="2" name="Factory" x="300" y="100" z="5" lwa="95"/>
  </Sources>
  <Receivers>
    <Receiver id="1" name="House 1" x="100" y="50" z="4"/>
    <Receiver id="2" name="House 2" x="150" y="50" z="4"/>
  </Receivers>
  <Obstacles>
    <Building id="1" name="Block A" points="10,10 30,10 30,30 10,30 10,10" height="12"/>
    <Barrier  id="2" name="Wall B"  points="40,0 40,60" height="3"/>
  </Obstacles>
</NoiseMappingProject>"#;

    #[test]
    fn crs_detected() {
        let scene = import_xml_str(SAMPLE_XML, "test.xml").unwrap();
        assert_eq!(scene.crs_epsg, Some(32650));
    }

    #[test]
    fn total_object_count() {
        let scene = import_xml_str(SAMPLE_XML, "test.xml").unwrap();
        assert_eq!(scene.total(), 6);
    }

    #[test]
    fn road_source_label() {
        let scene = import_xml_str(SAMPLE_XML, "test.xml").unwrap();
        let road = scene.objects.iter().find(|o| o.kind == ObjectKind::Road
            && matches!(o.geometry, ImportedGeometry::LineString(_))).unwrap();
        assert_eq!(road.label, "Main Street");
        assert_eq!(road.properties.get("speed").map(String::as_str), Some("50"));
    }

    #[test]
    fn receivers_count() {
        let scene = import_xml_str(SAMPLE_XML, "test.xml").unwrap();
        assert_eq!(scene.count_by_kind(ObjectKind::Receiver), 2);
    }

    #[test]
    fn building_is_polygon_with_height() {
        let scene = import_xml_str(SAMPLE_XML, "test.xml").unwrap();
        let b = scene.objects.iter().find(|o| o.kind == ObjectKind::Building).unwrap();
        assert!(matches!(b.geometry, ImportedGeometry::Polygon(_)));
        assert_eq!(b.properties.get("height").map(String::as_str), Some("12"));
    }

    #[test]
    fn barrier_is_linestring() {
        let scene = import_xml_str(SAMPLE_XML, "test.xml").unwrap();
        let w = scene.objects.iter().find(|o| o.kind == ObjectKind::Barrier).unwrap();
        assert!(matches!(w.geometry, ImportedGeometry::LineString(_)));
    }
}
