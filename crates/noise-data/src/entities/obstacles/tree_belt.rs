use nalgebra::Point3;
use serde::{Deserialize, Serialize};

/// A belt of trees providing extra attenuation (ISO 9613-2 §8.3).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeBelt {
    pub id: u64,
    pub name: String,
    /// Polygon boundary of the tree belt.
    pub boundary: Vec<Point3<f64>>,
    /// Average tree height (m).
    pub tree_height_m: f64,
    /// Foliage density: 0.0 (sparse) to 1.0 (dense).
    pub foliage_density: f64,
}

impl TreeBelt {
    /// Extra attenuation (dB) per ISO 9613-2: 0–10 dB depending on depth.
    /// depth_m = propagation path length through the belt (m).
    pub fn excess_attenuation_db(&self, depth_m: f64) -> f64 {
        // ISO 9613-2 §8.3: Afol = min(10, depth * 0.1 * density) dB
        (depth_m * 0.1 * self.foliage_density).min(10.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attenuation_capped_at_10db() {
        let t = TreeBelt {
            id: 1, name: "Belt".into(), boundary: vec![],
            tree_height_m: 10.0, foliage_density: 1.0,
        };
        assert!((t.excess_attenuation_db(1000.0) - 10.0).abs() < 1e-9);
    }

    #[test]
    fn sparse_belt_lower_attenuation() {
        let dense = TreeBelt { id:1, name:"".into(), boundary:vec![], tree_height_m:10.0, foliage_density:1.0 };
        let sparse = TreeBelt { id:2, name:"".into(), boundary:vec![], tree_height_m:10.0, foliage_density:0.3 };
        assert!(dense.excess_attenuation_db(50.0) > sparse.excess_attenuation_db(50.0));
    }
}
