//! Scene entity models.
//!
//! All scene objects implement [`SceneObjectMeta`] and are unified in the
//! [`SceneObject`] enum for generic storage and retrieval.

pub mod obstacles;
pub mod receiver;
pub mod sources;

pub use obstacles::{
    Barrier, Bridge, Building, Cylinder, GroundAbsorption, LandUseZone, Reflector3D, TreeBelt,
};
pub use receiver::ReceiverPoint;
pub use sources::{LineSource, PointSource, RailwaySource, RoadSource};

use nalgebra::Point3;
use serde::{Deserialize, Serialize};

// ─── ObjectType tag ──────────────────────────────────────────────────────────

/// Discriminant used to tag rows in the `scene_objects` table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectType {
    RoadSource,
    RailwaySource,
    PointSource,
    LineSource,
    Receiver,
    Building,
    Barrier,
    Bridge,
    Reflector3D,
    GroundAbsorption,
    TreeBelt,
    Cylinder,
    LandUseZone,
}

impl ObjectType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RoadSource      => "road_source",
            Self::RailwaySource   => "railway_source",
            Self::PointSource     => "point_source",
            Self::LineSource      => "line_source",
            Self::Receiver        => "receiver",
            Self::Building        => "building",
            Self::Barrier         => "barrier",
            Self::Bridge          => "bridge",
            Self::Reflector3D     => "reflector_3d",
            Self::GroundAbsorption => "ground_absorption",
            Self::TreeBelt        => "tree_belt",
            Self::Cylinder        => "cylinder",
            Self::LandUseZone     => "land_use_zone",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "road_source"       => Some(Self::RoadSource),
            "railway_source"    => Some(Self::RailwaySource),
            "point_source"      => Some(Self::PointSource),
            "line_source"       => Some(Self::LineSource),
            "receiver"          => Some(Self::Receiver),
            "building"          => Some(Self::Building),
            "barrier"           => Some(Self::Barrier),
            "bridge"            => Some(Self::Bridge),
            "reflector_3d"      => Some(Self::Reflector3D),
            "ground_absorption" => Some(Self::GroundAbsorption),
            "tree_belt"         => Some(Self::TreeBelt),
            "cylinder"          => Some(Self::Cylinder),
            "land_use_zone"     => Some(Self::LandUseZone),
            _                   => None,
        }
    }
}

// ─── Unified enum ────────────────────────────────────────────────────────────

/// All scene objects in a single enum for polymorphic storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SceneObject {
    RoadSource(RoadSource),
    RailwaySource(RailwaySource),
    PointSource(PointSource),
    LineSource(LineSource),
    Receiver(ReceiverPoint),
    Building(Building),
    Barrier(Barrier),
    Bridge(Bridge),
    Reflector3D(Reflector3D),
    GroundAbsorption(GroundAbsorption),
    TreeBelt(TreeBelt),
    Cylinder(Cylinder),
    LandUseZone(LandUseZone),
}

impl SceneObject {
    pub fn id(&self) -> u64 {
        match self {
            Self::RoadSource(x)       => x.id,
            Self::RailwaySource(x)    => x.id,
            Self::PointSource(x)      => x.id,
            Self::LineSource(x)       => x.id,
            Self::Receiver(x)         => x.id,
            Self::Building(x)         => x.id,
            Self::Barrier(x)          => x.id,
            Self::Bridge(x)           => x.id,
            Self::Reflector3D(x)      => x.id,
            Self::GroundAbsorption(x) => x.id,
            Self::TreeBelt(x)         => x.id,
            Self::Cylinder(x)         => x.id,
            Self::LandUseZone(x)      => x.id,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::RoadSource(x)       => &x.name,
            Self::RailwaySource(x)    => &x.name,
            Self::PointSource(x)      => &x.name,
            Self::LineSource(x)       => &x.name,
            Self::Receiver(x)         => &x.name,
            Self::Building(x)         => &x.name,
            Self::Barrier(x)          => &x.name,
            Self::Bridge(x)           => &x.name,
            Self::Reflector3D(x)      => &x.name,
            Self::GroundAbsorption(x) => &x.name,
            Self::TreeBelt(x)         => &x.name,
            Self::Cylinder(x)         => &x.name,
            Self::LandUseZone(x)      => &x.name,
        }
    }

