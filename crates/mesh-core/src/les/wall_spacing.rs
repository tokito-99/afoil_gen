//! Flat-plate first-cell-height estimate for preliminary mesh design.

use serde::{Deserialize, Serialize};

use crate::{config::MeshConfiguration, error::MeshError};

/// Intermediate and final values from the wall-spacing estimate.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WallSpacingEstimate {
    pub skin_friction_coefficient: f64,
    pub wall_shear_stress: f64,
    pub friction_velocity: f64,
    pub first_cell_height: f64,
}

/// The selected first-cell height and its optional engineering estimate.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedWallSpacing {
    pub first_cell_height: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimate: Option<WallSpacingEstimate>,
}

/// Estimates first-cell height from target y-plus using a flat-plate correlation.
///
/// The `Cf = 0.058 Re^-0.2` correlation is an engineering estimate. It cannot
/// guarantee the solver-computed y-plus or LES adequacy.
///
/// # Errors
///
/// Returns [`MeshError::GeometryError`] when any physical input is non-finite
/// or non-positive.
pub fn estimate_first_cell_height(
    reynolds_number: f64,
    freestream_velocity: f64,
    density: f64,
    dynamic_viscosity: f64,
    target_y_plus: f64,
) -> Result<WallSpacingEstimate, MeshError> {
    for (name, value) in [
        ("Reynolds number", reynolds_number),
        ("freestream velocity", freestream_velocity),
        ("density", density),
        ("dynamic viscosity", dynamic_viscosity),
        ("target y-plus", target_y_plus),
    ] {
        if !value.is_finite() || value <= 0.0 {
            return Err(MeshError::GeometryError(format!(
                "{name} must be finite and positive"
            )));
        }
    }

    // Turbulent flat-plate correlation requested by the project specification.
    let skin_friction_coefficient = 0.058 * reynolds_number.powf(-0.2);
    let wall_shear_stress = 0.5 * skin_friction_coefficient * density * freestream_velocity.powi(2);
    let friction_velocity = (wall_shear_stress / density).sqrt();
    let first_cell_height = target_y_plus * dynamic_viscosity / (density * friction_velocity);

    Ok(WallSpacingEstimate {
        skin_friction_coefficient,
        wall_shear_stress,
        friction_velocity,
        first_cell_height,
    })
}

/// Converts solver-reported maximum wall shear into a target first-cell height.
///
/// This is the preferred remeshing path because it uses the wall stress from the
/// actual airfoil pressure gradient, incidence, transition, and separation state.
/// The feedback remains specific to the flow condition and turbulence model that
/// produced it.
///
/// # Errors
///
/// Returns [`MeshError::GeometryError`] when a physical input is non-finite or
/// non-positive.
pub fn estimate_first_cell_height_from_wall_shear(
    wall_shear_stress: f64,
    freestream_velocity: f64,
    density: f64,
    dynamic_viscosity: f64,
    target_y_plus: f64,
) -> Result<WallSpacingEstimate, MeshError> {
    for (name, value) in [
        ("solver wall shear stress", wall_shear_stress),
        ("freestream velocity", freestream_velocity),
        ("density", density),
        ("dynamic viscosity", dynamic_viscosity),
        ("target y-plus", target_y_plus),
    ] {
        if !value.is_finite() || value <= 0.0 {
            return Err(MeshError::GeometryError(format!(
                "{name} must be finite and positive"
            )));
        }
    }

    let skin_friction_coefficient =
        2.0 * wall_shear_stress / (density * freestream_velocity.powi(2));
    let friction_velocity = (wall_shear_stress / density).sqrt();
    let first_cell_height = target_y_plus * dynamic_viscosity / (density * friction_velocity);

    Ok(WallSpacingEstimate {
        skin_friction_coefficient,
        wall_shear_stress,
        friction_velocity,
        first_cell_height,
    })
}

