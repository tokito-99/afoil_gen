//! Authoritative configuration validation.

use std::collections::BTreeSet;

use crate::{
    config::{FrontBackPatchType, MeshConfiguration, MeshIntent, PatchType, WakeAlignment},
    error::{ValidationError, ValidationReport},
    geometry::naca4::parse_naca4,
    les::{
        sizing::estimate_les_sizing, stretching::solve_layered_stretching,
        wall_spacing::resolve_wall_spacing,
    },
};

const CFD_NAME_RESERVED_WORDS: [&str; 7] = [
    "FoamFile",
    "boundary",
    "vertices",
    "blocks",
    "edges",
    "mergePatchPairs",
    "defaultFaces",
];

/// Validates the complete configuration without mutating or repairing it.
#[must_use]
pub fn validate_configuration(configuration: &MeshConfiguration) -> ValidationReport {
    let mut issues = Vec::new();
    validate_airfoil(configuration, &mut issues);
    validate_flow(configuration, &mut issues);
    validate_domain(configuration, &mut issues);
    validate_wake(configuration, &mut issues);
    validate_mesh_resolution(configuration, &mut issues);
    validate_optimization(configuration, &mut issues);
    validate_les(configuration, &mut issues);
    validate_extrusion(configuration, &mut issues);
    validate_boundaries(configuration, &mut issues);
    ValidationReport::new(issues)
}

fn validate_optimization(configuration: &MeshConfiguration, issues: &mut Vec<ValidationError>) {
    if configuration.optimization.smoothing_iterations > 50 {
        issues.push(ValidationError::error(
            "optimization.smoothingIterations",
            "SMOOTHING_LIMIT",
            "use between 0 and 50 safeguarded smoothing iterations",
        ));
    }
    if let Some(profile) = &configuration.les.wall_shear_profile {
        if let Err(error) = profile.validate(configuration) {
            issues.push(ValidationError::error(
                "les.wallShearProfile",
                "INVALID_WALL_PROFILE",
                error.to_string(),
            ));
        }
        if !configuration.les.estimate_first_cell_height
            || configuration
                .les
                .solver_wall_shear_stress
                .is_none_or(|tau| {
                    (tau - profile.maximum_shear()).abs() > 1e-9 * profile.maximum_shear().max(1.0)
                })
        {
            issues.push(ValidationError::error("les.wallShearProfile", "PROFILE_REFERENCE_MISMATCH", "profile mode requires estimated spacing and reference shear equal to the profile maximum"));
        }
        if !configuration.optimization.orthogonalize_wall {
            issues.push(ValidationError::error(
                "optimization.orthogonalizeWall",
                "PROFILE_REQUIRES_WALL_OPTIMIZATION",
                "enable wall orthogonalization for spatial feedback",
            ));
        }
    }
}

/// Suggests a CFD-safe identifier without changing the supplied configuration.
#[must_use]
pub fn sanitize_cfd_name(value: &str) -> String {
    let mut output = String::new();
    let mut previous_was_underscore = false;
    for character in value.trim().chars() {
        let safe = character.is_ascii_alphanumeric() || character == '_';
        let replacement = if safe { character } else { '_' };
        if replacement == '_' {
            if !previous_was_underscore {
                output.push('_');
            }
            previous_was_underscore = true;
        } else {
            output.push(replacement);
            previous_was_underscore = false;
        }
    }

    if output.as_bytes().first().is_some_and(u8::is_ascii_digit) {
        output.insert_str(0, "patch_");
    }
    if output.is_empty() {
        "patch".to_owned()
    } else {
        output
    }
}

