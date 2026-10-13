//! Damped, quality-monotone coordinate optimization of an existing C-grid.
//! Physical boundaries and complete logical interfaces are held exactly fixed.

use super::{
    StructuredMesh2D,
    quality::{MeshQualityReport, evaluate_mesh_2d},
};
use crate::les::profile::{SurfaceSide, chord_coordinate};
use crate::{
    MeshConfiguration, MeshError,
    geometry::Point2,
    topology::{BoundaryRole, CGridTopology},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OptimizationReport {
    pub requested_iterations: usize,
    pub accepted_iterations: usize,
    pub wall_step_accepted: bool,
    pub initial_minimum_orthogonal_quality: f64,
    pub final_minimum_orthogonal_quality: f64,
    pub initial_minimum_scaled_jacobian: f64,
    pub final_minimum_scaled_jacobian: f64,
    pub maximum_displacement: f64,
    pub measured_maximum_radial_growth: f64,
    pub warnings: Vec<String>,
}

/// Attempts normal wall-column relaxation, then inverse-length Laplacian sweeps.
/// A line search accepts only positive, complete meshes whose worst geometric
/// orthogonality, corner Jacobian, skewness and measured radial growth do not
/// regress. Growth stays within the original/configured envelope. Smoothing
/// holds the inflation region fixed after the normal step.
/// This is a safeguarded algebraic smoother, not a full elliptic PDE solver.
/// # Errors
/// Refuses to optimize an invalid initial grid.
pub(crate) fn optimize(
    mesh: &mut StructuredMesh2D,
    topology: &CGridTopology,
    configuration: &MeshConfiguration,
) -> Result<(), MeshError> {
    let options = &configuration.optimization;
    if !options.orthogonalize_wall && options.smoothing_iterations == 0 {
        return Ok(());
    }
    let initial = evaluate_mesh_2d(mesh);
    if !initial.valid {
        return Err(MeshError::QualityError(
            "optimization requires a positive initial grid".to_owned(),
        ));
    }
    let initial_points = mesh.points.clone();
    let mut quality = initial.clone();
    let mut growth = measured_radial_growth(mesh);
    let growth_limit = growth.max(configuration.les.max_radial_growth_ratio);
    let mut report = OptimizationReport {
        requested_iterations: options.smoothing_iterations,
        accepted_iterations: 0,
        wall_step_accepted: false,
        initial_minimum_orthogonal_quality: initial.minimum_orthogonal_quality,
        final_minimum_orthogonal_quality: initial.minimum_orthogonal_quality,
        initial_minimum_scaled_jacobian: initial.minimum_scaled_jacobian,
        final_minimum_scaled_jacobian: initial.minimum_scaled_jacobian,
        maximum_displacement: 0.0,
        measured_maximum_radial_growth: growth,
        warnings: Vec::new(),
    };
    if options.orthogonalize_wall {
        let proposed = wall_targets(mesh, topology, configuration);
        report.wall_step_accepted =
            accept_step(mesh, &proposed, &mut quality, &mut growth, growth_limit);
        if !report.wall_step_accepted {
            report.warnings.push("No wall-normal step satisfied the positivity and nonregression safeguards; initial coordinates retained.".to_owned());
        }
    }
    for _ in 0..options.smoothing_iterations {
        let proposed = smoothing_targets(mesh, topology.wall_layer_cells);
        if !accept_step(mesh, &proposed, &mut quality, &mut growth, growth_limit) {
            break;
        }
        report.accepted_iterations += 1;
    }
    report.final_minimum_orthogonal_quality = quality.minimum_orthogonal_quality;
    report.final_minimum_scaled_jacobian = quality.minimum_scaled_jacobian;
    report.measured_maximum_radial_growth = growth;
    report.maximum_displacement = mesh
        .points
        .iter()
        .zip(initial_points)
        .map(|(a, b)| a.distance_to(b))
        .fold(0.0, f64::max);
    if report.accepted_iterations < options.smoothing_iterations {
        report
            .warnings
            .push("Smoothing stopped when no further safeguarded step was found.".to_owned());
    }
    for block in &mut mesh.blocks {
        for (point, index) in block.points.iter_mut().zip(&block.global_point_indices) {
            *point = mesh.points[*index];
        }
    }
    mesh.optimization_report = Some(report);
    Ok(())
}

fn wall_targets(
    mesh: &StructuredMesh2D,
    topology: &CGridTopology,
    configuration: &MeshConfiguration,
) -> Vec<Point2> {
    let mut proposed = mesh.points.clone();
    let layers = topology.wall_layer_cells.max(1);
    for block in &mesh.blocks {
        if topology.blocks[block.id].boundaries.bottom != Some(BoundaryRole::Airfoil) {
            continue;
        }
        let side = if block.name.starts_with("upper") {
            SurfaceSide::Upper
        } else {
            SurfaceSide::Lower
        };
        for i in 1..block.ni {
            let wall = mesh.points[block.global_index(i, 0)];
            let before = mesh.points[block.global_index(i - 1, 0)];
            let after = mesh.points[block.global_index(i + 1, 0)];
            let tx = after.x - before.x;
            let ty = after.y - before.y;
            let length = tx.hypot(ty);
            if length <= 0.0 {
                continue;
            }
            let (nx, ny) = (-ty / length, tx / length);
            let first = mesh.points[block.global_index(i, 1)];
            let sign = ((first.x - wall.x) * nx + (first.y - wall.y) * ny).signum();
            let factor = configuration
                .les
                .wall_shear_profile
                .as_ref()
                .map_or(1.0, |profile| {
                    let tau = profile
                        .sample(side, chord_coordinate(wall, configuration))
                        .wall_shear_stress_pa;
                    // Do not coarsen zero-shear/separated points without a wall model.
                    if tau <= 0.0 {
                        1.0
                    } else {
                        (profile.maximum_shear() / tau).sqrt().clamp(1.0, 4.0)
                    }
                });
            for j in 1..block.nj.min(2 * layers + 1) {
                let index = block.global_index(i, j);
                let original = mesh.points[index];
                let distance = ((original.x - wall.x) * nx + (original.y - wall.y) * ny) * sign;
                #[allow(clippy::cast_precision_loss)]
                let taper = if j <= layers {
                    1.0
                } else {
                    1.0 - (j - layers) as f64 / layers as f64
                };
                let target = Point2::new(
                    wall.x + nx * sign * distance * factor,
                    wall.y + ny * sign * distance * factor,
                );
                proposed[index] = Point2::new(
                    original.x + taper * (target.x - original.x),
                    original.y + taper * (target.y - original.y),
                );
            }
        }
    }
    proposed
}

fn smoothing_targets(mesh: &StructuredMesh2D, protected_layers: usize) -> Vec<Point2> {
    let mut proposed = mesh.points.clone();
    for block in &mesh.blocks {
        for j in protected_layers.max(1) + 1..block.nj {
            for i in 1..block.ni {
                let index = block.global_index(i, j);
                let point = mesh.points[index];
                let neighbors = [
                    block.global_index(i - 1, j),
                    block.global_index(i + 1, j),
                    block.global_index(i, j - 1),
                    block.global_index(i, j + 1),
                ];
                let mut sum = 0.0;
                let mut dx = 0.0;
                let mut dy = 0.0;
                for neighbor in neighbors {
                    let other = mesh.points[neighbor];
                    let weight = 1.0 / point.distance_to(other).max(f64::MIN_POSITIVE);
                    sum += weight;
                    dx += weight * (other.x - point.x);
                    dy += weight * (other.y - point.y);
                }
                proposed[index] = Point2::new(point.x + dx / sum, point.y + dy / sum);
            }
        }
    }
    proposed
}

fn accept_step(
    mesh: &mut StructuredMesh2D,
    proposed: &[Point2],
    quality: &mut MeshQualityReport,
    growth: &mut f64,
    growth_limit: f64,
) -> bool {
    let original = mesh.points.clone();
    let mut relaxation = 1.0;
    for _ in 0..10 {
        for ((point, a), b) in mesh.points.iter_mut().zip(&original).zip(proposed) {
            *point = Point2::new(
                a.x + relaxation * (b.x - a.x),
                a.y + relaxation * (b.y - a.y),
            );
        }
        let candidate = evaluate_mesh_2d(mesh);
        let candidate_growth = measured_radial_growth(mesh);
        if candidate.valid
            && candidate.minimum_orthogonal_quality + 1e-12 >= quality.minimum_orthogonal_quality
            && candidate.minimum_scaled_jacobian + 1e-12 >= quality.minimum_scaled_jacobian
            && candidate.maximum_equiangle_skewness <= quality.maximum_equiangle_skewness + 1e-12
            && candidate_growth <= growth_limit * (1.0 + 1e-10)
            && mesh
                .points
                .iter()
                .zip(&original)
                .any(|(a, b)| a.distance_to(*b) > 1e-14)
        {
            *quality = candidate;
            *growth = candidate_growth;
            return true;
        }
        relaxation *= 0.5;
    }
    mesh.points = original;
    false
}

pub(crate) fn measured_radial_growth(mesh: &StructuredMesh2D) -> f64 {
    let mut maximum: f64 = 1.0;
    for block in &mesh.blocks {
        for i in 0..=block.ni {
            let mut previous = None;
            for j in 0..block.nj {
                let length = mesh.points[block.global_index(i, j)]
                    .distance_to(mesh.points[block.global_index(i, j + 1)]);
                if let Some(before) = previous
                    && before > 0.0
                    && length > 0.0
                {
                    maximum = maximum.max(length / before).max(before / length);
                }
                previous = Some(length);
            }
        }
    }
    maximum
}
