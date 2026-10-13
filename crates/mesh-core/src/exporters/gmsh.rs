//! Deterministic Gmsh ASCII interoperability export.

use std::fmt::Write;

use crate::{
    config::{GmshVersion, MeshConfiguration},
    error::MeshError,
    mesh::{
        HexCell, Point3, QuadCell,
        generation::{GeneratedCGrid, generate_cgrid},
    },
    topology::LogicalBlock2D,
};

/// Generates the configured Gmsh ASCII version from a validated mesh.
///
/// # Errors
///
/// Returns generation/quality errors or formatting failures.
pub fn export_gmsh(configuration: &MeshConfiguration) -> Result<String, MeshError> {
    let generated = generate_cgrid(configuration)?;
    export_gmsh_from_generated(configuration, &generated)
}

/// Generates Gmsh ASCII from an already validated Phase 4–6 result.
///
/// # Errors
///
/// Returns an export error when mesh connectivity is inconsistent.
pub fn export_gmsh_from_generated(
    configuration: &MeshConfiguration,
    generated: &GeneratedCGrid,
) -> Result<String, MeshError> {
    match configuration.output.gmsh_version {
        GmshVersion::V2_2 => export_v2_2(generated),
        GmshVersion::V4_1 => export_v4_1(generated),
    }
}

fn export_v2_2(generated: &GeneratedCGrid) -> Result<String, MeshError> {
    let mut output = String::new();
    append(
        &mut output,
        format_args!("$MeshFormat\n2.2 0 8\n$EndMeshFormat\n"),
    )?;
    write_physical_names(&mut output, generated)?;
    write_nodes_v2_2(&mut output, generated)?;
    write_elements_v2_2(&mut output, generated)?;
    Ok(output)
}

fn export_v4_1(generated: &GeneratedCGrid) -> Result<String, MeshError> {
    let mut output = String::new();
    append(
        &mut output,
        format_args!("$MeshFormat\n4.1 0 8\n$EndMeshFormat\n"),
    )?;
    write_physical_names(&mut output, generated)?;
    write_entities_v4_1(&mut output, generated)?;
    write_nodes_v4_1(&mut output, generated)?;
    write_elements_v4_1(&mut output, generated)?;
    Ok(output)
}

fn write_physical_names(output: &mut String, generated: &GeneratedCGrid) -> Result<(), MeshError> {
    let (mesh_dimension, boundary_count, block_count) = if let Some(mesh) = &generated.mesh_3d {
        (3, mesh.boundaries.len(), generated.topology.blocks.len())
    } else {
        (
            2,
            generated.mesh_2d.boundaries.len(),
            generated.topology.blocks.len(),
        )
    };
    append(
        output,
        format_args!("$PhysicalNames\n{}\n", boundary_count + block_count),
    )?;

    if let Some(mesh) = &generated.mesh_3d {
        for (index, patch) in mesh.boundaries.iter().enumerate() {
            append(output, format_args!("2 {} \"{}\"\n", index + 1, patch.name))?;
        }
    } else {
        for (index, patch) in generated.mesh_2d.boundaries.iter().enumerate() {
            append(output, format_args!("1 {} \"{}\"\n", index + 1, patch.name))?;
        }
    }
    for block in &generated.topology.blocks {
        append(
            output,
            format_args!(
                "{} {} \"cellZone_{}\"\n",
                mesh_dimension,
                boundary_count + block.id + 1,
                block.name
            ),
        )?;
    }
    append(output, format_args!("$EndPhysicalNames\n"))?;
    Ok(())
}

fn write_nodes_v2_2(output: &mut String, generated: &GeneratedCGrid) -> Result<(), MeshError> {
    if let Some(mesh) = &generated.mesh_3d {
        append(output, format_args!("$Nodes\n{}\n", mesh.points.len()))?;
        for (index, point) in mesh.points.iter().enumerate() {
            append(
                output,
                format_args!(
                    "{} {} {} {}\n",
                    index + 1,
                    scalar(point.x),
                    scalar(point.y),
                    scalar(point.z)
                ),
            )?;
        }
    } else {
        append(
            output,
            format_args!("$Nodes\n{}\n", generated.mesh_2d.points.len()),
        )?;
        for (index, point) in generated.mesh_2d.points.iter().enumerate() {
            append(
                output,
                format_args!("{} {} {} 0\n", index + 1, scalar(point.x), scalar(point.y)),
            )?;
        }
    }
    append(output, format_args!("$EndNodes\n"))?;
    Ok(())
}

