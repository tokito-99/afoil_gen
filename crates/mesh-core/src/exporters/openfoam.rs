//! Native structured `OpenFOAM` `blockMeshDict` export.

use std::{
    collections::{BTreeSet, HashMap},
    fmt::Write,
};

use crate::{
    config::{BoundaryDefinition, MeshConfiguration, PatchType},
    error::MeshError,
    geometry::Point2,
    les::stretching::GradingSection,
    mesh::{
        generation::GeneratedCGrid, structured::generate_structured_mesh,
        validation::require_valid_mesh_2d,
    },
    topology::{
        BlockSide, BoundaryRole, CurveSpec, LogicalBlock2D, ParametricCurve, build_cgrid_topology,
    },
};

const SPLINE_SEGMENT_COUNT: usize = 48;
const HEADER: &str = r"/*--------------------------------*- C++ -*----------------------------------*\
| afoil_gen deterministic structured C-grid                              |
\*---------------------------------------------------------------------------*/
FoamFile
{
    version     2.0;
    format      ascii;
    class       dictionary;
    object      blockMeshDict;
}
// Generated patch types are mesh classifications only.
// Solver field boundary values must be configured separately.

convertToMeters 1;

";

/// Generates a validated native `OpenFOAM` `blockMeshDict`.
///
/// # Errors
///
/// Returns generation/quality errors, rejects disabled extrusion, and rejects
/// cyclic patches other than the paired front/back planes.
pub fn export_openfoam(configuration: &MeshConfiguration) -> Result<String, MeshError> {
    let topology = build_cgrid_topology(configuration)?;
    let mesh_2d = generate_structured_mesh(&topology, configuration)?;
    let quality_2d = require_valid_mesh_2d(&mesh_2d)?;
    let generated = GeneratedCGrid {
        topology,
        mesh_2d,
        quality_2d,
        mesh_3d: None,
        quality_3d: None,
    };
    export_openfoam_from_generated(configuration, &generated)
}

/// Generates `blockMeshDict` from an already validated Phase 4–6 result.
///
/// # Errors
///
/// Returns an export error when the configuration cannot be represented by the
/// native sharp/finite trailing-edge extrusion.
pub fn export_openfoam_from_generated(
    configuration: &MeshConfiguration,
    generated: &GeneratedCGrid,
) -> Result<String, MeshError> {
    if configuration.optimization.orthogonalize_wall
        || configuration.optimization.smoothing_iterations > 0
        || configuration.les.wall_shear_profile.is_some()
    {
        return Err(MeshError::ExportError("blockMeshDict cannot represent optimized interior coordinates or spatial grading; export the actual Gmsh/Fluent mesh and convert it to OpenFOAM instead".to_owned()));
    }
    validate_openfoam_configuration(configuration)?;
    let compact = compact_block_geometry(generated)?;
    let z_front = -0.5 * configuration.extrusion.spanwise_thickness;
    let z_back = 0.5 * configuration.extrusion.spanwise_thickness;
    let plane_vertex_count = compact.points.len();

    let mut output = String::new();
    output.push_str(HEADER);

    append(&mut output, format_args!("vertices\n(\n"))?;
    for (plane, z) in [(0_usize, z_front), (1, z_back)] {
        append(
            &mut output,
            format_args!(
                "    // z-plane {plane}: {}\n",
                if plane == 0 { "front" } else { "back" }
            ),
        )?;
        for (index, point) in compact.points.iter().enumerate() {
            let vertex_id = plane * plane_vertex_count + index;
            append(
                &mut output,
                format_args!(
                    "    ({} {} {}) // {}\n",
                    scalar(point.x),
                    scalar(point.y),
                    scalar(z),
                    vertex_id
                ),
            )?;
        }
    }
    append(&mut output, format_args!(");\n\n"))?;

    append(&mut output, format_args!("blocks\n(\n"))?;
    for (block, corners) in generated.topology.blocks.iter().zip(&compact.block_corners) {
        let front = *corners;
        let back = corners.map(|vertex| vertex + plane_vertex_count);
        let xi_grading = grading_from_positions(&block.xi_positions)?;
        let eta_grading = multi_grading(&generated.topology.radial_grading_sections)?;
        append(
            &mut output,
            format_args!(
                "    // Block {}: {}\n\
                 \x20   hex ({} {} {} {} {} {} {} {}) ({} {} {}) simpleGrading ({} {} {})\n",
                block.id,
                block.name,
                front[0],
                front[1],
                front[2],
                front[3],
                back[0],
                back[1],
                back[2],
                back[3],
                block.ni,
                block.nj,
                configuration.mesh.spanwise_cells,
                xi_grading,
                eta_grading,
                scalar(configuration.mesh.spanwise_grading)
            ),
        )?;
    }
    append(&mut output, format_args!(");\n\n"))?;

    write_curved_edges(
        &mut output,
        &generated.topology.blocks,
        &compact.block_corners,
        plane_vertex_count,
        z_front,
        z_back,
    )?;
    write_boundaries(
        &mut output,
        configuration,
        &generated.topology.blocks,
        &compact.block_corners,
        plane_vertex_count,
    )?;
    append(
        &mut output,
        format_args!(
            "mergePatchPairs\n(\n);\n\n// ************************************************************************* //\n"
        ),
    )?;
    Ok(output)
}

