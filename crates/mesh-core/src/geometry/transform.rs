//! Rigid transforms used by geometry generation.

use crate::geometry::Point2;

/// Rotates a point counter-clockwise around `pivot`.
#[must_use]
pub fn rotate_about(point: Point2, pivot: Point2, angle_radians: f64) -> Point2 {
    let local_x = point.x - pivot.x;
    let local_y = point.y - pivot.y;
    let (sin_alpha, cos_alpha) = angle_radians.sin_cos();
    Point2 {
        x: pivot.x + local_x * cos_alpha - local_y * sin_alpha,
        y: pivot.y + local_x * sin_alpha + local_y * cos_alpha,
    }
}

/// Rotates all points counter-clockwise around `pivot`, preserving input order.
#[must_use]
pub fn rotate_points(points: &[Point2], pivot: Point2, angle_radians: f64) -> Vec<Point2> {
    points
        .iter()
        .map(|point| rotate_about(*point, pivot, angle_radians))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::f64::consts::{FRAC_PI_2, PI};

    use super::rotate_about;
    use crate::geometry::Point2;

    const TOLERANCE: f64 = 1.0e-12;

    fn assert_point_close(actual: Point2, expected: Point2) {
        assert!((actual.x - expected.x).abs() < TOLERANCE);
        assert!((actual.y - expected.y).abs() < TOLERANCE);
    }

    #[test]
    fn zero_degree_rotation_is_identity() {
        let point = Point2::new(0.8, -0.2);
        assert_point_close(rotate_about(point, Point2::new(0.25, 0.0), 0.0), point);
    }

    #[test]
    fn ninety_degree_rotation_is_counter_clockwise() {
        let pivot = Point2::new(0.25, 0.0);
        let rotated = rotate_about(Point2::new(1.25, 0.0), pivot, FRAC_PI_2);
        assert_point_close(rotated, Point2::new(0.25, 1.0));
    }

    #[test]
    fn negative_rotation_is_clockwise() {
        let rotated = rotate_about(Point2::new(1.0, 0.0), Point2::new(0.0, 0.0), -FRAC_PI_2);
        assert_point_close(rotated, Point2::new(0.0, -1.0));
    }

    #[test]
    fn rotation_preserves_distance_from_pivot() {
        let point = Point2::new(0.9, -0.14);
        let pivot = Point2::new(0.25, 0.0);
        let rotated = rotate_about(point, pivot, PI / 7.0);
        assert!((point.distance_to(pivot) - rotated.distance_to(pivot)).abs() < TOLERANCE);
    }
}
