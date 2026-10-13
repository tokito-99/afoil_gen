//! Wall-adjacent geometry audit, independent of visualization downsampling.

use super::{StructuredMesh2D, quality::quad_centroid};
use crate::les::wall_spacing::{
    estimate_first_cell_height, estimate_first_cell_height_from_wall_shear, resolve_wall_spacing,
};
use crate::{MeshConfiguration, MeshError, geometry::Point2, topology::BoundaryRole};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Actual geometry ranges plus predictions under one reference wall shear.
/// These are not solved/local y+ values and do not certify LES adequacy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WallResolutionAudit {
    pub wall_face_count: usize,
    pub reference_first_cell_height: f64,
    pub minimum_normal_height: f64,
    pub maximum_normal_height: f64,
    pub minimum_centroid_distance: f64,
    pub maximum_centroid_distance: f64,
    pub maximum_nonorthogonality_deg: f64,
    pub minimum_predicted_y_plus: f64,
    pub maximum_predicted_y_plus: f64,
    pub maximum_predicted_streamwise_plus: f64,
    pub minimum_cells_in_reference_layer: usize,
    pub reverse_shear_face_count: usize,
    pub transitional_face_count: usize,
    pub reference_wall_shear_pa: f64,
    pub shear_source: String,
    pub warnings: Vec<String>,
}

/// Audits every AIRFOIL boundary edge, including a finite trailing-edge base.
/// Normal height is the mean projection of the two opposite vertices onto
/// the wall-face normal. Wall distance uses the polygon's area centroid,
/// not half the requested stretching height or an average of its vertices.
/// For this generator's straight span extrusion the same distances apply in 3D.
///
/// # Errors
/// Rejects missing/nonmanifold wall edges, invalid indices and degenerate cells.
#[allow(clippy::too_many_lines)] // Complete face audit kept together; geometry helpers below.
pub fn audit_wall_resolution(
    mesh: &StructuredMesh2D,
    configuration: &MeshConfiguration,
) -> Result<WallResolutionAudit, MeshError> {
    let fail = |message: &str| MeshError::QualityError(message.to_owned());
    let owners = wall_owners(mesh)?;
    let flow = &configuration.flow;
    let reference = if let Some(tau) = configuration.les.solver_wall_shear_stress {
        estimate_first_cell_height_from_wall_shear(
            tau,
            flow.freestream_velocity,
            flow.density,
            flow.dynamic_viscosity,
            1.0,
        )?
    } else {
        estimate_first_cell_height(
            configuration.derived_reynolds_number(),
            flow.freestream_velocity,
            flow.density,
            flow.dynamic_viscosity,
            1.0,
        )?
    };
    let mut audit = WallResolutionAudit {
        wall_face_count: owners.len(),
        reference_first_cell_height: resolve_wall_spacing(configuration)?.first_cell_height,
        minimum_normal_height: f64::INFINITY,
        maximum_normal_height: 0.0,
        minimum_centroid_distance: f64::INFINITY,
        maximum_centroid_distance: 0.0,
        maximum_nonorthogonality_deg: 0.0,
        minimum_predicted_y_plus: f64::INFINITY,
        maximum_predicted_y_plus: 0.0,
        maximum_predicted_streamwise_plus: 0.0,
        minimum_cells_in_reference_layer: measured_layer_coverage(mesh, configuration),
        reverse_shear_face_count: 0,
        transitional_face_count: 0,
        reference_wall_shear_pa: reference.wall_shear_stress,
        shear_source: if configuration.les.wall_shear_profile.is_some() {
            "SPATIAL_SOLVER_PROFILE"
        } else if configuration.les.solver_wall_shear_stress.is_some() {
            "SOLVER_FEEDBACK"
        } else {
            "FLAT_PLATE_CORRELATION"
        }
        .to_owned(),
        warnings: Vec::new(),
    };
    for (edge, cells) in owners {
        if cells.len() != 1 {
            return Err(fail("wall edge must have exactly one adjacent cell"));
        }
        let cell = &mesh.cells[cells[0]];
        let mut points = [Point2::new(0.0, 0.0); 4];
        for (point, index) in points.iter_mut().zip(cell.vertices) {
            *point = *mesh
                .points
                .get(index)
                .ok_or_else(|| fail("wall cell has invalid point index"))?;
        }
        let a = *mesh
            .points
            .get(edge[0])
            .ok_or_else(|| fail("invalid wall point"))?;
        let b = *mesh
            .points
            .get(edge[1])
            .ok_or_else(|| fail("invalid wall point"))?;
        let length = a.distance_to(b);
        if length <= 0.0 || !length.is_finite() {
            return Err(fail("degenerate wall face"));
        }
        let nx = -(b.y - a.y) / length;
        let ny = (b.x - a.x) / length;
        let midpoint = Point2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
        let center = quad_centroid(points);
        let dx = center.x - midpoint.x;
        let dy = center.y - midpoint.y;
        let distance = (dx * nx + dy * ny).abs();
        let height = cell
            .vertices
            .iter()
            .zip(points)
            .filter(|(index, _)| !edge.contains(index))
            .map(|(_, p)| (p.x - a.x) * nx + (p.y - a.y) * ny)
            .sum::<f64>()
            .abs()
            * 0.5;
        if !distance.is_finite() || distance <= 0.0 || !height.is_finite() || height <= 0.0 {
            return Err(fail("wall cell has degenerate normal spacing"));
        }
        audit.minimum_normal_height = audit.minimum_normal_height.min(height);
        audit.maximum_normal_height = audit.maximum_normal_height.max(height);
        audit.minimum_centroid_distance = audit.minimum_centroid_distance.min(distance);
        audit.maximum_centroid_distance = audit.maximum_centroid_distance.max(distance);
        audit.maximum_nonorthogonality_deg = audit.maximum_nonorthogonality_deg.max(
            (distance / dx.hypot(dy))
                .clamp(0.0, 1.0)
                .acos()
                .to_degrees(),
        );
        let (tau, reversed, transitional) = local_shear(
            configuration,
            mesh,
            cell.block_id,
            a,
            b,
            reference.wall_shear_stress,
        );
        let inverse_viscous_length =
            (tau / flow.density).sqrt() * flow.density / flow.dynamic_viscosity;
        let y_plus = distance * inverse_viscous_length;
        audit.minimum_predicted_y_plus = audit.minimum_predicted_y_plus.min(y_plus);
        audit.maximum_predicted_y_plus = audit.maximum_predicted_y_plus.max(y_plus);
        audit.maximum_predicted_streamwise_plus = audit
            .maximum_predicted_streamwise_plus
            .max(length * inverse_viscous_length);
        audit.reverse_shear_face_count += usize::from(reversed);
        audit.transitional_face_count += usize::from(transitional);
    }
    append_audit_warnings(&mut audit, configuration.les.target_y_plus);
    Ok(audit)
}

