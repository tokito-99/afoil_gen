use mesh_core::{
    BudgetConstraints, PreviewLevel, generate_mesh_preview, load_configuration, mesh_counts,
    plan_cell_budget,
    studies::{StudyAxis, analyze_study, prepare_study},
};
use std::{fmt::Write as _, path::PathBuf};

fn configuration() -> mesh_core::MeshConfiguration {
    load_configuration(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/naca0012-student-wrles.json"),
    )
    .unwrap()
}

fn rans_configuration() -> mesh_core::MeshConfiguration {
    load_configuration(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/naca2412-rans.json"),
    )
    .unwrap()
}

#[test]
fn optimization_is_positive_deterministic_and_keeps_boundaries_exact() {
    let original = configuration();
    let baseline = generate_mesh_preview(&original, PreviewLevel::CoarseMesh).unwrap();
    let mut centered = original.clone();
    centered.optimization.target_cell_center_y_plus = true;
    let centered_preview = generate_mesh_preview(&centered, PreviewLevel::GeometryOnly).unwrap();
    let baseline_height = baseline.wall_spacing.as_ref().unwrap().first_cell_height;
    assert!(
        (centered_preview
            .wall_spacing
            .as_ref()
            .unwrap()
            .first_cell_height
            / baseline_height
            - 2.0)
            .abs()
            < 1e-12
    );
    let mut optimized = original.clone();
    optimized.optimization.orthogonalize_wall = true;
    optimized.optimization.smoothing_iterations = 2;
    let preview = generate_mesh_preview(&optimized, PreviewLevel::CoarseMesh).unwrap();
    assert_eq!(preview.boundary_patches, baseline.boundary_patches);
    assert_eq!(preview.quality.cell_count, baseline.quality.cell_count);
    assert!(preview.quality.valid);
    let report = preview.optimization.as_ref().unwrap();
    assert!(
        report.final_minimum_orthogonal_quality + 1e-12
            >= report.initial_minimum_orthogonal_quality
    );
    assert!(report.final_minimum_scaled_jacobian + 1e-12 >= report.initial_minimum_scaled_jacobian);
    assert!(report.maximum_displacement > 0.0, "{report:?}");
    assert_eq!(
        preview,
        generate_mesh_preview(&optimized, PreviewLevel::CoarseMesh).unwrap()
    );
    assert!(mesh_core::export_openfoam(&optimized).is_err());
}

#[test]
fn spatial_feedback_maps_states_and_rejects_stale_geometry() {
    let original = configuration();
    let csv = "side,xOverChord,wallShearStressPa,signedWallShearStressPa,intermittency\nUPPER,0,1,1,0\nUPPER,0.5,2,2,0.5\nUPPER,1,1,-1,1\nLOWER,0,1,1,0\nLOWER,1,1,1,1";
    let calibrated = mesh_core::les::profile::import_wall_shear_profile(&original, csv).unwrap();
    assert!(mesh_core::validate_configuration(&calibrated).valid);
    let preview = generate_mesh_preview(&calibrated, PreviewLevel::CoarseMesh).unwrap();
    let audit = preview.wall_resolution.unwrap();
    assert_eq!(audit.shear_source, "SPATIAL_SOLVER_PROFILE");
    assert!(audit.reverse_shear_face_count > 0);
    assert!(audit.transitional_face_count > 0);
    assert!(audit.maximum_predicted_streamwise_plus > 0.0);
    assert!(audit.minimum_cells_in_reference_layer > 0);
    let mut stale = calibrated;
    stale.airfoil.angle_of_attack_deg += 1.0;
    assert!(!mesh_core::validate_configuration(&stale).valid);
}

#[test]
fn budget_search_is_exact_and_never_hides_infeasibility() {
    let student = configuration();
    let counts = mesh_counts(&student).unwrap();
    assert_eq!(counts.hex_cells, 947_700);
    assert_eq!(counts.points, 993_440);
    let impossible = plan_cell_budget(
        &student,
        &BudgetConstraints {
            max_cells: 1,
            max_points: None,
        },
    )
    .unwrap();
    assert!(!impossible.feasible);
    assert!(impossible.configuration.is_none());
    assert!(impossible.counts.unwrap().hex_cells > 1);

    let feasible = plan_cell_budget(
        &rans_configuration(),
        &BudgetConstraints {
            max_cells: 2_000_000,
            max_points: None,
        },
    )
    .unwrap();
    assert!(feasible.feasible, "{:?}", feasible.reasons);
    let candidate = feasible.configuration.unwrap();
    assert!(candidate.optimization.target_cell_center_y_plus);
    assert_eq!(candidate.domain, rans_configuration().domain);
}

#[test]
fn studies_keep_axes_independent_and_require_real_uncertainty() {
    let plan = prepare_study(&configuration(), 1.0e-5, 1.25, Some(999_000)).unwrap();
    assert_eq!(plan.cases.len(), 9);
    assert!(
        plan.cases
            .iter()
            .any(|case| case.axis == StudyAxis::Grid && !case.within_cell_budget)
    );
    let base_grid = plan
        .cases
        .iter()
        .find(|case| case.name == "grid-0")
        .unwrap();
    let fine_time = plan
        .cases
        .iter()
        .find(|case| case.name == "time-2")
        .unwrap();
    assert_eq!(base_grid.configuration.mesh, fine_time.configuration.mesh);
    assert!(fine_time.time_step < base_grid.time_step);

    let mut csv = "case,metric,value,uncertainty\n".to_owned();
    for case in &plan.cases {
        let normalized = match case.axis {
            StudyAxis::Grid => case.resolution,
            StudyAxis::TimeStep => case.resolution / 1.0e-5,
            StudyAxis::Span => case.resolution / configuration().extrusion.spanwise_thickness,
        };
        writeln!(
            &mut csv,
            "{},cl,{},1e-12",
            case.name,
            1.0 + normalized * normalized
        )
        .unwrap();
    }
    let analysis = analyze_study(&plan, &csv).unwrap();
    let grid = analysis
        .series
        .iter()
        .find(|series| series.axis == StudyAxis::Grid)
        .unwrap();
    assert!(grid.observed_order.is_some());
    assert!(grid.gci_fine_absolute.is_some());
    let span = analysis
        .series
        .iter()
        .find(|series| series.axis == StudyAxis::Span)
        .unwrap();
    assert!(span.gci_fine_absolute.is_none());

    let zero_uncertainty = csv.replace(",1e-12", ",0");
    let unresolved = analyze_study(&plan, &zero_uncertainty).unwrap();
    assert!(
        unresolved
            .series
            .iter()
            .filter(|series| series.axis != StudyAxis::Span)
            .all(|series| series.observed_order.is_none())
    );
}
