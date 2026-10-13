//! Reproducible study planning and analysis of user-supplied solver measurements.
//! No flow solution or convergence evidence is fabricated by this module.

use crate::les::feedback::fields;
use crate::{MeshConfiguration, MeshCounts, MeshError, mesh_counts, validate_configuration};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StudyAxis {
    Grid,
    TimeStep,
    Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StudyCase {
    pub name: String,
    pub axis: StudyAxis,
    pub level: usize,
    pub resolution: f64,
    pub time_step: f64,
    pub configuration: MeshConfiguration,
    pub counts: MeshCounts,
    pub within_cell_budget: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StudyPlan {
    pub format_version: u32,
    pub refinement_ratio: f64,
    pub is_les: bool,
    pub cases: Vec<StudyCase>,
    pub notes: Vec<String>,
}

/// Creates three grid, three time-step and (for 3D) three span variants.
/// Physical flow and the airfoil are fixed; each axis is varied independently.
/// Above-budget cases are marked, never silently coarsened. Mesh generation
/// and actual solver runs remain required before collecting measurements.
/// # Errors
/// Rejects invalid base configurations, time steps or refinement ratios.
#[allow(clippy::too_many_lines)] // Independent-axis preparation and validation stay together.
pub fn prepare_study(
    configuration: &MeshConfiguration,
    time_step: f64,
    ratio: f64,
    max_cells: Option<usize>,
) -> Result<StudyPlan, MeshError> {
    let validation = validate_configuration(configuration);
    if !validation.valid {
        return Err(MeshError::InvalidConfiguration(validation.issues));
    }
    if !time_step.is_finite()
        || time_step <= 0.0
        || !ratio.is_finite()
        || !(1.1..=2.0).contains(&ratio)
        || max_cells == Some(0)
    {
        return Err(fail(
            "time step must be positive, refinement ratio in [1.1,2], and budget positive",
        ));
    }
    let is_3d = configuration.extrusion.enabled
        && configuration.extrusion.front_back_patch_type
            != crate::config::FrontBackPatchType::Empty;
    let base = mesh_counts(configuration)?;
    let dimensions = if is_3d { 3.0 } else { 2.0 };
    let mut cases = Vec::new();
    for axis in [StudyAxis::Grid, StudyAxis::TimeStep, StudyAxis::Span] {
        if axis == StudyAxis::Span && !is_3d {
            continue;
        }
        for level in 0..3_u32 {
            let factor = ratio.powf(f64::from(level));
            let mut candidate = configuration.clone();
            match axis {
                StudyAxis::Grid => {
                    for count in [
                        &mut candidate.mesh.leading_edge_cells,
                        &mut candidate.mesh.upper_surface_cells,
                        &mut candidate.mesh.lower_surface_cells,
                        &mut candidate.mesh.wake_cells,
                        &mut candidate.mesh.radial_cells,
                        &mut candidate.les.minimum_cells_across_boundary_layer,
                    ] {
                        *count = scaled_count(*count, factor);
                    }
                    if is_3d {
                        candidate.mesh.spanwise_cells =
                            scaled_count(candidate.mesh.spanwise_cells, factor);
                    }
                    if candidate.les.estimate_first_cell_height {
                        candidate.les.target_y_plus /= factor;
                    } else if let Some(height) = &mut candidate.les.manual_first_cell_height {
                        *height /= factor;
                    }
                }
                StudyAxis::Span => {
                    candidate.extrusion.spanwise_thickness *= factor;
                    candidate.mesh.spanwise_cells =
                        scaled_count(candidate.mesh.spanwise_cells, factor);
                }
                StudyAxis::TimeStep => {}
            }
            let report = validate_configuration(&candidate);
            if !report.valid {
                return Err(MeshError::InvalidConfiguration(report.issues));
            }
            let counts = mesh_counts(&candidate)?;
            let cells = if candidate.extrusion.enabled {
                counts.hex_cells
            } else {
                counts.quad_cells
            };
            #[allow(clippy::cast_precision_loss)]
            let resolution = match axis {
                StudyAxis::Grid => {
                    let original = if is_3d {
                        base.hex_cells
                    } else {
                        base.quad_cells
                    };
                    let refined = if is_3d {
                        counts.hex_cells
                    } else {
                        counts.quad_cells
                    };
                    (original as f64 / refined as f64).powf(1.0 / dimensions)
                }
                StudyAxis::TimeStep => time_step / factor,
                StudyAxis::Span => candidate.extrusion.spanwise_thickness,
            };
            cases.push(StudyCase {
                name: format!(
                    "{}-{level}",
                    match axis {
                        StudyAxis::Grid => "grid",
                        StudyAxis::TimeStep => "time",
                        StudyAxis::Span => "span",
                    }
                ),
                axis,
                level: level as usize,
                resolution,
                time_step: if axis == StudyAxis::TimeStep {
                    time_step / factor
                } else {
                    time_step
                },
                configuration: candidate,
                counts,
                within_cell_budget: max_cells.is_none_or(|limit| cells <= limit),
            });
        }
    }
    Ok(StudyPlan { format_version: 1,refinement_ratio: ratio,is_les: configuration.resolved_mesh_intent().is_les(),cases,
        notes: vec!["Prepared configurations, not completed solver studies. Mesh checks, convergence, matching averaging windows and uncertainty estimates are required.".to_owned(),
            "Compare time-averaged/integral quantities, not instantaneous chaotic LES snapshots. Grid refinement preserves the domain; span refinement preserves nominal spanwise spacing.".to_owned(),
            "Cell-budget violations are marked. Do not claim independence from cases you could not run. Span studies require two-point correlations and cannot use Richardson extrapolation.".to_owned()] })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeriesAnalysis {
    pub axis: StudyAxis,
    pub metric: String,
    pub complete: bool,
    pub absolute_change: Option<f64>,
    pub relative_change: Option<f64>,
    pub observed_order: Option<f64>,
    pub extrapolated_value: Option<f64>,
    pub gci_fine_absolute: Option<f64>,
    pub warnings: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudyAnalysis {
    pub series: Vec<SeriesAnalysis>,
    pub missing_cases: Vec<String>,
    pub warnings: Vec<String>,
}

/// Analyses strict CSV: case,metric,value,uncertainty. Uncertainty is an absolute
/// confidence-interval half-width, in the metric's units. Oscillatory, incomplete
/// or statistically unresolved sequences do not receive a formal GCI.
/// # Errors
/// Rejects malformed/duplicate/nonfinite measurements or unknown case names.
pub fn analyze_study(plan: &StudyPlan, csv: &str) -> Result<StudyAnalysis, MeshError> {
    validate_manifest(plan)?;
    let mut lines = csv
        .trim_start_matches('\u{feff}')
        .lines()
        .filter(|l| !l.trim().is_empty());
    let headers = fields(lines.next().ok_or_else(|| fail("empty results table"))?)?;
    if headers != ["case", "metric", "value", "uncertainty"] {
        return Err(fail("results header must be case,metric,value,uncertainty"));
    }
    let mut measurements = BTreeMap::new();
    let mut metrics = BTreeSet::new();
    let mut present_cases = BTreeSet::new();
    for line in lines {
        let row = fields(line)?;
        if row.len() != 4 || row[0].is_empty() || row[1].is_empty() {
            return Err(fail("invalid results row"));
        }
        let case = plan
            .cases
            .iter()
            .find(|c| c.name == row[0])
            .ok_or_else(|| fail("results reference an unknown case"))?;
        let value = row[2]
            .parse::<f64>()
            .map_err(|_| fail("invalid result value"))?;
        let uncertainty = row[3]
            .parse::<f64>()
            .map_err(|_| fail("invalid uncertainty"))?;
        if !value.is_finite() || !uncertainty.is_finite() || uncertainty < 0.0 {
            return Err(fail(
                "measurements must be finite and uncertainty nonnegative",
            ));
        }
        if measurements
            .insert((case.name.clone(), row[1].clone()), (value, uncertainty))
            .is_some()
        {
            return Err(fail("duplicate case/metric measurement"));
        }
        metrics.insert((case.axis, row[1].clone()));
        present_cases.insert(case.name.clone());
    }
    if measurements.is_empty() {
        return Err(fail(
            "results contain no measurements; run the solver cases first",
        ));
    }
    let mut series = Vec::new();
    for (axis, metric) in metrics {
        let mut cases = plan
            .cases
            .iter()
            .filter(|c| c.axis == axis)
            .collect::<Vec<_>>();
        cases.sort_by_key(|c| c.level);
        let mut report = SeriesAnalysis {
            axis,
            metric: metric.clone(),
            complete: false,
            absolute_change: None,
            relative_change: None,
            observed_order: None,
            extrapolated_value: None,
            gci_fine_absolute: None,
            warnings: vec![],
        };
        let data = cases
            .iter()
            .map(|c| measurements.get(&(c.name.clone(), metric.clone())).copied())
            .collect::<Option<Vec<_>>>();
        if let Some(data) = data {
            analyze_series(&mut report, &cases, &data, plan.is_les);
        } else {
            report
                .warnings
                .push("All three levels need matching measurements for this metric.".to_owned());
        }
        series.push(report);
    }
    Ok(StudyAnalysis { series,missing_cases: plan.cases.iter().filter(|c| !present_cases.contains(&c.name)).map(|c| c.name.clone()).collect(),
        warnings: vec!["Computed estimates are conditional on asymptotic refinement and adequately converged/averaged measurements; they do not automatically establish independence or validate the turbulence model.".to_owned()] })
}

fn analyze_series(
    report: &mut SeriesAnalysis,
    cases: &[&StudyCase],
    data: &[(f64, f64)],
    is_les: bool,
) {
    report.complete = true;
    let change = (data[2].0 - data[1].0).abs();
    if !change.is_finite() {
        report.warnings.push(
            "Measurement differences overflow; rescale the metric before analysis.".to_owned(),
        );
        return;
    }
    report.absolute_change = Some(change);
    report.relative_change = (data[2].0.abs() > 0.0)
        .then(|| change / data[2].0.abs())
        .filter(|value| value.is_finite());
    if report.axis == StudyAxis::Span {
        report.warnings.push("Span changes are physical-domain sensitivity, not discretization error. Check spanwise two-point correlations separately.".to_owned());
        return;
    }
    if is_les && data.iter().any(|(_, uncertainty)| *uncertainty == 0.0) {
        report.warnings.push("LES statistical uncertainty is unquantified at one or more levels; no formal order/GCI is reported.".to_owned());
        return;
    }
    let d21 = data[1].0 - data[2].0;
    let d32 = data[0].0 - data[1].0;
    if d21.abs() <= data[1].1 + data[2].1
        || d32.abs() <= data[0].1 + data[1].1
        || !d21.is_finite()
        || !d32.is_finite()
        || d21.signum() != d32.signum()
    {
        report.warnings.push("Sequence is oscillatory, unchanged or unresolved within uncertainty; no positive asymptotic order/GCI is reported.".to_owned());
        return;
    }
    let r_coarse = cases[0].resolution / cases[1].resolution;
    let r_fine = cases[1].resolution / cases[2].resolution;
    let ratio = d32 / d21;
    let function = |p: f64| (r_coarse.powf(p) - 1.0) / (1.0 - r_fine.powf(-p)) - ratio;
    let (mut low, mut high) = (0.05, 10.0);
    if function(low) * function(high) > 0.0 {
        report.warnings.push("No observed order in (0.05,10) fits the refinement sequence; more levels or averaging are needed.".to_owned());
        return;
    }
    for _ in 0..100 {
        let middle = f64::midpoint(low, high);
        if function(middle) > 0.0 {
            high = middle;
        } else {
            low = middle;
        }
    }
    let order = f64::midpoint(low, high);
    let denominator = r_fine.powf(order) - 1.0;
    let extrapolated = data[2].0 - d21 / denominator;
    let gci = 1.25 * d21.abs() / denominator;
    if !extrapolated.is_finite() || !gci.is_finite() {
        report
            .warnings
            .push("Extrapolation overflow; rescale the supplied metric.".to_owned());
        return;
    }
    report.observed_order = Some(order);
    report.extrapolated_value = Some(extrapolated);
    if report.axis == StudyAxis::Grid {
        report.gci_fine_absolute = Some(gci);
    }
    report.warnings.push("Formal Richardson estimate assumes the asymptotic range. For LES, statistical error, SGS/filter changes and anisotropic refinement can invalidate that assumption.".to_owned());
}

fn validate_manifest(plan: &StudyPlan) -> Result<(), MeshError> {
    if plan.format_version != 1 {
        return Err(fail("unsupported study manifest version"));
    }
    let mut names = BTreeSet::new();
    let mut levels = BTreeSet::new();
    for case in &plan.cases {
        if !names.insert(&case.name)
            || !levels.insert((case.axis, case.level))
            || case.level > 2
            || !case.resolution.is_finite()
            || case.resolution <= 0.0
        {
            return Err(fail("invalid study case names, levels or resolutions"));
        }
    }
    for axis in [StudyAxis::Grid, StudyAxis::TimeStep, StudyAxis::Span] {
        let mut cases = plan
            .cases
            .iter()
            .filter(|c| c.axis == axis)
            .collect::<Vec<_>>();
        cases.sort_by_key(|c| c.level);
        if !cases.is_empty()
            && (cases.len() != 3
                || cases[0].level != 0
                || cases[1].level != 1
                || cases[2].level != 2)
        {
            return Err(fail("each study axis must have levels 0,1,2"));
        }
        if axis != StudyAxis::Span
            && cases
                .windows(2)
                .any(|pair| pair[0].resolution <= pair[1].resolution)
        {
            return Err(fail("grid/time resolutions must strictly decrease"));
        }
    }
    Ok(())
}

#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn scaled_count(count: usize, factor: f64) -> usize {
    (count as f64 * factor).ceil() as usize
}
fn fail(message: &str) -> MeshError {
    MeshError::FeedbackError(message.to_owned())
}
