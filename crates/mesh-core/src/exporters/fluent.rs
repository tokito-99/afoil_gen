//! Low-memory ANSYS Fluent legacy ASCII mesh export.

use std::{
    collections::{BTreeMap, btree_map::Entry},
    fmt,
    io::Write,
};

use crate::{
    config::{FrontBackPatchType, MeshConfiguration, PatchType},
    error::MeshError,
    mesh::{
        BoundaryPatch2D, StructuredMesh2D, extrusion::spanwise_planes, quality::MeshQualityReport,
        structured::generate_structured_mesh, validation::require_valid_mesh_2d,
    },
    topology::{BoundaryRole, build_cgrid_topology},
};

const INTERIOR_ZONE_ID: usize = 2;
const FIRST_BOUNDARY_ZONE_ID: usize = 3;

/// Counts produced while streaming a Fluent mesh.
#[derive(Debug, Clone, PartialEq)]
pub struct FluentStreamSummary {
    pub dimension: usize,
    pub point_count: usize,
    pub cell_count: usize,
    pub face_count: usize,
    pub quality_2d: MeshQualityReport,
}

#[derive(Debug, Clone, Copy)]
struct EdgeFace {
    nodes: [usize; 2],
    cell_0: usize,
    cell_1: usize,
}

#[derive(Debug, Clone)]
struct FaceZone {
    id: usize,
    boundary_condition: usize,
    zone_type: &'static str,
    name: String,
}

/// Generates an ANSYS Fluent legacy ASCII `.msh` file in memory.
///
/// For large three-dimensional meshes, prefer [`write_fluent_streaming`] so
/// the complete file is never duplicated as a Rust and JavaScript string.
///
/// # Errors
///
/// Returns configuration, topology, connectivity, formatting, or I/O errors.
pub fn export_fluent(configuration: &MeshConfiguration) -> Result<String, MeshError> {
    let mut output = Vec::new();
    write_fluent_streaming(configuration, &mut output)?;
    String::from_utf8(output)
        .map_err(|error| MeshError::ExportError(format!("Fluent output was not UTF-8: {error}")))
}

/// Writes an ANSYS Fluent legacy ASCII `.msh` without allocating the complete
/// extruded mesh or output string.
///
/// `EMPTY` or non-extruded configurations are written as native 2-D
/// quadrilateral meshes. Other extrusions are written as 3-D hexahedral meshes.
///
/// # Errors
///
/// Returns configuration, topology, connectivity, formatting, or I/O errors.
pub fn write_fluent_streaming(
    configuration: &MeshConfiguration,
    output: &mut impl Write,
) -> Result<FluentStreamSummary, MeshError> {
    let topology = build_cgrid_topology(configuration)?;
    let mesh = generate_structured_mesh(&topology, configuration)?;
    let quality_2d = require_valid_mesh_2d(&mesh)?;
    let edges = collect_edges(&mesh)?;
    let dimension = fluent_dimension(configuration);

    let (point_count, cell_count, face_count) = if dimension == 2 {
        write_2d(configuration, &mesh, &edges, output)?
    } else {
        write_3d(configuration, &mesh, &edges, output)?
    };

    Ok(FluentStreamSummary {
        dimension,
        point_count,
        cell_count,
        face_count,
        quality_2d,
    })
}

const fn fluent_dimension(configuration: &MeshConfiguration) -> usize {
    if !configuration.extrusion.enabled
        || matches!(
            configuration.extrusion.front_back_patch_type,
            FrontBackPatchType::Empty
        )
    {
        2
    } else {
        3
    }
}

fn collect_edges(mesh: &StructuredMesh2D) -> Result<BTreeMap<[usize; 2], EdgeFace>, MeshError> {
    let mut edges = BTreeMap::new();
    for (cell_index, cell) in mesh.cells.iter().enumerate() {
        let cell_id = cell_index + 1;
        for nodes in [
            [cell.vertices[0], cell.vertices[1]],
            [cell.vertices[1], cell.vertices[2]],
            [cell.vertices[2], cell.vertices[3]],
            [cell.vertices[3], cell.vertices[0]],
        ] {
            let key = canonical_edge(nodes);
            match edges.entry(key) {
                Entry::Vacant(entry) => {
                    entry.insert(EdgeFace {
                        nodes,
                        cell_0: cell_id,
                        cell_1: 0,
                    });
                }
                Entry::Occupied(mut entry) => {
                    let face = entry.get_mut();
                    if face.cell_1 != 0 {
                        return Err(MeshError::ExportError(format!(
                            "edge {key:?} has more than two adjacent cells"
                        )));
                    }
                    if face.nodes != [nodes[1], nodes[0]] {
                        return Err(MeshError::ExportError(format!(
                            "cells sharing edge {key:?} do not have opposite orientation"
                        )));
                    }
                    face.cell_1 = cell_id;
                }
            }
        }
    }
    Ok(edges)
}

