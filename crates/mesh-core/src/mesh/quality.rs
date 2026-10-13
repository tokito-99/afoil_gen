//! Deterministic mesh-quality and boundary-completeness checks.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::{
    error::MeshError,
    geometry::Point2,
    mesh::{HexCell, Point3, QuadCell, StructuredMesh2D, StructuredMesh3D},
};

const QUALITY_RELATIVE_TOLERANCE: f64 = 1.0e-12;
const MAX_DETAILED_ERRORS: usize = 32;

/// Quality statistics and fatal/non-fatal findings for 2D or 3D meshes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeshQualityReport {
    pub valid: bool,
    pub point_count: usize,
    pub cell_count: usize,
    pub block_count: usize,
    pub minimum_cell_area: Option<f64>,
    pub maximum_cell_area: Option<f64>,
    pub minimum_cell_volume: Option<f64>,
    pub maximum_cell_volume: Option<f64>,
    pub maximum_aspect_ratio: f64,
    /// Face-normal alignment metric in [0, 1], where one is orthogonal.
    pub minimum_orthogonal_quality: f64,
    /// Maximum equiangle skewness in [0, 1], where zero is ideal.
    pub maximum_equiangle_skewness: f64,
    /// Minimum corner scaled Jacobian in [-1, 1], where one is ideal.
    pub minimum_scaled_jacobian: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worst_aspect_cell: Option<WorstAspectCell>,
    pub minimum_edge_length: f64,
    pub maximum_edge_length: f64,
    pub maximum_radial_growth_ratio: f64,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

/// Identifies the cell responsible for the reported maximum edge ratio.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorstAspectCell {
    pub cell_index: usize,
    pub block_id: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_name: Option<String>,
    pub aspect_ratio: f64,
}

/// Evaluates signed areas, cell edges, duplicate points, and 2D boundary
/// completeness.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn evaluate_mesh_2d(mesh: &StructuredMesh2D) -> MeshQualityReport {
    let scale = point_scale_2d(&mesh.points);
    let edge_tolerance = QUALITY_RELATIVE_TOLERANCE * scale;
    let area_tolerance = edge_tolerance.powi(2);
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let mut minimum_area = f64::INFINITY;
    let mut maximum_area = f64::NEG_INFINITY;
    let mut minimum_edge = f64::INFINITY;
    let mut maximum_edge: f64 = 0.0;
    let mut maximum_aspect: f64 = 0.0;
    let mut maximum_equiangle_skewness: f64 = 0.0;
    let mut minimum_scaled_jacobian = f64::INFINITY;
    let mut worst_aspect_cell = None;
    let mut edge_incidence = HashMap::<[usize; 2], usize>::new();
    let mut edge_cells = HashMap::<[usize; 2], Vec<usize>>::new();

    detect_duplicate_points_2d(&mesh.points, &mut errors);
    for (cell_index, cell) in mesh.cells.iter().enumerate() {
        let Some(points) = quad_points(&mesh.points, cell) else {
            push_error(
                &mut errors,
                format!("cell {cell_index} references an invalid point index"),
            );
            continue;
        };
        if cell.vertices.into_iter().collect::<HashSet<_>>().len() != 4 {
            push_error(
                &mut errors,
                format!("cell {cell_index} contains duplicate vertex indices"),
            );
        }

        let area = signed_quad_area(points);
        if !area.is_finite() {
            push_error(
                &mut errors,
                format!("cell {cell_index} has non-finite signed area"),
            );
            continue;
        }
        minimum_area = minimum_area.min(area);
        maximum_area = maximum_area.max(area);
        if area < -area_tolerance {
            push_error(
                &mut errors,
                format!("cell {cell_index} has negative signed area {area:.12e}"),
            );
        } else if area <= area_tolerance {
            push_error(
                &mut errors,
                format!("cell {cell_index} has zero or near-zero area {area:.12e}"),
            );
        }

        let mut cell_min_edge = f64::INFINITY;
        let mut cell_max_edge: f64 = 0.0;
        for edge in quad_edges(cell.vertices) {
            if edge[0] == edge[1] {
                push_error(
                    &mut errors,
                    format!("cell {cell_index} contains a degenerate edge"),
                );
                continue;
            }
            let canonical = canonical_edge(edge);
            *edge_incidence.entry(canonical).or_default() += 1;
            edge_cells.entry(canonical).or_default().push(cell_index);
            let length = mesh.points[edge[0]].distance_to(mesh.points[edge[1]]);
            if !length.is_finite() {
                push_error(
                    &mut errors,
                    format!("cell {cell_index} has a non-finite edge length"),
                );
                continue;
            }
            minimum_edge = minimum_edge.min(length);
            maximum_edge = maximum_edge.max(length);
            cell_min_edge = cell_min_edge.min(length);
            cell_max_edge = cell_max_edge.max(length);
            if length <= edge_tolerance {
                push_error(
                    &mut errors,
                    format!("cell {cell_index} has a zero or near-zero edge"),
                );
            }
        }
        if cell_min_edge.is_finite() && cell_min_edge > 0.0 {
            let aspect_ratio = cell_max_edge / cell_min_edge;
            if aspect_ratio > maximum_aspect {
                maximum_aspect = aspect_ratio;
                worst_aspect_cell = Some(WorstAspectCell {
                    cell_index,
                    block_id: cell.block_id,
                    block_name: mesh
                        .blocks
                        .get(cell.block_id)
                        .map(|block| block.name.clone()),
                    aspect_ratio,
                });
            }
        }
        maximum_equiangle_skewness =
            maximum_equiangle_skewness.max(quad_equiangle_skewness(points));
        let jacobian = quad_minimum_scaled_jacobian(points);
        minimum_scaled_jacobian = minimum_scaled_jacobian.min(jacobian);
        if jacobian <= 0.0 {
            push_error(
                &mut errors,
                format!("cell {cell_index} has a nonpositive corner Jacobian {jacobian:.12e}"),
            );
        }
    }

    let minimum_orthogonal_quality =
        minimum_orthogonal_quality_2d(mesh, &edge_cells).unwrap_or(0.0);

    let boundary_assignments = collect_boundary_edges_2d(mesh, &edge_incidence, &mut errors);
    for (edge, incidence) in &edge_incidence {
        match *incidence {
            1 => {
                if !boundary_assignments.contains_key(edge) {
                    push_error(&mut errors, format!("exterior edge {edge:?} is unassigned"));
                }
            }
            2 => {}
            count => push_error(
                &mut errors,
                format!("edge {edge:?} is nonmanifold with {count} incident cells"),
            ),
        }
    }

    if mesh.maximum_radial_growth_ratio > 1.2 {
        warnings.push(format!(
            "maximum radial growth ratio {:.6} exceeds the LES-oriented recommendation of 1.2",
            mesh.maximum_radial_growth_ratio
        ));
    }
    if maximum_aspect > 1_000.0 {
        let location = worst_aspect_cell.as_ref().map_or_else(
            || "an unidentified cell".to_owned(),
            |cell| {
                format!(
                    "{} (cell {})",
                    cell.block_name
                        .as_deref()
                        .map_or_else(|| format!("block {}", cell.block_id), str::to_owned),
                    cell.cell_index
                )
            },
        );
        warnings.push(format!(
            "maximum edge-based aspect ratio {maximum_aspect:.3} is high in {location}"
        ));
    }
    append_shape_quality_warnings(
        minimum_orthogonal_quality,
        maximum_equiangle_skewness,
        &mut warnings,
    );

    finish_report(
        mesh.points.len(),
        mesh.cells.len(),
        mesh.blocks.len(),
        finite_option(minimum_area),
        finite_option(maximum_area),
        None,
        None,
        maximum_aspect,
        minimum_orthogonal_quality,
        maximum_equiangle_skewness,
        finite_or_zero(minimum_scaled_jacobian),
        worst_aspect_cell,
        minimum_edge,
        maximum_edge,
        mesh.maximum_radial_growth_ratio,
        warnings,
        errors,
    )
}

