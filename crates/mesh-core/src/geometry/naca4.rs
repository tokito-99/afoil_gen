//! Standard analytical NACA 4-digit airfoil geometry.

use serde::{Deserialize, Serialize};

use crate::{
    config::AirfoilConfiguration,
    error::MeshError,
    geometry::{
        Point2,
        spacing::cosine_clustered,
        transform::{rotate_about, rotate_points},
    },
};

const CLOSED_TRAILING_EDGE_COEFFICIENT: f64 = 0.1036;
const FINITE_TRAILING_EDGE_COEFFICIENT: f64 = 0.1015;

/// Parsed, normalized NACA 4-digit parameters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Naca4Parameters {
    /// Maximum camber as a chord fraction.
    pub maximum_camber: f64,
    /// Maximum-camber position as a chord fraction.
    pub camber_position: f64,
    /// Maximum thickness as a chord fraction.
    pub thickness: f64,
}

/// Generated upper/lower surfaces and a deterministic perimeter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AirfoilGeometry {
    /// Upper surface from leading edge to trailing edge.
    pub upper: Vec<Point2>,
    /// Lower surface from leading edge to trailing edge.
    pub lower: Vec<Point2>,
    /// Trailing-edge upper surface to leading edge, then lower surface to trailing edge.
    pub perimeter: Vec<Point2>,
    /// Quarter-chord rotation point.
    pub rotation_pivot: Point2,
}

/// Parses a four-digit NACA designation.
///
/// # Errors
///
/// Returns [`MeshError::InvalidNacaCode`] for malformed codes, zero thickness,
/// or a cambered designation with a zero camber position.
pub fn parse_naca4(code: &str) -> Result<Naca4Parameters, MeshError> {
    if code.len() != 4 || !code.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(MeshError::InvalidNacaCode(
            "expected exactly four ASCII digits".to_owned(),
        ));
    }

    let digits = code.as_bytes();
    let maximum_camber_digit = digits[0] - b'0';
    let camber_position_digit = digits[1] - b'0';
    let thickness_digits = (digits[2] - b'0') * 10 + (digits[3] - b'0');

    if thickness_digits == 0 {
        return Err(MeshError::InvalidNacaCode(
            "thickness digits must be greater than zero".to_owned(),
        ));
    }
    if maximum_camber_digit > 0 && camber_position_digit == 0 {
        return Err(MeshError::InvalidNacaCode(
            "a cambered airfoil requires a nonzero camber position".to_owned(),
        ));
    }

    Ok(Naca4Parameters {
        maximum_camber: f64::from(maximum_camber_digit) / 100.0,
        camber_position: f64::from(camber_position_digit) / 10.0,
        thickness: f64::from(thickness_digits) / 100.0,
    })
}

/// Generates cosine-clustered NACA surfaces and applies angle of attack.
///
/// Positive aerodynamic incidence raises the leading edge and lowers the
/// trailing edge while the freestream remains aligned with global positive x.
///
/// # Errors
///
/// Returns an error when the NACA code is invalid or a required geometry input
/// is non-finite or outside its supported range.
pub fn generate_airfoil(config: &AirfoilConfiguration) -> Result<AirfoilGeometry, MeshError> {
    let parameters = parse_naca4(&config.naca_code)?;
    if !config.chord_length.is_finite() || config.chord_length <= 0.0 {
        return Err(MeshError::GeometryError(
            "chord length must be finite and positive".to_owned(),
        ));
    }
    if config.surface_point_count < 2 {
        return Err(MeshError::GeometryError(
            "surface point count must be at least two".to_owned(),
        ));
    }
    if !config.angle_of_attack_deg.is_finite() {
        return Err(MeshError::GeometryError(
            "angle of attack must be finite".to_owned(),
        ));
    }

    let x_values = cosine_clustered(config.surface_point_count)?;
    let trailing_edge_coefficient = if config.finite_trailing_edge {
        FINITE_TRAILING_EDGE_COEFFICIENT
    } else {
        CLOSED_TRAILING_EDGE_COEFFICIENT
    };

    let mut upper = Vec::with_capacity(config.surface_point_count);
    let mut lower = Vec::with_capacity(config.surface_point_count);
    for x_bar in x_values {
        let (camber, slope) = camber_and_slope(parameters, config.chord_length, x_bar);
        let thickness = half_thickness(
            parameters.thickness,
            config.chord_length,
            x_bar,
            trailing_edge_coefficient,
        );
        let theta = slope.atan();
        let x = x_bar * config.chord_length;
        let (sin_theta, cos_theta) = theta.sin_cos();

        upper.push(Point2::new(
            x - thickness * sin_theta,
            camber + thickness * cos_theta,
        ));
        lower.push(Point2::new(
            x + thickness * sin_theta,
            camber - thickness * cos_theta,
        ));
    }

    let rotation_pivot = Point2::new(0.25 * config.chord_length, 0.0);
    // Mathematical positive rotation is counter-clockwise, but positive
    // aerodynamic angle of attack is nose-up for a left-to-right freestream.
    let angle_radians = -config.angle_of_attack_deg.to_radians();
    let upper = rotate_points(&upper, rotation_pivot, angle_radians);
    let lower = rotate_points(&lower, rotation_pivot, angle_radians);
    let perimeter = build_perimeter(&upper, &lower, config.finite_trailing_edge);

    Ok(AirfoilGeometry {
        upper,
        lower,
        perimeter,
        rotation_pivot: rotate_about(rotation_pivot, rotation_pivot, angle_radians),
    })
}