#[allow(clippy::too_many_lines)]
fn write_2d(
    _configuration: &MeshConfiguration,
    mesh: &StructuredMesh2D,
    edges: &BTreeMap<[usize; 2], EdgeFace>,
    output: &mut impl Write,
) -> Result<(usize, usize, usize), MeshError> {
    let point_count = mesh.points.len();
    let cell_count = mesh.cells.len();
    let face_count = edges.len();
    let interior_count = edges.values().filter(|face| face.cell_1 != 0).count();
    let zones = boundary_zones(&mesh.boundaries);

    write_declarations(output, 2, point_count, cell_count, face_count)?;
    writeln!(output, "(10 (1 1 {point_count:x} 1 2)(")?;
    for point in &mesh.points {
        writeln!(output, "{} {}", Scalar(point.x), Scalar(point.y))?;
    }
    writeln!(output, "))")?;
    writeln!(output, "(12 (1 1 {cell_count:x} 1 3))")?;

    let mut next_face = 1;
    if interior_count > 0 {
        let end = next_face + interior_count - 1;
        writeln!(
            output,
            "(13 ({INTERIOR_ZONE_ID:x} {next_face:x} {end:x} 2 2)("
        )?;
        for face in edges.values().filter(|face| face.cell_1 != 0) {
            writeln!(
                output,
                "{:x} {:x} {:x} {:x}",
                face.nodes[0] + 1,
                face.nodes[1] + 1,
                face.cell_0,
                face.cell_1
            )?;
        }
        writeln!(output, "))")?;
        next_face = end + 1;
    }

    for (patch, zone) in mesh.boundaries.iter().zip(&zones) {
        let count = patch.edges.len();
        if count == 0 {
            continue;
        }
        let end = next_face + count - 1;
        writeln!(
            output,
            "(13 ({:x} {:x} {:x} {:x} 2)(",
            zone.id, next_face, end, zone.boundary_condition
        )?;
        for edge in &patch.edges {
            let face = require_boundary_face(edges, *edge, &patch.name)?;
            writeln!(
                output,
                "{:x} {:x} {:x} 0",
                face.nodes[0] + 1,
                face.nodes[1] + 1,
                face.cell_0
            )?;
        }
        writeln!(output, "))")?;
        next_face = end + 1;
    }

    ensure_face_count(next_face, face_count)?;
    write_zone_metadata(output, &zones, interior_count > 0)?;
    Ok((point_count, cell_count, face_count))
}

