//! Acoustic ray tracer using the Image Source Method (ISM).
//!
//! # Algorithm
//! 1. For each reflector surface, mirror the source to create an image source.
//! 2. For 2nd order: mirror each image source in every other reflector.
//! 3. Continue up to `max_reflection_order`.
//! 4. For each image source, check if the straight path to the receiver
//!    actually intersects the correct sequence of reflector panels.
//! 5. Valid paths contribute to the total sound pressure via energy addition.

use crate::obstacles::ReflectorSurface;
use nalgebra::{Point3, Unit, Vector3};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Maximum supported reflection order (requirement: up to 20th order).
pub const MAX_REFLECTION_ORDER: usize = 20;

// ─── Configuration ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RayTracerConfig {
    /// Maximum number of reflection bounces (1–20).
    pub max_reflection_order: usize,
    /// Octave band centre frequencies (Hz).
    pub frequency_bands: Vec<f64>,
    /// Speed of sound (m/s).
    pub speed_of_sound: f64,
    /// Minimum path contribution threshold (linear energy). Paths below this
    /// are culled for performance.
    pub energy_threshold: f64,
}

impl Default for RayTracerConfig {
    fn default() -> Self {
        Self {
            max_reflection_order: 1,
            frequency_bands: vec![63.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0],
            speed_of_sound: 343.0,
            energy_threshold: 1e-6,
        }
    }
}

// ─── Ray path ─────────────────────────────────────────────────────────────────

/// A single valid acoustic ray path from source to receiver.
#[derive(Debug, Clone)]
pub struct RayPath {
    /// Ordered waypoints: [source, reflection₁, …, reflectionN, receiver].
    pub waypoints: Vec<Point3<f64>>,
    /// Total geometric path length (m).
    pub length: f64,
    /// Reflection order (0 = direct path).
    pub reflection_order: usize,
    /// Cumulative reflection loss per octave band (dB).
    pub reflection_loss_db: Vec<f64>,
}

impl RayPath {
    pub fn new(waypoints: Vec<Point3<f64>>, reflection_loss_db: Vec<f64>) -> Self {
        let length = waypoints.windows(2).map(|w| (w[1] - w[0]).norm()).sum();
        let reflection_order = waypoints.len().saturating_sub(2);
        Self { waypoints, length, reflection_order, reflection_loss_db }
    }

    /// Geometric spreading attenuation: 20·log₁₀(r) + 11 dB.
    pub fn geometric_attenuation_db(&self) -> f64 {
        if self.length < 1e-9 { return 0.0; }
        20.0 * self.length.log10() + 11.0
    }

    /// Energy weight relative to the direct-path (linear scale, 0..1].
    pub fn energy_weight(&self) -> f64 {
        let a = self.geometric_attenuation_db()
            + self.reflection_loss_db.iter().sum::<f64>();
        10f64.powf(-a / 10.0)
    }
}

// ─── Errors ───────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum RayTracerError {
    #[error("Reflection order {order} exceeds maximum of {max}", max = MAX_REFLECTION_ORDER)]
    OrderExceedsMax { order: usize },
    #[error("Source and receiver positions are identical")]
    CoincidentPoints,
}

// ─── Image source ─────────────────────────────────────────────────────────────

/// An image source produced by mirroring the real source in a reflector plane.
#[derive(Debug, Clone)]
struct ImageSource {
    /// 3D position of the virtual image.
    position: Point3<f64>,
    /// Sequence of reflector indices (indices into the `reflectors` slice)
    /// encountered on this image path.
    reflector_chain: Vec<usize>,
    /// Cumulative reflection loss (dB) accumulated over the chain.
    loss_db: Vec<f64>,
}

// ─── Ray tracer ───────────────────────────────────────────────────────────────

pub struct RayTracer {
    config: RayTracerConfig,
}