/// Evaluates signed volumes, hexahedral edges, duplicate points, and complete
/// 3D boundary-face ownership.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn evaluate_mesh_3d(mesh: &StructuredMesh3D) -> MeshQualityReport {
    let scale = point_scale_3d(&mesh.points);
    let edge_tolerance = QUALITY_RELATIVE_TOLERANCE * scale;
    let volume_tolerance = edge_tolerance.powi(3);
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let mut minimum_volume = f64::INFINITY;
    let mut maximum_volume = f64::NEG_INFINITY;
    let mut minimum_edge = f64::INFINITY;
    let mut maximum_edge: f64 = 0.0;
    let mut maximum_aspect: f64 = 0.0;
    let mut maximum_equiangle_skewness: f64 = 0.0;
    let mut minimum_scaled_jacobian = f64::INFINITY;
    let mut worst_aspect_cell = None;
    let mut face_incidence = HashMap::<[usize; 4], usize>::new();
    let mut face_cells = HashMap::<[usize; 4], Vec<usize>>::new();
    let mut face_nodes = HashMap::<[usize; 4], [usize; 4]>::new();

    detect_duplicate_points_3d(&mesh.points, &mut errors);
    for (cell_index, cell) in mesh.cells.iter().enumerate() {
        let Some(points) = hex_points(&mesh.points, cell) else {
            push_error(
                &mut errors,
                format!("cell {cell_index} references an invalid point index"),
            );
            continue;
        };
        if cell.vertices.into_iter().collect::<HashSet<_>>().len() != 8 {
            push_error(
                &mut errors,
                format!("cell {cell_index} contains duplicate vertex indices"),
            );
        }

        let volume = signed_hex_volume(points);
        if !volume.is_finite() {
            push_error(
                &mut errors,
                format!("cell {cell_index} has non-finite signed volume"),
            );
            continue;
        }
        minimum_volume = minimum_volume.min(volume);
        maximum_volume = maximum_volume.max(volume);
        if volume < -volume_tolerance {
            push_error(
                &mut errors,
                format!("cell {cell_index} has negative signed volume {volume:.12e}"),
            );
        } else if volume <= volume_tolerance {
            push_error(
                &mut errors,
                format!("cell {cell_index} has zero or near-zero volume {volume:.12e}"),
            );
        }

        let mut cell_min_edge = f64::INFINITY;
        let mut cell_max_edge: f64 = 0.0;
        for edge in hex_edges(cell.vertices) {
            let length = mesh.points[edge[0]].distance_to(mesh.points[edge[1]]);
            if !length.is_finite() {
                push_error(
                    &mut errors,
                    format!("cell {cell_index} has a non-finite edge length"),
                );
                continue;
            }
            minimum_edge = minimum_edge.min(length);
            maximum_edge = maximum_edge.max(length);
            cell_min_edge = cell_min_edge.min(length);
            cell_max_edge = cell_max_edge.max(length);
            if length <= edge_tolerance {
                push_error(
                    &mut errors,
                    format!("cell {cell_index} has a zero or near-zero edge"),
                );
            }
        }
        if cell_min_edge.is_finite() && cell_min_edge > 0.0 {
            let aspect_ratio = cell_max_edge / cell_min_edge;
            if aspect_ratio > maximum_aspect {
                maximum_aspect = aspect_ratio;
                worst_aspect_cell = Some(WorstAspectCell {
                    cell_index,
                    block_id: cell.block_id,
                    block_name: None,
                    aspect_ratio,
                });
            }
        }

        maximum_equiangle_skewness =
            maximum_equiangle_skewness.max(hex_maximum_face_equiangle_skewness(points));
        let jacobian = hex_minimum_scaled_jacobian(points);
        minimum_scaled_jacobian = minimum_scaled_jacobian.min(jacobian);
        if jacobian <= 0.0 {
            push_error(
                &mut errors,
                format!("cell {cell_index} has a nonpositive corner Jacobian {jacobian:.12e}"),
            );
        }

        for face in hex_faces(cell.vertices) {
            let canonical = canonical_face(face);
            *face_incidence.entry(canonical).or_default() += 1;
            face_cells.entry(canonical).or_default().push(cell_index);
            face_nodes.entry(canonical).or_insert(face);
        }
    }

    let minimum_orthogonal_quality =
        minimum_orthogonal_quality_3d(mesh, &face_cells, &face_nodes).unwrap_or(0.0);

    let boundary_assignments = collect_boundary_faces_3d(mesh, &face_incidence, &mut errors);
    for (face, incidence) in &face_incidence {
        match *incidence {
            1 => {
                if !boundary_assignments.contains_key(face) {
                    push_error(&mut errors, format!("exterior face {face:?} is unassigned"));
                }
            }
            2 => {}
            count => push_error(
                &mut errors,
                format!("face {face:?} is nonmanifold with {count} incident cells"),
            ),
        }
    }
    if mesh.maximum_radial_growth_ratio > 1.2 {
        warnings.push(format!(
            "maximum radial growth ratio {:.6} exceeds the LES-oriented recommendation of 1.2",
            mesh.maximum_radial_growth_ratio
        ));
    }
    if maximum_aspect > 1_000.0 {
        let location = worst_aspect_cell.as_ref().map_or_else(
            || "an unidentified cell".to_owned(),
            |cell| format!("block {} (cell {})", cell.block_id, cell.cell_index),
        );
        warnings.push(format!(
            "maximum edge-based aspect ratio {maximum_aspect:.3} is high in {location}"
        ));
    }
    append_shape_quality_warnings(
        minimum_orthogonal_quality,
        maximum_equiangle_skewness,
        &mut warnings,
    );

    finish_report(
        mesh.points.len(),
        mesh.cells.len(),
        mesh.block_count,
        None,
        None,
        finite_option(minimum_volume),
        finite_option(maximum_volume),
        maximum_aspect,
        minimum_orthogonal_quality,
        maximum_equiangle_skewness,
        finite_or_zero(minimum_scaled_jacobian),
        worst_aspect_cell,
        minimum_edge,
        maximum_edge,
        mesh.maximum_radial_growth_ratio,
        warnings,
        errors,
    )
}

