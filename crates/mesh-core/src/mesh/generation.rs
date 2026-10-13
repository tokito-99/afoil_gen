//! End-to-end Phase 4–6 C-grid generation pipeline.

use serde::Serialize;

use crate::{
    config::MeshConfiguration,
    error::MeshError,
    mesh::{
        StructuredMesh2D, StructuredMesh3D,
        extrusion::extrude_mesh,
        quality::MeshQualityReport,
        structured::generate_structured_mesh,
        validation::{require_valid_mesh_2d, require_valid_mesh_3d},
    },
    topology::{CGridTopology, build_cgrid_topology},
};

/// Validated topology, 2D mesh, and optional extruded mesh.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedCGrid {
    pub topology: CGridTopology,
    pub mesh_2d: StructuredMesh2D,
    pub quality_2d: MeshQualityReport,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mesh_3d: Option<StructuredMesh3D>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality_3d: Option<MeshQualityReport>,
}

/// Builds topology, coordinates, connectivity, boundaries, quality reports, and
/// optional spanwise extrusion.
///
/// # Errors
///
/// Returns configuration/topology errors directly and rejects any generated
/// mesh whose quality report contains fatal errors.
pub fn generate_cgrid(configuration: &MeshConfiguration) -> Result<GeneratedCGrid, MeshError> {
    let topology = build_cgrid_topology(configuration)?;
    let mesh_2d = generate_structured_mesh(&topology, configuration)?;
    let quality_2d = require_valid_mesh_2d(&mesh_2d)?;

    let (mesh_3d, quality_3d) = if configuration.extrusion.enabled {
        let extruded = extrude_mesh(&mesh_2d, configuration)?;
        let quality = require_valid_mesh_3d(&extruded)?;
        (Some(extruded), Some(quality))
    } else {
        (None, None)
    };

    Ok(GeneratedCGrid {
        topology,
        mesh_2d,
        quality_2d,
        mesh_3d,
        quality_3d,
    })
}

#[cfg(test)]
mod tests {
    use super::generate_cgrid;
    use crate::test_support::example_configuration;

    #[test]
    fn default_configuration_generates_valid_2d_and_3d_meshes() {
        let generated =
            generate_cgrid(&example_configuration()).expect("valid complete generation");
        assert!(generated.quality_2d.valid);
        assert!(
            generated
                .quality_3d
                .as_ref()
                .is_some_and(|report| report.valid)
        );
        assert_eq!(generated.mesh_2d.cells.len(), 105_600);
        assert_eq!(
            generated.mesh_3d.as_ref().map(|mesh| mesh.cells.len()),
            Some(211_200)
        );
    }
}
