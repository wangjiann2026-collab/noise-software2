use nalgebra::Point3;
use serde::{Deserialize, Serialize};

/// A receiver point (assessment point).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiverPoint {
    pub id: u64,
    pub name: String,
    pub position: Point3<f64>,
    /// Height above ground (m).
    pub height_m: f64,
}

/// Enum wrapping all scene object types for unified storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SceneObject {
    Receiver(ReceiverPoint),
    // Additional types (Building, Barrier, RoadSource, etc.) added in Phase 3.
}

impl SceneObject {
    pub fn id(&self) -> u64 {
        match self {
            Self::Receiver(r) => r.id,
        }
    }
}