/// Computes exact counts, edge extrema, and volume extrema for a vertical
/// extrusion without allocating its points, faces, or hexahedra.
///
/// The topology is valid by construction when the source 2-D mesh is valid and
/// the supplied z planes are strictly increasing.
///
/// # Errors
///
/// Returns an error for fewer than two planes, non-increasing/non-finite plane
/// coordinates, or count overflow.
pub fn evaluate_extruded_quality_analytically(
    mesh: &StructuredMesh2D,
    quality_2d: &MeshQualityReport,
    spanwise_planes: &[f64],
) -> Result<MeshQualityReport, MeshError> {
    if spanwise_planes.len() < 2
        || spanwise_planes
            .windows(2)
            .any(|pair| !pair[0].is_finite() || !pair[1].is_finite() || pair[1] <= pair[0])
    {
        return Err(MeshError::QualityError(
            "analytical extrusion requires finite, strictly increasing z planes".to_owned(),
        ));
    }
    let spanwise_cells = spanwise_planes.len() - 1;
    let point_count = mesh
        .points
        .len()
        .checked_mul(spanwise_planes.len())
        .ok_or_else(|| MeshError::QualityError("extruded point count overflow".to_owned()))?;
    let cell_count = mesh
        .cells
        .len()
        .checked_mul(spanwise_cells)
        .ok_or_else(|| MeshError::QualityError("extruded cell count overflow".to_owned()))?;
    let (minimum_span_edge, maximum_span_edge) = spanwise_planes
        .windows(2)
        .map(|pair| pair[1] - pair[0])
        .fold((f64::INFINITY, 0.0_f64), |(minimum, maximum), edge| {
            (minimum.min(edge), maximum.max(edge))
        });
    let minimum_edge_length = quality_2d.minimum_edge_length.min(minimum_span_edge);
    let maximum_edge_length = quality_2d.maximum_edge_length.max(maximum_span_edge);
    let maximum_aspect_ratio = quality_2d
        .maximum_aspect_ratio
        .max(quality_2d.maximum_edge_length / minimum_span_edge)
        .max(maximum_span_edge / quality_2d.minimum_edge_length);
    let mut warnings = Vec::new();
    if mesh.maximum_radial_growth_ratio > 1.2 {
        warnings.push(format!(
            "maximum radial growth ratio {:.6} exceeds the LES-oriented recommendation of 1.2",
            mesh.maximum_radial_growth_ratio
        ));
    }
    if maximum_aspect_ratio > 1_000.0 {
        warnings.push(format!(
            "maximum edge-based aspect ratio {maximum_aspect_ratio:.3} is high in the analytical extrusion"
        ));
    }
    append_shape_quality_warnings(
        quality_2d.minimum_orthogonal_quality,
        quality_2d.maximum_equiangle_skewness,
        &mut warnings,
    );

    Ok(MeshQualityReport {
        valid: quality_2d.valid,
        point_count,
        cell_count,
        block_count: mesh.blocks.len(),
        minimum_cell_area: None,
        maximum_cell_area: None,
        minimum_cell_volume: quality_2d
            .minimum_cell_area
            .map(|area| area * minimum_span_edge),
        maximum_cell_volume: quality_2d
            .maximum_cell_area
            .map(|area| area * maximum_span_edge),
        maximum_aspect_ratio,
        minimum_orthogonal_quality: quality_2d.minimum_orthogonal_quality,
        maximum_equiangle_skewness: quality_2d.maximum_equiangle_skewness,
        minimum_scaled_jacobian: quality_2d.minimum_scaled_jacobian,
        worst_aspect_cell: None,
        minimum_edge_length,
        maximum_edge_length,
        maximum_radial_growth_ratio: mesh.maximum_radial_growth_ratio,
        warnings,
        errors: quality_2d.errors.clone(),
    })
}