#[derive(Debug)]
struct CompactBlockGeometry {
    points: Vec<Point2>,
    block_corners: Vec<[usize; 4]>,
}

fn compact_block_geometry(generated: &GeneratedCGrid) -> Result<CompactBlockGeometry, MeshError> {
    let mut global_to_compact = HashMap::new();
    let mut points = Vec::new();
    let mut block_corners = Vec::with_capacity(generated.mesh_2d.blocks.len());
    for block in &generated.mesh_2d.blocks {
        let globals = [
            block.global_index(0, 0),
            block.global_index(block.ni, 0),
            block.global_index(block.ni, block.nj),
            block.global_index(0, block.nj),
        ];
        let mut compact_ids = [0_usize; 4];
        for (corner_index, global_id) in globals.into_iter().enumerate() {
            let compact_id = if let Some(existing) = global_to_compact.get(&global_id) {
                *existing
            } else {
                let id = points.len();
                let point = *generated.mesh_2d.points.get(global_id).ok_or_else(|| {
                    MeshError::ExportError(
                        "block corner references an unknown global point".to_owned(),
                    )
                })?;
                global_to_compact.insert(global_id, id);
                points.push(point);
                id
            };
            compact_ids[corner_index] = compact_id;
        }
        block_corners.push(compact_ids);
    }
    Ok(CompactBlockGeometry {
        points,
        block_corners,
    })
}

fn write_curved_edges(
    output: &mut String,
    blocks: &[LogicalBlock2D],
    block_corners: &[[usize; 4]],
    plane_vertex_count: usize,
    z_front: f64,
    z_back: f64,
) -> Result<(), MeshError> {
    append(output, format_args!("edges\n(\n"))?;
    let mut written = BTreeSet::new();
    for (block, corners) in blocks.iter().zip(block_corners) {
        for (side, curve, start, end) in [
            (BlockSide::Bottom, &block.bottom, corners[0], corners[1]),
            (BlockSide::Top, &block.top, corners[3], corners[2]),
            (BlockSide::Left, &block.left, corners[0], corners[3]),
            (BlockSide::Right, &block.right, corners[1], corners[2]),
        ] {
            if curve.is_linear() {
                continue;
            }
            for (plane, z) in [(0_usize, z_front), (1, z_back)] {
                let plane_offset = plane * plane_vertex_count;
                let start_id = plane_offset + start;
                let end_id = plane_offset + end;
                let key = ordered_pair(start_id, end_id);
                if !written.insert(key) {
                    continue;
                }
                append(
                    output,
                    format_args!(
                        "    // Block {} {:?}, {} plane\n",
                        block.id,
                        side,
                        if plane == 0 { "front" } else { "back" },
                    ),
                )?;
                if matches!(curve, CurveSpec::CircularArc(_)) {
                    let midpoint = curve.point(0.5);
                    append(
                        output,
                        format_args!(
                            "    arc {} {} ({} {} {})\n",
                            start_id,
                            end_id,
                            scalar(midpoint.x),
                            scalar(midpoint.y),
                            scalar(z)
                        ),
                    )?;
                } else {
                    append(
                        output,
                        format_args!("    spline {start_id} {end_id}\n    (\n"),
                    )?;
                    for point in curve
                        .sampled_points(SPLINE_SEGMENT_COUNT)
                        .into_iter()
                        .skip(1)
                        .take(SPLINE_SEGMENT_COUNT - 1)
                    {
                        append(
                            output,
                            format_args!(
                                "        ({} {} {})\n",
                                scalar(point.x),
                                scalar(point.y),
                                scalar(z)
                            ),
                        )?;
                    }
                    append(output, format_args!("    )\n"))?;
                }
            }
        }
    }
    append(output, format_args!(");\n\n"))?;
    Ok(())
}

