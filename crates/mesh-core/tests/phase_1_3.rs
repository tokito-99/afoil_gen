use std::{fs, path::PathBuf};

use mesh_core::{
    configuration_from_json, generate_geometry_preview, load_configuration, validate_configuration,
};

fn example_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(name)
}

#[test]
fn naca0012_example_loads_validates_and_previews() {
    let configuration =
        load_configuration(example_path("naca0012-les.json")).expect("example must load");
    let report = validate_configuration(&configuration);
    assert!(report.valid, "issues: {:?}", report.issues);

    let preview = generate_geometry_preview(&configuration).expect("preview must generate");
    assert_eq!(preview.phase, "geometry-and-les-spacing");
    assert_eq!(preview.airfoil_upper.len(), 161);
    assert_eq!(preview.airfoil_lower.len(), 161);
    assert!(preview.wall_spacing.is_some());
    assert!(preview.radial_stretching.is_some());
    assert!(
        preview
            .airfoil_upper
            .iter()
            .chain(&preview.airfoil_lower)
            .all(|point| point.x.is_finite() && point.y.is_finite())
    );
}

#[test]
fn naca2412_example_loads_and_rotates_with_rans_wall_spacing() {
    let configuration =
        load_configuration(example_path("naca2412-rans.json")).expect("example must load");
    let report = validate_configuration(&configuration);
    assert!(report.valid, "issues: {:?}", report.issues);

    let preview = generate_geometry_preview(&configuration).expect("preview must generate");
    assert!(preview.wall_spacing.is_some());
    assert!(preview.radial_stretching.is_some());
    assert!(
        preview
            .airfoil_upper
            .iter()
            .zip(&preview.airfoil_lower)
            .any(|(upper, lower)| 0.5 * (upper.y + lower.y) > 0.0)
    );
}

#[test]
fn identical_inputs_produce_byte_identical_preview_json() {
    let configuration =
        load_configuration(example_path("naca0012-les.json")).expect("example must load");
    let first = serde_json::to_vec_pretty(
        &generate_geometry_preview(&configuration).expect("first preview"),
    )
    .expect("first serialization");
    let second = serde_json::to_vec_pretty(
        &generate_geometry_preview(&configuration).expect("second preview"),
    )
    .expect("second serialization");
    assert_eq!(first, second);
}

#[test]
fn strict_schema_rejects_unknown_fields() {
    let contents = fs::read_to_string(example_path("naca0012-les.json")).expect("read example");
    let modified = contents.replacen(
        "\"nacaCode\": \"0012\",",
        "\"nacaCode\": \"0012\", \"unexpected\": true,",
        1,
    );
    assert!(configuration_from_json(&modified).is_err());
}
