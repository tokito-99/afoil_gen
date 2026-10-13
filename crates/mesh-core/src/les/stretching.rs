//! Robust wall-normal geometric-series solver.

use serde::{Deserialize, Serialize};

use crate::error::MeshError;

const RELATIVE_TOLERANCE: f64 = 1.0e-10;
const MAX_BISECTION_ITERATIONS: usize = 256;
const MINIMUM_OUTER_FIELD_CELLS: usize = 8;

/// A solved outward-growing cell-size progression.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StretchingSolution {
    pub ratio: f64,
    pub cell_sizes: Vec<f64>,
    pub point_positions: Vec<f64>,
    pub total_length: f64,
}

/// One OpenFOAM-compatible section of a piecewise radial distribution.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GradingSection {
    pub block_fraction: f64,
    pub division_fraction: f64,
    pub expansion_ratio: f64,
}

/// A wall-layer region followed by an independently graded outer region.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayeredStretchingSolution {
    pub cell_sizes: Vec<f64>,
    pub point_positions: Vec<f64>,
    pub total_length: f64,
    pub wall_layer_thickness: f64,
    pub wall_layer_cells: usize,
    pub maximum_ratio: f64,
    pub grading_sections: Vec<GradingSection>,
}

/// Solves independent wall-layer and outer-field geometric sections.
///
/// The outer section begins with the final wall-layer cell size, avoiding a
/// cell-size jump at the section interface. Both sections respect the same
/// maximum adjacent-cell growth ratio.
///
/// # Errors
///
/// Returns [`MeshError::InfeasibleStretching`] when either section cannot span
/// its requested distance with the supplied cell count and growth limit.
pub fn solve_layered_stretching(
    first_cell_height: f64,
    total_length: f64,
    total_cells: usize,
    wall_layer_thickness: f64,
    wall_layer_cells: usize,
    maximum_ratio: f64,
) -> Result<LayeredStretchingSolution, MeshError> {
    if !wall_layer_thickness.is_finite()
        || wall_layer_thickness <= 0.0
        || wall_layer_thickness >= total_length
    {
        return Err(MeshError::InfeasibleStretching(
            "wall-layer thickness must be positive and smaller than the radial extent".to_owned(),
        ));
    }
    if wall_layer_cells == 0
        || wall_layer_cells
            .checked_add(MINIMUM_OUTER_FIELD_CELLS)
            .is_none_or(|minimum| minimum > total_cells)
    {
        return Err(MeshError::InfeasibleStretching(format!(
            "wall-layer cells must leave at least {MINIMUM_OUTER_FIELD_CELLS} outer-field cells"
        )));
    }

    let mut selected = None;
    for candidate_wall_cells in wall_layer_cells..=total_cells - MINIMUM_OUTER_FIELD_CELLS {
        let Ok(wall) = solve_geometric_stretching(
            first_cell_height,
            wall_layer_thickness,
            candidate_wall_cells,
            maximum_ratio,
        ) else {
            continue;
        };
        let outer_cells = total_cells - candidate_wall_cells;
        let Some(outer_first) = wall.cell_sizes.last().copied() else {
            continue;
        };
        let Ok(outer) = solve_geometric_stretching(
            outer_first,
            total_length - wall_layer_thickness,
            outer_cells,
            maximum_ratio,
        ) else {
            continue;
        };
        selected = Some((candidate_wall_cells, wall, outer));
        break;
    }
    let Some((wall_layer_cells, wall, outer)) = selected else {
        return Err(MeshError::InfeasibleStretching(format!(
            "no two-section distribution fits {total_cells} radial cells, minimum \
             {wall_layer_cells} wall-layer cells, and maximum ratio {maximum_ratio:.8}; \
             increase radialCells or maxRadialGrowthRatio"
        )));
    };

    let mut cell_sizes = wall.cell_sizes.clone();
    cell_sizes.extend_from_slice(&outer.cell_sizes);
    let mut point_positions = wall.point_positions.clone();
    point_positions.extend(
        outer
            .point_positions
            .iter()
            .skip(1)
            .map(|position| wall_layer_thickness + position),
    );
    if let Some(last) = point_positions.last_mut() {
        *last = total_length;
    }

    #[allow(clippy::cast_precision_loss)]
    let total_cells_f64 = total_cells as f64;
    #[allow(clippy::cast_precision_loss)]
    let wall_cells_fraction = wall_layer_cells as f64 / total_cells_f64;
    let grading_sections = vec![
        GradingSection {
            block_fraction: wall_layer_thickness / total_length,
            division_fraction: wall_cells_fraction,
            expansion_ratio: section_expansion_ratio(&wall.cell_sizes),
        },
        GradingSection {
            block_fraction: (total_length - wall_layer_thickness) / total_length,
            division_fraction: 1.0 - wall_cells_fraction,
            expansion_ratio: section_expansion_ratio(&outer.cell_sizes),
        },
    ];

    Ok(LayeredStretchingSolution {
        cell_sizes,
        point_positions,
        total_length,
        wall_layer_thickness,
        wall_layer_cells,
        maximum_ratio: wall.ratio.max(outer.ratio),
        grading_sections,
    })
}