/// Resolves estimated or manually supplied first-cell height from a configuration.
///
/// # Errors
///
/// Returns an error when estimation inputs are invalid or manual mode lacks a
/// finite, positive first-cell height.
pub fn resolve_wall_spacing(
    configuration: &MeshConfiguration,
) -> Result<ResolvedWallSpacing, MeshError> {
    if configuration.les.estimate_first_cell_height {
        let estimate = if let Some(wall_shear_stress) = configuration.les.solver_wall_shear_stress {
            estimate_first_cell_height_from_wall_shear(
                wall_shear_stress,
                configuration.flow.freestream_velocity,
                configuration.flow.density,
                configuration.flow.dynamic_viscosity,
                configuration.les.target_y_plus,
            )?
        } else {
            estimate_first_cell_height(
                configuration.derived_reynolds_number(),
                configuration.flow.freestream_velocity,
                configuration.flow.density,
                configuration.flow.dynamic_viscosity,
                configuration.les.target_y_plus,
            )?
        };
        Ok(ResolvedWallSpacing {
            first_cell_height: estimate.first_cell_height
                * if configuration.optimization.target_cell_center_y_plus {
                    2.0
                } else {
                    1.0
                },
            estimate: Some(estimate),
        })
    } else {
        let manual = configuration.les.manual_first_cell_height.ok_or_else(|| {
            MeshError::GeometryError(
                "manual first-cell height is required when estimation is disabled".to_owned(),
            )
        })?;
        if !manual.is_finite() || manual <= 0.0 {
            return Err(MeshError::GeometryError(
                "manual first-cell height must be finite and positive".to_owned(),
            ));
        }
        Ok(ResolvedWallSpacing {
            first_cell_height: manual,
            estimate: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        estimate_first_cell_height, estimate_first_cell_height_from_wall_shear,
        resolve_wall_spacing,
    };
    use crate::test_support::example_configuration;

    #[test]
    fn estimate_matches_reference_calculation() {
        let estimate = estimate_first_cell_height(1_000_000.0, 30.0, 1.225, 0.000_018_1, 1.0)
            .expect("valid properties");
        let expected_cf = 0.058 * 1_000_000.0_f64.powf(-0.2);
        let expected_tau = 0.5 * expected_cf * 1.225 * 30.0_f64.powi(2);
        let expected_u_tau = (expected_tau / 1.225).sqrt();
        let expected_height = 0.000_018_1 / (1.225 * expected_u_tau);

        assert!((estimate.skin_friction_coefficient - expected_cf).abs() < 1.0e-15);
        assert!((estimate.wall_shear_stress - expected_tau).abs() < 1.0e-15);
        assert!((estimate.friction_velocity - expected_u_tau).abs() < 1.0e-15);
        assert!((estimate.first_cell_height - expected_height).abs() < 1.0e-15);
        assert!(estimate.first_cell_height > 0.0);
    }

    #[test]
    fn invalid_physical_properties_are_rejected() {
        assert!(estimate_first_cell_height(0.0, 30.0, 1.225, 0.000_018_1, 1.0).is_err());
        assert!(estimate_first_cell_height(1.0e6, -1.0, 1.225, 0.000_018_1, 1.0).is_err());
        assert!(estimate_first_cell_height(1.0e6, 30.0, f64::NAN, 0.000_018_1, 1.0).is_err());
    }

    #[test]
    fn manual_mode_returns_the_supplied_height_without_an_estimate() {
        let mut configuration = example_configuration();
        configuration.les.estimate_first_cell_height = false;
        configuration.les.manual_first_cell_height = Some(2.5e-5);
        let resolved = resolve_wall_spacing(&configuration).expect("valid manual spacing");
        assert!((resolved.first_cell_height - 2.5e-5).abs() < f64::EPSILON);
        assert!(resolved.estimate.is_none());
    }

    #[test]
    fn configured_reynolds_value_does_not_override_physical_properties() {
        let mut configuration = example_configuration();
        configuration.flow.reynolds_number = 42.0;
        let resolved = resolve_wall_spacing(&configuration).expect("derived Reynolds spacing");
        let expected = estimate_first_cell_height(
            configuration.derived_reynolds_number(),
            configuration.flow.freestream_velocity,
            configuration.flow.density,
            configuration.flow.dynamic_viscosity,
            configuration.les.target_y_plus,
        )
        .expect("reference spacing");
        assert_eq!(
            resolved.estimate.expect("estimated spacing"),
            expected,
            "the physical properties are authoritative"
        );
    }

    #[test]
    fn solver_wall_shear_feedback_controls_first_cell_height() {
        let estimate =
            estimate_first_cell_height_from_wall_shear(8.0, 25.0, 1.225, 0.000_018_1, 1.0)
                .expect("valid solver feedback");
        let expected = 0.000_018_1 / (1.225_f64 * (8.0_f64 / 1.225).sqrt());
        assert!((estimate.first_cell_height - expected).abs() < 1.0e-15);

        let mut configuration = example_configuration();
        configuration.les.solver_wall_shear_stress = Some(8.0);
        let resolved = resolve_wall_spacing(&configuration).expect("feedback spacing");
        assert!((resolved.first_cell_height - expected).abs() < 1.0e-15);
    }
}
