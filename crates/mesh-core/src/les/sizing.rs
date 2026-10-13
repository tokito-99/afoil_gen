//! Three-dimensional LES mesh-sizing estimates and readiness diagnostics.

use serde::{Deserialize, Serialize};

use crate::{
    config::{FrontBackPatchType, LesMode, MeshConfiguration},
    error::MeshError,
    geometry::{Point2, naca4::generate_airfoil},
    les::{
        stretching::solve_layered_stretching,
        wall_spacing::{
            estimate_first_cell_height, estimate_first_cell_height_from_wall_shear,
            resolve_wall_spacing,
        },
    },
};

const LEADING_EDGE_SPLIT_FRACTION: f64 = 0.3;

struct SurfaceSizing {
    maximum_spacing: f64,
    maximum_plus: f64,
    recommended_leading_cells: usize,
    recommended_upper_cells: usize,
    recommended_lower_cells: usize,
}

/// Deterministic pre-solution sizing guidance.
///
/// Wall units use the same preliminary skin-friction estimate as the
/// first-cell calculation. Solver-derived values must replace these estimates
/// during mesh verification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LesSizingEstimate {
    pub mode: LesMode,
    pub property_reynolds_number: f64,
    pub reynolds_mismatch_percent: f64,
    pub viscous_length: f64,
    pub first_cell_height: f64,
    pub target_streamwise_spacing: f64,
    pub maximum_surface_streamwise_spacing: f64,
    pub estimated_maximum_surface_streamwise_plus: f64,
    pub recommended_leading_edge_cells: usize,
    pub recommended_upper_surface_cells: usize,
    pub recommended_lower_surface_cells: usize,
    pub target_spanwise_spacing: f64,
    pub maximum_spanwise_spacing: f64,
    pub estimated_maximum_spanwise_plus: f64,
    pub recommended_spanwise_cells: usize,
    pub spanwise_extent_chords: f64,
    pub reference_boundary_layer_thickness: f64,
    pub radial_cells_within_boundary_layer: usize,
    pub periodic_three_dimensional: bool,
}

/// Estimates wall-unit spacings, spanwise count, and boundary-layer coverage.
///
/// # Errors
///
/// Returns physical-input or stretching errors for configurations that cannot
/// produce meaningful LES sizing guidance.
pub fn estimate_les_sizing(
    configuration: &MeshConfiguration,
) -> Result<LesSizingEstimate, MeshError> {
    let flow = &configuration.flow;
    let chord = configuration.airfoil.chord_length;
    if !configuration.les.target_streamwise_plus.is_finite()
        || configuration.les.target_streamwise_plus <= 0.0
        || !configuration.les.target_spanwise_plus.is_finite()
        || configuration.les.target_spanwise_plus <= 0.0
        || !configuration.mesh.spanwise_grading.is_finite()
        || configuration.mesh.spanwise_grading <= 0.0
        || configuration.mesh.spanwise_cells == 0
        || !configuration.extrusion.spanwise_thickness.is_finite()
        || configuration.extrusion.spanwise_thickness <= 0.0
    {
        return Err(MeshError::GeometryError(
            "LES sizing requires positive wall-unit targets, span, grading, and cell count"
                .to_owned(),
        ));
    }
    let property_reynolds_number = configuration.derived_reynolds_number();
    let reynolds_mismatch_percent = 0.0;
    let unit_wall_spacing =
        if let Some(wall_shear_stress) = configuration.les.solver_wall_shear_stress {
            estimate_first_cell_height_from_wall_shear(
                wall_shear_stress,
                flow.freestream_velocity,
                flow.density,
                flow.dynamic_viscosity,
                1.0,
            )?
        } else {
            estimate_first_cell_height(
                property_reynolds_number,
                flow.freestream_velocity,
                flow.density,
                flow.dynamic_viscosity,
                1.0,
            )?
        };
    let viscous_length = unit_wall_spacing.first_cell_height;
    let first_cell_height = resolve_wall_spacing(configuration)?.first_cell_height;
    let target_streamwise_spacing = configuration.les.target_streamwise_plus * viscous_length;
    let surface =
        estimate_surface_sizing(configuration, target_streamwise_spacing, viscous_length)?;
    let target_spanwise_spacing = configuration.les.target_spanwise_plus * viscous_length;
    let maximum_spanwise_spacing = maximum_graded_cell_size(
        configuration.extrusion.spanwise_thickness,
        configuration.mesh.spanwise_cells,
        configuration.mesh.spanwise_grading,
    );
    let estimated_maximum_spanwise_plus = maximum_spanwise_spacing / viscous_length;
    let recommended_spanwise_cells = recommended_cell_count(
        configuration.extrusion.spanwise_thickness,
        target_spanwise_spacing,
        configuration.mesh.spanwise_grading,
    );
    let reference_boundary_layer_thickness =
        chord * configuration.les.reference_boundary_layer_thickness_chords;
    let radial_extent = chord
        * configuration
            .domain
            .upper_radius_chords
            .min(configuration.domain.lower_radius_chords);
    let stretching = solve_layered_stretching(
        first_cell_height,
        radial_extent,
        configuration.mesh.radial_cells,
        reference_boundary_layer_thickness,
        configuration.les.minimum_cells_across_boundary_layer,
        configuration.les.max_radial_growth_ratio,
    )?;
    let radial_cells_within_boundary_layer = stretching
        .point_positions
        .windows(2)
        .take_while(|points| {
            f64::midpoint(points[0], points[1]) <= reference_boundary_layer_thickness
        })
        .count();

    Ok(LesSizingEstimate {
        mode: configuration.les.mode,
        property_reynolds_number,
        reynolds_mismatch_percent,
        viscous_length,
        first_cell_height,
        target_streamwise_spacing,
        maximum_surface_streamwise_spacing: surface.maximum_spacing,
        estimated_maximum_surface_streamwise_plus: surface.maximum_plus,
        recommended_leading_edge_cells: surface.recommended_leading_cells,
        recommended_upper_surface_cells: surface.recommended_upper_cells,
        recommended_lower_surface_cells: surface.recommended_lower_cells,
        target_spanwise_spacing,
        maximum_spanwise_spacing,
        estimated_maximum_spanwise_plus,
        recommended_spanwise_cells,
        spanwise_extent_chords: configuration.extrusion.spanwise_thickness / chord,
        reference_boundary_layer_thickness,
        radial_cells_within_boundary_layer,
        periodic_three_dimensional: configuration.extrusion.enabled
            && configuration.extrusion.front_back_patch_type == FrontBackPatchType::Cyclic
            && configuration.mesh.spanwise_cells > 1,
    })
}