/// Solves an outward-growing geometric progression using deterministic bisection.
///
/// Ratios below one are intentionally unsupported. The returned point positions
/// start at zero and end at the requested total length.
///
/// # Errors
///
/// Returns [`MeshError::InfeasibleStretching`] for invalid inputs or when no
/// ratio in `[1, maximum_ratio]` reaches the requested total length.
pub fn solve_geometric_stretching(
    first_cell_height: f64,
    total_length: f64,
    cell_count: usize,
    maximum_ratio: f64,
) -> Result<StretchingSolution, MeshError> {
    validate_inputs(first_cell_height, total_length, cell_count, maximum_ratio)?;

    #[allow(clippy::cast_precision_loss)]
    let uniform_length = first_cell_height * cell_count as f64;
    let tolerance = RELATIVE_TOLERANCE * total_length.abs().max(first_cell_height);

    if total_length + tolerance < uniform_length {
        return Err(MeshError::InfeasibleStretching(format!(
            "requested distance {total_length:.12e} is smaller than the uniform \
             distance {uniform_length:.12e}; ratios below 1 are disabled"
        )));
    }

    if (total_length - uniform_length).abs() <= tolerance {
        return Ok(build_solution(
            first_cell_height,
            total_length,
            cell_count,
            1.0,
        ));
    }

    let maximum_length = geometric_sum(first_cell_height, cell_count, maximum_ratio);
    if maximum_length + tolerance < total_length {
        return Err(MeshError::InfeasibleStretching(format!(
            "{cell_count} cells with maximum ratio {maximum_ratio:.8} reach only \
             {maximum_length:.12e}, below the required {total_length:.12e}; \
             increase radialCells or maxRadialGrowthRatio"
        )));
    }

    let mut lower = 1.0;
    let mut upper = maximum_ratio;
    let mut ratio = f64::midpoint(lower, upper);
    for _ in 0..MAX_BISECTION_ITERATIONS {
        ratio = f64::midpoint(lower, upper);
        let length = geometric_sum(first_cell_height, cell_count, ratio);
        if (length - total_length).abs() <= tolerance {
            break;
        }
        if length < total_length {
            lower = ratio;
        } else {
            upper = ratio;
        }
    }

    let solved_length = geometric_sum(first_cell_height, cell_count, ratio);
    if (solved_length - total_length).abs() > tolerance {
        return Err(MeshError::InfeasibleStretching(format!(
            "bisection did not converge: solved {solved_length:.12e}, \
             required {total_length:.12e}"
        )));
    }

    Ok(build_solution(
        first_cell_height,
        total_length,
        cell_count,
        ratio,
    ))
}

fn validate_inputs(
    first_cell_height: f64,
    total_length: f64,
    cell_count: usize,
    maximum_ratio: f64,
) -> Result<(), MeshError> {
    if !first_cell_height.is_finite() || first_cell_height <= 0.0 {
        return Err(MeshError::InfeasibleStretching(
            "first-cell height must be finite and positive".to_owned(),
        ));
    }
    if !total_length.is_finite() || total_length <= 0.0 {
        return Err(MeshError::InfeasibleStretching(
            "total length must be finite and positive".to_owned(),
        ));
    }
    if cell_count == 0 {
        return Err(MeshError::InfeasibleStretching(
            "cell count must be greater than zero".to_owned(),
        ));
    }
    if !maximum_ratio.is_finite() || maximum_ratio < 1.0 {
        return Err(MeshError::InfeasibleStretching(
            "maximum growth ratio must be finite and at least 1".to_owned(),
        ));
    }
    Ok(())
}

