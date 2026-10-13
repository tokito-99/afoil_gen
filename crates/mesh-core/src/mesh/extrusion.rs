//! Deterministic z-axis extrusion from quadrilaterals to hexahedra.

use crate::{
    config::{FrontBackPatchType, MeshConfiguration},
    error::MeshError,
    mesh::{BoundaryPatch3D, HexCell, Point3, StructuredMesh2D, StructuredMesh3D},
};

/// Extrudes a valid 2D mesh into one or more spanwise hexahedral layers.
///
/// Point ordering is plane-major, and every hexahedron uses the lower-plane
/// quadrilateral followed by its corresponding upper-plane vertices.
///
/// # Errors
///
/// Returns an error when extrusion is disabled, thickness/count/grading is
/// invalid, or `EMPTY` is requested with more than one spanwise cell.
#[allow(clippy::too_many_lines)]
pub fn extrude_mesh(
    mesh: &StructuredMesh2D,
    configuration: &MeshConfiguration,
) -> Result<StructuredMesh3D, MeshError> {
    let spanwise_planes = spanwise_planes(configuration)?;
    let spanwise_cells = configuration.mesh.spanwise_cells;
    let thickness = configuration.extrusion.spanwise_thickness;

    debug_assert_eq!(spanwise_planes.len(), spanwise_cells + 1);
    debug_assert!(thickness > 0.0);

    let points_per_plane = mesh.points.len();
    let mut points = Vec::with_capacity(points_per_plane * (spanwise_cells + 1));
    for &z in &spanwise_planes {
        points.extend(
            mesh.points
                .iter()
                .map(|point| Point3::new(point.x, point.y, z)),
        );
    }

    let mut cells = Vec::with_capacity(mesh.cells.len() * spanwise_cells);
    for span_index in 0..spanwise_cells {
        let lower_offset = span_index * points_per_plane;
        let upper_offset = (span_index + 1) * points_per_plane;
        for cell in &mesh.cells {
            cells.push(HexCell {
                vertices: [
                    lower_offset + cell.vertices[0],
                    lower_offset + cell.vertices[1],
                    lower_offset + cell.vertices[2],
                    lower_offset + cell.vertices[3],
                    upper_offset + cell.vertices[0],
                    upper_offset + cell.vertices[1],
                    upper_offset + cell.vertices[2],
                    upper_offset + cell.vertices[3],
                ],
                block_id: cell.block_id,
            });
        }
    }

    let mut boundaries = Vec::with_capacity(mesh.boundaries.len() + 2);
    for patch in &mesh.boundaries {
        let mut faces = Vec::with_capacity(patch.edges.len() * spanwise_cells);
        for span_index in 0..spanwise_cells {
            let lower_offset = span_index * points_per_plane;
            let upper_offset = (span_index + 1) * points_per_plane;
            faces.extend(patch.edges.iter().map(|edge| {
                [
                    lower_offset + edge[0],
                    lower_offset + edge[1],
                    upper_offset + edge[1],
                    upper_offset + edge[0],
                ]
            }));
        }
        boundaries.push(BoundaryPatch3D {
            name: patch.name.clone(),
            patch_type: patch.patch_type,
            faces,
        });
    }

    let front_faces = mesh
        .cells
        .iter()
        .map(|cell| {
            [
                cell.vertices[0],
                cell.vertices[3],
                cell.vertices[2],
                cell.vertices[1],
            ]
        })
        .collect();
    let back_offset = spanwise_cells * points_per_plane;
    let back_faces = mesh
        .cells
        .iter()
        .map(|cell| {
            [
                back_offset + cell.vertices[0],
                back_offset + cell.vertices[1],
                back_offset + cell.vertices[2],
                back_offset + cell.vertices[3],
            ]
        })
        .collect();
    boundaries.push(BoundaryPatch3D {
        name: configuration.boundaries.front.name.clone(),
        patch_type: configuration.boundaries.front.patch_type,
        faces: front_faces,
    });
    boundaries.push(BoundaryPatch3D {
        name: configuration.boundaries.back.name.clone(),
        patch_type: configuration.boundaries.back.patch_type,
        faces: back_faces,
    });

    Ok(StructuredMesh3D {
        points,
        cells,
        boundaries,
        spanwise_planes,
        source_2d_point_count: points_per_plane,
        block_count: mesh.blocks.len(),
        maximum_radial_growth_ratio: mesh.maximum_radial_growth_ratio,
    })
}