#[allow(clippy::too_many_lines)]
fn write_3d(
    configuration: &MeshConfiguration,
    mesh: &StructuredMesh2D,
    edges: &BTreeMap<[usize; 2], EdgeFace>,
    output: &mut impl Write,
) -> Result<(usize, usize, usize), MeshError> {
    let planes = spanwise_planes(configuration)?;
    let spanwise_cells = configuration.mesh.spanwise_cells;
    let points_per_plane = mesh.points.len();
    let cells_per_layer = mesh.cells.len();
    let point_count = checked_product(points_per_plane, planes.len(), "Fluent point count")?;
    let cell_count = checked_product(cells_per_layer, spanwise_cells, "Fluent cell count")?;
    let side_face_count = checked_product(edges.len(), spanwise_cells, "Fluent face count")?;
    let span_face_count = checked_product(cells_per_layer, planes.len(), "Fluent face count")?;
    let face_count = side_face_count
        .checked_add(span_face_count)
        .ok_or_else(|| MeshError::ExportError("Fluent face count overflow".to_owned()))?;
    let interior_edge_count = edges.values().filter(|face| face.cell_1 != 0).count();
    let interior_side_count = checked_product(
        interior_edge_count,
        spanwise_cells,
        "Fluent interior face count",
    )?;
    let interior_span_count = checked_product(
        cells_per_layer,
        spanwise_cells.saturating_sub(1),
        "Fluent interior face count",
    )?;
    let interior_count = interior_side_count
        .checked_add(interior_span_count)
        .ok_or_else(|| MeshError::ExportError("Fluent interior face count overflow".to_owned()))?;
    let mut zones = boundary_zones(&mesh.boundaries);
    let front_zone = front_back_zone(
        FIRST_BOUNDARY_ZONE_ID + zones.len(),
        &configuration.boundaries.front.name,
        configuration.boundaries.front.patch_type,
        false,
    );
    let back_zone = front_back_zone(
        front_zone.id + 1,
        &configuration.boundaries.back.name,
        configuration.boundaries.back.patch_type,
        true,
    );
    zones.extend([front_zone.clone(), back_zone.clone()]);

    write_declarations(output, 3, point_count, cell_count, face_count)?;
    writeln!(output, "(10 (1 1 {point_count:x} 1 3)(")?;
    for &z in &planes {
        for point in &mesh.points {
            writeln!(
                output,
                "{} {} {}",
                Scalar(point.x),
                Scalar(point.y),
                Scalar(z)
            )?;
        }
    }
    writeln!(output, "))")?;
    writeln!(output, "(12 (1 1 {cell_count:x} 1 4))")?;

    let mut next_face = 1;
    if interior_count > 0 {
        let end = next_face + interior_count - 1;
        writeln!(
            output,
            "(13 ({INTERIOR_ZONE_ID:x} {next_face:x} {end:x} 2 4)("
        )?;
        for span_index in 0..spanwise_cells {
            let lower_offset = span_index * points_per_plane;
            let upper_offset = (span_index + 1) * points_per_plane;
            let cell_offset = span_index * cells_per_layer;
            for face in edges.values().filter(|face| face.cell_1 != 0) {
                write_quad_face(
                    output,
                    [
                        lower_offset + face.nodes[0] + 1,
                        upper_offset + face.nodes[0] + 1,
                        upper_offset + face.nodes[1] + 1,
                        lower_offset + face.nodes[1] + 1,
                    ],
                    cell_offset + face.cell_0,
                    cell_offset + face.cell_1,
                )?;
            }
        }
        for span_index in 0..spanwise_cells.saturating_sub(1) {
            let plane_offset = (span_index + 1) * points_per_plane;
            let lower_cell_offset = span_index * cells_per_layer;
            let upper_cell_offset = (span_index + 1) * cells_per_layer;
            for (cell_index, cell) in mesh.cells.iter().enumerate() {
                write_quad_face(
                    output,
                    [
                        plane_offset + cell.vertices[0] + 1,
                        plane_offset + cell.vertices[3] + 1,
                        plane_offset + cell.vertices[2] + 1,
                        plane_offset + cell.vertices[1] + 1,
                    ],
                    lower_cell_offset + cell_index + 1,
                    upper_cell_offset + cell_index + 1,
                )?;
            }
        }
        writeln!(output, "))")?;
        next_face = end + 1;
    }

    for (patch, zone) in mesh
        .boundaries
        .iter()
        .zip(zones.iter().take(mesh.boundaries.len()))
    {
        let count = checked_product(patch.edges.len(), spanwise_cells, "Fluent patch face count")?;
        if count == 0 {
            continue;
        }
        let end = next_face + count - 1;
        writeln!(
            output,
            "(13 ({:x} {:x} {:x} {:x} 4)(",
            zone.id, next_face, end, zone.boundary_condition
        )?;
        for span_index in 0..spanwise_cells {
            let lower_offset = span_index * points_per_plane;
            let upper_offset = (span_index + 1) * points_per_plane;
            let cell_offset = span_index * cells_per_layer;
            for edge in &patch.edges {
                let face = require_boundary_face(edges, *edge, &patch.name)?;
                write_quad_face(
                    output,
                    [
                        lower_offset + face.nodes[0] + 1,
                        upper_offset + face.nodes[0] + 1,
                        upper_offset + face.nodes[1] + 1,
                        lower_offset + face.nodes[1] + 1,
                    ],
                    cell_offset + face.cell_0,
                    0,
                )?;
            }
        }
        writeln!(output, "))")?;
        next_face = end + 1;
    }

    let front_start = next_face;
    let front_end = front_start + cells_per_layer - 1;
    writeln!(
        output,
        "(13 ({:x} {:x} {:x} {:x} 4)(",
        front_zone.id, front_start, front_end, front_zone.boundary_condition
    )?;
    for (cell_index, cell) in mesh.cells.iter().enumerate() {
        write_quad_face(
            output,
            [
                cell.vertices[0] + 1,
                cell.vertices[1] + 1,
                cell.vertices[2] + 1,
                cell.vertices[3] + 1,
            ],
            cell_index + 1,
            0,
        )?;
    }
    writeln!(output, "))")?;
    next_face = front_end + 1;

    let back_start = next_face;
    let back_end = back_start + cells_per_layer - 1;
    let back_node_offset = spanwise_cells * points_per_plane;
    let back_cell_offset = (spanwise_cells - 1) * cells_per_layer;
    writeln!(
        output,
        "(13 ({:x} {:x} {:x} {:x} 4)(",
        back_zone.id, back_start, back_end, back_zone.boundary_condition
    )?;
    for (cell_index, cell) in mesh.cells.iter().enumerate() {
        write_quad_face(
            output,
            [
                back_node_offset + cell.vertices[0] + 1,
                back_node_offset + cell.vertices[1] + 1,
                back_node_offset + cell.vertices[2] + 1,
                back_node_offset + cell.vertices[3] + 1,
            ],
            0,
            back_cell_offset + cell_index + 1,
        )?;
    }
    writeln!(output, "))")?;
    next_face = back_end + 1;

    ensure_face_count(next_face, face_count)?;
    if configuration.extrusion.front_back_patch_type == FrontBackPatchType::Cyclic {
        writeln!(
            output,
            "(18 (1 {:x} {:x} {:x})(",
            cells_per_layer, front_zone.id, back_zone.id
        )?;
        for offset in 0..cells_per_layer {
            writeln!(
                output,
                "{:x} {:x}",
                front_start + offset,
                back_start + offset
            )?;
        }
        writeln!(output, "))")?;
    }
    write_zone_metadata(output, &zones, interior_count > 0)?;
    Ok((point_count, cell_count, face_count))
}