fn write_boundaries(
    output: &mut String,
    configuration: &MeshConfiguration,
    blocks: &[LogicalBlock2D],
    block_corners: &[[usize; 4]],
    plane_vertex_count: usize,
) -> Result<(), MeshError> {
    let mut role_faces = [
        (BoundaryRole::Inlet, Vec::new()),
        (BoundaryRole::Outlet, Vec::new()),
        (BoundaryRole::Airfoil, Vec::new()),
        (BoundaryRole::UpperFarfield, Vec::new()),
        (BoundaryRole::LowerFarfield, Vec::new()),
    ];
    for (block, corners) in blocks.iter().zip(block_corners) {
        for side in [
            BlockSide::Bottom,
            BlockSide::Top,
            BlockSide::Left,
            BlockSide::Right,
        ] {
            if let Some(role) = block.boundaries.get(side) {
                let face = side_face(*corners, plane_vertex_count, side);
                role_faces[role_index(role)].1.push(face);
            }
        }
    }

    append(output, format_args!("boundary\n(\n"))?;
    let definitions = [
        &configuration.boundaries.inlet,
        &configuration.boundaries.outlet,
        &configuration.boundaries.airfoil,
        &configuration.boundaries.upper_farfield,
        &configuration.boundaries.lower_farfield,
    ];
    for ((_, faces), definition) in role_faces.iter().zip(definitions) {
        write_patch(output, definition, faces, None)?;
    }

    let front_faces = block_corners
        .iter()
        .map(|corners| [corners[0], corners[3], corners[2], corners[1]])
        .collect::<Vec<_>>();
    let back_faces = block_corners
        .iter()
        .map(|corners| {
            [
                plane_vertex_count + corners[0],
                plane_vertex_count + corners[1],
                plane_vertex_count + corners[2],
                plane_vertex_count + corners[3],
            ]
        })
        .collect::<Vec<_>>();
    let front_neighbor = (configuration.boundaries.front.patch_type == PatchType::Cyclic)
        .then_some(configuration.boundaries.back.name.as_str());
    let back_neighbor = (configuration.boundaries.back.patch_type == PatchType::Cyclic)
        .then_some(configuration.boundaries.front.name.as_str());
    write_patch(
        output,
        &configuration.boundaries.front,
        &front_faces,
        front_neighbor,
    )?;
    write_patch(
        output,
        &configuration.boundaries.back,
        &back_faces,
        back_neighbor,
    )?;
    append(output, format_args!(");\n\n"))?;
    Ok(())
}

fn write_patch(
    output: &mut String,
    definition: &BoundaryDefinition,
    faces: &[[usize; 4]],
    neighbor: Option<&str>,
) -> Result<(), MeshError> {
    append(
        output,
        format_args!(
            "    // Mesh patch classification; solver field values are not assigned here.\n\
             \x20   {}\n\
             \x20   {{\n\
             \x20       type {};\n",
            definition.name,
            foam_patch_type(definition.patch_type)
        ),
    )?;
    if let Some(neighbor_name) = neighbor {
        append(
            output,
            format_args!("        neighbourPatch {neighbor_name};\n"),
        )?;
    }
    append(output, format_args!("        faces\n        (\n"))?;
    for face in faces {
        append(
            output,
            format_args!(
                "            ({} {} {} {})\n",
                face[0], face[1], face[2], face[3]
            ),
        )?;
    }
    append(output, format_args!("        );\n    }}\n"))?;
    Ok(())
}

fn side_face(corners: [usize; 4], plane_vertex_count: usize, side: BlockSide) -> [usize; 4] {
    let back = corners.map(|vertex| vertex + plane_vertex_count);
    match side {
        BlockSide::Bottom => [corners[0], corners[1], back[1], back[0]],
        BlockSide::Top => [corners[3], back[3], back[2], corners[2]],
        BlockSide::Left => [corners[0], back[0], back[3], corners[3]],
        BlockSide::Right => [corners[1], corners[2], back[2], back[1]],
    }
}

fn expansion_ratio(positions: &[f64]) -> Result<f64, MeshError> {
    if positions.len() < 2 {
        return Err(MeshError::ExportError(
            "grading requires at least one cell".to_owned(),
        ));
    }
    let first = positions[1] - positions[0];
    let last = positions[positions.len() - 1] - positions[positions.len() - 2];
    if !first.is_finite() || !last.is_finite() || first <= 0.0 || last <= 0.0 {
        return Err(MeshError::ExportError(
            "grading positions are invalid".to_owned(),
        ));
    }
    Ok(last / first)
}