/// Returns validated physical z-plane coordinates without allocating 3-D cells.
///
/// # Errors
///
/// Returns an error for disabled extrusion, invalid thickness/count/grading,
/// or an `EMPTY` configuration with more than one spanwise cell.
pub fn spanwise_planes(configuration: &MeshConfiguration) -> Result<Vec<f64>, MeshError> {
    if !configuration.extrusion.enabled {
        return Err(MeshError::TopologyError(
            "extrusion is disabled in the configuration".to_owned(),
        ));
    }
    let thickness = configuration.extrusion.spanwise_thickness;
    if !thickness.is_finite() || thickness <= 0.0 {
        return Err(MeshError::TopologyError(
            "spanwise thickness must be finite and positive".to_owned(),
        ));
    }
    let spanwise_cells = configuration.mesh.spanwise_cells;
    if spanwise_cells == 0 {
        return Err(MeshError::TopologyError(
            "spanwise cell count must be at least one".to_owned(),
        ));
    }
    if configuration.extrusion.front_back_patch_type == FrontBackPatchType::Empty
        && spanwise_cells != 1
    {
        return Err(MeshError::TopologyError(
            "EMPTY front/back patches require exactly one spanwise cell".to_owned(),
        ));
    }
    if !configuration.mesh.spanwise_grading.is_finite()
        || configuration.mesh.spanwise_grading <= 0.0
    {
        return Err(MeshError::TopologyError(
            "spanwise grading must be finite and positive".to_owned(),
        ));
    }

    Ok(
        graded_positions(spanwise_cells, configuration.mesh.spanwise_grading)
            .iter()
            .map(|position| -0.5 * thickness + position * thickness)
            .collect(),
    )
}

fn graded_positions(cell_count: usize, total_grading: f64) -> Vec<f64> {
    if cell_count == 1 || (total_grading - 1.0).abs() <= f64::EPSILON {
        return (0..=cell_count)
            .map(|index| {
                #[allow(clippy::cast_precision_loss)]
                let value = index as f64 / cell_count as f64;
                value
            })
            .collect();
    }
    #[allow(clippy::cast_precision_loss)]
    let ratio = total_grading.powf(1.0 / (cell_count - 1) as f64);
    let mut sizes = Vec::with_capacity(cell_count);
    let mut size = 1.0;
    for _ in 0..cell_count {
        sizes.push(size);
        size *= ratio;
    }
    let total: f64 = sizes.iter().sum();
    let mut positions = Vec::with_capacity(cell_count + 1);
    positions.push(0.0);
    let mut cumulative = 0.0;
    for size in sizes {
        cumulative += size / total;
        positions.push(cumulative);
    }
    positions[cell_count] = 1.0;
    positions
}

#[cfg(test)]
mod tests {
    use super::extrude_mesh;
    use crate::{
        config::PatchType,
        geometry::Point2,
        mesh::{
            BoundaryPatch2D, QuadCell, StructuredBlock2D, StructuredMesh2D,
            quality::{evaluate_extruded_quality_analytically, evaluate_mesh_2d, evaluate_mesh_3d},
        },
        test_support::example_configuration,
        topology::BoundaryRole,
    };

    #[test]
    fn one_quad_becomes_one_positive_hex() {
        let mut configuration = example_configuration();
        configuration.mesh.spanwise_cells = 1;
        configuration.extrusion.front_back_patch_type = crate::config::FrontBackPatchType::Empty;
        let mesh = square_mesh();
        let extruded = extrude_mesh(&mesh, &configuration).expect("valid extrusion");
        assert_eq!(extruded.points.len(), 8);
        assert_eq!(extruded.cells.len(), 1);
        assert_eq!(extruded.boundaries.len(), 6);
        assert_eq!(extruded.spanwise_planes, vec![-0.005, 0.005]);
        let report = evaluate_mesh_3d(&extruded);
        assert!(report.valid, "{:?}", report.errors);
        assert!((report.minimum_orthogonal_quality - 1.0).abs() < 1.0e-12);
        assert!(report.maximum_equiangle_skewness < 1.0e-12);
        assert!((report.minimum_scaled_jacobian - 1.0).abs() < 1.0e-12);
        assert!(
            report
                .minimum_cell_volume
                .is_some_and(|volume| volume > 0.0)
        );
    }