pub(crate) fn quad_centroid(points: [Point2; 4]) -> Point2 {
    // Translate before the polygon formula to avoid cancellation in thin cells.
    let origin = points[0];
    let mut double_area = 0.0;
    let mut x = 0.0;
    let mut y = 0.0;
    for i in 0..4 {
        let a = points[i];
        let b = points[(i + 1) % 4];
        let ax = a.x - origin.x;
        let ay = a.y - origin.y;
        let bx = b.x - origin.x;
        let by = b.y - origin.y;
        let cross = ax * by - bx * ay;
        double_area += cross;
        x += (ax + bx) * cross;
        y += (ay + by) * cross;
    }
    if double_area == 0.0 {
        return Point2::new(f64::NAN, f64::NAN);
    }
    Point2::new(
        origin.x + x / (3.0 * double_area),
        origin.y + y / (3.0 * double_area),
    )
}

fn hex_centroid(points: [Point3; 8]) -> Point3 {
    // This engine generates straight vertical extrusions of polygonal quads.
    let base =
        quad_centroid([points[0], points[1], points[2], points[3]].map(|p| Point2::new(p.x, p.y)));
    Point3::new(
        base.x,
        base.y,
        points.iter().map(|point| point.z).sum::<f64>() * 0.125,
    )
}

fn minimum_orthogonal_quality_3d(
    mesh: &StructuredMesh3D,
    face_cells: &HashMap<[usize; 4], Vec<usize>>,
    face_nodes: &HashMap<[usize; 4], [usize; 4]>,
) -> Option<f64> {
    let centroids = mesh
        .cells
        .iter()
        .map(|cell| hex_points(&mesh.points, cell).map(hex_centroid))
        .collect::<Vec<_>>();
    let mut minimum = f64::INFINITY;
    for (canonical, cells) in face_cells {
        let nodes = face_nodes.get(canonical)?;
        let points = [
            *mesh.points.get(nodes[0])?,
            *mesh.points.get(nodes[1])?,
            *mesh.points.get(nodes[2])?,
            *mesh.points.get(nodes[3])?,
        ];
        let face = planar_face_centroid(points);
        let normal = add3(
            cross3(sub3(points[1], points[0]), sub3(points[2], points[0])),
            cross3(sub3(points[2], points[0]), sub3(points[3], points[0])),
        );
        let first_centroid = *centroids.get(*cells.first()?)?.as_ref()?;
        for cell in cells {
            let centroid = *centroids.get(*cell)?.as_ref()?;
            let direction = sub3(face, centroid);
            let denominator = norm3(normal) * norm3(direction);
            if denominator > 0.0 {
                minimum =
                    minimum.min((dot3(normal, direction).abs() / denominator).clamp(0.0, 1.0));
            }
        }
        let direction = if let Some(second_cell) = cells.get(1) {
            sub3(*centroids.get(*second_cell)?.as_ref()?, first_centroid)
        } else {
            sub3(face, first_centroid)
        };
        let denominator = norm3(normal) * norm3(direction);
        if denominator > 0.0 {
            minimum = minimum.min((dot3(normal, direction).abs() / denominator).clamp(0.0, 1.0));
        }
    }
    minimum.is_finite().then_some(minimum)
}

fn planar_face_centroid(points: [Point3; 4]) -> Point3 {
    let a = norm3(cross3(
        sub3(points[1], points[0]),
        sub3(points[2], points[0]),
    ));
    let b = norm3(cross3(
        sub3(points[2], points[0]),
        sub3(points[3], points[0]),
    ));
    let weight = a / (a + b);
    let coordinate = |values: [f64; 4]| {
        values[0]
            + ((values[2] - values[0])
                + weight * (values[1] - values[0])
                + (1.0 - weight) * (values[3] - values[0]))
                / 3.0
    };
    Point3::new(
        coordinate(points.map(|p| p.x)),
        coordinate(points.map(|p| p.y)),
        coordinate(points.map(|p| p.z)),
    )
}

fn hex_maximum_face_equiangle_skewness(points: [Point3; 8]) -> f64 {
    const FACES: [[usize; 4]; 6] = [
        [0, 3, 2, 1],
        [4, 5, 6, 7],
        [0, 1, 5, 4],
        [1, 2, 6, 5],
        [2, 3, 7, 6],
        [3, 0, 4, 7],
    ];
    FACES
        .iter()
        .map(|face| face_equiangle_skewness_3d(face.map(|index| points[index])))
        .fold(0.0, f64::max)
}