    pub fn object_type(&self) -> ObjectType {
        match self {
            Self::RoadSource(_)       => ObjectType::RoadSource,
            Self::RailwaySource(_)    => ObjectType::RailwaySource,
            Self::PointSource(_)      => ObjectType::PointSource,
            Self::LineSource(_)       => ObjectType::LineSource,
            Self::Receiver(_)         => ObjectType::Receiver,
            Self::Building(_)         => ObjectType::Building,
            Self::Barrier(_)          => ObjectType::Barrier,
            Self::Bridge(_)           => ObjectType::Bridge,
            Self::Reflector3D(_)      => ObjectType::Reflector3D,
            Self::GroundAbsorption(_) => ObjectType::GroundAbsorption,
            Self::TreeBelt(_)         => ObjectType::TreeBelt,
            Self::Cylinder(_)         => ObjectType::Cylinder,
            Self::LandUseZone(_)      => ObjectType::LandUseZone,
        }
    }

    /// Centroid / representative point for spatial indexing.
    pub fn centroid(&self) -> Option<Point3<f64>> {
        match self {
            Self::PointSource(x)  => Some(x.position),
            Self::Receiver(x)     => Some(x.position),
            Self::Cylinder(x)     => Some(x.center),
            _ => None,
        }
    }
}

// ─── BoundingBox ─────────────────────────────────────────────────────────────

/// Axis-aligned bounding box in project coordinates.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct BoundingBox {
    pub min: Point3<f64>,
    pub max: Point3<f64>,
}

impl BoundingBox {
    pub fn from_points(pts: &[Point3<f64>]) -> Option<Self> {
        if pts.is_empty() { return None; }
        let mut min = pts[0];
        let mut max = pts[0];
        for p in pts.iter().skip(1) {
            min.x = min.x.min(p.x);
            min.y = min.y.min(p.y);
            min.z = min.z.min(p.z);
            max.x = max.x.max(p.x);
            max.y = max.y.max(p.y);
            max.z = max.z.max(p.z);
        }
        Some(Self { min, max })
    }

    pub fn contains_xy(&self, x: f64, y: f64) -> bool {
        x >= self.min.x && x <= self.max.x && y >= self.min.y && y <= self.max.y
    }

    pub fn intersects(&self, other: &Self) -> bool {
        self.min.x <= other.max.x && self.max.x >= other.min.x
            && self.min.y <= other.max.y && self.max.y >= other.min.y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounding_box_from_points() {
        let pts = vec![
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(10.0, 5.0, 3.0),
            Point3::new(-2.0, 8.0, 1.0),
        ];
        let bb = BoundingBox::from_points(&pts).unwrap();
        assert_eq!(bb.min.x, -2.0);
        assert_eq!(bb.max.x, 10.0);
        assert_eq!(bb.max.y, 8.0);
    }

    #[test]
    fn bounding_box_contains_xy() {
        let bb = BoundingBox {
            min: Point3::new(0.0, 0.0, 0.0),
            max: Point3::new(100.0, 100.0, 0.0),
        };
        assert!(bb.contains_xy(50.0, 50.0));
        assert!(!bb.contains_xy(150.0, 50.0));
    }

    #[test]
    fn bounding_box_intersects() {
        let a = BoundingBox { min: Point3::new(0.0,0.0,0.0), max: Point3::new(10.0,10.0,0.0) };
        let b = BoundingBox { min: Point3::new(5.0,5.0,0.0), max: Point3::new(15.0,15.0,0.0) };
        let c = BoundingBox { min: Point3::new(20.0,20.0,0.0), max: Point3::new(30.0,30.0,0.0) };
        assert!(a.intersects(&b));
        assert!(!a.intersects(&c));
    }

    #[test]
    fn object_type_roundtrip() {
        for ot in [
            ObjectType::RoadSource, ObjectType::Building, ObjectType::Receiver,
            ObjectType::TreeBelt, ObjectType::Cylinder,
        ] {
            assert_eq!(ObjectType::from_str(ot.as_str()), Some(ot));
        }
    }
}