fn local_shear(
    configuration: &MeshConfiguration,
    mesh: &StructuredMesh2D,
    block_id: usize,
    a: Point2,
    b: Point2,
    fallback: f64,
) -> (f64, bool, bool) {
    use crate::les::profile::{SurfaceSide, chord_coordinate};
    let Some(profile) = &configuration.les.wall_shear_profile else {
        return (fallback, false, false);
    };
    let Some(block) = mesh.blocks.get(block_id) else {
        return (fallback, false, false);
    };
    let side = if block.name.starts_with("upper") {
        SurfaceSide::Upper
    } else if block.name.starts_with("lower") {
        SurfaceSide::Lower
    } else {
        return (fallback, false, false);
    };
    let x0 = chord_coordinate(a, configuration);
    let x1 = chord_coordinate(b, configuration);
    let sample = profile.sample(side, (x0 + x1) * 0.5);
    (
        profile.maximum_on_interval(side, x0, x1),
        sample.signed_wall_shear_stress_pa.is_some_and(|v| v < 0.0),
        sample.intermittency.is_some_and(|v| v > 0.05 && v < 0.95),
    )
}

fn measured_layer_coverage(mesh: &StructuredMesh2D, configuration: &MeshConfiguration) -> usize {
    use crate::les::profile::{SurfaceSide, chord_coordinate};
    let mut minimum = usize::MAX;
    for block in &mesh.blocks {
        if !(block.name == "upper_surface"
            || block.name == "lower_surface"
            || block.name == "upper_leading_farfield"
            || block.name == "lower_leading_farfield")
        {
            continue;
        }
        for i in 0..block.ni {
            let a = mesh.points[block.global_index(i, 0)];
            let b = mesh.points[block.global_index(i + 1, 0)];
            let length = a.distance_to(b);
            let (nx, ny) = (-(b.y - a.y) / length, (b.x - a.x) / length);
            let midpoint = Point2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
            let thickness = configuration
                .les
                .wall_shear_profile
                .as_ref()
                .and_then(|profile| {
                    let side = if block.name.starts_with("upper") {
                        SurfaceSide::Upper
                    } else {
                        SurfaceSide::Lower
                    };
                    profile
                        .sample(side, chord_coordinate(midpoint, configuration))
                        .boundary_layer_thickness_chords
                })
                .unwrap_or(configuration.les.reference_boundary_layer_thickness_chords)
                * configuration.airfoil.chord_length;
            let mut count = 0;
            for j in 0..block.nj {
                let center = quad_centroid([
                    mesh.points[block.global_index(i, j)],
                    mesh.points[block.global_index(i + 1, j)],
                    mesh.points[block.global_index(i + 1, j + 1)],
                    mesh.points[block.global_index(i, j + 1)],
                ]);
                if ((center.x - midpoint.x) * nx + (center.y - midpoint.y) * ny).abs() > thickness {
                    break;
                }
                count += 1;
            }
            minimum = minimum.min(count);
        }
    }
    if minimum == usize::MAX { 0 } else { minimum }
}