fn camber_and_slope(parameters: Naca4Parameters, chord: f64, x_bar: f64) -> (f64, f64) {
    let m = parameters.maximum_camber;
    let p = parameters.camber_position;
    if m == 0.0 {
        return (0.0, 0.0);
    }

    if x_bar < p {
        let camber = m * chord / p.powi(2) * (2.0 * p * x_bar - x_bar.powi(2));
        let slope = 2.0 * m / p.powi(2) * (p - x_bar);
        (camber, slope)
    } else {
        let denominator = (1.0 - p).powi(2);
        let camber = m * chord / denominator * ((1.0 - 2.0 * p) + 2.0 * p * x_bar - x_bar.powi(2));
        let slope = 2.0 * m / denominator * (p - x_bar);
        (camber, slope)
    }
}

fn half_thickness(thickness: f64, chord: f64, x_bar: f64, a4: f64) -> f64 {
    5.0 * thickness
        * chord
        * (0.2969 * x_bar.sqrt() - 0.1260 * x_bar - 0.3516 * x_bar.powi(2) + 0.2843 * x_bar.powi(3)
            - a4 * x_bar.powi(4))
}

fn build_perimeter(upper: &[Point2], lower: &[Point2], finite_trailing_edge: bool) -> Vec<Point2> {
    let trailing_edge_extra = usize::from(finite_trailing_edge);
    let mut perimeter = Vec::with_capacity(upper.len() + lower.len() - 2 + trailing_edge_extra);
    perimeter.extend(upper.iter().rev().copied());

    let lower_end = if finite_trailing_edge {
        lower.len()
    } else {
        lower.len().saturating_sub(1)
    };
    perimeter.extend(lower[1..lower_end].iter().copied());
    perimeter
}

#[cfg(test)]
mod tests {
    use super::{generate_airfoil, parse_naca4};
    use crate::config::AirfoilConfiguration;

    const TOLERANCE: f64 = 1.0e-11;

    fn config(code: &str) -> AirfoilConfiguration {
        AirfoilConfiguration {
            naca_code: code.to_owned(),
            chord_length: 1.0,
            angle_of_attack_deg: 0.0,
            finite_trailing_edge: false,
            surface_point_count: 161,
        }
    }

    #[test]
    fn parses_symmetric_and_cambered_designations() {
        let symmetric = parse_naca4("0012").expect("valid NACA code");
        assert!((symmetric.maximum_camber - 0.0).abs() < TOLERANCE);
        assert!((symmetric.thickness - 0.12).abs() < TOLERANCE);

        let cambered = parse_naca4("2412").expect("valid NACA code");
        assert!((cambered.maximum_camber - 0.02).abs() < TOLERANCE);
        assert!((cambered.camber_position - 0.4).abs() < TOLERANCE);
    }

    #[test]
    fn rejects_invalid_designations() {
        for code in ["0000", "2012", "12A4", "012", "00120"] {
            assert!(parse_naca4(code).is_err(), "{code} should be invalid");
        }
    }

    #[test]
    fn naca0012_is_symmetric_at_zero_incidence() {
        let geometry = generate_airfoil(&config("0012")).expect("valid geometry");
        assert_eq!(geometry.upper.len(), 161);
        assert_eq!(geometry.lower.len(), 161);
        assert_eq!(geometry.perimeter.len(), 320);

        for (upper, lower) in geometry.upper.iter().zip(&geometry.lower) {
            assert!((upper.x - lower.x).abs() < TOLERANCE);
            assert!((upper.y + lower.y).abs() < TOLERANCE);
        }
    }

    #[test]
    fn naca2412_has_positive_camber() {
        let geometry = generate_airfoil(&config("2412")).expect("valid geometry");
        let middle = geometry.upper.len() / 2;
        let mean_y = 0.5 * (geometry.upper[middle].y + geometry.lower[middle].y);
        assert!(mean_y > 0.0);
    }

    #[test]
    fn leading_edge_and_closed_trailing_edge_are_continuous() {
        let geometry = generate_airfoil(&config("0012")).expect("valid geometry");
        assert!(geometry.upper[0].distance_to(geometry.lower[0]) < TOLERANCE);
        let last = geometry.upper.len() - 1;
        assert!(geometry.upper[last].distance_to(geometry.lower[last]) < TOLERANCE);
        assert!(geometry.upper[0].distance_to(crate::geometry::Point2::new(0.0, 0.0)) < TOLERANCE);
    }

    #[test]
    fn finite_trailing_edge_keeps_distinct_surface_points() {
        let mut input = config("0012");
        input.finite_trailing_edge = true;
        let geometry = generate_airfoil(&input).expect("valid geometry");
        let last = geometry.upper.len() - 1;
        assert!(geometry.upper[last].distance_to(geometry.lower[last]) > 1.0e-4);
        assert_eq!(geometry.perimeter.len(), 321);
    }

    #[test]
    fn chordwise_coordinates_are_monotonic_before_perimeter_reordering() {
        let geometry = generate_airfoil(&config("0012")).expect("valid geometry");
        assert!(geometry.upper.windows(2).all(|pair| pair[0].x < pair[1].x));
        assert!(geometry.lower.windows(2).all(|pair| pair[0].x < pair[1].x));
    }

    #[test]
    fn angle_of_attack_rotates_about_quarter_chord() {
        let mut input = config("0012");
        let zero = generate_airfoil(&input).expect("valid geometry");
        input.angle_of_attack_deg = 17.0;
        let rotated = generate_airfoil(&input).expect("valid geometry");
        for (before, after) in zero.upper.iter().zip(&rotated.upper) {
            let before_radius = before.distance_to(zero.rotation_pivot);
            let after_radius = after.distance_to(rotated.rotation_pivot);
            assert!((before_radius - after_radius).abs() < TOLERANCE);
        }
        assert!(rotated.upper[0].y > zero.upper[0].y);
        assert!(
            rotated.upper.last().expect("trailing-edge point").y
                < zero.upper.last().expect("trailing-edge point").y
        );
    }
}