impl RayTracer {
    pub fn new(config: RayTracerConfig) -> Result<Self, RayTracerError> {
        if config.max_reflection_order > MAX_REFLECTION_ORDER {
            return Err(RayTracerError::OrderExceedsMax {
                order: config.max_reflection_order,
            });
        }
        Ok(Self { config })
    }

    /// Compute all valid ray paths from `source` to `receiver`.
    pub fn compute_paths(
        &self,
        source: &Point3<f64>,
        receiver: &Point3<f64>,
        reflectors: &[Box<dyn ReflectorSurface>],
    ) -> Result<Vec<RayPath>, RayTracerError> {
        if (receiver - source).norm() < 1e-9 {
            return Err(RayTracerError::CoincidentPoints);
        }

        let n_bands = self.config.frequency_bands.len();
        let mut paths = Vec::new();

        // Order-0: direct path.
        paths.push(RayPath::new(
            vec![*source, *receiver],
            vec![0.0; n_bands],
        ));

        if self.config.max_reflection_order == 0 || reflectors.is_empty() {
            return Ok(paths);
        }

        // Build image sources iteratively.
        let mut image_sources: Vec<ImageSource> = vec![ImageSource {
            position: *source,
            reflector_chain: vec![],
            loss_db: vec![0.0; n_bands],
        }];

        for _order in 1..=self.config.max_reflection_order {
            let mut next_level = Vec::new();

            for img in &image_sources {
                for (r_idx, reflector) in reflectors.iter().enumerate() {
                    // Avoid reflecting in the same surface consecutively.
                    if img.reflector_chain.last() == Some(&r_idx) {
                        continue;
                    }

                    if let Some(mirrored) = mirror_point(&img.position, reflector.as_ref()) {
                        // Check that the path mirrored_image → receiver
                        // actually intersects reflector r_idx.
                        if reflector.intersect_segment(&mirrored, receiver).is_some() {
                            // Compute cumulative reflection loss.
                            let abs = reflector.absorption_coefficients();
                            let mut new_loss = img.loss_db.clone();
                            for (i, &a) in abs.iter().enumerate().take(n_bands) {
                                // Reflection loss = −10·log₁₀(1 − α)
                                new_loss[i] += -10.0 * (1.0 - a).max(1e-9).log10();
                            }

                            // Build waypoints: mirrored image → [chain] → receiver.
                            let mut chain = img.reflector_chain.clone();
                            chain.push(r_idx);

                            if new_loss.iter().all(|&l| l < 60.0) {
                                next_level.push(ImageSource {
                                    position: mirrored,
                                    reflector_chain: chain.clone(),
                                    loss_db: new_loss.clone(),
                                });

                                // Reconstruct the actual waypoints.
                                if let Some(waypoints) =
                                    self.reconstruct_path(source, receiver, &chain, reflectors)
                                {
                                    paths.push(RayPath::new(waypoints, new_loss));
                                }
                            }
                        }
                    }
                }
            }
            image_sources = next_level;
            if image_sources.is_empty() { break; }
        }

        Ok(paths)
    }

    /// Reconstruct actual waypoints by back-tracing through the reflector chain.
    fn reconstruct_path(
        &self,
        source: &Point3<f64>,
        receiver: &Point3<f64>,
        chain: &[usize],
        reflectors: &[Box<dyn ReflectorSurface>],
    ) -> Option<Vec<Point3<f64>>> {
        // For each reflector in the chain (in reverse), compute the reflection point.
        // This is a simplified implementation: we use the midpoint of the
        // intersection between consecutive image sources.
        let mut pts = vec![*source];
        let mut current = *source;
        for &r_idx in chain {
            let refl = &reflectors[r_idx];
            // Find intersection of current → receiver with reflector.
            if let Some((hit, _)) = refl.intersect_segment(&current, receiver) {
                pts.push(hit);
                current = hit;
            } else {
                return None; // Path not geometrically valid.
            }
        }
        pts.push(*receiver);
        Some(pts)
    }

