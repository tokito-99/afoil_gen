//! Fatal quality gates used before later exporter phases.

use crate::{
    error::MeshError,
    mesh::{
        StructuredMesh2D, StructuredMesh3D,
        quality::{MeshQualityReport, evaluate_mesh_2d, evaluate_mesh_3d},
    },
};

/// Evaluates a 2D mesh and rejects any fatal quality finding.
///
/// # Errors
///
/// Returns [`MeshError::QualityError`] when signed areas, topology, or boundary
/// ownership are invalid.
pub fn require_valid_mesh_2d(mesh: &StructuredMesh2D) -> Result<MeshQualityReport, MeshError> {
    let report = evaluate_mesh_2d(mesh);
    if report.valid {
        Ok(report)
    } else {
        Err(MeshError::QualityError(report.errors.join("; ")))
    }
}

/// Evaluates a 3D mesh and rejects any fatal quality finding.
///
/// # Errors
///
/// Returns [`MeshError::QualityError`] when signed volumes, topology, or
/// boundary ownership are invalid.
pub fn require_valid_mesh_3d(mesh: &StructuredMesh3D) -> Result<MeshQualityReport, MeshError> {
    let report = evaluate_mesh_3d(mesh);
    if report.valid {
        Ok(report)
    } else {
        Err(MeshError::QualityError(report.errors.join("; ")))
    }
}