fn validate_airfoil(configuration: &MeshConfiguration, issues: &mut Vec<ValidationError>) {
    if let Err(error) = parse_naca4(&configuration.airfoil.naca_code) {
        issues.push(ValidationError::error(
            "airfoil.nacaCode",
            "INVALID_NACA_CODE",
            error.to_string(),
        ));
    }
    positive(
        "airfoil.chordLength",
        configuration.airfoil.chord_length,
        issues,
    );
    finite(
        "airfoil.angleOfAttackDeg",
        configuration.airfoil.angle_of_attack_deg,
        issues,
    );
    if configuration.airfoil.surface_point_count < 41 {
        issues.push(ValidationError::error(
            "airfoil.surfacePointCount",
            "TOO_FEW_SURFACE_POINTS",
            "surface point count must be at least 41",
        ));
    }
}

fn validate_flow(configuration: &MeshConfiguration, issues: &mut Vec<ValidationError>) {
    positive(
        "flow.freestreamVelocity",
        configuration.flow.freestream_velocity,
        issues,
    );
    positive("flow.density", configuration.flow.density, issues);
    positive(
        "flow.dynamicViscosity",
        configuration.flow.dynamic_viscosity,
        issues,
    );
}

fn validate_domain(configuration: &MeshConfiguration, issues: &mut Vec<ValidationError>) {
    greater_than(
        "domain.upstreamRadiusChords",
        configuration.domain.upstream_radius_chords,
        1.0,
        issues,
    );
    greater_than(
        "domain.upperRadiusChords",
        configuration.domain.upper_radius_chords,
        1.0,
        issues,
    );
    greater_than(
        "domain.lowerRadiusChords",
        configuration.domain.lower_radius_chords,
        1.0,
        issues,
    );
    greater_than(
        "domain.wakeLengthChords",
        configuration.domain.wake_length_chords,
        1.0,
        issues,
    );
    positive(
        "domain.wakeHalfHeightChords",
        configuration.domain.wake_half_height_chords,
        issues,
    );
}

fn validate_wake(configuration: &MeshConfiguration, issues: &mut Vec<ValidationError>) {
    if configuration.wake.alignment == WakeAlignment::Blended {
        positive(
            "wake.blendLengthChords",
            configuration.wake.blend_length_chords,
            issues,
        );
    } else {
        finite(
            "wake.blendLengthChords",
            configuration.wake.blend_length_chords,
            issues,
        );
    }
    finite(
        "wake.customAngleDeg",
        configuration.wake.custom_angle_deg,
        issues,
    );
    let effective_angle = match configuration.wake.alignment {
        WakeAlignment::Blended | WakeAlignment::Chord => -configuration.airfoil.angle_of_attack_deg,
        WakeAlignment::Freestream => 0.0,
        WakeAlignment::Custom => configuration.wake.custom_angle_deg,
    };
    if effective_angle.is_finite() && effective_angle.abs() >= 80.0 {
        issues.push(ValidationError::error(
            "wake.customAngleDeg",
            "WAKE_ANGLE_TOO_STEEP",
            "wake alignment angle must remain between -80 and 80 degrees",
        ));
    }
}

