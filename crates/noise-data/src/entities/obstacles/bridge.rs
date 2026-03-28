use nalgebra::Point3;
use serde::{Deserialize, Serialize};

/// A bridge structure — acts as a noise reflector above and a barrier at sides.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bridge {
    pub id: u64,
    pub name: String,
    /// Centreline vertices of the bridge deck.
    pub deck_vertices: Vec<Point3<f64>>,
    /// Deck width (m).
    pub width_m: f64,
    /// Deck soffit height above ground (m).
    pub soffit_height_m: f64,
    /// Parapet height above deck surface (m).
    pub parapet_height_m: f64,
}

impl Bridge {
    pub fn deck_area_m2(&self) -> f64 {
        let len: f64 = self.deck_vertices.windows(2).map(|w| (w[1] - w[0]).norm()).sum();
        len * self.width_m
    }
}
