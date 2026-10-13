//! Low-memory Gmsh 2.2 streaming for large structured extrusions.

use std::{fmt, io::Write};

use crate::{
    config::{GmshVersion, MeshConfiguration},
    error::MeshError,
    mesh::{
        extrusion::spanwise_planes, quality::MeshQualityReport,
        structured::generate_structured_mesh, validation::require_valid_mesh_2d,
    },
    topology::build_cgrid_topology,
};

/// Counts produced while streaming a Gmsh file.
#[derive(Debug, Clone, PartialEq)]
pub struct GmshStreamSummary {
    pub point_count: usize,
    pub hex_cell_count: usize,
    pub boundary_element_count: usize,
    pub quality_2d: MeshQualityReport,
}

/// Writes an extruded Gmsh 2.2 ASCII mesh without allocating the complete 3-D
/// mesh or output string.
///
/// Only the validated 2-D C-grid and z-plane coordinates are retained. Nodes,
/// boundary quads, and hexahedra are emitted directly to `output`.
///
/// # Errors
///
/// Returns configuration, topology, quality, output-version, formatting, or
/// I/O errors.
#[allow(clippy::too_many_lines)]
pub fn write_gmsh_streaming(
    configuration: &MeshConfiguration,
    output: &mut impl Write,
) -> Result<GmshStreamSummary, MeshError> {
    if configuration.output.gmsh_version != GmshVersion::V2_2 {
        return Err(MeshError::ExportError(
            "large-mesh streaming currently requires Gmsh ASCII 2.2".to_owned(),
        ));
    }

    let topology = build_cgrid_topology(configuration)?;
    let mesh = generate_structured_mesh(&topology, configuration)?;
    let quality_2d = require_valid_mesh_2d(&mesh)?;
    let planes = spanwise_planes(configuration)?;
    let spanwise_cells = configuration.mesh.spanwise_cells;
    let points_per_plane = mesh.points.len();
    let point_count = points_per_plane
        .checked_mul(planes.len())
        .ok_or_else(|| MeshError::ExportError("Gmsh point count overflow".to_owned()))?;
    let hex_cell_count = mesh
        .cells
        .len()
        .checked_mul(spanwise_cells)
        .ok_or_else(|| MeshError::ExportError("Gmsh cell count overflow".to_owned()))?;
    let side_face_count = mesh
        .boundaries
        .iter()
        .map(|patch| patch.edges.len())
        .sum::<usize>()
        .checked_mul(spanwise_cells)
        .ok_or_else(|| MeshError::ExportError("Gmsh boundary count overflow".to_owned()))?;
    let boundary_element_count = side_face_count
        .checked_add(2 * mesh.cells.len())
        .ok_or_else(|| MeshError::ExportError("Gmsh boundary count overflow".to_owned()))?;
    let boundary_count = mesh.boundaries.len() + 2;

    writeln!(output, "$MeshFormat\n2.2 0 8\n$EndMeshFormat")?;
    writeln!(
        output,
        "$PhysicalNames\n{}",
        boundary_count + topology.blocks.len()
    )?;
    for (index, patch) in mesh.boundaries.iter().enumerate() {
        writeln!(output, "2 {} \"{}\"", index + 1, patch.name)?;
    }
    writeln!(
        output,
        "2 {} \"{}\"",
        mesh.boundaries.len() + 1,
        configuration.boundaries.front.name
    )?;
    writeln!(
        output,
        "2 {} \"{}\"",
        mesh.boundaries.len() + 2,
        configuration.boundaries.back.name
    )?;
    for block in &topology.blocks {
        writeln!(
            output,
            "3 {} \"cellZone_{}\"",
            boundary_count + block.id + 1,
            block.name
        )?;
    }
    writeln!(output, "$EndPhysicalNames")?;

    writeln!(output, "$Nodes\n{point_count}")?;
    let mut node_id = 1;
    for &z in &planes {
        for point in &mesh.points {
            writeln!(
                output,
                "{} {} {} {}",
                node_id,
                Scalar(point.x),
                Scalar(point.y),
                Scalar(z)
            )?;
            node_id += 1;
        }
    }
    writeln!(output, "$EndNodes")?;

    writeln!(
        output,
        "$Elements\n{}",
        boundary_element_count + hex_cell_count
    )?;
    let mut element_id = 1;
    for (patch_index, patch) in mesh.boundaries.iter().enumerate() {
        let tag = patch_index + 1;
        for span_index in 0..spanwise_cells {
            let lower_offset = span_index * points_per_plane;
            let upper_offset = (span_index + 1) * points_per_plane;
            for edge in &patch.edges {
                writeln!(
                    output,
                    "{} 3 2 {} {} {} {} {} {}",
                    element_id,
                    tag,
                    tag,
                    lower_offset + edge[0] + 1,
                    lower_offset + edge[1] + 1,
                    upper_offset + edge[1] + 1,
                    upper_offset + edge[0] + 1
                )?;
                element_id += 1;
            }
        }
    }

    let front_tag = mesh.boundaries.len() + 1;
    for cell in &mesh.cells {
        writeln!(
            output,
            "{} 3 2 {} {} {} {} {} {}",
            element_id,
            front_tag,
            front_tag,
            cell.vertices[0] + 1,
            cell.vertices[3] + 1,
            cell.vertices[2] + 1,
            cell.vertices[1] + 1
        )?;
        element_id += 1;
    }
    let back_tag = front_tag + 1;
    let back_offset = spanwise_cells * points_per_plane;
    for cell in &mesh.cells {
        writeln!(
            output,
            "{} 3 2 {} {} {} {} {} {}",
            element_id,
            back_tag,
            back_tag,
            back_offset + cell.vertices[0] + 1,
            back_offset + cell.vertices[1] + 1,
            back_offset + cell.vertices[2] + 1,
            back_offset + cell.vertices[3] + 1
        )?;
        element_id += 1;
    }

    for span_index in 0..spanwise_cells {
        let lower_offset = span_index * points_per_plane;
        let upper_offset = (span_index + 1) * points_per_plane;
        for cell in &mesh.cells {
            let tag = boundary_count + cell.block_id + 1;
            writeln!(
                output,
                "{} 5 2 {} {} {} {} {} {} {} {} {} {}",
                element_id,
                tag,
                cell.block_id + 1,
                lower_offset + cell.vertices[0] + 1,
                lower_offset + cell.vertices[1] + 1,
                lower_offset + cell.vertices[2] + 1,
                lower_offset + cell.vertices[3] + 1,
                upper_offset + cell.vertices[0] + 1,
                upper_offset + cell.vertices[1] + 1,
                upper_offset + cell.vertices[2] + 1,
                upper_offset + cell.vertices[3] + 1
            )?;
            element_id += 1;
        }
    }
    writeln!(output, "$EndElements")?;

    Ok(GmshStreamSummary {
        point_count,
        hex_cell_count,
        boundary_element_count,
        quality_2d,
    })
}

struct Scalar(f64);

impl fmt::Display for Scalar {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0 == 0.0 {
            formatter.write_str("0")
        } else {
            write!(formatter, "{:.15e}", self.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::write_gmsh_streaming;
    use crate::{exporters::gmsh::export_gmsh, test_support::small_configuration};

    #[test]
    fn streaming_output_matches_in_memory_v2_2_byte_for_byte() {
        let configuration = small_configuration();
        let expected = export_gmsh(&configuration).expect("in-memory Gmsh");
        let mut actual = Vec::new();
        let summary =
            write_gmsh_streaming(&configuration, &mut actual).expect("streamed Gmsh output");
        assert_eq!(actual, expected.as_bytes());
        assert!(summary.point_count > 0);
        assert!(summary.hex_cell_count > 0);
        assert!(summary.quality_2d.valid);
    }
}