fn write_elements_v2_2(output: &mut String, generated: &GeneratedCGrid) -> Result<(), MeshError> {
    if let Some(mesh) = &generated.mesh_3d {
        let boundary_element_count: usize =
            mesh.boundaries.iter().map(|patch| patch.faces.len()).sum();
        append(
            output,
            format_args!("$Elements\n{}\n", boundary_element_count + mesh.cells.len()),
        )?;
        let mut element_id = 1;
        for (patch_index, patch) in mesh.boundaries.iter().enumerate() {
            let tag = patch_index + 1;
            for face in &patch.faces {
                append(
                    output,
                    format_args!(
                        "{} 3 2 {} {} {} {} {} {}\n",
                        element_id,
                        tag,
                        tag,
                        face[0] + 1,
                        face[1] + 1,
                        face[2] + 1,
                        face[3] + 1
                    ),
                )?;
                element_id += 1;
            }
        }
        let boundary_count = mesh.boundaries.len();
        for cell in &mesh.cells {
            let tag = boundary_count + cell.block_id + 1;
            append_hex_v2_2(output, element_id, tag, cell)?;
            element_id += 1;
        }
    } else {
        let boundary_element_count: usize = generated
            .mesh_2d
            .boundaries
            .iter()
            .map(|patch| patch.edges.len())
            .sum();
        append(
            output,
            format_args!(
                "$Elements\n{}\n",
                boundary_element_count + generated.mesh_2d.cells.len()
            ),
        )?;
        let mut element_id = 1;
        for (patch_index, patch) in generated.mesh_2d.boundaries.iter().enumerate() {
            let tag = patch_index + 1;
            for edge in &patch.edges {
                append(
                    output,
                    format_args!(
                        "{} 1 2 {} {} {} {}\n",
                        element_id,
                        tag,
                        tag,
                        edge[0] + 1,
                        edge[1] + 1
                    ),
                )?;
                element_id += 1;
            }
        }
        let boundary_count = generated.mesh_2d.boundaries.len();
        for cell in &generated.mesh_2d.cells {
            let tag = boundary_count + cell.block_id + 1;
            append_quad_v2_2(output, element_id, tag, cell)?;
            element_id += 1;
        }
    }
    append(output, format_args!("$EndElements\n"))?;
    Ok(())
}

fn append_quad_v2_2(
    output: &mut String,
    element_id: usize,
    tag: usize,
    cell: &QuadCell,
) -> Result<(), MeshError> {
    append(
        output,
        format_args!(
            "{} 3 2 {} {} {} {} {} {}\n",
            element_id,
            tag,
            cell.block_id + 1,
            cell.vertices[0] + 1,
            cell.vertices[1] + 1,
            cell.vertices[2] + 1,
            cell.vertices[3] + 1
        ),
    )
}

fn append_hex_v2_2(
    output: &mut String,
    element_id: usize,
    tag: usize,
    cell: &HexCell,
) -> Result<(), MeshError> {
    append(
        output,
        format_args!(
            "{} 5 2 {} {} {} {} {} {} {} {} {} {}\n",
            element_id,
            tag,
            cell.block_id + 1,
            cell.vertices[0] + 1,
            cell.vertices[1] + 1,
            cell.vertices[2] + 1,
            cell.vertices[3] + 1,
            cell.vertices[4] + 1,
            cell.vertices[5] + 1,
            cell.vertices[6] + 1,
            cell.vertices[7] + 1
        ),
    )
}

fn write_entities_v4_1(output: &mut String, generated: &GeneratedCGrid) -> Result<(), MeshError> {
    let bounds = global_bounds(generated)?;
    if let Some(mesh) = &generated.mesh_3d {
        append(
            output,
            format_args!(
                "$Entities\n0 0 {} {}\n",
                mesh.boundaries.len(),
                generated.topology.blocks.len()
            ),
        )?;
        for (index, _) in mesh.boundaries.iter().enumerate() {
            append_entity(output, index + 1, bounds, index + 1)?;
        }
        let boundary_count = mesh.boundaries.len();
        for block in &generated.topology.blocks {
            append_entity(output, block.id + 1, bounds, boundary_count + block.id + 1)?;
        }
    } else {
        append(
            output,
            format_args!(
                "$Entities\n0 {} {} 0\n",
                generated.mesh_2d.boundaries.len(),
                generated.topology.blocks.len()
            ),
        )?;
        for (index, _) in generated.mesh_2d.boundaries.iter().enumerate() {
            append_entity(output, index + 1, bounds, index + 1)?;
        }
        let boundary_count = generated.mesh_2d.boundaries.len();
        for block in &generated.topology.blocks {
            append_entity(output, block.id + 1, bounds, boundary_count + block.id + 1)?;
        }
    }
    append(output, format_args!("$EndEntities\n"))?;
    Ok(())
}