fn append_audit_warnings(audit: &mut WallResolutionAudit, target_y_plus: f64) {
    if audit.maximum_predicted_y_plus > target_y_plus * 1.05 {
        audit.warnings.push("Predicted cell-center y+ exceeds the target under the reference shear; reduce first-layer spacing and check solved wall y+.".to_owned());
    }
    if audit.maximum_nonorthogonality_deg > 30.0 {
        audit.warnings.push("Some wall cells deviate more than 30 degrees from the face normal; review the near-wall grid.".to_owned());
    }
}

fn wall_owners(mesh: &StructuredMesh2D) -> Result<HashMap<[usize; 2], Vec<usize>>, MeshError> {
    let fail = |message: &str| MeshError::QualityError(message.to_owned());
    let mut owners = HashMap::new();
    for patch in &mesh.boundaries {
        if patch.role == BoundaryRole::Airfoil {
            for edge in &patch.edges {
                if owners.insert(key(*edge), Vec::<usize>::new()).is_some() {
                    return Err(fail("wall audit found a duplicate wall edge"));
                }
            }
        }
    }
    if owners.is_empty() {
        return Err(fail("wall audit found no airfoil edges"));
    }
    for (index, cell) in mesh.cells.iter().enumerate() {
        for i in 0..4 {
            if let Some(cells) =
                owners.get_mut(&key([cell.vertices[i], cell.vertices[(i + 1) % 4]]))
            {
                cells.push(index);
            }
        }
    }
    Ok(owners)
}

fn key(mut edge: [usize; 2]) -> [usize; 2] {
    edge.sort_unstable();
    edge
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::PatchType,
        mesh::{BoundaryPatch2D, QuadCell},
    };

    #[test]
    fn measures_centroid_not_requested_height_and_includes_only_wall() {
        let configuration = crate::test_support::example_configuration();
        let mesh = StructuredMesh2D {
            optimization_report: None,
            points: vec![
                Point2::new(0.0, 0.0),
                Point2::new(2.0, 0.0),
                Point2::new(1.0, 1.0),
                Point2::new(0.0, 1.0),
            ],
            cells: vec![QuadCell {
                vertices: [0, 1, 2, 3],
                block_id: 0,
            }],
            blocks: vec![],
            boundaries: vec![BoundaryPatch2D {
                role: BoundaryRole::Airfoil,
                name: "wall".to_owned(),
                patch_type: PatchType::Wall,
                edges: vec![[0, 1]],
            }],
            maximum_radial_growth_ratio: 1.0,
        };
        let report = audit_wall_resolution(&mesh, &configuration).unwrap();
        assert_eq!(report.wall_face_count, 1);
        assert!((report.minimum_normal_height - 1.0).abs() < 1e-12);
        assert!((report.minimum_centroid_distance - 4.0 / 9.0).abs() < 1e-12);
        assert!(report.maximum_nonorthogonality_deg > 0.0);
        assert!(report.maximum_predicted_y_plus > 0.0);
    }

    #[test]
    fn rejects_missing_wall() {
        let mesh = StructuredMesh2D {
            optimization_report: None,
            points: vec![],
            cells: vec![],
            blocks: vec![],
            boundaries: vec![],
            maximum_radial_growth_ratio: 1.0,
        };
        assert!(
            audit_wall_resolution(&mesh, &crate::test_support::example_configuration()).is_err()
        );
    }
}
