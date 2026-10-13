use std::path::PathBuf;

use mesh_core::config::{FrontBackPatchType, MeshIntent, PatchType};
use mesh_core::{PreviewLevel, generate_cgrid, generate_mesh_preview, load_configuration};

fn example_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(name)
}

#[test]
fn default_les_case_generates_complete_positive_cgrid_and_extrusion() {
    let configuration =
        load_configuration(example_path("naca0012-les.json")).expect("example must load");
    let generated = generate_cgrid(&configuration).expect("complete mesh generation");
    assert_eq!(generated.topology.blocks.len(), 6);
    assert_eq!(generated.mesh_2d.blocks.len(), 6);
    assert!(generated.quality_2d.valid);
    assert!(
        generated
            .quality_2d
            .minimum_cell_area
            .is_some_and(|area| area > 0.0)
    );
    assert!(generated.quality_3d.as_ref().is_some_and(|quality| {
        quality.valid
            && quality
                .minimum_cell_volume
                .is_some_and(|volume| volume > 0.0)
    }));

    let names = generated
        .mesh_3d
        .as_ref()
        .expect("extruded mesh")
        .boundaries
        .iter()
        .map(|patch| patch.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec![
            "inlet",
            "outlet",
            "airfoil_wall",
            "upper_farfield",
            "lower_farfield",
            "front_plane",
            "back_plane",
        ]
    );
}

#[test]
fn cambered_rotated_finite_trailing_edge_case_remains_positive() {
    let mut configuration =
        load_configuration(example_path("naca2412-rans.json")).expect("example must load");
    configuration.airfoil.finite_trailing_edge = true;
    configuration.mesh.upper_surface_cells = 32;
    configuration.mesh.lower_surface_cells = 32;
    configuration.mesh.leading_edge_cells = 10;
    configuration.mesh.wake_cells = 40;
    configuration.mesh.radial_cells = 96;
    configuration.mesh.spanwise_cells = 3;
    configuration.extrusion.front_back_patch_type = FrontBackPatchType::Symmetry;
    configuration.boundaries.front.patch_type = PatchType::SymmetryPlane;
    configuration.boundaries.back.patch_type = PatchType::SymmetryPlane;
    let generated = generate_cgrid(&configuration).expect("cambered C-grid");
    assert!(generated.quality_2d.valid);
    assert!(generated.quality_3d.is_some_and(|quality| quality.valid));
    assert!(generated.topology.wake_half_thickness > 1.0e-5);
}

#[test]
fn repeated_phase_4_6_previews_are_byte_identical_and_finite() {
    let mut configuration =
        load_configuration(example_path("naca0012-les.json")).expect("example must load");
    configuration.airfoil.surface_point_count = 81;
    configuration.domain.upstream_radius_chords = 4.0;
    configuration.domain.upper_radius_chords = 4.0;
    configuration.domain.lower_radius_chords = 4.0;
    configuration.domain.wake_length_chords = 6.0;
    configuration.domain.wake_half_height_chords = 1.5;
    configuration.mesh.upper_surface_cells = 24;
    configuration.mesh.lower_surface_cells = 24;
    configuration.mesh.leading_edge_cells = 8;
    configuration.mesh.wake_cells = 32;
    configuration.mesh.radial_cells = 96;
    configuration.mesh.spanwise_cells = 1;
    configuration.mesh_intent = Some(MeshIntent::RansWallResolved);
    configuration.les.enabled = false;
    configuration.les.reference_boundary_layer_thickness_chords = 0.01;
    configuration.les.minimum_cells_across_boundary_layer = 16;
    configuration.les.estimate_first_cell_height = false;
    configuration.les.manual_first_cell_height = Some(1.2e-4);
    configuration.les.max_radial_growth_ratio = 1.2;
    configuration.extrusion.front_back_patch_type = FrontBackPatchType::Empty;
    configuration.boundaries.front.patch_type = PatchType::Empty;
    configuration.boundaries.back.patch_type = PatchType::Empty;

    let first = serde_json::to_vec_pretty(
        &generate_mesh_preview(&configuration, PreviewLevel::CoarseMesh).expect("first preview"),
    )
    .expect("serialize first");
    let second = serde_json::to_vec_pretty(
        &generate_mesh_preview(&configuration, PreviewLevel::CoarseMesh).expect("second preview"),
    )
    .expect("serialize second");
    assert_eq!(first, second);
    let text = String::from_utf8(first).expect("JSON is UTF-8");
    assert!(!text.contains("NaN"));
    assert!(!text.contains("inf"));
}