fn grading_from_positions(positions: &[f64]) -> Result<String, MeshError> {
    let overall_expansion = expansion_ratio(positions)?;
    let sizes = positions
        .windows(2)
        .map(|pair| pair[1] - pair[0])
        .collect::<Vec<_>>();
    if sizes.iter().any(|size| !size.is_finite() || *size <= 0.0) {
        return Err(MeshError::ExportError(
            "grading positions are invalid".to_owned(),
        ));
    }
    if sizes.len() == 1 {
        return Ok(scalar(overall_expansion));
    }

    let first_ratio = sizes[1] / sizes[0];
    if sizes
        .windows(2)
        .all(|pair| approximately_equal(pair[1] / pair[0], first_ratio))
    {
        return Ok(scalar(overall_expansion));
    }

    let total_size = sizes.iter().sum::<f64>();
    #[allow(clippy::cast_precision_loss)]
    let total_cells = sizes.len() as f64;
    let mut sections = Vec::new();
    let mut start = 0;
    while start < sizes.len() {
        let mut end = (start + 1).min(sizes.len());
        if start + 1 < sizes.len() {
            let section_ratio = sizes[start + 1] / sizes[start];
            end = start + 2;
            while end < sizes.len()
                && approximately_equal(sizes[end] / sizes[end - 1], section_ratio)
            {
                end += 1;
            }
        }

        let section_size = sizes[start..end].iter().sum::<f64>();
        #[allow(clippy::cast_precision_loss)]
        let division_fraction = (end - start) as f64 / total_cells;
        let section_expansion = sizes[end - 1] / sizes[start];
        sections.push(format!(
            "({} {} {})",
            scalar(section_size / total_size),
            scalar(division_fraction),
            scalar(section_expansion)
        ));
        start = end;
    }

    Ok(format!("({})", sections.join(" ")))
}

fn approximately_equal(left: f64, right: f64) -> bool {
    (left - right).abs() <= 1.0e-10 * left.abs().max(right.abs()).max(1.0)
}

fn multi_grading(sections: &[GradingSection]) -> Result<String, MeshError> {
    if sections.is_empty()
        || sections.iter().any(|section| {
            !section.block_fraction.is_finite()
                || section.block_fraction <= 0.0
                || !section.division_fraction.is_finite()
                || section.division_fraction <= 0.0
                || !section.expansion_ratio.is_finite()
                || section.expansion_ratio <= 0.0
        })
    {
        return Err(MeshError::ExportError(
            "radial multi-grading sections are invalid".to_owned(),
        ));
    }
    let values = sections
        .iter()
        .map(|section| {
            format!(
                "({} {} {})",
                scalar(section.block_fraction),
                scalar(section.division_fraction),
                scalar(section.expansion_ratio)
            )
        })
        .collect::<Vec<_>>()
        .join(" ");
    Ok(format!("({values})"))
}

fn validate_openfoam_configuration(configuration: &MeshConfiguration) -> Result<(), MeshError> {
    if !configuration.extrusion.enabled {
        return Err(MeshError::ExportError(
            "OpenFOAM blockMesh export requires extrusion to be enabled".to_owned(),
        ));
    }
    for (field, boundary) in configuration.boundaries.ordered().into_iter().take(5) {
        if boundary.patch_type == PatchType::Cyclic {
            return Err(MeshError::ExportError(format!(
                "{field} is CYCLIC but only the paired front/back cyclic configuration is supported"
            )));
        }
    }
    Ok(())
}

const fn role_index(role: BoundaryRole) -> usize {
    match role {
        BoundaryRole::Inlet => 0,
        BoundaryRole::Outlet => 1,
        BoundaryRole::Airfoil => 2,
        BoundaryRole::UpperFarfield => 3,
        BoundaryRole::LowerFarfield => 4,
    }
}

const fn foam_patch_type(patch_type: PatchType) -> &'static str {
    match patch_type {
        PatchType::Patch => "patch",
        PatchType::Wall => "wall",
        PatchType::Empty => "empty",
        PatchType::SymmetryPlane => "symmetryPlane",
        PatchType::Cyclic => "cyclic",
    }
}

fn scalar(value: f64) -> String {
    if value == 0.0 {
        "0".to_owned()
    } else {
        format!("{value:.15e}")
    }
}

const fn ordered_pair(first: usize, second: usize) -> (usize, usize) {
    if first < second {
        (first, second)
    } else {
        (second, first)
    }
}

fn append(output: &mut String, arguments: std::fmt::Arguments<'_>) -> Result<(), MeshError> {
    output
        .write_fmt(arguments)
        .map_err(|_| MeshError::ExportError("failed to format blockMeshDict".to_owned()))
}