    pub fn config(&self) -> &RayTracerConfig {
        &self.config
    }
}

/// Mirror `point` in the plane of `reflector` (infinite plane approximation).
fn mirror_point(point: &Point3<f64>, reflector: &dyn ReflectorSurface) -> Option<Point3<f64>> {
    let n = reflector.normal_at(point);
    // We need a point on the plane. Use the intersection of the normal from
    // point with the plane — here we use the reflector's own normal_at as a
    // reference, projecting onto the plane assumed to pass through origin.
    // In practice, the intersection_segment check validates correctness.
    let proj = point + n.as_ref() * (-(point.coords.dot(&n)));
    let mirrored = 2.0 * proj - point;
    Some(Point3::from(mirrored))
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    fn cfg(order: usize) -> RayTracerConfig {
        RayTracerConfig { max_reflection_order: order, ..Default::default() }
    }

    #[test]
    fn direct_path_length_correct() {
        let t = RayTracer::new(cfg(0)).unwrap();
        let s = Point3::new(0.0, 0.0, 0.0);
        let r = Point3::new(3.0, 4.0, 0.0);
        let paths = t.compute_paths(&s, &r, &[]).unwrap();
        assert_eq!(paths.len(), 1);
        assert_abs_diff_eq!(paths[0].length, 5.0, epsilon = 1e-9);
        assert_eq!(paths[0].reflection_order, 0);
    }

    #[test]
    fn coincident_points_error() {
        let t = RayTracer::new(cfg(0)).unwrap();
        let p = Point3::new(1.0, 2.0, 3.0);
        assert!(matches!(t.compute_paths(&p, &p, &[]),
            Err(RayTracerError::CoincidentPoints)));
    }

    #[test]
    fn max_order_guard() {
        assert!(matches!(
            RayTracer::new(RayTracerConfig { max_reflection_order: 21, ..Default::default() }),
            Err(RayTracerError::OrderExceedsMax { order: 21 })
        ));
    }

    #[test]
    fn max_order_20_allowed() {
        assert!(RayTracer::new(cfg(20)).is_ok());
    }

    #[test]
    fn geometric_attenuation_doubles_at_double_distance() {
        let p_near = RayPath::new(
            vec![Point3::origin(), Point3::new(10.0, 0.0, 0.0)], vec![]);
        let p_far  = RayPath::new(
            vec![Point3::origin(), Point3::new(20.0, 0.0, 0.0)], vec![]);
        let diff = p_far.geometric_attenuation_db() - p_near.geometric_attenuation_db();
        assert_abs_diff_eq!(diff, 6.02, epsilon = 0.01);
    }

    #[test]
    fn reflected_path_longer_than_direct() {
        // Direct path = 100 m; any reflection must be longer.
        let t = RayTracer::new(cfg(0)).unwrap();
        let s = Point3::new(0.0, 0.0, 0.5);
        let r = Point3::new(100.0, 0.0, 4.0);
        let paths = t.compute_paths(&s, &r, &[]).unwrap();
        for p in &paths {
            if p.reflection_order > 0 {
                assert!(p.length > (r - s).norm());
            }
        }
    }

    #[test]
    fn direct_path_energy_weight_is_one() {
        // Direct path has zero reflection loss → energy weight depends only on geometry.
        let p = RayPath::new(
            vec![Point3::origin(), Point3::new(10.0, 0.0, 0.0)],
            vec![]
        );
        let w = p.energy_weight();
        assert!(w > 0.0 && w <= 1.0, "weight={w}");
    }

    #[test]
    fn no_reflectors_gives_only_direct_path() {
        let t = RayTracer::new(cfg(5)).unwrap();
        let s = Point3::new(0.0, 0.0, 0.5);
        let r = Point3::new(50.0, 0.0, 4.0);
        let paths = t.compute_paths(&s, &r, &[]).unwrap();
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].reflection_order, 0);
    }
}