fn write_declarations(
    output: &mut impl Write,
    dimension: usize,
    point_count: usize,
    cell_count: usize,
    face_count: usize,
) -> Result<(), MeshError> {
    writeln!(output, "(0 \"afoil_gen ANSYS Fluent legacy ASCII mesh\")")?;
    writeln!(output, "(1 \"afoil_gen\")")?;
    writeln!(output, "(2 {dimension})")?;
    writeln!(output, "(10 (0 1 {point_count:x} 0 {dimension}))")?;
    writeln!(output, "(12 (0 1 {cell_count:x} 0))")?;
    writeln!(output, "(13 (0 1 {face_count:x} 0))")?;
    Ok(())
}

fn write_quad_face(
    output: &mut impl Write,
    nodes: [usize; 4],
    cell_0: usize,
    cell_1: usize,
) -> Result<(), MeshError> {
    writeln!(
        output,
        "{:x} {:x} {:x} {:x} {:x} {:x}",
        nodes[0], nodes[1], nodes[2], nodes[3], cell_0, cell_1
    )?;
    Ok(())
}

fn write_zone_metadata(
    output: &mut impl Write,
    zones: &[FaceZone],
    has_interior: bool,
) -> Result<(), MeshError> {
    writeln!(output, "(39 (1 fluid fluid 1)())")?;
    if has_interior {
        writeln!(output, "(39 (2 interior interior 1)())")?;
    }
    for zone in zones {
        writeln!(
            output,
            "(39 ({} {} {} 1)())",
            zone.id, zone.zone_type, zone.name
        )?;
    }
    Ok(())
}

fn boundary_zones(patches: &[BoundaryPatch2D]) -> Vec<FaceZone> {
    patches
        .iter()
        .enumerate()
        .map(|(index, patch)| {
            let (boundary_condition, zone_type) = patch_zone_type(patch.role, patch.patch_type);
            FaceZone {
                id: FIRST_BOUNDARY_ZONE_ID + index,
                boundary_condition,
                zone_type,
                name: fluent_name(&patch.name),
            }
        })
        .collect()
}

const fn patch_zone_type(role: BoundaryRole, patch_type: PatchType) -> (usize, &'static str) {
    match patch_type {
        PatchType::Wall => (3, "wall"),
        PatchType::SymmetryPlane | PatchType::Empty => (7, "symmetry"),
        PatchType::Cyclic => (12, "periodic"),
        PatchType::Patch => match role {
            BoundaryRole::Inlet => (10, "velocity-inlet"),
            BoundaryRole::Outlet => (5, "pressure-outlet"),
            BoundaryRole::Airfoil => (3, "wall"),
            BoundaryRole::UpperFarfield | BoundaryRole::LowerFarfield => (9, "pressure-far-field"),
        },
    }
}

fn front_back_zone(id: usize, name: &str, patch_type: PatchType, shadow: bool) -> FaceZone {
    let (boundary_condition, zone_type) = match patch_type {
        PatchType::Cyclic if shadow => (8, "periodic"),
        PatchType::Cyclic => (12, "periodic"),
        PatchType::SymmetryPlane | PatchType::Empty => (7, "symmetry"),
        PatchType::Wall | PatchType::Patch => (3, "wall"),
    };
    FaceZone {
        id,
        boundary_condition,
        zone_type,
        name: fluent_name(name),
    }
}