fn face_equiangle_skewness_3d(points: [Point3; 4]) -> f64 {
    let mut minimum_angle = f64::INFINITY;
    let mut maximum_angle: f64 = 0.0;
    for index in 0..4 {
        let first = sub3(points[(index + 3) % 4], points[index]);
        let second = sub3(points[(index + 1) % 4], points[index]);
        let denominator = norm3(first) * norm3(second);
        if denominator <= 0.0 {
            return 1.0;
        }
        let angle = (dot3(first, second) / denominator).clamp(-1.0, 1.0).acos();
        minimum_angle = minimum_angle.min(angle);
        maximum_angle = maximum_angle.max(angle);
    }
    let ideal = std::f64::consts::FRAC_PI_2;
    ((maximum_angle - ideal).max(ideal - minimum_angle) / ideal).clamp(0.0, 1.0)
}

fn hex_minimum_scaled_jacobian(points: [Point3; 8]) -> f64 {
    const CORNERS: [[usize; 4]; 8] = [
        [0, 1, 3, 4],
        [1, 2, 0, 5],
        [2, 3, 1, 6],
        [3, 0, 2, 7],
        [4, 7, 5, 0],
        [5, 4, 6, 1],
        [6, 5, 7, 2],
        [7, 6, 4, 3],
    ];
    CORNERS
        .iter()
        .map(|corner| {
            let first = sub3(points[corner[1]], points[corner[0]]);
            let second = sub3(points[corner[2]], points[corner[0]]);
            let third = sub3(points[corner[3]], points[corner[0]]);
            let denominator = norm3(first) * norm3(second) * norm3(third);
            if denominator <= 0.0 {
                -1.0
            } else {
                dot3(cross3(first, second), third) / denominator
            }
        })
        .fold(f64::INFINITY, f64::min)
        .clamp(-1.0, 1.0)
}

fn sub3(first: Point3, second: Point3) -> [f64; 3] {
    [first.x - second.x, first.y - second.y, first.z - second.z]
}

fn add3(first: [f64; 3], second: [f64; 3]) -> [f64; 3] {
    [
        first[0] + second[0],
        first[1] + second[1],
        first[2] + second[2],
    ]
}

fn cross3(first: [f64; 3], second: [f64; 3]) -> [f64; 3] {
    [
        first[1] * second[2] - first[2] * second[1],
        first[2] * second[0] - first[0] * second[2],
        first[0] * second[1] - first[1] * second[0],
    ]
}

fn dot3(first: [f64; 3], second: [f64; 3]) -> f64 {
    first[0] * second[0] + first[1] * second[1] + first[2] * second[2]
}

fn norm3(vector: [f64; 3]) -> f64 {
    dot3(vector, vector).sqrt()
}

fn minimum_orthogonal_quality_2d(
    mesh: &StructuredMesh2D,
    edge_cells: &HashMap<[usize; 2], Vec<usize>>,
) -> Option<f64> {
    let centroids = mesh
        .cells
        .iter()
        .map(|cell| quad_points(&mesh.points, cell).map(quad_centroid))
        .collect::<Vec<_>>();
    let mut minimum = f64::INFINITY;
    for (edge, cells) in edge_cells {
        let [first, second] = *edge;
        let p0 = *mesh.points.get(first)?;
        let p1 = *mesh.points.get(second)?;
        let tangent_x = p1.x - p0.x;
        let tangent_y = p1.y - p0.y;
        let normal_x = -tangent_y;
        let normal_y = tangent_x;
        let normal_length = normal_x.hypot(normal_y);
        let face = Point2::new(f64::midpoint(p0.x, p1.x), f64::midpoint(p0.y, p1.y));
        for cell in cells {
            let centroid = centroids.get(*cell)?.as_ref()?;
            let dx = face.x - centroid.x;
            let dy = face.y - centroid.y;
            let denominator = normal_length * dx.hypot(dy);
            if denominator > 0.0 {
                minimum = minimum
                    .min(((normal_x * dx + normal_y * dy).abs() / denominator).clamp(0.0, 1.0));
            }
        }
        let first_centroid = centroids.get(*cells.first()?)?.as_ref()?;
        let (dx, dy) = if let Some(second_cell) = cells.get(1) {
            let second_centroid = centroids.get(*second_cell)?.as_ref()?;
            (
                second_centroid.x - first_centroid.x,
                second_centroid.y - first_centroid.y,
            )
        } else {
            (face.x - first_centroid.x, face.y - first_centroid.y)
        };
        let distance = dx.hypot(dy);
        if normal_length > 0.0 && distance > 0.0 {
            let quality = (normal_x * dx + normal_y * dy).abs() / (normal_length * distance);
            minimum = minimum.min(quality.clamp(0.0, 1.0));
        }
    }
    minimum.is_finite().then_some(minimum)
}

fn quad_equiangle_skewness(points: [Point2; 4]) -> f64 {
    let mut minimum_angle = f64::INFINITY;
    let mut maximum_angle: f64 = 0.0;
    for index in 0..4 {
        let previous = points[(index + 3) % 4];
        let current = points[index];
        let next = points[(index + 1) % 4];
        let ax = previous.x - current.x;
        let ay = previous.y - current.y;
        let bx = next.x - current.x;
        let by = next.y - current.y;
        let denominator = ax.hypot(ay) * bx.hypot(by);
        if denominator <= 0.0 {
            return 1.0;
        }
        let angle = ((ax * bx + ay * by) / denominator).clamp(-1.0, 1.0).acos();
        minimum_angle = minimum_angle.min(angle);
        maximum_angle = maximum_angle.max(angle);
    }
    let ideal = std::f64::consts::FRAC_PI_2;
    ((maximum_angle - ideal).max(ideal - minimum_angle) / ideal).clamp(0.0, 1.0)
}