fn geometric_sum(first_cell_height: f64, cell_count: usize, ratio: f64) -> f64 {
    let mut sum = 0.0;
    let mut size = first_cell_height;
    for _ in 0..cell_count {
        sum += size;
        size *= ratio;
    }
    sum
}

fn build_solution(
    first_cell_height: f64,
    total_length: f64,
    cell_count: usize,
    ratio: f64,
) -> StretchingSolution {
    let mut cell_sizes = Vec::with_capacity(cell_count);
    let mut point_positions = Vec::with_capacity(cell_count + 1);
    point_positions.push(0.0);

    let mut size = first_cell_height;
    let mut position = 0.0;
    for _ in 0..cell_count {
        cell_sizes.push(size);
        position += size;
        point_positions.push(position);
        size *= ratio;
    }

    // The bisection residual is below tolerance; pin the boundary exactly.
    if let Some(last) = point_positions.last_mut() {
        *last = total_length;
    }

    StretchingSolution {
        ratio,
        cell_sizes,
        point_positions,
        total_length,
    }
}

fn section_expansion_ratio(cell_sizes: &[f64]) -> f64 {
    match (cell_sizes.first(), cell_sizes.last()) {
        (Some(first), Some(last)) => last / first,
        _ => 1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::{solve_geometric_stretching, solve_layered_stretching};

    #[test]
    fn solves_uniform_spacing_exactly() {
        let solution = solve_geometric_stretching(0.1, 1.0, 10, 1.2).expect("uniform solution");
        assert!((solution.ratio - 1.0).abs() < f64::EPSILON);
        assert!(
            solution
                .cell_sizes
                .iter()
                .all(|size| (*size - 0.1).abs() < 1.0e-15)
        );
        assert_eq!(solution.point_positions.first().copied(), Some(0.0));
        assert_eq!(solution.point_positions.last().copied(), Some(1.0));
    }

    #[test]
    fn solves_growing_progression_and_matches_total() {
        let solution = solve_geometric_stretching(0.01, 1.0, 20, 1.2).expect("feasible solution");
        assert!(solution.ratio > 1.0);
        assert!(solution.ratio <= 1.2);
        assert!(solution.cell_sizes.windows(2).all(|pair| pair[1] > pair[0]));
        let sum: f64 = solution.cell_sizes.iter().sum();
        assert!((sum - 1.0).abs() < 1.0e-10);
        assert!((solution.point_positions[20] - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn rejects_infeasible_maximum_ratio() {
        let result = solve_geometric_stretching(0.001, 10.0, 10, 1.05);
        assert!(result.is_err());
    }

    #[test]
    fn rejects_required_inward_shrinking() {
        let result = solve_geometric_stretching(0.2, 1.0, 10, 1.2);
        assert!(result.is_err());
    }

    #[test]
    fn rejects_invalid_inputs() {
        assert!(solve_geometric_stretching(0.0, 1.0, 10, 1.2).is_err());
        assert!(solve_geometric_stretching(0.1, 0.0, 10, 1.2).is_err());
        assert!(solve_geometric_stretching(0.1, 1.0, 0, 1.2).is_err());
        assert!(solve_geometric_stretching(0.1, 1.0, 10, 0.9).is_err());
    }

    #[test]
    fn layered_solution_pins_wall_region_and_farfield_extent() {
        let solution = solve_layered_stretching(1.0e-4, 10.0, 120, 0.03, 30, 1.2)
            .expect("feasible layered solution");
        assert_eq!(solution.point_positions.len(), 121);
        assert!((solution.point_positions[30] - 0.03).abs() < 1.0e-12);
        assert_eq!(solution.point_positions.last().copied(), Some(10.0));
        assert_eq!(solution.grading_sections.len(), 2);
        assert!(solution.maximum_ratio <= 1.2);
        assert!(
            solution
                .cell_sizes
                .windows(2)
                .all(|cells| cells[1] / cells[0] <= 1.2 + 1.0e-10)
        );
    }
}
