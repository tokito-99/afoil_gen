use serde::{Deserialize, Serialize};

/// Complete deterministic input shared by Rust, TypeScript, and future WASM bindings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MeshConfiguration {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mesh_intent: Option<MeshIntent>,
    pub airfoil: AirfoilConfiguration,
    pub flow: FlowConfiguration,
    pub domain: DomainConfiguration,
    pub mesh: MeshResolutionConfiguration,
    pub les: LesConfiguration,
    #[serde(default)]
    pub wake: WakeConfiguration,
    pub extrusion: ExtrusionConfiguration,
    pub boundaries: BoundaryConfiguration,
    pub output: OutputConfiguration,
    #[serde(default)]
    pub optimization: OptimizationConfiguration,
}

/// Opt-in coordinate optimization; defaults preserve existing mesh coordinates.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OptimizationConfiguration {
    #[serde(default)]
    pub target_cell_center_y_plus: bool,
    #[serde(default)]
    pub orthogonalize_wall: bool,
    #[serde(default)]
    pub smoothing_iterations: usize,
}

impl MeshConfiguration {
    /// Returns the explicit mesh intent or infers one from the legacy LES fields.
    #[must_use]
    pub fn resolved_mesh_intent(&self) -> MeshIntent {
        self.mesh_intent.unwrap_or({
            if self.les.enabled {
                match self.les.mode {
                    LesMode::Wrles => MeshIntent::Wrles,
                    LesMode::Wmles => MeshIntent::Wmles,
                }
            } else if self.les.target_y_plus >= 10.0 {
                MeshIntent::RansWallFunction
            } else {
                MeshIntent::RansWallResolved
            }
        })
    }

    /// Computes the chord Reynolds number from the physical flow properties.
    #[must_use]
    pub fn derived_reynolds_number(&self) -> f64 {
        self.flow.density * self.flow.freestream_velocity * self.airfoil.chord_length
            / self.flow.dynamic_viscosity
    }
}

/// Solver-oriented mesh density and near-wall treatment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MeshIntent {
    RansWallResolved,
    RansWallFunction,
    Wmles,
    Wrles,
}

impl MeshIntent {
    /// Returns true for three-dimensional LES intents.
    #[must_use]
    pub const fn is_les(self) -> bool {
        matches!(self, Self::Wmles | Self::Wrles)
    }
}

/// NACA 4-digit geometry controls.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AirfoilConfiguration {
    pub naca_code: String,
    pub chord_length: f64,
    pub angle_of_attack_deg: f64,
    pub finite_trailing_edge: bool,
    pub surface_point_count: usize,
}

/// Reference flow properties used for LES wall-spacing estimates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FlowConfiguration {
    pub reynolds_number: f64,
    pub freestream_velocity: f64,
    pub density: f64,
    pub dynamic_viscosity: f64,
}

/// C-shaped farfield dimensions, expressed in chord lengths.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DomainConfiguration {
    pub upstream_radius_chords: f64,
    pub upper_radius_chords: f64,
    pub lower_radius_chords: f64,
    pub wake_length_chords: f64,
    pub wake_half_height_chords: f64,
}

/// Cell counts and grading controls reserved for structured-grid construction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MeshResolutionConfiguration {
    pub upper_surface_cells: usize,
    pub lower_surface_cells: usize,
    pub leading_edge_cells: usize,
    pub wake_cells: usize,
    pub radial_cells: usize,
    pub spanwise_cells: usize,
    pub surface_grading: f64,
    pub radial_grading: f64,
    pub wake_grading: f64,
    pub spanwise_grading: f64,
}

