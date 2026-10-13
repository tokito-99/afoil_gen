//! Bounded, deterministic count search. Physical domains and wall-unit limits
//! are never reduced to fit a license budget; infeasibility is an explicit result.

use super::{
    sizing::estimate_les_sizing, stretching::solve_layered_stretching,
    wall_spacing::resolve_wall_spacing,
};
use crate::{
    MeshConfiguration, MeshError, PreviewLevel, generate_mesh_preview, validate_configuration,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeshCounts {
    pub quad_cells: usize,
    pub hex_cells: usize,
    pub points_2d: usize,
    pub points: usize,
}

/// Exact counts for this six/seven-block topology, without allocating nodes.
/// # Errors
/// Returns an error on integer overflow or zero counts.
pub fn mesh_counts(configuration: &MeshConfiguration) -> Result<MeshCounts, MeshError> {
    let mesh = &configuration.mesh;
    let finite = configuration.airfoil.finite_trailing_edge;
    let fail =
        || MeshError::GeometryError("mesh count arithmetic overflow or zero resolution".to_owned());
    if [
        mesh.leading_edge_cells,
        mesh.upper_surface_cells,
        mesh.lower_surface_cells,
        mesh.wake_cells,
        mesh.radial_cells,
        mesh.spanwise_cells,
    ]
    .contains(&0)
    {
        return Err(fail());
    }
    let sum = mesh
        .leading_edge_cells
        .checked_mul(2)
        .and_then(|v| v.checked_add(mesh.upper_surface_cells))
        .and_then(|v| v.checked_add(mesh.lower_surface_cells))
        .and_then(|v| {
            mesh.wake_cells
                .checked_mul(if finite { 3 } else { 2 })
                .and_then(|wake| v.checked_add(wake))
        })
        .ok_or_else(fail)?;
    let radial_nodes = mesh.radial_cells.checked_add(1).ok_or_else(fail)?;
    let extra = if finite { 2_usize } else { 1 };
    let points_2d = sum
        .checked_add(extra)
        .and_then(|v| v.checked_mul(radial_nodes))
        .and_then(|v| {
            mesh.wake_cells
                .checked_add(1)
                .and_then(|w| w.checked_mul(extra))
                .and_then(|w| v.checked_sub(w))
        })
        .ok_or_else(fail)?;
    let quad_cells = sum.checked_mul(mesh.radial_cells).ok_or_else(fail)?;
    let hex_cells = quad_cells
        .checked_mul(mesh.spanwise_cells)
        .ok_or_else(fail)?;
    let points = if configuration.extrusion.enabled {
        mesh.spanwise_cells
            .checked_add(1)
            .and_then(|planes| points_2d.checked_mul(planes))
            .ok_or_else(fail)?
    } else {
        points_2d
    };
    Ok(MeshCounts {
        quad_cells,
        hex_cells,
        points_2d,
        points,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BudgetConstraints {
    pub max_cells: usize,
    #[serde(default)]
    pub max_points: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetPlan {
    pub feasible: bool,
    pub candidates_evaluated: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configuration: Option<MeshConfiguration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub counts: Option<MeshCounts>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wall_audit: Option<crate::mesh::wall_resolution::WallResolutionAudit>,
    pub reasons: Vec<String>,
}

/// Searches radial counts up to 512 and verifies at most eight generated 2D
/// candidates. Conservative surface/span recommendations seed the search.
/// This is not a global optimum proof or certification of LES adequacy.
/// # Errors
/// Rejects invalid budgets and non-resolution configuration errors.
#[allow(clippy::too_many_lines)]
pub fn plan_cell_budget(
    configuration: &MeshConfiguration,
    constraints: &BudgetConstraints,
) -> Result<BudgetPlan, MeshError> {
    if constraints.max_cells == 0 || constraints.max_points == Some(0) {
        return Err(MeshError::GeometryError(
            "cell and point budgets must be positive".to_owned(),
        ));
    }
    let report = validate_configuration(configuration);
    let errors = report
        .issues
        .into_iter()
        .filter(|i| {
            i.severity == crate::ValidationSeverity::Error
                && ![
                    "INFEASIBLE_RADIAL_STRETCHING",
                    "WALL_LAYERS_USE_ALL_RADIAL_CELLS",
                    "RADIAL_CELL_COUNT_TOO_SMALL",
                    "LES_REQUIRES_MULTIPLE_SPANWISE_CELLS",
                    "RADIAL_RESOURCE_LIMIT",
                    "MESH_RESOURCE_LIMIT",
                ]
                .contains(&i.code.as_str())
        })
        .collect::<Vec<_>>();
    if !errors.is_empty() {
        return Err(MeshError::InvalidConfiguration(errors));
    }
    let mut plan = BudgetPlan {
        feasible: false,
        candidates_evaluated: 0,
        configuration: None,
        counts: None,
        wall_audit: None,
        reasons: vec![],
    };
    if configuration.resolved_mesh_intent().is_les()
        && configuration.extrusion.spanwise_thickness
            < configuration.airfoil.chord_length * 0.1 * (1.0 - 1e-12)
    {
        plan.reasons.push("Span is below the 0.1c screening minimum. Increase it and perform a span study; the planner will not silently change the physical span.".to_owned());
        return Ok(plan);
    }
    let mut candidate = configuration.clone();
    // Explicit, physical cell-center interpretation of the unchanged y+ limit.
    candidate.optimization.target_cell_center_y_plus = true;
    for _ in 0..8 {
        let Some(radial) = minimum_radial_count(&candidate) else {
            plan.reasons.push("No radial distribution within the search bound of 512 cells meets the requested wall layers and growth limit.".to_owned());
            return Ok(plan);
        };
        candidate.mesh.radial_cells = radial;
        let sizing = estimate_les_sizing(&candidate)?;
        if plan.candidates_evaluated == 0 {
            candidate.mesh.leading_edge_cells = sizing.recommended_leading_edge_cells;
            candidate.mesh.upper_surface_cells = sizing.recommended_upper_surface_cells;
            candidate.mesh.lower_surface_cells = sizing.recommended_lower_surface_cells;
            if candidate.resolved_mesh_intent().is_les() {
                candidate.mesh.spanwise_cells = sizing.recommended_spanwise_cells.max(2);
            }
        }
        let counts = mesh_counts(&candidate)?;
        let cells = if candidate.extrusion.enabled {
            counts.hex_cells
        } else {
            counts.quad_cells
        };
        plan.counts = Some(counts.clone());
        if cells > constraints.max_cells
            || constraints
                .max_points
                .is_some_and(|maximum| counts.points > maximum)
        {
            plan.reasons.push(format!("The conservative candidate needs {cells} cells and {} points, exceeding the budget. Targets and the physical domain were not relaxed; this bounded search is not a global infeasibility proof.",counts.points));
            return Ok(plan);
        }
        let preview = match generate_mesh_preview(&candidate, PreviewLevel::GeometryOnly) {
            Ok(preview) => preview,
            Err(error) => {
                plan.reasons
                    .push(format!("Candidate mesh generation failed: {error}"));
                return Ok(plan);
            }
        };
        plan.candidates_evaluated += 1;
        let audit = preview.wall_resolution.ok_or_else(|| {
            MeshError::QualityError("budget verification requires a wall audit".to_owned())
        })?;
        let surface_ratio =
            audit.maximum_predicted_streamwise_plus / configuration.les.target_streamwise_plus;
        let layers_ok = audit.minimum_cells_in_reference_layer
            >= configuration.les.minimum_cells_across_boundary_layer;
        let span_ok = !candidate.resolved_mesh_intent().is_les()
            || estimate_les_sizing(&candidate)?.estimated_maximum_spanwise_plus
                <= configuration.les.target_spanwise_plus * (1.0 + 1e-8);
        let y_ok = audit.maximum_predicted_y_plus <= configuration.les.target_y_plus * (1.0 + 1e-5);
        let shape_ok = preview.quality.minimum_orthogonal_quality >= 0.1
            && preview.quality.maximum_equiangle_skewness <= 0.95;
        plan.wall_audit = Some(audit.clone());
        if surface_ratio <= 1.0 + 1e-8 && layers_ok && span_ok && y_ok && shape_ok {
            plan.feasible = true;
            plan.configuration = Some(candidate);
            plan.reasons.push("Meets generated-geometry checks under the reference shear. Cell-center y+ interpretation is enabled. Verify solved wall units, wake resolution, transition, spectra and span/time/grid independence in the solver.".to_owned());
            return Ok(plan);
        }
        if !y_ok || !shape_ok || !span_ok {
            plan.reasons.push(format!("Candidate fails unchanged constraints: y+ OK={y_ok}, geometric shape OK={shape_ok}, spanwise spacing OK={span_ok}. Review wall construction, manual height or grading."));
            return Ok(plan);
        }
        if surface_ratio > 1.0 + 1e-8 {
            for count in [
                &mut candidate.mesh.leading_edge_cells,
                &mut candidate.mesh.upper_surface_cells,
                &mut candidate.mesh.lower_surface_cells,
            ] {
                *count = increased_count(*count, surface_ratio * 1.02);
            }
        }
        if !layers_ok {
            #[allow(clippy::cast_precision_loss)]
            let factor = configuration.les.minimum_cells_across_boundary_layer as f64
                / audit.minimum_cells_in_reference_layer.max(1) as f64;
            candidate.les.minimum_cells_across_boundary_layer = increased_count(
                candidate.les.minimum_cells_across_boundary_layer,
                factor * 1.02,
            );
        }
    }
    plan.reasons.push(
        "No verified candidate was found within eight geometry checks; no targets were relaxed."
            .to_owned(),
    );
    Ok(plan)
}

fn minimum_radial_count(configuration: &MeshConfiguration) -> Option<usize> {
    let spacing = resolve_wall_spacing(configuration).ok()?;
    let chord = configuration.airfoil.chord_length;
    let extent = chord
        * configuration
            .domain
            .upper_radius_chords
            .min(configuration.domain.lower_radius_chords);
    (configuration
        .les
        .minimum_cells_across_boundary_layer
        .saturating_add(8)
        .max(10)..=512)
        .find(|count| {
            solve_layered_stretching(
                spacing.first_cell_height,
                extent,
                *count,
                chord * configuration.les.reference_boundary_layer_thickness_chords,
                configuration.les.minimum_cells_across_boundary_layer,
                configuration.les.max_radial_growth_ratio,
            )
            .is_ok()
        })
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn increased_count(count: usize, factor: f64) -> usize {
    (count as f64 * factor).ceil().min(100_000.0) as usize
}