fn validate_mesh_resolution(configuration: &MeshConfiguration, issues: &mut Vec<ValidationError>) {
    if configuration.mesh.radial_cells > 2048 {
        issues.push(ValidationError::error(
            "mesh.radialCells",
            "RADIAL_RESOURCE_LIMIT",
            "radialCells exceeds the supported limit of 2048",
        ));
    }
    if let Ok(counts) = crate::les::budget::mesh_counts(configuration) {
        if counts.quad_cells > 2_000_000 {
            issues.push(ValidationError::error(
                "mesh",
                "MESH_RESOURCE_LIMIT",
                "2D source mesh exceeds two million quads; reduce counts before generating",
            ));
        }
    } else if [
        configuration.mesh.leading_edge_cells,
        configuration.mesh.upper_surface_cells,
        configuration.mesh.lower_surface_cells,
        configuration.mesh.wake_cells,
        configuration.mesh.radial_cells,
        configuration.mesh.spanwise_cells,
    ]
    .iter()
    .all(|count| *count > 0)
    {
        issues.push(ValidationError::error(
            "mesh",
            "COUNT_OVERFLOW",
            "mesh counts overflow supported integer arithmetic",
        ));
    }
    for (field, count) in [
        (
            "mesh.upperSurfaceCells",
            configuration.mesh.upper_surface_cells,
        ),
        (
            "mesh.lowerSurfaceCells",
            configuration.mesh.lower_surface_cells,
        ),
        (
            "mesh.leadingEdgeCells",
            configuration.mesh.leading_edge_cells,
        ),
    ] {
        if count == 0 {
            issues.push(ValidationError::error(
                field,
                "CELL_COUNT_TOO_SMALL",
                "cell count must be greater than zero",
            ));
        }
    }
    if configuration.mesh.wake_cells < 4 {
        issues.push(ValidationError::error(
            "mesh.wakeCells",
            "WAKE_CELL_COUNT_TOO_SMALL",
            "wake cell count must be at least 4",
        ));
    }
    if configuration.mesh.radial_cells < 10 {
        issues.push(ValidationError::error(
            "mesh.radialCells",
            "RADIAL_CELL_COUNT_TOO_SMALL",
            "radial cell count must be at least 10",
        ));
    }
    if configuration.mesh.spanwise_cells < 1 {
        issues.push(ValidationError::error(
            "mesh.spanwiseCells",
            "SPANWISE_CELL_COUNT_TOO_SMALL",
            "spanwise cell count must be at least 1",
        ));
    }

    for (field, value) in [
        ("mesh.surfaceGrading", configuration.mesh.surface_grading),
        ("mesh.radialGrading", configuration.mesh.radial_grading),
        ("mesh.wakeGrading", configuration.mesh.wake_grading),
        ("mesh.spanwiseGrading", configuration.mesh.spanwise_grading),
    ] {
        positive(field, value, issues);
    }
}