fn quad_minimum_scaled_jacobian(points: [Point2; 4]) -> f64 {
    let mut minimum = f64::INFINITY;
    for index in 0..4 {
        let current = points[index];
        let next = points[(index + 1) % 4];
        let previous = points[(index + 3) % 4];
        let ax = next.x - current.x;
        let ay = next.y - current.y;
        let bx = previous.x - current.x;
        let by = previous.y - current.y;
        let denominator = ax.hypot(ay) * bx.hypot(by);
        if denominator <= 0.0 {
            return -1.0;
        }
        minimum = minimum.min((ax * by - ay * bx) / denominator);
    }
    finite_or_zero(minimum.clamp(-1.0, 1.0))
}

fn append_shape_quality_warnings(
    minimum_orthogonal_quality: f64,
    maximum_equiangle_skewness: f64,
    warnings: &mut Vec<String>,
) {
    if minimum_orthogonal_quality < 0.1 {
        warnings.push(format!(
            "minimum geometric orthogonal quality {minimum_orthogonal_quality:.6} is below 0.1"
        ));
    }
    if maximum_equiangle_skewness > 0.95 {
        warnings.push(format!(
            "maximum equiangle skewness {maximum_equiangle_skewness:.6} exceeds 0.95"
        ));
    }
}

/// Signed quadrilateral area using the shoelace formula.
#[must_use]
pub fn signed_quad_area(points: [Point2; 4]) -> f64 {
    let mut twice_area = 0.0;
    for index in 0..4 {
        let next = (index + 1) % 4;
        twice_area += points[index].x * points[next].y - points[next].x * points[index].y;
    }
    0.5 * twice_area
}

/// Signed volume for a vertically extruded hexahedron.
///
/// The extrusion stage guarantees matching x/y coordinates across paired
/// lower and upper vertices. Quality checks separately flag broken pairs.
#[must_use]
pub fn signed_hex_volume(points: [Point3; 8]) -> f64 {
    let bottom = [
        Point2::new(points[0].x, points[0].y),
        Point2::new(points[1].x, points[1].y),
        Point2::new(points[2].x, points[2].y),
        Point2::new(points[3].x, points[3].y),
    ];
    let bottom_z = 0.25 * (points[0].z + points[1].z + points[2].z + points[3].z);
    let top_z = 0.25 * (points[4].z + points[5].z + points[6].z + points[7].z);
    signed_quad_area(bottom) * (top_z - bottom_z)
}

#[allow(clippy::too_many_arguments)]
fn finish_report(
    point_count: usize,
    cell_count: usize,
    block_count: usize,
    minimum_cell_area: Option<f64>,
    maximum_cell_area: Option<f64>,
    minimum_cell_volume: Option<f64>,
    maximum_cell_volume: Option<f64>,
    maximum_aspect_ratio: f64,
    minimum_orthogonal_quality: f64,
    maximum_equiangle_skewness: f64,
    minimum_scaled_jacobian: f64,
    worst_aspect_cell: Option<WorstAspectCell>,
    minimum_edge_length: f64,
    maximum_edge_length: f64,
    maximum_radial_growth_ratio: f64,
    warnings: Vec<String>,
    errors: Vec<String>,
) -> MeshQualityReport {
    MeshQualityReport {
        valid: errors.is_empty(),
        point_count,
        cell_count,
        block_count,
        minimum_cell_area,
        maximum_cell_area,
        minimum_cell_volume,
        maximum_cell_volume,
        maximum_aspect_ratio,
        minimum_orthogonal_quality,
        maximum_equiangle_skewness,
        minimum_scaled_jacobian,
        worst_aspect_cell,
        minimum_edge_length: finite_or_zero(minimum_edge_length),
        maximum_edge_length: finite_or_zero(maximum_edge_length),
        maximum_radial_growth_ratio,
        warnings,
        errors,
    }
}

fn collect_boundary_edges_2d(
    mesh: &StructuredMesh2D,
    incidence: &HashMap<[usize; 2], usize>,
    errors: &mut Vec<String>,
) -> HashMap<[usize; 2], String> {
    let mut assignments = HashMap::new();
    for patch in &mesh.boundaries {
        if patch.edges.is_empty() {
            push_error(
                errors,
                format!("patch {} has no boundary edges", patch.name),
            );
        } else if !edge_set_is_connected(&patch.edges) {
            push_error(
                errors,
                format!("patch {} is not a continuous boundary", patch.name),
            );
        }
        for &edge in &patch.edges {
            if edge[0] == edge[1] || edge.iter().any(|index| *index >= mesh.points.len()) {
                push_error(
                    errors,
                    format!("patch {} contains an invalid edge", patch.name),
                );
                continue;
            }
            let key = canonical_edge(edge);
            if incidence.get(&key).copied() != Some(1) {
                push_error(
                    errors,
                    format!("patch {} assigns non-exterior edge {key:?}", patch.name),
                );
            }
            if let Some(previous) = assignments.insert(key, patch.name.clone()) {
                push_error(
                    errors,
                    format!(
                        "edge {key:?} is assigned to multiple patches: {previous} and {}",
                        patch.name
                    ),
                );
            }
        }
    }
    assignments
}

fn edge_set_is_connected(edges: &[[usize; 2]]) -> bool {
    let mut adjacency = HashMap::<usize, Vec<usize>>::new();
    for edge in edges {
        adjacency.entry(edge[0]).or_default().push(edge[1]);
        adjacency.entry(edge[1]).or_default().push(edge[0]);
    }
    let Some(start) = adjacency.keys().next().copied() else {
        return false;
    };
    let mut visited = HashSet::new();
    let mut stack = vec![start];
    while let Some(vertex) = stack.pop() {
        if visited.insert(vertex)
            && let Some(neighbors) = adjacency.get(&vertex)
        {
            stack.extend(neighbors);
        }
    }
    visited.len() == adjacency.len()
}

