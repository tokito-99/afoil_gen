use mesh_core::{PreviewLevel, calibrate_configuration, generate_mesh_preview, load_configuration};
use std::path::PathBuf;

fn example(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(name)
}

#[test]
fn student_wall_audit_is_independent_of_preview_density_and_keeps_cell_budget() {
    let configuration = load_configuration(example("naca0012-student-wrles.json")).unwrap();
    let geometry = generate_mesh_preview(&configuration, PreviewLevel::GeometryOnly).unwrap();
    let full = generate_mesh_preview(&configuration, PreviewLevel::FullMesh).unwrap();
    assert_eq!(geometry.wall_resolution, full.wall_resolution);
    let audit = geometry.wall_resolution.unwrap();
    assert_eq!(audit.wall_face_count, 190);
    assert!(audit.minimum_centroid_distance > 0.0);
    assert!(audit.maximum_centroid_distance < audit.maximum_normal_height);
    assert!(audit.maximum_predicted_y_plus < 1.0);
    assert_eq!(geometry.quality_3d.unwrap().cell_count, 947_700);
}

#[test]
fn pilot_feedback_reaches_preview_and_preserves_mesh_counts() {
    let original = load_configuration(example("naca0012-les.json")).unwrap();
    let result = calibrate_configuration(
        &original,
        "wallShearStressPa\n0.2\n0.4\n0.1",
        "wallShearStressPa",
        100.0,
    )
    .unwrap();
    assert!(result.validation.valid, "{:?}", result.validation.issues);
    assert_eq!(result.configuration.mesh, original.mesh);
    let preview = generate_mesh_preview(&result.configuration, PreviewLevel::GeometryOnly).unwrap();
    let audit = preview.wall_resolution.unwrap();
    assert_eq!(audit.shear_source, "SOLVER_FEEDBACK");
    assert!((audit.reference_wall_shear_pa - 0.4).abs() < 1e-12);
    assert!(audit.maximum_predicted_y_plus.is_finite());
    assert!(preview.quality.valid);
}

#[test]
fn infeasible_feedback_is_reported_without_changing_the_budget() {
    let original = load_configuration(example("naca0012-les.json")).unwrap();
    let result = calibrate_configuration(
        &original,
        "wallShearStressPa\n8",
        "wallShearStressPa",
        100.0,
    )
    .unwrap();
    assert!(!result.validation.valid);
    assert!(
        result
            .validation
            .issues
            .iter()
            .any(|issue| issue.code == "INFEASIBLE_RADIAL_STRETCHING")
    );
    assert_eq!(result.configuration.mesh, original.mesh);
    assert_eq!(
        result.configuration.les.max_radial_growth_ratio.to_bits(),
        original.les.max_radial_growth_ratio.to_bits()
    );
}

#[test]
fn finite_trailing_edge_base_is_part_of_the_wall_audit() {
    let mut configuration = load_configuration(example("naca0012-student-wrles.json")).unwrap();
    configuration.airfoil.finite_trailing_edge = true;
    let preview = generate_mesh_preview(&configuration, PreviewLevel::GeometryOnly).unwrap();
    let wall = preview
        .boundary_patches
        .iter()
        .find(|patch| patch.role == mesh_core::topology::BoundaryRole::Airfoil)
        .unwrap();
    assert_eq!(
        preview.wall_resolution.unwrap().wall_face_count,
        wall.segments.len()
    );
    assert!(wall.segments.len() > 190);
}