#[allow(clippy::too_many_lines)]
fn validate_les(configuration: &MeshConfiguration, issues: &mut Vec<ValidationError>) {
    let intent = configuration.resolved_mesh_intent();
    positive("les.targetYPlus", configuration.les.target_y_plus, issues);
    positive(
        "les.targetStreamwisePlus",
        configuration.les.target_streamwise_plus,
        issues,
    );
    positive(
        "les.targetSpanwisePlus",
        configuration.les.target_spanwise_plus,
        issues,
    );
    positive(
        "les.referenceBoundaryLayerThicknessChords",
        configuration.les.reference_boundary_layer_thickness_chords,
        issues,
    );
    if configuration.les.minimum_cells_across_boundary_layer < 8 {
        issues.push(ValidationError::error(
            "les.minimumCellsAcrossBoundaryLayer",
            "BOUNDARY_LAYER_CELL_TARGET_TOO_SMALL",
            "boundary-layer cell target must be at least 8",
        ));
    }
    if configuration
        .les
        .minimum_cells_across_boundary_layer
        .checked_add(8)
        .is_none_or(|minimum| minimum > configuration.mesh.radial_cells)
    {
        issues.push(ValidationError::error(
            "les.minimumCellsAcrossBoundaryLayer",
            "WALL_LAYERS_USE_ALL_RADIAL_CELLS",
            "wall-layer cells must leave at least eight radial cells for the outer field",
        ));
    }
    at_least(
        "les.maxRadialGrowthRatio",
        configuration.les.max_radial_growth_ratio,
        1.0,
        issues,
    );
    if configuration.les.max_radial_growth_ratio.is_finite()
        && configuration.les.max_radial_growth_ratio > 1.2
    {
        issues.push(ValidationError::warning(
            "les.maxRadialGrowthRatio",
            "HIGH_WALL_LAYER_GROWTH_RATIO",
            "growth ratios above 1.2 deserve review for wall-bounded CFD",
        ));
    }

    if intent.is_les() && !configuration.extrusion.enabled {
        issues.push(ValidationError::error(
            "extrusion.enabled",
            "LES_REQUIRES_3D_EXTRUSION",
            "LES requires an enabled three-dimensional extrusion",
        ));
    }
    if intent.is_les()
        && configuration.extrusion.front_back_patch_type != FrontBackPatchType::Cyclic
    {
        issues.push(ValidationError::error(
            "extrusion.frontBackPatchType",
            "LES_REQUIRES_PERIODIC_SPAN",
            "LES requires a CYCLIC front/back pair; EMPTY produces a two-dimensional case",
        ));
    }
    if intent.is_les() && configuration.mesh.spanwise_cells < 2 {
        issues.push(ValidationError::error(
            "mesh.spanwiseCells",
            "LES_REQUIRES_MULTIPLE_SPANWISE_CELLS",
            "LES requires at least two spanwise cells",
        ));
    }
    if intent.is_les()
        && is_positive(configuration.airfoil.chord_length)
        && is_positive(configuration.extrusion.spanwise_thickness)
        && configuration.extrusion.spanwise_thickness / configuration.airfoil.chord_length
            < 0.1 - 1.0e-12
    {
        issues.push(ValidationError::warning(
            "extrusion.spanwiseThickness",
            "SHORT_LES_SPAN",
            "periodic LES span is below 0.1 chord; use at least 0.1c as an initial screening width and verify two-point correlations with a wider-span study",
        ));
    }

    match intent {
        MeshIntent::Wrles => {
            if configuration.les.target_y_plus > 2.0 {
                issues.push(ValidationError::warning(
                    "les.targetYPlus",
                    "HIGH_WRLES_Y_PLUS",
                    "WRLES normally starts with a wall-normal target near y-plus 1",
                ));
            }
            if configuration.les.target_streamwise_plus > 150.0 {
                issues.push(ValidationError::warning(
                    "les.targetStreamwisePlus",
                    "COARSE_WRLES_STREAMWISE_TARGET",
                    "WRLES streamwise targets above 150 wall units deserve review",
                ));
            }
            if configuration.les.target_spanwise_plus > 40.0 {
                issues.push(ValidationError::warning(
                    "les.targetSpanwisePlus",
                    "COARSE_WRLES_SPANWISE_TARGET",
                    "WRLES spanwise targets above 40 wall units deserve review",
                ));
            }
        }
        MeshIntent::Wmles => {
            if configuration.les.target_y_plus < 10.0 {
                issues.push(ValidationError::warning(
                    "les.targetYPlus",
                    "LOW_WMLES_EXCHANGE_HEIGHT",
                    "WMLES first-cell placement must match the selected wall model; very low y-plus removes much of its cost benefit",
                ));
            }
        }
        MeshIntent::RansWallResolved => {
            if configuration.les.target_y_plus > 2.0 {
                issues.push(ValidationError::warning(
                    "les.targetYPlus",
                    "HIGH_WALL_RESOLVED_RANS_Y_PLUS",
                    "wall-resolved RANS normally targets the first cell near y-plus 1",
                ));
            }
        }
        MeshIntent::RansWallFunction => {
            if configuration.les.target_y_plus < 30.0 || configuration.les.target_y_plus > 300.0 {
                issues.push(ValidationError::warning(
                    "les.targetYPlus",
                    "RANS_WALL_FUNCTION_Y_PLUS_RANGE",
                    "a conventional wall-function starting point is y-plus 30 to 300; verify the selected solver wall treatment",
                ));
            }
        }
    }

    if !configuration.les.estimate_first_cell_height {
        match configuration.les.manual_first_cell_height {
            Some(value) => positive("les.manualFirstCellHeight", value, issues),
            None => issues.push(ValidationError::error(
                "les.manualFirstCellHeight",
                "MANUAL_FIRST_CELL_HEIGHT_REQUIRED",
                "manual first-cell height is required when estimation is disabled",
            )),
        }
    }

    if let Some(wall_shear_stress) = configuration.les.solver_wall_shear_stress {
        positive("les.solverWallShearStress", wall_shear_stress, issues);
    }

    let wall_spacing_message = if configuration.les.solver_wall_shear_stress.is_some() {
        "first-cell sizing uses solver wall-shear feedback; repeat the y-plus check after remeshing"
    } else {
        "the pre-solution flat-plate wall-spacing estimate does not guarantee solver-computed y-plus; calibrate it with airfoil-wall shear from a matching solution"
    };
    issues.push(ValidationError::warning(
        "les.targetYPlus",
        "Y_PLUS_IS_AN_ESTIMATE",
        wall_spacing_message,
    ));

    let wall_spacing_inputs_valid = if configuration.les.estimate_first_cell_height {
        is_positive(configuration.derived_reynolds_number())
            && is_positive(configuration.flow.freestream_velocity)
            && is_positive(configuration.flow.density)
            && is_positive(configuration.flow.dynamic_viscosity)
            && is_positive(configuration.les.target_y_plus)
    } else {
        configuration
            .les
            .manual_first_cell_height
            .is_some_and(is_positive)
    };

    if wall_spacing_inputs_valid
        && is_positive(configuration.airfoil.chord_length)
        && is_positive(configuration.domain.upper_radius_chords)
        && is_positive(configuration.domain.lower_radius_chords)
        && configuration.mesh.radial_cells > 0
        && configuration.mesh.radial_cells <= 2048
        && configuration.les.max_radial_growth_ratio.is_finite()
        && configuration.les.max_radial_growth_ratio >= 1.0
    {
        match resolve_wall_spacing(configuration).and_then(|wall_spacing| {
            let radial_extent = configuration.airfoil.chord_length
                * configuration
                    .domain
                    .upper_radius_chords
                    .min(configuration.domain.lower_radius_chords);
            let wall_layer_thickness = configuration.airfoil.chord_length
                * configuration.les.reference_boundary_layer_thickness_chords;
            solve_layered_stretching(
                wall_spacing.first_cell_height,
                radial_extent,
                configuration.mesh.radial_cells,
                wall_layer_thickness,
                configuration.les.minimum_cells_across_boundary_layer,
                configuration.les.max_radial_growth_ratio,
            )
        }) {
            Ok(_) => {}
            Err(error) => issues.push(ValidationError::error(
                "mesh.radialCells",
                "INFEASIBLE_RADIAL_STRETCHING",
                error.to_string(),
            )),
        }
    }

    if intent.is_les()
        && let Ok(sizing) = estimate_les_sizing(configuration)
    {
        if intent == MeshIntent::Wrles
            && sizing.estimated_maximum_surface_streamwise_plus
                > 1.1 * configuration.les.target_streamwise_plus
        {
            issues.push(ValidationError::warning(
                "mesh.upperSurfaceCells",
                "STREAMWISE_LES_TARGET_NOT_MET",
                format!(
                    "estimated maximum wall spacing is s-plus {:.1}; recommended leading/upper/lower counts are approximately {}/{}/{}",
                    sizing.estimated_maximum_surface_streamwise_plus,
                    sizing.recommended_leading_edge_cells,
                    sizing.recommended_upper_surface_cells,
                    sizing.recommended_lower_surface_cells
                ),
            ));
        }
        if intent == MeshIntent::Wrles
            && sizing.estimated_maximum_spanwise_plus > 1.1 * configuration.les.target_spanwise_plus
        {
            issues.push(ValidationError::warning(
                "mesh.spanwiseCells",
                "SPANWISE_LES_TARGET_NOT_MET",
                format!(
                    "estimated maximum spanwise spacing is z-plus {:.1}; approximately {} cells are needed for the target",
                    sizing.estimated_maximum_spanwise_plus, sizing.recommended_spanwise_cells
                ),
            ));
        }
        if sizing.radial_cells_within_boundary_layer
            < configuration.les.minimum_cells_across_boundary_layer
        {
            issues.push(ValidationError::warning(
                "mesh.radialCells",
                "BOUNDARY_LAYER_CELL_TARGET_NOT_MET",
                format!(
                    "{} radial cell centers lie within the reference boundary layer; target is {}",
                    sizing.radial_cells_within_boundary_layer,
                    configuration.les.minimum_cells_across_boundary_layer
                ),
            ));
        }
        if configuration.extrusion.spanwise_thickness
            < 2.0 * sizing.reference_boundary_layer_thickness
        {
            issues.push(ValidationError::warning(
                "extrusion.spanwiseThickness",
                "NARROW_PERIODIC_SPAN",
                "periodic span is less than twice the reference boundary-layer thickness; verify span independence",
            ));
        }
    }
}