fn collect_boundary_faces_3d(
    mesh: &StructuredMesh3D,
    incidence: &HashMap<[usize; 4], usize>,
    errors: &mut Vec<String>,
) -> HashMap<[usize; 4], String> {
    let mut assignments = HashMap::new();
    for patch in &mesh.boundaries {
        for &face in &patch.faces {
            if face.into_iter().collect::<HashSet<_>>().len() != 4
                || face.iter().any(|index| *index >= mesh.points.len())
            {
                push_error(
                    errors,
                    format!("patch {} contains an invalid face", patch.name),
                );
                continue;
            }
            let key = canonical_face(face);
            if incidence.get(&key).copied() != Some(1) {
                push_error(
                    errors,
                    format!("patch {} assigns non-exterior face {key:?}", patch.name),
                );
            }
            if let Some(previous) = assignments.insert(key, patch.name.clone()) {
                push_error(
                    errors,
                    format!(
                        "face {key:?} is assigned to multiple patches: {previous} and {}",
                        patch.name
                    ),
                );
            }
        }
    }
    assignments
}

fn detect_duplicate_points_2d(points: &[Point2], errors: &mut Vec<String>) {
    let mut coordinates = HashMap::new();
    for (index, point) in points.iter().enumerate() {
        if !point.x.is_finite() || !point.y.is_finite() {
            push_error(errors, format!("point {index} has non-finite coordinates"));
            continue;
        }
        let key = (normalized_bits(point.x), normalized_bits(point.y));
        if let Some(previous) = coordinates.insert(key, index) {
            push_error(
                errors,
                format!("points {previous} and {index} have duplicate coordinates"),
            );
        }
    }
}

fn detect_duplicate_points_3d(points: &[Point3], errors: &mut Vec<String>) {
    let mut coordinates = HashMap::new();
    for (index, point) in points.iter().enumerate() {
        if !point.x.is_finite() || !point.y.is_finite() || !point.z.is_finite() {
            push_error(errors, format!("point {index} has non-finite coordinates"));
            continue;
        }
        let key = (
            normalized_bits(point.x),
            normalized_bits(point.y),
            normalized_bits(point.z),
        );
        if let Some(previous) = coordinates.insert(key, index) {
            push_error(
                errors,
                format!("points {previous} and {index} have duplicate coordinates"),
            );
        }
    }
}

fn normalized_bits(value: f64) -> u64 {
    if value == 0.0 { 0 } else { value.to_bits() }
}

fn quad_points(points: &[Point2], cell: &QuadCell) -> Option<[Point2; 4]> {
    Some([
        *points.get(cell.vertices[0])?,
        *points.get(cell.vertices[1])?,
        *points.get(cell.vertices[2])?,
        *points.get(cell.vertices[3])?,
    ])
}

fn hex_points(points: &[Point3], cell: &HexCell) -> Option<[Point3; 8]> {
    Some([
        *points.get(cell.vertices[0])?,
        *points.get(cell.vertices[1])?,
        *points.get(cell.vertices[2])?,
        *points.get(cell.vertices[3])?,
        *points.get(cell.vertices[4])?,
        *points.get(cell.vertices[5])?,
        *points.get(cell.vertices[6])?,
        *points.get(cell.vertices[7])?,
    ])
}

const fn quad_edges(vertices: [usize; 4]) -> [[usize; 2]; 4] {
    [
        [vertices[0], vertices[1]],
        [vertices[1], vertices[2]],
        [vertices[2], vertices[3]],
        [vertices[3], vertices[0]],
    ]
}

const fn hex_edges(vertices: [usize; 8]) -> [[usize; 2]; 12] {
    [
        [vertices[0], vertices[1]],
        [vertices[1], vertices[2]],
        [vertices[2], vertices[3]],
        [vertices[3], vertices[0]],
        [vertices[4], vertices[5]],
        [vertices[5], vertices[6]],
        [vertices[6], vertices[7]],
        [vertices[7], vertices[4]],
        [vertices[0], vertices[4]],
        [vertices[1], vertices[5]],
        [vertices[2], vertices[6]],
        [vertices[3], vertices[7]],
    ]
}

const fn hex_faces(vertices: [usize; 8]) -> [[usize; 4]; 6] {
    [
        [vertices[0], vertices[3], vertices[2], vertices[1]],
        [vertices[4], vertices[5], vertices[6], vertices[7]],
        [vertices[0], vertices[1], vertices[5], vertices[4]],
        [vertices[1], vertices[2], vertices[6], vertices[5]],
        [vertices[2], vertices[3], vertices[7], vertices[6]],
        [vertices[3], vertices[0], vertices[4], vertices[7]],
    ]
}

fn canonical_edge(mut edge: [usize; 2]) -> [usize; 2] {
    edge.sort_unstable();
    edge
}

fn canonical_face(mut face: [usize; 4]) -> [usize; 4] {
    face.sort_unstable();
    face
}

fn point_scale_2d(points: &[Point2]) -> f64 {
    points
        .iter()
        .map(|point| point.x.abs().max(point.y.abs()))
        .fold(1.0, f64::max)
}

fn point_scale_3d(points: &[Point3]) -> f64 {
    points
        .iter()
        .map(|point| point.x.abs().max(point.y.abs()).max(point.z.abs()))
        .fold(1.0, f64::max)
}

