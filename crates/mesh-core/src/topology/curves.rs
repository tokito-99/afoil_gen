//! Parametric curves used by Gordon–Hall interpolation.

use serde::Serialize;

use crate::geometry::Point2;

/// A deterministic curve over the normalized interval `[0, 1]`.
pub trait ParametricCurve {
    /// Evaluates the curve at a normalized parameter.
    fn point(&self, parameter: f64) -> Point2;
}

/// A straight line between two points.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LineCurve {
    pub start: Point2,
    pub end: Point2,
}

impl LineCurve {
    /// Creates a line.
    #[must_use]
    pub const fn new(start: Point2, end: Point2) -> Self {
        Self { start, end }
    }
}

impl ParametricCurve for LineCurve {
    fn point(&self, parameter: f64) -> Point2 {
        interpolate(self.start, self.end, parameter)
    }
}

/// A circular arc described by center, radius, and angular sweep.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CircularArcCurve {
    pub center: Point2,
    pub radius: f64,
    pub start_angle_radians: f64,
    pub sweep_angle_radians: f64,
}

impl ParametricCurve for CircularArcCurve {
    fn point(&self, parameter: f64) -> Point2 {
        let angle = self.start_angle_radians + parameter * self.sweep_angle_radians;
        Point2::new(
            self.center.x + self.radius * angle.cos(),
            self.center.y + self.radius * angle.sin(),
        )
    }
}

/// Piecewise-linear interpolation with equal parameter width per segment.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PolylineCurve {
    pub points: Vec<Point2>,
}

impl PolylineCurve {
    /// Creates a polyline. Callers must provide at least two points.
    #[must_use]
    pub fn new(points: Vec<Point2>) -> Self {
        Self { points }
    }
}

impl ParametricCurve for PolylineCurve {
    fn point(&self, parameter: f64) -> Point2 {
        interpolate_samples(&self.points, parameter)
    }
}

/// An airfoil-surface curve retaining the analytical sample ordering.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AirfoilSurfaceCurve {
    pub points: Vec<Point2>,
    #[serde(skip)]
    cumulative_lengths: Vec<f64>,
}

impl AirfoilSurfaceCurve {
    /// Creates an airfoil curve whose parameter follows physical arc length.
    /// Geometry sampling density must not silently become mesh spacing.
    #[must_use]
    pub fn new(points: Vec<Point2>) -> Self {
        let cumulative_lengths = cumulative_lengths(&points);
        Self {
            points,
            cumulative_lengths,
        }
    }
}

impl ParametricCurve for AirfoilSurfaceCurve {
    fn point(&self, parameter: f64) -> Point2 {
        interpolate_samples_by_arc_length(&self.points, &self.cumulative_lengths, parameter)
    }
}

/// A user- or engine-sampled curve.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SampledCurve {
    pub points: Vec<Point2>,
}

impl ParametricCurve for SampledCurve {
    fn point(&self, parameter: f64) -> Point2 {
        interpolate_samples(&self.points, parameter)
    }
}

/// One side of the downstream wake refinement region.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WakeCenterlineCurve {
    pub start: Point2,
    pub end: Point2,
}

impl ParametricCurve for WakeCenterlineCurve {
    fn point(&self, parameter: f64) -> Point2 {
        interpolate(self.start, self.end, parameter)
    }
}

/// Owned curve variant used by logical block definitions.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CurveSpec {
    Line(LineCurve),
    CircularArc(CircularArcCurve),
    Polyline(PolylineCurve),
    AirfoilSurface(AirfoilSurfaceCurve),
    Sampled(SampledCurve),
    WakeCenterline(WakeCenterlineCurve),
}

impl ParametricCurve for CurveSpec {
    fn point(&self, parameter: f64) -> Point2 {
        match self {
            Self::Line(curve) => curve.point(parameter),
            Self::CircularArc(curve) => curve.point(parameter),
            Self::Polyline(curve) => curve.point(parameter),
            Self::AirfoilSurface(curve) => curve.point(parameter),
            Self::Sampled(curve) => curve.point(parameter),
            Self::WakeCenterline(curve) => curve.point(parameter),
        }
    }
}

impl CurveSpec {
    /// Returns true when the curve is exactly represented by its endpoints.
    #[must_use]
    pub const fn is_linear(&self) -> bool {
        matches!(self, Self::Line(_) | Self::WakeCenterline(_))
    }

    /// Returns a curve with its parameter direction reversed.
    #[must_use]
    pub fn reversed(&self) -> Self {
        match self {
            Self::Line(curve) => Self::Line(LineCurve::new(curve.end, curve.start)),
            Self::CircularArc(curve) => Self::CircularArc(CircularArcCurve {
                center: curve.center,
                radius: curve.radius,
                start_angle_radians: curve.start_angle_radians + curve.sweep_angle_radians,
                sweep_angle_radians: -curve.sweep_angle_radians,
            }),
            Self::Polyline(curve) => {
                let mut points = curve.points.clone();
                points.reverse();
                Self::Polyline(PolylineCurve::new(points))
            }
            Self::AirfoilSurface(curve) => {
                let mut points = curve.points.clone();
                points.reverse();
                Self::AirfoilSurface(AirfoilSurfaceCurve::new(points))
            }
            Self::Sampled(curve) => {
                let mut points = curve.points.clone();
                points.reverse();
                Self::Sampled(SampledCurve { points })
            }
            Self::WakeCenterline(curve) => Self::WakeCenterline(WakeCenterlineCurve {
                start: curve.end,
                end: curve.start,
            }),
        }
    }

