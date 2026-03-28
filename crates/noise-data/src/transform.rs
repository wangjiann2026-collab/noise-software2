//! Geometric transformations for scene objects.

use nalgebra::{Matrix4, Point3, Rotation3, Translation3, Unit, Vector3};
use serde::{Deserialize, Serialize};

/// A geometric transform applied to a collection of 3D points.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeometricTransform {
    /// 4×4 homogeneous transformation matrix (row-major).
    matrix: [[f64; 4]; 4],
}

impl GeometricTransform {
    pub fn identity() -> Self {
        Self { matrix: Matrix4::identity().into() }
    }

    pub fn translation(dx: f64, dy: f64, dz: f64) -> Self {
        let m = Translation3::new(dx, dy, dz).to_homogeneous();
        Self { matrix: m.into() }
    }

    pub fn rotation_z(angle_rad: f64) -> Self {
        let rot = Rotation3::from_axis_angle(&Unit::new_normalize(Vector3::z()), angle_rad);
        Self { matrix: rot.to_homogeneous().into() }
    }

    pub fn scale(sx: f64, sy: f64, sz: f64) -> Self {
        let m = Matrix4::new_nonuniform_scaling(&Vector3::new(sx, sy, sz));
        Self { matrix: m.into() }
    }

    pub fn compose(&self, other: &Self) -> Self {
        let a: Matrix4<f64> = self.matrix.into();
        let b: Matrix4<f64> = other.matrix.into();
        Self { matrix: (a * b).into() }
    }

    pub fn apply(&self, points: &[Point3<f64>]) -> Vec<Point3<f64>> {
        let m: Matrix4<f64> = self.matrix.into();
        points
            .iter()
            .map(|p| {
                let h = m * p.to_homogeneous();
                Point3::from_homogeneous(h).unwrap_or(*p)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    #[test]
    fn identity_leaves_points_unchanged() {
        let pts = vec![Point3::new(1.0, 2.0, 3.0)];
        let result = GeometricTransform::identity().apply(&pts);
        assert_abs_diff_eq!(result[0].x, 1.0, epsilon = 1e-9);
        assert_abs_diff_eq!(result[0].y, 2.0, epsilon = 1e-9);
    }

    #[test]
    fn translation_moves_point() {
        let pts = vec![Point3::new(0.0, 0.0, 0.0)];
        let result = GeometricTransform::translation(5.0, -3.0, 1.0).apply(&pts);
        assert_abs_diff_eq!(result[0].x, 5.0, epsilon = 1e-9);
        assert_abs_diff_eq!(result[0].y, -3.0, epsilon = 1e-9);
        assert_abs_diff_eq!(result[0].z, 1.0, epsilon = 1e-9);
    }

    #[test]
    fn rotation_z_90deg_swaps_xy() {
        let pts = vec![Point3::new(1.0, 0.0, 0.0)];
        let result = GeometricTransform::rotation_z(std::f64::consts::FRAC_PI_2).apply(&pts);
        assert_abs_diff_eq!(result[0].x, 0.0, epsilon = 1e-9);
        assert_abs_diff_eq!(result[0].y, 1.0, epsilon = 1e-9);
    }

    #[test]
    fn scale_doubles_coordinates() {
        let pts = vec![Point3::new(3.0, 4.0, 5.0)];
        let result = GeometricTransform::scale(2.0, 2.0, 2.0).apply(&pts);
        assert_abs_diff_eq!(result[0].x, 6.0, epsilon = 1e-9);
        assert_abs_diff_eq!(result[0].y, 8.0, epsilon = 1e-9);
        assert_abs_diff_eq!(result[0].z, 10.0, epsilon = 1e-9);
    }
}