#[cfg(test)]
mod tests {
    use std::f64::consts::FRAC_PI_2;

    use super::{export_openfoam, grading_from_positions, write_curved_edges};
    use crate::{
        config::{FrontBackPatchType, PatchType},
        geometry::Point2,
        test_support::small_configuration,
        topology::{
            BlockBoundaryAssignments, CircularArcCurve, CurveSpec, LineCurve, LogicalBlock2D,
        },
    };

    #[test]
    fn native_export_contains_sharp_te_wake_cut_and_stable_patches() {
        let dictionary = export_openfoam(&small_configuration()).expect("valid blockMeshDict");
        assert!(dictionary.contains("object      blockMeshDict;"));
        assert_eq!(dictionary.matches("hex (").count(), 6);
        assert!(dictionary.contains("spline "));
        for name in [
            "inlet",
            "outlet",
            "airfoil_wall",
            "upper_farfield",
            "lower_farfield",
            "front_plane",
            "back_plane",
        ] {
            assert!(dictionary.contains(name));
        }
        assert!(!dictionary.contains("NaN"));
        assert!(!dictionary.contains("inf"));
    }

    #[test]
    fn openfoam_output_is_byte_deterministic() {
        let configuration = small_configuration();
        let first = export_openfoam(&configuration).expect("first export");
        let second = export_openfoam(&configuration).expect("second export");
        assert_eq!(first, second);
    }

    #[test]
    fn piecewise_positions_become_exact_multigrading_sections() {
        let grading =
            grading_from_positions(&[0.0, 0.1, 0.3, 0.65, 1.0]).expect("valid piecewise grading");
        assert_eq!(
            grading,
            "((3.000000000000000e-1 5.000000000000000e-1 2.000000000000000e0) \
             (7.000000000000000e-1 5.000000000000000e-1 9.999999999999999e-1))"
        );
    }

    #[test]
    fn empty_front_and_back_have_one_spanwise_cell() {
        let dictionary = export_openfoam(&small_configuration()).expect("valid blockMeshDict");
        assert!(dictionary.contains("type empty;"));
        assert!(dictionary.contains("(8 64 1)"));
    }

    #[test]
    fn disabled_extrusion_is_rejected() {
        let mut configuration = small_configuration();
        configuration.extrusion.enabled = false;
        assert!(export_openfoam(&configuration).is_err());
    }

    #[test]
    fn paired_cyclic_planes_have_reciprocal_neighbors() {
        let mut configuration = small_configuration();
        configuration.extrusion.front_back_patch_type = FrontBackPatchType::Cyclic;
        configuration.boundaries.front.patch_type = PatchType::Cyclic;
        configuration.boundaries.back.patch_type = PatchType::Cyclic;
        let dictionary = export_openfoam(&configuration).expect("cyclic blockMeshDict");
        assert!(dictionary.contains("neighbourPatch back_plane;"));
        assert!(dictionary.contains("neighbourPatch front_plane;"));
    }

    #[test]
    fn an_unpaired_side_cyclic_patch_is_rejected() {
        let mut configuration = small_configuration();
        configuration.boundaries.inlet.patch_type = PatchType::Cyclic;
        assert!(export_openfoam(&configuration).is_err());
    }

    #[test]
    fn circular_curve_uses_native_arc_edge() {
        let block = LogicalBlock2D {
            id: 0,
            name: "arc_test".to_owned(),
            ni: 1,
            nj: 1,
            xi_positions: vec![0.0, 1.0],
            eta_positions: vec![0.0, 1.0],
            bottom: CurveSpec::CircularArc(CircularArcCurve {
                center: Point2::new(0.0, 0.0),
                radius: 1.0,
                start_angle_radians: 0.0,
                sweep_angle_radians: FRAC_PI_2,
            }),
            top: line((0.0, 2.0), (1.0, 2.0)),
            left: line((1.0, 0.0), (0.0, 2.0)),
            right: line((0.0, 1.0), (1.0, 2.0)),
            boundaries: BlockBoundaryAssignments::default(),
        };
        let mut edges = String::new();
        write_curved_edges(&mut edges, &[block], &[[0, 1, 2, 3]], 4, -0.5, 0.5)
            .expect("arc output");
        assert_eq!(edges.matches("    arc ").count(), 2);
        assert!(!edges.contains("spline "));
    }

    fn line(start: (f64, f64), end: (f64, f64)) -> CurveSpec {
        CurveSpec::Line(LineCurve::new(
            Point2::new(start.0, start.1),
            Point2::new(end.0, end.1),
        ))
    }
}