    /// Samples the curve at `segment_count + 1` deterministic parameters.
    #[must_use]
    pub fn sampled_points(&self, segment_count: usize) -> Vec<Point2> {
        if segment_count == 0 {
            return vec![self.point(0.0)];
        }
        (0..=segment_count)
            .map(|index| {
                #[allow(clippy::cast_precision_loss)]
                let parameter = index as f64 / segment_count as f64;
                self.point(parameter)
            })
            .collect()
    }
}

fn interpolate(start: Point2, end: Point2, parameter: f64) -> Point2 {
    Point2::new(
        (1.0 - parameter) * start.x + parameter * end.x,
        (1.0 - parameter) * start.y + parameter * end.y,
    )
}

fn interpolate_samples(points: &[Point2], parameter: f64) -> Point2 {
    match points {
        [] => Point2::new(f64::NAN, f64::NAN),
        [point] => *point,
        _ => {
            let clamped = parameter.clamp(0.0, 1.0);
            #[allow(clippy::cast_precision_loss)]
            let scaled = clamped * (points.len() - 1) as f64;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let lower = scaled.floor() as usize;
            if lower >= points.len() - 1 {
                return points[points.len() - 1];
            }
            #[allow(clippy::cast_precision_loss)]
            let local_parameter = scaled - lower as f64;
            interpolate(points[lower], points[lower + 1], local_parameter)
        }
    }
}

fn cumulative_lengths(points: &[Point2]) -> Vec<f64> {
    let mut lengths = Vec::with_capacity(points.len());
    let mut total = 0.0;
    lengths.push(total);
    for pair in points.windows(2) {
        total += pair[0].distance_to(pair[1]);
        lengths.push(total);
    }
    lengths
}

fn interpolate_samples_by_arc_length(
    points: &[Point2],
    cumulative_lengths: &[f64],
    parameter: f64,
) -> Point2 {
    match points {
        [] => Point2::new(f64::NAN, f64::NAN),
        [point] => *point,
        _ => {
            let total = cumulative_lengths.last().copied().unwrap_or_default();
            if !total.is_finite() || total <= f64::EPSILON {
                return interpolate_samples(points, parameter);
            }
            let target = parameter.clamp(0.0, 1.0) * total;
            let upper = cumulative_lengths.partition_point(|&length| length < target);
            if upper == 0 {
                return points[0];
            }
            if upper >= points.len() {
                return points[points.len() - 1];
            }
            let lower = upper - 1;
            let segment_length = cumulative_lengths[upper] - cumulative_lengths[lower];
            if segment_length <= f64::EPSILON {
                return points[upper];
            }
            let local_parameter = (target - cumulative_lengths[lower]) / segment_length;
            interpolate(points[lower], points[upper], local_parameter)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::f64::consts::{FRAC_PI_2, PI};

    use super::{AirfoilSurfaceCurve, CircularArcCurve, LineCurve, ParametricCurve, PolylineCurve};
    use crate::geometry::Point2;

    #[test]
    fn line_interpolation_pins_endpoints() {
        let line = LineCurve::new(Point2::new(0.0, 1.0), Point2::new(2.0, 3.0));
        assert_eq!(line.point(0.0), Point2::new(0.0, 1.0));
        assert_eq!(line.point(1.0), Point2::new(2.0, 3.0));
        assert_eq!(line.point(0.5), Point2::new(1.0, 2.0));
    }

    #[test]
    fn airfoil_parameter_uses_arc_length_not_sample_index() {
        let curve = AirfoilSurfaceCurve::new(vec![
            Point2::new(0.0, 0.0),
            Point2::new(0.1, 0.0),
            Point2::new(1.0, 0.0),
        ]);

        let midpoint = curve.point(0.5);

        assert!((midpoint.x - 0.5).abs() < 1.0e-12);
        assert!(midpoint.y.abs() < 1.0e-12);
    }

    #[test]
    fn circular_arc_follows_requested_sweep() {
        let arc = CircularArcCurve {
            center: Point2::new(0.0, 0.0),
            radius: 2.0,
            start_angle_radians: FRAC_PI_2,
            sweep_angle_radians: PI,
        };
        let end = arc.point(1.0);
        assert!(end.x.abs() < 1.0e-12);
        assert!((end.y + 2.0).abs() < 1.0e-12);
    }

    #[test]
    fn polyline_interpolates_each_segment() {
        let curve = PolylineCurve::new(vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(1.0, 1.0),
        ]);
        assert_eq!(curve.point(0.25), Point2::new(0.5, 0.0));
        assert_eq!(curve.point(0.75), Point2::new(1.0, 0.5));
    }
}