fn validate_extrusion(configuration: &MeshConfiguration, issues: &mut Vec<ValidationError>) {
    if configuration.extrusion.enabled {
        positive(
            "extrusion.spanwiseThickness",
            configuration.extrusion.spanwise_thickness,
            issues,
        );
    }
    if configuration.extrusion.front_back_patch_type == FrontBackPatchType::Empty
        && configuration.mesh.spanwise_cells > 1
    {
        issues.push(ValidationError::error(
            "mesh.spanwiseCells",
            "EMPTY_REQUIRES_ONE_SPANWISE_CELL",
            "EMPTY front/back patches require exactly one spanwise cell",
        ));
    }
    let expected_patch_type = match configuration.extrusion.front_back_patch_type {
        FrontBackPatchType::Empty => PatchType::Empty,
        FrontBackPatchType::Symmetry => PatchType::SymmetryPlane,
        FrontBackPatchType::Cyclic => PatchType::Cyclic,
        FrontBackPatchType::Wall => PatchType::Wall,
    };
    if configuration.boundaries.front.patch_type != expected_patch_type
        || configuration.boundaries.back.patch_type != expected_patch_type
    {
        issues.push(ValidationError::error(
            "boundaries.front",
            "FRONT_BACK_PATCH_TYPE_MISMATCH",
            format!(
                "front and back boundary patch types must match frontBackPatchType ({expected_patch_type:?})"
            ),
        ));
    }
}