fn require_boundary_face<'a>(
    edges: &'a BTreeMap<[usize; 2], EdgeFace>,
    edge: [usize; 2],
    patch_name: &str,
) -> Result<&'a EdgeFace, MeshError> {
    let key = canonical_edge(edge);
    let face = edges.get(&key).ok_or_else(|| {
        MeshError::ExportError(format!(
            "boundary patch {patch_name:?} references unknown edge {key:?}"
        ))
    })?;
    if face.cell_1 != 0 {
        return Err(MeshError::ExportError(format!(
            "boundary patch {patch_name:?} references interior edge {key:?}"
        )));
    }
    Ok(face)
}

const fn canonical_edge(edge: [usize; 2]) -> [usize; 2] {
    if edge[0] < edge[1] {
        edge
    } else {
        [edge[1], edge[0]]
    }
}

fn checked_product(left: usize, right: usize, label: &str) -> Result<usize, MeshError> {
    left.checked_mul(right)
        .ok_or_else(|| MeshError::ExportError(format!("{label} overflow")))
}

fn ensure_face_count(next_face: usize, expected_count: usize) -> Result<(), MeshError> {
    if next_face != expected_count + 1 {
        return Err(MeshError::ExportError(format!(
            "Fluent exporter wrote {} faces but expected {expected_count}",
            next_face - 1
        )));
    }
    Ok(())
}

fn fluent_name(name: &str) -> String {
    name.to_ascii_lowercase()
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
    use super::{export_fluent, write_fluent_streaming};
    use crate::{
        config::{FrontBackPatchType, MeshIntent, PatchType},
        test_support::small_configuration,
    };

    #[test]
    fn empty_extrusion_exports_native_2d_quadrilaterals_and_named_zones() {
        let configuration = small_configuration();
        let first = export_fluent(&configuration).expect("2D Fluent export");
        let second = export_fluent(&configuration).expect("repeated Fluent export");

        assert_eq!(first, second);
        assert!(first.starts_with("(0 \"afoil_gen ANSYS Fluent legacy ASCII mesh\")\n"));
        assert!(first.contains("(2 2)\n"));
        assert!(first.contains("(12 (1 1 "));
        assert!(first.contains(" 1 3))"));
        assert!(first.contains(" wall airfoil_wall 1)())"));
        assert!(first.contains(" velocity-inlet inlet 1)())"));
        assert!(!first.contains("(18 "));
    }

    #[test]
    fn cyclic_extrusion_streams_3d_hexes_and_periodic_face_pairs() {
        let mut configuration = small_configuration();
        configuration.mesh_intent = Some(MeshIntent::Wrles);
        configuration.les.enabled = true;
        configuration.mesh.spanwise_cells = 2;
        configuration.extrusion.front_back_patch_type = FrontBackPatchType::Cyclic;
        configuration.boundaries.front.patch_type = PatchType::Cyclic;
        configuration.boundaries.back.patch_type = PatchType::Cyclic;

        let mut output = Vec::new();
        let summary =
            write_fluent_streaming(&configuration, &mut output).expect("3D Fluent export");
        let contents = String::from_utf8(output).expect("ASCII output");

        assert_eq!(summary.dimension, 3);
        assert_eq!(summary.cell_count, summary.quality_2d.cell_count * 2);
        assert!(contents.contains("(2 3)\n"));
        assert!(contents.contains(" 1 4))"));
        assert!(contents.contains("(18 (1 "));
        assert!(contents.contains(" periodic front_plane 1)())"));
        assert!(contents.contains(" periodic back_plane 1)())"));

        let front = first_face_in_zone(&contents, 8);
        let back = first_face_in_zone(&contents, 9);
        let node_offsets = (0..4)
            .map(|index| back[index] - front[index])
            .collect::<Vec<_>>();
        assert!(
            node_offsets.windows(2).all(|pair| pair[0] == pair[1]),
            "periodic node correspondence must be a pure translation"
        );
        assert!(front[4] > 0);
        assert_eq!(front[5], 0);
        assert_eq!(back[4], 0);
        assert!(back[5] > 0);
    }

    fn first_face_in_zone(contents: &str, zone_id: usize) -> Vec<usize> {
        let marker = format!("(13 ({zone_id:x} ");
        let mut lines = contents.lines();
        lines
            .find(|line| line.starts_with(&marker))
            .expect("face-zone header");
        lines
            .next()
            .expect("first face")
            .split_ascii_whitespace()
            .map(|value| usize::from_str_radix(value, 16).expect("hex face field"))
            .collect()
    }
}