fn append_entity(
    output: &mut String,
    entity_tag: usize,
    bounds: [f64; 6],
    physical_tag: usize,
) -> Result<(), MeshError> {
    append(
        output,
        format_args!(
            "{} {} {} {} {} {} {} 1 {} 0\n",
            entity_tag,
            scalar(bounds[0]),
            scalar(bounds[1]),
            scalar(bounds[2]),
            scalar(bounds[3]),
            scalar(bounds[4]),
            scalar(bounds[5]),
            physical_tag
        ),
    )
}

fn write_nodes_v4_1(output: &mut String, generated: &GeneratedCGrid) -> Result<(), MeshError> {
    let (dimension, points) = if let Some(mesh) = &generated.mesh_3d {
        (3, mesh.points.clone())
    } else {
        (
            2,
            generated
                .mesh_2d
                .points
                .iter()
                .map(|point| Point3::new(point.x, point.y, 0.0))
                .collect(),
        )
    };
    append(
        output,
        format_args!(
            "$Nodes\n1 {} 1 {}\n{} 1 0 {}\n",
            points.len(),
            points.len(),
            dimension,
            points.len()
        ),
    )?;
    for node_tag in 1..=points.len() {
        append(output, format_args!("{node_tag}\n"))?;
    }
    for point in points {
        append(
            output,
            format_args!(
                "{} {} {}\n",
                scalar(point.x),
                scalar(point.y),
                scalar(point.z)
            ),
        )?;
    }
    append(output, format_args!("$EndNodes\n"))?;
    Ok(())
}

fn write_elements_v4_1(output: &mut String, generated: &GeneratedCGrid) -> Result<(), MeshError> {
    if let Some(mesh) = &generated.mesh_3d {
        let total: usize = mesh
            .boundaries
            .iter()
            .map(|patch| patch.faces.len())
            .sum::<usize>()
            + mesh.cells.len();
        append(
            output,
            format_args!(
                "$Elements\n{} {} 1 {}\n",
                mesh.boundaries.len() + generated.topology.blocks.len(),
                total,
                total
            ),
        )?;
        let mut element_tag = 1;
        for (patch_index, patch) in mesh.boundaries.iter().enumerate() {
            append(
                output,
                format_args!("2 {} 3 {}\n", patch_index + 1, patch.faces.len()),
            )?;
            for face in &patch.faces {
                append(
                    output,
                    format_args!(
                        "{} {} {} {} {}\n",
                        element_tag,
                        face[0] + 1,
                        face[1] + 1,
                        face[2] + 1,
                        face[3] + 1
                    ),
                )?;
                element_tag += 1;
            }
        }
        write_hex_blocks_v4_1(
            output,
            &generated.topology.blocks,
            &mesh.cells,
            &mut element_tag,
        )?;
    } else {
        let total: usize = generated
            .mesh_2d
            .boundaries
            .iter()
            .map(|patch| patch.edges.len())
            .sum::<usize>()
            + generated.mesh_2d.cells.len();
        append(
            output,
            format_args!(
                "$Elements\n{} {} 1 {}\n",
                generated.mesh_2d.boundaries.len() + generated.topology.blocks.len(),
                total,
                total
            ),
        )?;
        let mut element_tag = 1;
        for (patch_index, patch) in generated.mesh_2d.boundaries.iter().enumerate() {
            append(
                output,
                format_args!("1 {} 1 {}\n", patch_index + 1, patch.edges.len()),
            )?;
            for edge in &patch.edges {
                append(
                    output,
                    format_args!("{} {} {}\n", element_tag, edge[0] + 1, edge[1] + 1),
                )?;
                element_tag += 1;
            }
        }
        write_quad_blocks_v4_1(
            output,
            &generated.topology.blocks,
            &generated.mesh_2d.cells,
            &mut element_tag,
        )?;
    }
    append(output, format_args!("$EndElements\n"))?;
    Ok(())
}

fn write_quad_blocks_v4_1(
    output: &mut String,
    blocks: &[LogicalBlock2D],
    cells: &[QuadCell],
    element_tag: &mut usize,
) -> Result<(), MeshError> {
    for block in blocks {
        let block_cells = cells
            .iter()
            .filter(|cell| cell.block_id == block.id)
            .collect::<Vec<_>>();
        append(
            output,
            format_args!("2 {} 3 {}\n", block.id + 1, block_cells.len()),
        )?;
        for cell in block_cells {
            append(
                output,
                format_args!(
                    "{} {} {} {} {}\n",
                    *element_tag,
                    cell.vertices[0] + 1,
                    cell.vertices[1] + 1,
                    cell.vertices[2] + 1,
                    cell.vertices[3] + 1
                ),
            )?;
            *element_tag += 1;
        }
    }
    Ok(())
}

