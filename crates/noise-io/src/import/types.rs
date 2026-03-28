//! Common data model for all imported scene objects.
//!
//! Every importer (DXF, GeoJSON, Shapefile, ASCII, XML) converts its native
//! format into this representation, which is then consumed by
//! `noise-data` entity constructors.

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// Semantic type of an imported object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObjectKind {
    Building,
    Barrier,
    Road,
    Rail,
    Receiver,
    GroundZone,
    Reflector,
    Unknown,
}

impl ObjectKind {
    /// Parse from a layer name or property string (case-insensitive).
    pub fn from_hint(hint: &str) -> Self {
        match hint.to_ascii_lowercase().trim() {
            s if s.contains("build") => Self::Building,
            s if s.contains("barrier") || s.contains("wall") || s.contains("fence") => Self::Barrier,
            s if s.contains("road") || s.contains("street") || s.contains("highway") => Self::Road,
            s if s.contains("rail") || s.contains("train") || s.contains("tram") => Self::Rail,
            s if s.contains("recv") || s.contains("receiver") || s.contains("point") => Self::Receiver,
            s if s.contains("ground") || s.contains("land") => Self::GroundZone,
            s if s.contains("reflect") => Self::Reflector,
            _ => Self::Unknown,
        }
    }
}

/// Geometry of an imported object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImportedGeometry {
    /// Single 3D point (X, Y, Z in metres).
    Point([f64; 3]),
    /// Open or closed 2D polyline.
    LineString(Vec<[f64; 2]>),
    /// Closed polygon exterior ring.
    Polygon(Vec<[f64; 2]>),
}

impl ImportedGeometry {
    /// Whether this geometry is a closed polygon.
    pub fn is_closed(&self) -> bool {
        match self {
            Self::Polygon(_) => true,
            Self::LineString(pts) => pts.len() >= 3 && pts.first() == pts.last(),
            _ => false,
        }
    }

    /// Approximate centroid (2D).
    pub fn centroid_2d(&self) -> [f64; 2] {
        let pts: &[[f64; 2]] = match self {
            Self::Point(p) => return [p[0], p[1]],
            Self::LineString(v) => v,
            Self::Polygon(v) => v,
        };
        if pts.is_empty() { return [0.0, 0.0]; }
        let n = pts.len() as f64;
        [pts.iter().map(|p| p[0]).sum::<f64>() / n,
         pts.iter().map(|p| p[1]).sum::<f64>() / n]
    }

    /// Number of vertices.
    pub fn vertex_count(&self) -> usize {
        match self {
            Self::Point(_) => 1,
            Self::LineString(v) | Self::Polygon(v) => v.len(),
        }
    }
}

/// A single scene object as parsed from an external file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportedObject {
    /// Sequential ID assigned during import.
    pub id: u64,
    /// Semantic type.
    pub kind: ObjectKind,
    /// Human-readable label (layer name, feature ID, etc.).
    pub label: String,
    /// Geometry.
    pub geometry: ImportedGeometry,
    /// Arbitrary key→value properties from the source file.
    pub properties: HashMap<String, String>,
}

impl ImportedObject {
    pub fn new(id: u64, kind: ObjectKind, label: impl Into<String>, geom: ImportedGeometry) -> Self {
        Self { id, kind, label: label.into(), geometry: geom, properties: HashMap::new() }
    }

    pub fn with_property(mut self, key: impl Into<String>, val: impl Into<String>) -> Self {
        self.properties.insert(key.into(), val.into());
        self
    }
}

/// Collection of all objects parsed from one file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportedScene {
    /// Parsed objects.
    pub objects: Vec<ImportedObject>,
    /// EPSG code if detected from the file metadata.
    pub crs_epsg: Option<u32>,
    /// Original source file path.
    pub source_file: String,
}

impl ImportedScene {
    pub fn new(source_file: impl Into<String>) -> Self {
        Self { objects: Vec::new(), crs_epsg: None, source_file: source_file.into() }
    }

    pub fn add(&mut self, obj: ImportedObject) {
        self.objects.push(obj);
    }

    /// Count of objects by kind.
    pub fn count_by_kind(&self, kind: ObjectKind) -> usize {
        self.objects.iter().filter(|o| o.kind == kind).count()
    }

    pub fn total(&self) -> usize { self.objects.len() }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_from_hint_building() {
        assert_eq!(ObjectKind::from_hint("BUILDINGS"), ObjectKind::Building);
    }

    #[test]
    fn kind_from_hint_barrier() {
        assert_eq!(ObjectKind::from_hint("noise_barrier"), ObjectKind::Barrier);
    }

    #[test]
    fn kind_from_hint_road() {
        assert_eq!(ObjectKind::from_hint("road_centerline"), ObjectKind::Road);
    }

    #[test]
    fn kind_from_hint_unknown() {
        assert_eq!(ObjectKind::from_hint("misc_layer"), ObjectKind::Unknown);
    }

    #[test]
    fn geometry_centroid_point() {
        let g = ImportedGeometry::Point([3.0, 4.0, 0.0]);
        let c = g.centroid_2d();
        assert_eq!(c, [3.0, 4.0]);
    }

    #[test]
    fn geometry_centroid_line() {
        let g = ImportedGeometry::LineString(vec![[0.0, 0.0], [10.0, 0.0]]);
        let c = g.centroid_2d();
        assert!((c[0] - 5.0).abs() < 1e-9);
    }

    #[test]
    fn geometry_is_closed_polygon() {
        let g = ImportedGeometry::Polygon(vec![[0.0,0.0],[1.0,0.0],[1.0,1.0],[0.0,0.0]]);
        assert!(g.is_closed());
    }

    #[test]
    fn geometry_vertex_count() {
        let g = ImportedGeometry::LineString(vec![[0.0,0.0],[1.0,1.0],[2.0,0.0]]);
        assert_eq!(g.vertex_count(), 3);
    }

    #[test]
    fn scene_count_by_kind() {
        let mut scene = ImportedScene::new("test.dxf");
        scene.add(ImportedObject::new(1, ObjectKind::Building, "B1",
            ImportedGeometry::Polygon(vec![])));
        scene.add(ImportedObject::new(2, ObjectKind::Barrier, "W1",
            ImportedGeometry::LineString(vec![])));
        scene.add(ImportedObject::new(3, ObjectKind::Building, "B2",
            ImportedGeometry::Polygon(vec![])));
        assert_eq!(scene.count_by_kind(ObjectKind::Building), 2);
        assert_eq!(scene.count_by_kind(ObjectKind::Barrier), 1);
        assert_eq!(scene.total(), 3);
    }
}
