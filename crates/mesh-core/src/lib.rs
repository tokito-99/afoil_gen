//! Deterministic Phase 1–8 engine for `afoil_gen`.
//!
//! This crate covers validated configuration, NACA 4-digit geometry, LES wall
//! spacing, conformal structured C-grid generation, hexahedral extrusion,
//! quality checks, preview generation, and native `OpenFOAM`, Gmsh, and ANSYS
//! Fluent export.

pub mod config;
pub mod error;
pub mod exporters;
pub mod geometry;
pub mod les;
pub mod mesh;
pub mod preview;
pub mod studies;
pub mod topology;
pub mod validation;

use std::{fs::File, io::Read, path::Path};

pub use config::MeshConfiguration;
pub use error::{MeshError, ValidationError, ValidationReport, ValidationSeverity};
pub use exporters::{
    FluentStreamSummary, GmshStreamSummary, export_fluent, export_gmsh, export_gmsh_from_generated,
    export_openfoam, export_openfoam_from_generated, export_preview_json,
    export_preview_json_from_generated, write_fluent_streaming, write_gmsh_streaming,
};
pub use les::budget::{BudgetConstraints, BudgetPlan, MeshCounts, mesh_counts, plan_cell_budget};
pub use les::feedback::{
    CalibratedConfiguration, WallShearFeedback, calibrate_configuration, parse_wall_shear_csv,
};
pub use les::profile::{SurfaceSide, WallShearProfile, WallShearSample, import_wall_shear_profile};
pub use les::sizing::{LesSizingEstimate, estimate_les_sizing};
pub use mesh::generation::{GeneratedCGrid, generate_cgrid};
pub use mesh::wall_resolution::{WallResolutionAudit, audit_wall_resolution};
pub use preview::{
    BoundaryLabel, GeometryPreview, MeshPreview, PreviewBoundaryPatch, PreviewLevel,
    PreviewPolyline, PreviewPolylineKind, generate_geometry_preview, generate_mesh_preview,
    generate_mesh_preview_from_generated,
};
pub use validation::{sanitize_cfd_name, validate_configuration};

/// Deserializes a strict mesh configuration from JSON text.
///
/// # Errors
///
/// Returns [`MeshError::Json`] for malformed JSON or schema mismatches.
pub fn configuration_from_json(json: &str) -> Result<MeshConfiguration, MeshError> {
    Ok(serde_json::from_str(json)?)
}

/// Loads and deserializes a strict mesh configuration from a JSON file.
///
/// # Errors
///
/// Returns [`MeshError::Io`] when the file cannot be read or [`MeshError::Json`]
/// when its contents do not match the strict schema.
pub fn load_configuration(path: impl AsRef<Path>) -> Result<MeshConfiguration, MeshError> {
    let file = File::open(path)?;
    configuration_from_reader(file)
}

/// Deserializes a strict mesh configuration from any reader.
///
/// # Errors
///
/// Returns [`MeshError::Io`] when reading fails or [`MeshError::Json`] when the
/// input does not match the strict schema.
pub fn configuration_from_reader(mut reader: impl Read) -> Result<MeshConfiguration, MeshError> {
    let mut json = String::new();
    reader.read_to_string(&mut json)?;
    configuration_from_json(&json)
}

#[cfg(test)]
pub(crate) mod test_support;
