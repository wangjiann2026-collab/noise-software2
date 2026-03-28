//! Ray tracing engine for acoustic propagation.
//!
//! Implements image source method combined with ray tracing for computing
//! direct, reflected (up to N-th order), and diffracted sound paths.

use crate::obstacles::ReflectorSurface;
use nalgebra::{Point3, Unit, Vector3};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Maximum supported reflection order (requirement: up to 20th order).
pub const MAX_REFLECTION_ORDER: usize = 20;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RayTracerConfig {
    /// Maximum number of reflection bounces (1–20).
    pub max_reflection_order: usize,
    /// Frequency bands to compute (Hz).
    pub frequency_bands: Vec<f64>,
    /// Speed of sound (m/s), defaults to 343.0.
    pub speed_of_sound: f64,
    /// Ray divergence threshold for path culling.
    pub divergence_threshold: f64,
}

impl Default for RayTracerConfig {
    fn default() -> Self {
        Self {
            max_reflection_order: 1,
            frequency_bands: vec![63.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0],
            speed_of_sound: 343.0,
            divergence_threshold: 1e-6,
        }
    }
}

/// A single acoustic ray path from source to receiver.
#[derive(Debug, Clone)]
pub struct RayPath {
    /// Ordered list of 3D waypoints: source → reflections → receiver.
    pub waypoints: Vec<Point3<f64>>,
    /// Total path length (m).
    pub length: f64,
    /// Reflection order (0 = direct path).
    pub reflection_order: usize,
    /// Per-band excess attenuation from reflections (dB), indexed by frequency band.
    pub reflection_loss_db: Vec<f64>,
}

impl RayPath {
    pub fn new(waypoints: Vec<Point3<f64>>, reflection_loss_db: Vec<f64>) -> Self {
        let length = waypoints
            .windows(2)
            .map(|w| (w[1] - w[0]).norm())
            .sum();
        let reflection_order = waypoints.len().saturating_sub(2);
        Self { waypoints, length, reflection_order, reflection_loss_db }
    }

    /// Geometric spreading attenuation: 20·log10(r) + 11 dB (point source).
    pub fn geometric_attenuation_db(&self) -> f64 {
        if self.length < 1e-9 {
            return 0.0;
        }
        20.0 * self.length.log10() + 11.0
    }
}

#[derive(Debug, Error)]
pub enum RayTracerError {
    #[error("Reflection order {order} exceeds maximum of {max}", max = MAX_REFLECTION_ORDER)]
    OrderExceedsMax { order: usize },
    #[error("Source and receiver positions are identical")]
    CoincidentPoints,
}

/// Acoustic ray tracer using the image source method.
///
/// The image source method creates virtual mirror images of the source
/// for each reflecting surface and finds valid paths by geometric back-tracing.
pub struct RayTracer {
    config: RayTracerConfig,
}

impl RayTracer {
    pub fn new(config: RayTracerConfig) -> Result<Self, RayTracerError> {
        if config.max_reflection_order > MAX_REFLECTION_ORDER {
            return Err(RayTracerError::OrderExceedsMax { order: config.max_reflection_order });
        }
        Ok(Self { config })
    }

    /// Compute all valid ray paths from `source` to `receiver` given a set of
    /// reflecting surfaces. Returns paths in ascending reflection order.
    pub fn compute_paths(
        &self,
        source: &Point3<f64>,
        receiver: &Point3<f64>,
        reflectors: &[Box<dyn ReflectorSurface>],
    ) -> Result<Vec<RayPath>, RayTracerError> {
        if (receiver - source).norm() < 1e-9 {
            return Err(RayTracerError::CoincidentPoints);
        }

        let mut paths = Vec::new();

        // Order 0: direct path (check for obstruction separately via diffraction).
        let direct = RayPath::new(
            vec![*source, *receiver],
            vec![0.0; self.config.frequency_bands.len()],
        );
        paths.push(direct);

        // Higher-order reflections via image source method.
        if self.config.max_reflection_order > 0 && !reflectors.is_empty() {
            self.find_reflected_paths(source, receiver, reflectors, &mut paths);
        }

        Ok(paths)
    }

    fn find_reflected_paths(
        &self,
        _source: &Point3<f64>,
        _receiver: &Point3<f64>,
        _reflectors: &[Box<dyn ReflectorSurface>],
        _paths: &mut Vec<RayPath>,
    ) {
        // Iterative image source expansion up to max_reflection_order.
        // Each level generates image sources mirrored in every reflector plane.
        // Full implementation in Phase 4 (TDD).
    }

    pub fn config(&self) -> &RayTracerConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_path_length_is_correct() {
        let cfg = RayTracerConfig { max_reflection_order: 0, ..Default::default() };
        let tracer = RayTracer::new(cfg).unwrap();
        let src = Point3::new(0.0, 0.0, 0.0);
        let rcv = Point3::new(3.0, 4.0, 0.0);
        let paths = tracer.compute_paths(&src, &rcv, &[]).unwrap();
        assert_eq!(paths.len(), 1);
        assert!((paths[0].length - 5.0).abs() < 1e-9);
        assert_eq!(paths[0].reflection_order, 0);
    }

    #[test]
    fn coincident_points_returns_error() {
        let cfg = RayTracerConfig::default();
        let tracer = RayTracer::new(cfg).unwrap();
        let pt = Point3::new(1.0, 2.0, 3.0);
        assert!(matches!(
            tracer.compute_paths(&pt, &pt, &[]),
            Err(RayTracerError::CoincidentPoints)
        ));
    }

    #[test]
    fn max_order_clamped_at_20() {
        let cfg = RayTracerConfig { max_reflection_order: 21, ..Default::default() };
        assert!(matches!(RayTracer::new(cfg), Err(RayTracerError::OrderExceedsMax { order: 21 })));
    }

    #[test]
    fn geometric_attenuation_increases_with_distance() {
        let path_near = RayPath::new(
            vec![Point3::origin(), Point3::new(10.0, 0.0, 0.0)],
            vec![],
        );
        let path_far = RayPath::new(
            vec![Point3::origin(), Point3::new(100.0, 0.0, 0.0)],
            vec![],
        );
        assert!(path_far.geometric_attenuation_db() > path_near.geometric_attenuation_db());
    }
}
