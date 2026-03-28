use nalgebra::Point3;
use serde::{Deserialize, Serialize};

/// A discrete assessment receiver point.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiverPoint {
    pub id: u64,
    pub name: String,
    /// 3D position (x, y, z) in project CRS.
    pub position: Point3<f64>,
    /// Height above ground (m), typically 4.0 m (EU default).
    pub height_m: f64,
    /// Optional group tag for batch reporting.
    pub group: Option<String>,
}

impl ReceiverPoint {
    pub fn new(id: u64, name: impl Into<String>, x: f64, y: f64, z: f64, height_m: f64) -> Self {
        Self {
            id,
            name: name.into(),
            position: Point3::new(x, y, z),
            height_m,
            group: None,
        }
    }

    /// Effective 3D position including height offset.
    pub fn effective_position(&self) -> Point3<f64> {
        Point3::new(self.position.x, self.position.y, self.position.z + self.height_m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_position_adds_height() {
        let r = ReceiverPoint::new(1, "R1", 100.0, 200.0, 10.0, 4.0);
        let ep = r.effective_position();
        assert!((ep.z - 14.0).abs() < 1e-9);
    }
}