fn write_hex_blocks_v4_1(
    output: &mut String,
    blocks: &[LogicalBlock2D],
    cells: &[HexCell],
    element_tag: &mut usize,
) -> Result<(), MeshError> {
    for block in blocks {
        let block_cells = cells
            .iter()
            .filter(|cell| cell.block_id == block.id)
            .collect::<Vec<_>>();
        append(
            output,
            format_args!("3 {} 5 {}\n", block.id + 1, block_cells.len()),
        )?;
        for cell in block_cells {
            append(
                output,
                format_args!(
                    "{} {} {} {} {} {} {} {} {}\n",
                    *element_tag,
                    cell.vertices[0] + 1,
                    cell.vertices[1] + 1,
                    cell.vertices[2] + 1,
                    cell.vertices[3] + 1,
                    cell.vertices[4] + 1,
                    cell.vertices[5] + 1,
                    cell.vertices[6] + 1,
                    cell.vertices[7] + 1
                ),
            )?;
            *element_tag += 1;
        }
    }
    Ok(())
}

fn global_bounds(generated: &GeneratedCGrid) -> Result<[f64; 6], MeshError> {
    let points = if let Some(mesh) = &generated.mesh_3d {
        mesh.points.clone()
    } else {
        generated
            .mesh_2d
            .points
            .iter()
            .map(|point| Point3::new(point.x, point.y, 0.0))
            .collect()
    };
    if points.is_empty() {
        return Err(MeshError::ExportError(
            "cannot export a Gmsh mesh without points".to_owned(),
        ));
    }
    let mut bounds = [
        f64::INFINITY,
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for point in points {
        bounds[0] = bounds[0].min(point.x);
        bounds[1] = bounds[1].min(point.y);
        bounds[2] = bounds[2].min(point.z);
        bounds[3] = bounds[3].max(point.x);
        bounds[4] = bounds[4].max(point.y);
        bounds[5] = bounds[5].max(point.z);
    }
    Ok(bounds)
}

fn scalar(value: f64) -> String {
    if value == 0.0 {
        "0".to_owned()
    } else {
        format!("{value:.15e}")
    }
}

fn append(output: &mut String, arguments: std::fmt::Arguments<'_>) -> Result<(), MeshError> {
    output
        .write_fmt(arguments)
        .map_err(|_| MeshError::ExportError("failed to format Gmsh output".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::export_gmsh;
    use crate::{config::GmshVersion, test_support::small_configuration};

    #[test]
    fn gmsh_2_2_contains_physical_names_quads_and_hexes() {
        let mesh = export_gmsh(&small_configuration()).expect("valid Gmsh 2.2");
        assert!(mesh.starts_with("$MeshFormat\n2.2 0 8\n$EndMeshFormat\n"));
        assert!(mesh.contains("\"airfoil_wall\""));
        assert!(mesh.contains("\"cellZone_upper_wake_band\""));
        assert!(mesh.lines().any(|line| line.contains(" 3 2 ")));
        assert!(mesh.lines().any(|line| line.contains(" 5 2 ")));
        assert!(!mesh.contains("NaN"));
        assert!(!mesh.contains("inf"));
    }

    #[test]
    fn gmsh_4_1_contains_entities_and_element_blocks() {
        let mut configuration = small_configuration();
        configuration.output.gmsh_version = GmshVersion::V4_1;
        let mesh = export_gmsh(&configuration).expect("valid Gmsh 4.1");
        assert!(mesh.starts_with("$MeshFormat\n4.1 0 8\n$EndMeshFormat\n"));
        assert!(mesh.contains("$Entities"));
        assert!(mesh.contains("$Nodes"));
        assert!(mesh.contains("$Elements"));
    }

    #[test]
    fn two_dimensional_export_uses_lines_and_quads() {
        let mut configuration = small_configuration();
        configuration.extrusion.enabled = false;
        let mesh = export_gmsh(&configuration).expect("valid 2D Gmsh");
        assert!(mesh.lines().any(|line| line.contains(" 1 2 ")));
        assert!(mesh.lines().any(|line| line.contains(" 3 2 ")));
        assert!(!mesh.lines().any(|line| line.contains(" 5 2 ")));
    }

    #[test]
    fn gmsh_output_is_byte_deterministic() {
        let configuration = small_configuration();
        let first = export_gmsh(&configuration).expect("first export");
        let second = export_gmsh(&configuration).expect("second export");
        assert_eq!(first, second);
    }
}