fn finite_option(value: f64) -> Option<f64> {
    value.is_finite().then_some(value)
}

fn finite_or_zero(value: f64) -> f64 {
    if value.is_finite() { value } else { 0.0 }
}

fn push_error(errors: &mut Vec<String>, message: String) {
    if errors.len() < MAX_DETAILED_ERRORS {
        errors.push(message);
    } else if errors.len() == MAX_DETAILED_ERRORS {
        errors.push("additional quality errors omitted".to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::{evaluate_mesh_2d, signed_quad_area};
    use crate::{
        geometry::Point2,
        mesh::{BoundaryPatch2D, QuadCell, StructuredMesh2D},
        test_support::example_configuration,
        topology::{BoundaryRole, build_cgrid_topology},
    };

    #[test]
    fn positive_area_does_not_hide_an_inverted_corner() {
        let points = vec![
            Point2::new(0.0, 0.0),
            Point2::new(2.0, 0.0),
            Point2::new(0.5, 0.2),
            Point2::new(0.0, 1.0),
        ];
        assert!(signed_quad_area([points[0], points[1], points[2], points[3]]) > 0.0);
        let mesh = StructuredMesh2D {
            optimization_report: None,
            points,
            cells: vec![QuadCell {
                vertices: [0, 1, 2, 3],
                block_id: 0,
            }],
            blocks: vec![],
            boundaries: vec![BoundaryPatch2D {
                role: BoundaryRole::Inlet,
                name: "outer".to_owned(),
                patch_type: crate::config::PatchType::Patch,
                edges: vec![[0, 1], [1, 2], [2, 3], [3, 0]],
            }],
            maximum_radial_growth_ratio: 1.0,
        };
        let report = evaluate_mesh_2d(&mesh);
        assert!(!report.valid);
        assert!(report.minimum_scaled_jacobian < 0.0);
        assert!(
            report
                .errors
                .iter()
                .any(|error| error.contains("corner Jacobian"))
        );
    }

    #[test]
    fn interior_orthogonality_checks_both_face_to_cell_vectors() {
        let mesh = StructuredMesh2D {
            optimization_report: None,
            points: vec![
                Point2::new(-1.0, 1.0),
                Point2::new(1.0, 0.0),
                Point2::new(1.0, 1.0),
                Point2::new(-1.0, 2.0),
                Point2::new(3.0, 1.0),
                Point2::new(3.0, 2.0),
            ],
            cells: vec![
                QuadCell {
                    vertices: [0, 1, 2, 3],
                    block_id: 0,
                },
                QuadCell {
                    vertices: [1, 4, 5, 2],
                    block_id: 0,
                },
            ],
            blocks: vec![],
            boundaries: vec![],
            maximum_radial_growth_ratio: 1.0,
        };
        // Neighbor-center alignment is perfect, but each face-to-center vector is skewed.
        let edges = std::collections::HashMap::from([([1, 2], vec![0, 1])]);
        let quality = super::minimum_orthogonal_quality_2d(&mesh, &edges).unwrap();
        assert!((quality - 1.0 / 1.25_f64.sqrt()).abs() < 1e-12);
    }

    #[test]
    fn polygon_centroid_is_area_weighted() {
        let centroid = super::quad_centroid([
            Point2::new(0.0, 0.0),
            Point2::new(2.0, 0.0),
            Point2::new(1.0, 1.0),
            Point2::new(0.0, 1.0),
        ]);
        assert!((centroid.x - 7.0 / 9.0).abs() < 1e-12);
        assert!((centroid.y - 4.0 / 9.0).abs() < 1e-12);
    }

    #[test]
    fn signed_area_distinguishes_orientation() {
        let points = [
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(1.0, 1.0),
            Point2::new(0.0, 1.0),
        ];
        assert!((signed_quad_area(points) - 1.0).abs() < f64::EPSILON);
        assert!(signed_quad_area([points[0], points[3], points[2], points[1]]) < 0.0);
    }

    #[test]
    fn generated_default_mesh_is_positive_and_boundary_complete() {
        let configuration = example_configuration();
        let topology = build_cgrid_topology(&configuration).expect("valid topology");
        let mesh = crate::mesh::structured::generate_structured_mesh(&topology, &configuration)
            .expect("valid mesh");
        let report = evaluate_mesh_2d(&mesh);
        assert!(report.valid, "{:?}", report.errors);
        assert!(report.minimum_cell_area.is_some_and(|area| area > 0.0));
        assert!(report.minimum_orthogonal_quality > 0.0);
        assert!(report.maximum_equiangle_skewness < 1.0);
        assert!(report.minimum_scaled_jacobian > 0.0);
        assert_eq!(report.block_count, 6);
    }

    #[test]
    fn missing_boundary_assignment_is_fatal() {
        let mesh = StructuredMesh2D {
            optimization_report: None,
            points: vec![
                Point2::new(0.0, 0.0),
                Point2::new(1.0, 0.0),
                Point2::new(1.0, 1.0),
                Point2::new(0.0, 1.0),
            ],
            cells: vec![QuadCell {
                vertices: [0, 1, 2, 3],
                block_id: 0,
            }],
            blocks: Vec::new(),
            boundaries: vec![BoundaryPatch2D {
                role: BoundaryRole::Inlet,
                name: "inlet".to_owned(),
                patch_type: crate::config::PatchType::Patch,
                edges: vec![[0, 1]],
            }],
            maximum_radial_growth_ratio: 1.0,
        };
        let report = evaluate_mesh_2d(&mesh);
        assert!(!report.valid);
        assert!(
            report
                .errors
                .iter()
                .any(|error| error.contains("unassigned"))
        );
    }
}