    #[test]
    fn multiple_spanwise_cells_create_expected_planes_and_hexes() {
        let mut configuration = example_configuration();
        configuration.mesh.spanwise_cells = 3;
        configuration.extrusion.front_back_patch_type = crate::config::FrontBackPatchType::Symmetry;
        configuration.boundaries.front.patch_type = PatchType::SymmetryPlane;
        configuration.boundaries.back.patch_type = PatchType::SymmetryPlane;
        let extruded = extrude_mesh(&square_mesh(), &configuration).expect("valid extrusion");
        assert_eq!(extruded.points.len(), 16);
        assert_eq!(extruded.cells.len(), 3);
        assert_eq!(extruded.spanwise_planes.len(), 4);
        assert!(evaluate_mesh_3d(&extruded).valid);
    }

    #[test]
    fn analytical_extrusion_quality_matches_allocated_mesh_extrema() {
        let mut configuration = example_configuration();
        configuration.mesh.spanwise_cells = 3;
        configuration.mesh.spanwise_grading = 2.0;
        configuration.extrusion.front_back_patch_type = crate::config::FrontBackPatchType::Symmetry;
        configuration.boundaries.front.patch_type = PatchType::SymmetryPlane;
        configuration.boundaries.back.patch_type = PatchType::SymmetryPlane;
        let mut mesh_2d = square_mesh();
        // A non-parallelogram catches arithmetic-centroid errors on thin extrusions.
        mesh_2d.points[2].x = 0.7;
        let quality_2d = evaluate_mesh_2d(&mesh_2d);
        let extruded = extrude_mesh(&mesh_2d, &configuration).expect("valid extrusion");
        let allocated = evaluate_mesh_3d(&extruded);
        let analytical = evaluate_extruded_quality_analytically(
            &mesh_2d,
            &quality_2d,
            &extruded.spanwise_planes,
        )
        .expect("analytical quality");
        assert_eq!(analytical.point_count, allocated.point_count);
        assert_eq!(analytical.cell_count, allocated.cell_count);
        assert_eq!(
            analytical.minimum_cell_volume,
            allocated.minimum_cell_volume
        );
        assert_eq!(
            analytical.maximum_cell_volume,
            allocated.maximum_cell_volume
        );
        assert!(
            (analytical.maximum_aspect_ratio - allocated.maximum_aspect_ratio).abs()
                <= f64::EPSILON
        );
        assert!(
            (analytical.minimum_orthogonal_quality - allocated.minimum_orthogonal_quality).abs()
                <= f64::EPSILON
        );
        assert!(
            (analytical.maximum_equiangle_skewness - allocated.maximum_equiangle_skewness).abs()
                <= f64::EPSILON
        );
        assert!(
            (analytical.minimum_scaled_jacobian - allocated.minimum_scaled_jacobian).abs()
                <= f64::EPSILON
        );
    }

    #[test]
    fn empty_rejects_multiple_spanwise_cells() {
        let mut configuration = example_configuration();
        configuration.mesh.spanwise_cells = 2;
        configuration.extrusion.front_back_patch_type = crate::config::FrontBackPatchType::Empty;
        assert!(extrude_mesh(&square_mesh(), &configuration).is_err());
    }

    fn square_mesh() -> StructuredMesh2D {
        let points = vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(1.0, 1.0),
            Point2::new(0.0, 1.0),
        ];
        StructuredMesh2D {
            optimization_report: None,
            points: points.clone(),
            cells: vec![QuadCell {
                vertices: [0, 1, 2, 3],
                block_id: 0,
            }],
            blocks: vec![StructuredBlock2D {
                id: 0,
                name: "square".to_owned(),
                ni: 1,
                nj: 1,
                points,
                global_point_indices: vec![0, 1, 2, 3],
            }],
            boundaries: vec![
                patch(BoundaryRole::Inlet, "inlet", [3, 0]),
                patch(BoundaryRole::Outlet, "outlet", [1, 2]),
                patch(BoundaryRole::Airfoil, "airfoil", [0, 1]),
                patch(BoundaryRole::UpperFarfield, "upper", [2, 3]),
            ],
            maximum_radial_growth_ratio: 1.0,
        }
    }

    fn patch(role: BoundaryRole, name: &str, edge: [usize; 2]) -> BoundaryPatch2D {
        BoundaryPatch2D {
            role,
            name: name.to_owned(),
            patch_type: if role == BoundaryRole::Airfoil {
                PatchType::Wall
            } else {
                PatchType::Patch
            },
            edges: vec![edge],
        }
    }
}