fn validate_boundaries(configuration: &MeshConfiguration, issues: &mut Vec<ValidationError>) {
    let mut names = BTreeSet::new();
    for (field, boundary) in configuration.boundaries.ordered() {
        let name_field = format!("{field}.name");
        if !is_cfd_safe_name(&boundary.name) {
            issues.push(ValidationError::error(
                &name_field,
                "INVALID_CFD_NAME",
                format!(
                    "boundary name must match ^[A-Za-z_][A-Za-z0-9_]*$; suggested name: {}",
                    sanitize_cfd_name(&boundary.name)
                ),
            ));
        } else if CFD_NAME_RESERVED_WORDS.contains(&boundary.name.as_str()) {
            issues.push(ValidationError::error(
                &name_field,
                "RESERVED_CFD_NAME",
                "boundary name is reserved by the OpenFOAM dictionary format",
            ));
        }
        if !names.insert(boundary.name.as_str()) {
            issues.push(ValidationError::error(
                name_field,
                "DUPLICATE_BOUNDARY_NAME",
                "boundary names must be unique",
            ));
        }
    }
    if configuration.boundaries.airfoil.patch_type != PatchType::Wall {
        issues.push(ValidationError::warning(
            "boundaries.airfoil.patchType",
            "AIRFOIL_NOT_WALL",
            "the airfoil patch is normally classified as WALL",
        ));
    }
}

