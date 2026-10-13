use crate::config::{
    AirfoilConfiguration, BoundaryConfiguration, BoundaryDefinition, DomainConfiguration,
    ExtrusionConfiguration, FlowConfiguration, FrontBackPatchType, GmshVersion, LesConfiguration,
    LesMode, MeshConfiguration, MeshIntent, MeshResolutionConfiguration, OutputConfiguration,
    PatchType, WakeConfiguration,
};

pub(crate) fn example_configuration() -> MeshConfiguration {
    MeshConfiguration {
        optimization: crate::config::OptimizationConfiguration::default(),
        mesh_intent: None,
        airfoil: AirfoilConfiguration {
            naca_code: "0012".to_owned(),
            chord_length: 1.0,
            angle_of_attack_deg: 0.0,
            finite_trailing_edge: false,
            surface_point_count: 161,
        },
        flow: FlowConfiguration {
            reynolds_number: 1_000_000.0,
            freestream_velocity: 30.0,
            density: 1.225,
            dynamic_viscosity: 0.000_018_1,
        },
        domain: DomainConfiguration {
            upstream_radius_chords: 10.0,
            upper_radius_chords: 10.0,
            lower_radius_chords: 10.0,
            wake_length_chords: 20.0,
            wake_half_height_chords: 3.0,
        },
        mesh: MeshResolutionConfiguration {
            upper_surface_cells: 160,
            lower_surface_cells: 160,
            leading_edge_cells: 40,
            wake_cells: 240,
            radial_cells: 120,
            spanwise_cells: 2,
            surface_grading: 1.0,
            radial_grading: 1.15,
            wake_grading: 1.05,
            spanwise_grading: 1.0,
        },
        les: LesConfiguration {
            enabled: true,
            mode: LesMode::Wrles,
            target_y_plus: 1.0,
            target_streamwise_plus: 100.0,
            target_spanwise_plus: 20.0,
            reference_boundary_layer_thickness_chords: 0.02,
            minimum_cells_across_boundary_layer: 30,
            max_radial_growth_ratio: 1.2,
            estimate_first_cell_height: true,
            solver_wall_shear_stress: None,
            manual_first_cell_height: None,
            wall_shear_profile: None,
        },
        wake: WakeConfiguration::default(),
        extrusion: ExtrusionConfiguration {
            enabled: true,
            spanwise_thickness: 0.01,
            front_back_patch_type: FrontBackPatchType::Cyclic,
        },
        boundaries: BoundaryConfiguration {
            inlet: boundary("inlet", PatchType::Patch),
            outlet: boundary("outlet", PatchType::Patch),
            airfoil: boundary("airfoil_wall", PatchType::Wall),
            upper_farfield: boundary("upper_farfield", PatchType::Patch),
            lower_farfield: boundary("lower_farfield", PatchType::Patch),
            front: boundary("front_plane", PatchType::Cyclic),
            back: boundary("back_plane", PatchType::Cyclic),
        },
        output: OutputConfiguration {
            write_openfoam: true,
            write_gmsh: true,
            write_geometry_json: true,
            gmsh_version: GmshVersion::V2_2,
        },
    }
}

pub(crate) fn small_configuration() -> MeshConfiguration {
    let mut configuration = example_configuration();
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
    configuration.mesh.radial_cells = 64;
    configuration.mesh.spanwise_cells = 1;
    configuration.mesh_intent = Some(MeshIntent::RansWallResolved);
    configuration.les.enabled = false;
    configuration.les.reference_boundary_layer_thickness_chords = 0.01;
    configuration.les.minimum_cells_across_boundary_layer = 16;
    configuration.les.estimate_first_cell_height = false;
    configuration.les.manual_first_cell_height = Some(1.2e-4);
    configuration.extrusion.front_back_patch_type = FrontBackPatchType::Empty;
    configuration.boundaries.front.patch_type = PatchType::Empty;
    configuration.boundaries.back.patch_type = PatchType::Empty;
    configuration
}

fn boundary(name: &str, patch_type: PatchType) -> BoundaryDefinition {
    BoundaryDefinition {
        name: name.to_owned(),
        patch_type,
    }
}