/// LES-oriented wall-normal resolution settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LesConfiguration {
    pub enabled: bool,
    #[serde(default = "default_les_mode")]
    pub mode: LesMode,
    pub target_y_plus: f64,
    #[serde(default = "default_target_streamwise_plus")]
    pub target_streamwise_plus: f64,
    #[serde(default = "default_target_spanwise_plus")]
    pub target_spanwise_plus: f64,
    #[serde(default = "default_boundary_layer_thickness_chords")]
    pub reference_boundary_layer_thickness_chords: f64,
    #[serde(default = "default_minimum_boundary_layer_cells")]
    pub minimum_cells_across_boundary_layer: usize,
    pub max_radial_growth_ratio: f64,
    pub estimate_first_cell_height: bool,
    /// Maximum airfoil-wall shear stress from a matching converged solver run.
    /// When supplied, this replaces the pre-solution flat-plate estimate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub solver_wall_shear_stress: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manual_first_cell_height: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wall_shear_profile: Option<crate::les::profile::WallShearProfile>,
}

/// Downstream wake-line orientation and turning behavior.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WakeConfiguration {
    pub alignment: WakeAlignment,
    pub blend_length_chords: f64,
    pub custom_angle_deg: f64,
}

impl Default for WakeConfiguration {
    fn default() -> Self {
        Self {
            alignment: WakeAlignment::Blended,
            blend_length_chords: 2.0,
            custom_angle_deg: 0.0,
        }
    }
}

/// Direction used by the structured wake cut.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WakeAlignment {
    Blended,
    Chord,
    Freestream,
    Custom,
}

/// Near-wall LES treatment used for mesh-sizing guidance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LesMode {
    Wrles,
    Wmles,
}

const fn default_les_mode() -> LesMode {
    LesMode::Wrles
}

const fn default_target_streamwise_plus() -> f64 {
    100.0
}

const fn default_target_spanwise_plus() -> f64 {
    20.0
}

const fn default_boundary_layer_thickness_chords() -> f64 {
    0.02
}

const fn default_minimum_boundary_layer_cells() -> usize {
    30
}

/// Spanwise extrusion controls.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtrusionConfiguration {
    pub enabled: bool,
    pub spanwise_thickness: f64,
    pub front_back_patch_type: FrontBackPatchType,
}

/// Supported front/back behavior for an extruded mesh.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FrontBackPatchType {
    Empty,
    Symmetry,
    Cyclic,
    Wall,
}

/// All exterior patch definitions in stable export order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BoundaryConfiguration {
    pub inlet: BoundaryDefinition,
    pub outlet: BoundaryDefinition,
    pub airfoil: BoundaryDefinition,
    pub upper_farfield: BoundaryDefinition,
    pub lower_farfield: BoundaryDefinition,
    pub front: BoundaryDefinition,
    pub back: BoundaryDefinition,
}

impl BoundaryConfiguration {
    /// Returns all boundary definitions in deterministic export order.
    #[must_use]
    pub fn ordered(&self) -> [(&'static str, &BoundaryDefinition); 7] {
        [
            ("boundaries.inlet", &self.inlet),
            ("boundaries.outlet", &self.outlet),
            ("boundaries.airfoil", &self.airfoil),
            ("boundaries.upperFarfield", &self.upper_farfield),
            ("boundaries.lowerFarfield", &self.lower_farfield),
            ("boundaries.front", &self.front),
            ("boundaries.back", &self.back),
        ]
    }
}

/// Name and OpenFOAM-oriented mesh patch classification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BoundaryDefinition {
    pub name: String,
    pub patch_type: PatchType,
}

/// Mesh-level patch types. Solver field boundary values remain user-managed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PatchType {
    Patch,
    Wall,
    Empty,
    SymmetryPlane,
    Cyclic,
}

/// Output selections retained for later exporter phases.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OutputConfiguration {
    pub write_openfoam: bool,
    pub write_gmsh: bool,
    pub write_geometry_json: bool,
    pub gmsh_version: GmshVersion,
}

/// Supported Gmsh ASCII versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GmshVersion {
    #[serde(rename = "2.2")]
    V2_2,
    #[serde(rename = "4.1")]
    V4_1,
}