fn estimate_surface_sizing(
    configuration: &MeshConfiguration,
    target_spacing: f64,
    viscous_length: f64,
) -> Result<SurfaceSizing, MeshError> {
    let airfoil = generate_airfoil(&configuration.airfoil)?;
    let split_index = leading_edge_split_index(airfoil.upper.len());
    let upper_leading_length = polyline_length(&airfoil.upper[..=split_index]);
    let lower_leading_length = polyline_length(&airfoil.lower[..=split_index]);
    let upper_surface_length = polyline_length(&airfoil.upper[split_index..]);
    let lower_surface_length = polyline_length(&airfoil.lower[split_index..]);
    let grading = configuration.mesh.surface_grading;
    let maximum_spacing = [
        maximum_graded_cell_size(
            upper_leading_length,
            configuration.mesh.leading_edge_cells,
            grading,
        ),
        maximum_graded_cell_size(
            lower_leading_length,
            configuration.mesh.leading_edge_cells,
            grading,
        ),
        maximum_graded_cell_size(
            upper_surface_length,
            configuration.mesh.upper_surface_cells,
            grading,
        ),
        maximum_graded_cell_size(
            lower_surface_length,
            configuration.mesh.lower_surface_cells,
            grading,
        ),
    ]
    .into_iter()
    .fold(0.0, f64::max);

    Ok(SurfaceSizing {
        maximum_spacing,
        maximum_plus: maximum_spacing / viscous_length,
        recommended_leading_cells: recommended_cell_count(
            upper_leading_length.max(lower_leading_length),
            target_spacing,
            grading,
        ),
        recommended_upper_cells: recommended_cell_count(
            upper_surface_length,
            target_spacing,
            grading,
        ),
        recommended_lower_cells: recommended_cell_count(
            lower_surface_length,
            target_spacing,
            grading,
        ),
    })
}

fn maximum_graded_cell_size(length: f64, cell_count: usize, total_grading: f64) -> f64 {
    if cell_count <= 1 {
        return length;
    }
    if (total_grading - 1.0).abs() <= f64::EPSILON {
        #[allow(clippy::cast_precision_loss)]
        return length / cell_count as f64;
    }
    #[allow(clippy::cast_precision_loss)]
    let log_ratio = total_grading.ln() / (cell_count - 1) as f64;
    #[allow(clippy::cast_precision_loss)]
    let weight_sum = ((cell_count as f64) * log_ratio).exp_m1() / log_ratio.exp_m1();
    length * total_grading.max(1.0) / weight_sum
}

fn recommended_cell_count(length: f64, target: f64, total_grading: f64) -> usize {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let mut upper = (length / target).ceil().max(2.0) as usize;
    while maximum_graded_cell_size(length, upper, total_grading) > target {
        let Some(next) = upper.checked_mul(2) else {
            return usize::MAX;
        };
        upper = next;
    }

    let mut lower = 2;
    while lower < upper {
        let middle = lower + (upper - lower) / 2;
        if maximum_graded_cell_size(length, middle, total_grading) <= target {
            upper = middle;
        } else {
            lower = middle + 1;
        }
    }
    lower
}

fn leading_edge_split_index(point_count: usize) -> usize {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss
    )]
    let index = ((point_count - 1) as f64 * LEADING_EDGE_SPLIT_FRACTION).round() as usize;
    index.clamp(1, point_count - 2)
}

fn polyline_length(points: &[Point2]) -> f64 {
    points
        .windows(2)
        .map(|pair| pair[0].distance_to(pair[1]))
        .sum()
}

#[cfg(test)]
mod tests {
    use super::estimate_les_sizing;
    use crate::test_support::example_configuration;

    #[test]
    fn sizing_exposes_flow_consistency_and_periodic_span_requirements() {
        let mut configuration = example_configuration();
        configuration.mesh.spanwise_cells = 8;
        configuration.extrusion.front_back_patch_type = crate::config::FrontBackPatchType::Cyclic;
        let sizing = estimate_les_sizing(&configuration).expect("LES sizing");
        assert!(sizing.property_reynolds_number > 0.0);
        assert!(sizing.viscous_length > 0.0);
        assert!(sizing.estimated_maximum_surface_streamwise_plus > 0.0);
        assert!(sizing.recommended_upper_surface_cells >= 2);
        assert!(sizing.recommended_spanwise_cells >= 2);
        assert!(sizing.radial_cells_within_boundary_layer > 0);
        assert!(sizing.periodic_three_dimensional);
    }
}