fn is_cfd_safe_name(value: &str) -> bool {
    let mut characters = value.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn positive(field: &str, value: f64, issues: &mut Vec<ValidationError>) {
    if !is_positive(value) {
        issues.push(ValidationError::error(
            field,
            "MUST_BE_POSITIVE",
            "value must be finite and greater than zero",
        ));
    }
}

fn is_positive(value: f64) -> bool {
    value.is_finite() && value > 0.0
}

fn finite(field: &str, value: f64, issues: &mut Vec<ValidationError>) {
    if !value.is_finite() {
        issues.push(ValidationError::error(
            field,
            "MUST_BE_FINITE",
            "value must be finite",
        ));
    }
}

fn greater_than(field: &str, value: f64, minimum: f64, issues: &mut Vec<ValidationError>) {
    if !value.is_finite() || value <= minimum {
        issues.push(ValidationError::error(
            field,
            "VALUE_TOO_SMALL",
            format!("value must be finite and greater than {minimum}"),
        ));
    }
}

fn at_least(field: &str, value: f64, minimum: f64, issues: &mut Vec<ValidationError>) {
    if !value.is_finite() || value < minimum {
        issues.push(ValidationError::error(
            field,
            "VALUE_TOO_SMALL",
            format!("value must be finite and at least {minimum}"),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::{sanitize_cfd_name, validate_configuration};
    use crate::{
        config::{FrontBackPatchType, MeshConfiguration, PatchType},
        error::ValidationSeverity,
        test_support::example_configuration,
    };

    fn has_code(configuration: &MeshConfiguration, code: &str) -> bool {
        validate_configuration(configuration)
            .issues
            .iter()
            .any(|issue| issue.code == code)
    }

    #[test]
    fn default_example_is_valid_with_engineering_warning() {
        let report = validate_configuration(&example_configuration());
        assert!(report.valid);
        assert!(report.issues.iter().any(|issue| {
            issue.code == "Y_PLUS_IS_AN_ESTIMATE" && issue.severity == ValidationSeverity::Warning
        }));
    }

    #[test]
    fn stored_reynolds_value_does_not_override_physical_properties() {
        let mut configuration = example_configuration();
        configuration.flow.reynolds_number = 0.0;

        let report = validate_configuration(&configuration);

        assert!(report.valid, "{:?}", report.issues);
    }

    #[test]
    fn invalid_naca_and_numerics_are_reported() {
        let mut configuration = example_configuration();
        configuration.airfoil.naca_code = "0000".to_owned();
        configuration.flow.density = 0.0;
        configuration.mesh.radial_cells = 3;
        let report = validate_configuration(&configuration);
        assert!(!report.valid);
        assert!(has_code(&configuration, "INVALID_NACA_CODE"));
        assert!(has_code(&configuration, "MUST_BE_POSITIVE"));
        assert!(has_code(&configuration, "RADIAL_CELL_COUNT_TOO_SMALL"));
    }

    #[test]
    fn duplicate_and_unsafe_names_are_rejected() {
        let mut configuration = example_configuration();
        configuration.boundaries.outlet.name = configuration.boundaries.inlet.name.clone();
        configuration.boundaries.front.name = "1 bad-name".to_owned();
        assert!(has_code(&configuration, "DUPLICATE_BOUNDARY_NAME"));
        assert!(has_code(&configuration, "INVALID_CFD_NAME"));
    }

    #[test]
    fn sanitizer_is_predictable_but_non_mutating() {
        assert_eq!(
            sanitize_cfd_name(" 1 bad--name.txt "),
            "patch_1_bad_name_txt"
        );
        assert_eq!(sanitize_cfd_name(""), "patch");
        assert_eq!(sanitize_cfd_name("already_safe"), "already_safe");
    }

    #[test]
    fn manual_first_cell_height_is_required_and_accepted() {
        let mut configuration = example_configuration();
        configuration.les.estimate_first_cell_height = false;
        configuration.les.manual_first_cell_height = None;
        assert!(has_code(
            &configuration,
            "MANUAL_FIRST_CELL_HEIGHT_REQUIRED"
        ));

        configuration.les.manual_first_cell_height = Some(1.0e-5);
        assert!(!has_code(
            &configuration,
            "MANUAL_FIRST_CELL_HEIGHT_REQUIRED"
        ));
    }

    #[test]
    fn empty_direction_requires_one_cell() {
        let mut configuration = example_configuration();
        configuration.mesh.spanwise_cells = 2;
        configuration.extrusion.front_back_patch_type = FrontBackPatchType::Empty;
        configuration.boundaries.front.patch_type = PatchType::Empty;
        configuration.boundaries.back.patch_type = PatchType::Empty;
        assert!(has_code(&configuration, "EMPTY_REQUIRES_ONE_SPANWISE_CELL"));
    }
}
