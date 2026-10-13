//! Deterministic six/seven-block realization of a conformal airfoil C-grid.

use std::f64::consts::PI;

use serde::Serialize;

use crate::{
    config::{MeshConfiguration, WakeAlignment},
    error::{MeshError, ValidationSeverity},
    geometry::{Point2, naca4::generate_airfoil},
    les::{
        stretching::{GradingSection, solve_layered_stretching},
        wall_spacing::{ResolvedWallSpacing, resolve_wall_spacing},
    },
    topology::{
        AirfoilSurfaceCurve, BlockBoundaryAssignments, BlockInterface, BlockSide, BoundaryRole,
        CurveSpec, LineCurve, LogicalBlock2D, ParametricCurve, SampledCurve, WakeCenterlineCurve,
    },
    validation::validate_configuration,
};

const LEADING_EDGE_SPLIT_FRACTION: f64 = 0.3;
const MINIMUM_FINITE_WAKE_HALF_THICKNESS_CHORDS: f64 = 1.0e-8;
const FARFIELD_CURVE_SAMPLE_COUNT: usize = 33;
const WAKE_CURVE_SAMPLE_COUNT: usize = 129;

type RadialPositions = (
    Vec<f64>,
    f64,
    Option<ResolvedWallSpacing>,
    Vec<GradingSection>,
    usize,
    f64,
);

/// Complete logical topology prior to coordinate merging and cell construction.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CGridTopology {
    pub blocks: Vec<LogicalBlock2D>,
    pub interfaces: Vec<BlockInterface>,
    pub airfoil_upper: Vec<Point2>,
    pub airfoil_lower: Vec<Point2>,
    pub outer_boundary: Vec<Point2>,
    pub wake_upper: Vec<Point2>,
    pub wake_lower: Vec<Point2>,
    pub wake_half_thickness: f64,
    pub maximum_radial_growth_ratio: f64,
    pub radial_grading_sections: Vec<GradingSection>,
    pub wall_layer_cells: usize,
    pub wall_layer_thickness: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wall_spacing: Option<ResolvedWallSpacing>,
}

/// Builds a conformal C-grid topology.
///
/// Sharp trailing edges use six blocks and a zero-thickness internal wake cut.
/// Finite trailing edges use a seventh center-wake block spanning the physical
/// base thickness. This avoids subdividing an artificial trailing-edge gap
/// into every radial row.
///
/// # Errors
///
/// Returns [`MeshError::InvalidConfiguration`] for fatal configuration issues
/// and propagates geometry or stretching failures.
#[allow(clippy::too_many_lines)]
pub fn build_cgrid_topology(configuration: &MeshConfiguration) -> Result<CGridTopology, MeshError> {
    ensure_valid(configuration)?;
    let geometry = generate_airfoil(&configuration.airfoil)?;
    let chord = configuration.airfoil.chord_length;
    let split_index = leading_edge_split_index(geometry.upper.len());

    let upper_leading_points = geometry.upper[..=split_index].to_vec();
    let mut upper_aft_points = geometry.upper[split_index..].to_vec();
    let mut lower_aft_points = geometry.lower[split_index..]
        .iter()
        .rev()
        .copied()
        .collect::<Vec<_>>();
    let lower_leading_points = geometry.lower[..=split_index]
        .iter()
        .rev()
        .copied()
        .collect::<Vec<_>>();

    let actual_upper_te = *geometry.upper.last().ok_or_else(|| {
        MeshError::TopologyError("upper airfoil surface contains no points".to_owned())
    })?;
    let actual_lower_te = *geometry.lower.last().ok_or_else(|| {
        MeshError::TopologyError("lower airfoil surface contains no points".to_owned())
    })?;
    let trailing_edge_center = Point2::new(
        f64::midpoint(actual_upper_te.x, actual_lower_te.x),
        f64::midpoint(actual_upper_te.y, actual_lower_te.y),
    );
    let actual_half_thickness = 0.5 * actual_upper_te.distance_to(actual_lower_te);
    let finite_trailing_edge = configuration.airfoil.finite_trailing_edge;
    let wake_half_thickness = if finite_trailing_edge {
        actual_half_thickness.max(MINIMUM_FINITE_WAKE_HALF_THICKNESS_CHORDS * chord)
    } else {
        0.0
    };
    let angle = -configuration.airfoil.angle_of_attack_deg.to_radians();
    let thickness_normal = Point2::new(-angle.sin(), angle.cos());
    let regularized_upper_te = Point2::new(
        trailing_edge_center.x + wake_half_thickness * thickness_normal.x,
        trailing_edge_center.y + wake_half_thickness * thickness_normal.y,
    );
    let regularized_lower_te = Point2::new(
        trailing_edge_center.x - wake_half_thickness * thickness_normal.x,
        trailing_edge_center.y - wake_half_thickness * thickness_normal.y,
    );

    if finite_trailing_edge {
        replace_last(&mut upper_aft_points, regularized_upper_te)?;
        replace_first(&mut lower_aft_points, regularized_lower_te)?;
    }

    let upstream_x = -configuration.domain.upstream_radius_chords * chord;
    let join_x = chord;
    let outlet_x = chord * (1.0 + configuration.domain.wake_length_chords);
    let upper_radius = configuration.domain.upper_radius_chords * chord;
    let lower_radius = configuration.domain.lower_radius_chords * chord;
    let wake_half_height = configuration.domain.wake_half_height_chords * chord;

    let upper_join = Point2::new(join_x, upper_radius);
    let lower_join = Point2::new(join_x, -lower_radius);
    let upper_outlet_outer = Point2::new(outlet_x, upper_radius);
    let lower_outlet_outer = Point2::new(outlet_x, -lower_radius);
    let wake_angle = match configuration.wake.alignment {
        WakeAlignment::Blended | WakeAlignment::Chord => angle,
        WakeAlignment::Freestream => 0.0,
        WakeAlignment::Custom => configuration.wake.custom_angle_deg.to_radians(),
    };
    let streamwise_wake_length = outlet_x - trailing_edge_center.x;
    let blend_length = (configuration.wake.blend_length_chords * chord).min(streamwise_wake_length);
    let wake_outlet_center_y = match configuration.wake.alignment {
        WakeAlignment::Blended => trailing_edge_center.y + wake_angle.tan() * blend_length / 3.0,
        WakeAlignment::Chord | WakeAlignment::Custom => {
            trailing_edge_center.y + wake_angle.tan() * streamwise_wake_length
        }
        WakeAlignment::Freestream => trailing_edge_center.y,
    };
    let upper_outlet_wake = Point2::new(
        outlet_x,
        wake_outlet_center_y
            + if finite_trailing_edge {
                wake_half_height
            } else {
                0.0
            },
    );
    let lower_outlet_wake = Point2::new(
        outlet_x,
        wake_outlet_center_y
            - if finite_trailing_edge {
                wake_half_height
            } else {
                0.0
            },
    );
    if upper_outlet_wake.y >= upper_outlet_outer.y || lower_outlet_wake.y <= lower_outlet_outer.y {
        return Err(MeshError::TopologyError(
            "wake alignment exits the upper or lower farfield before the outlet; reduce the wake angle or blend length, or enlarge the domain"
                .to_owned(),
        ));
    }

    let lower_aft_outer_points =
        elliptical_arc_samples(join_x, upstream_x, lower_radius, -PI / 2.0, -3.0 * PI / 4.0);
    let lower_leading_outer_points =
        elliptical_arc_samples(join_x, upstream_x, lower_radius, -3.0 * PI / 4.0, -PI);
    let upper_leading_outer_points =
        elliptical_arc_samples(join_x, upstream_x, upper_radius, PI, 3.0 * PI / 4.0);
    let upper_aft_outer_points =
        elliptical_arc_samples(join_x, upstream_x, upper_radius, 3.0 * PI / 4.0, PI / 2.0);

    let upper_wake_start = if finite_trailing_edge {
        regularized_upper_te
    } else {
        trailing_edge_center
    };
    let lower_wake_start = if finite_trailing_edge {
        regularized_lower_te
    } else {
        trailing_edge_center
    };
    let (upper_wake, lower_wake) = if configuration.wake.alignment == WakeAlignment::Blended {
        blended_wake_curves(
            trailing_edge_center,
            upper_wake_start,
            lower_wake_start,
            upper_outlet_wake,
            lower_outlet_wake,
            outlet_x,
            wake_angle,
            blend_length,
        )
    } else {
        (
            CurveSpec::WakeCenterline(WakeCenterlineCurve {
                start: upper_wake_start,
                end: upper_outlet_wake,
            }),
            CurveSpec::WakeCenterline(WakeCenterlineCurve {
                start: lower_wake_start,
                end: lower_outlet_wake,
            }),
        )
    };

    let (
        eta_positions,
        maximum_radial_growth_ratio,
        wall_spacing,
        radial_grading_sections,
        wall_layer_cells,
        wall_layer_thickness,
    ) = radial_positions(configuration)?;
    let leading_positions = graded_positions(
        configuration.mesh.leading_edge_cells,
        configuration.mesh.surface_grading,
        false,
    );
    let upper_surface_positions = graded_positions(
        configuration.mesh.upper_surface_cells,
        configuration.mesh.surface_grading,
        false,
    );
    let lower_surface_positions = graded_positions(
        configuration.mesh.lower_surface_cells,
        configuration.mesh.surface_grading,
        false,
    );
    let upper_wake_positions = graded_positions(
        configuration.mesh.wake_cells,
        configuration.mesh.wake_grading,
        false,
    );
    let lower_wake_positions = graded_positions(
        configuration.mesh.wake_cells,
        configuration.mesh.wake_grading,
        true,
    );
    let upper_leading_bottom = CurveSpec::AirfoilSurface(AirfoilSurfaceCurve {
        points: upper_leading_points,
    });
    let upper_leading_top = CurveSpec::Sampled(SampledCurve {
        points: upper_leading_outer_points,
    });
    let upper_aft_bottom = CurveSpec::AirfoilSurface(AirfoilSurfaceCurve {
        points: upper_aft_points,
    });
    let upper_aft_top = CurveSpec::Sampled(SampledCurve {
        points: upper_aft_outer_points,
    });
    let lower_aft_bottom = CurveSpec::AirfoilSurface(AirfoilSurfaceCurve {
        points: lower_aft_points,
    });
    let lower_aft_top = CurveSpec::Sampled(SampledCurve {
        points: lower_aft_outer_points,
    });
    let lower_leading_bottom = CurveSpec::AirfoilSurface(AirfoilSurfaceCurve {
        points: lower_leading_points,
    });
    let lower_leading_top = CurveSpec::Sampled(SampledCurve {
        points: lower_leading_outer_points,
    });

    let upper_wake_top = CurveSpec::Line(LineCurve::new(upper_join, upper_outlet_outer));
    let lower_wake_top = CurveSpec::Line(LineCurve::new(lower_outlet_outer, lower_join));

    let mut blocks = vec![
        make_block(
            0,
            "upper_leading_farfield",
            leading_positions.clone(),
            eta_positions.clone(),
            upper_leading_bottom,
            upper_leading_top,
            BlockBoundaryAssignments {
                bottom: Some(BoundaryRole::Airfoil),
                top: Some(BoundaryRole::Inlet),
                ..BlockBoundaryAssignments::default()
            },
        ),
        make_block(
            1,
            "upper_surface",
            upper_surface_positions,
            eta_positions.clone(),
            upper_aft_bottom,
            upper_aft_top,
            BlockBoundaryAssignments {
                bottom: Some(BoundaryRole::Airfoil),
                top: Some(BoundaryRole::Inlet),
                ..BlockBoundaryAssignments::default()
            },
        ),
        make_block(
            2,
            "lower_surface",
            lower_surface_positions,
            eta_positions.clone(),
            lower_aft_bottom,
            lower_aft_top,
            BlockBoundaryAssignments {
                bottom: Some(BoundaryRole::Airfoil),
                top: Some(BoundaryRole::Inlet),
                ..BlockBoundaryAssignments::default()
            },
        ),
        make_block(
            3,
            "lower_leading_farfield",
            leading_positions,
            eta_positions.clone(),
            lower_leading_bottom,
            lower_leading_top,
            BlockBoundaryAssignments {
                bottom: Some(BoundaryRole::Airfoil),
                top: Some(BoundaryRole::Inlet),
                ..BlockBoundaryAssignments::default()
            },
        ),
    ];

    let mut interfaces = vec![
        interface(0, BlockSide::Right, 1, BlockSide::Left, false),
        interface(3, BlockSide::Right, 0, BlockSide::Left, false),
        interface(2, BlockSide::Right, 3, BlockSide::Left, false),
    ];

    let (lower_wake_block, upper_wake_block) = if finite_trailing_edge {
        blocks.push(make_block(
            4,
            "downstream_wake",
            upper_wake_positions.clone(),
            uniform_positions(configuration.mesh.radial_cells),
            lower_wake.clone(),
            upper_wake.clone(),
            BlockBoundaryAssignments {
                left: Some(BoundaryRole::Airfoil),
                right: Some(BoundaryRole::Outlet),
                ..BlockBoundaryAssignments::default()
            },
        ));
        blocks.push(make_block(
            5,
            "upper_wake_band",
            upper_wake_positions,
            eta_positions.clone(),
            upper_wake.clone(),
            upper_wake_top,
            BlockBoundaryAssignments {
                top: Some(BoundaryRole::UpperFarfield),
                right: Some(BoundaryRole::Outlet),
                ..BlockBoundaryAssignments::default()
            },
        ));
        blocks.push(make_block(
            6,
            "lower_wake_band",
            lower_wake_positions,
            eta_positions,
            lower_wake.reversed(),
            lower_wake_top,
            BlockBoundaryAssignments {
                top: Some(BoundaryRole::LowerFarfield),
                left: Some(BoundaryRole::Outlet),
                ..BlockBoundaryAssignments::default()
            },
        ));
        interfaces.extend([
            interface(6, BlockSide::Right, 2, BlockSide::Left, false),
            interface(1, BlockSide::Right, 5, BlockSide::Left, false),
            interface(4, BlockSide::Top, 5, BlockSide::Bottom, false),
            interface(4, BlockSide::Bottom, 6, BlockSide::Bottom, true),
        ]);
        (6, 5)
    } else {
        blocks.push(make_block(
            4,
            "upper_wake_band",
            upper_wake_positions,
            eta_positions.clone(),
            upper_wake.clone(),
            upper_wake_top,
            BlockBoundaryAssignments {
                top: Some(BoundaryRole::UpperFarfield),
                right: Some(BoundaryRole::Outlet),
                ..BlockBoundaryAssignments::default()
            },
        ));
        blocks.push(make_block(
            5,
            "lower_wake_band",
            lower_wake_positions,
            eta_positions,
            lower_wake.reversed(),
            lower_wake_top,
            BlockBoundaryAssignments {
                top: Some(BoundaryRole::LowerFarfield),
                left: Some(BoundaryRole::Outlet),
                ..BlockBoundaryAssignments::default()
            },
        ));
        interfaces.extend([
            interface(5, BlockSide::Right, 2, BlockSide::Left, false),
            interface(1, BlockSide::Right, 4, BlockSide::Left, false),
            interface(4, BlockSide::Bottom, 5, BlockSide::Bottom, true),
        ]);
        (5, 4)
    };

    crate::les::profile::redistribute_wall_positions(configuration, &mut blocks);
    let outer_boundary = concatenate_curves(&[
        &blocks[lower_wake_block].top,
        &blocks[2].top,
        &blocks[3].top,
        &blocks[0].top,
        &blocks[1].top,
        &blocks[upper_wake_block].top,
    ]);

    Ok(CGridTopology {
        blocks,
        interfaces,
        airfoil_upper: geometry.upper,
        airfoil_lower: geometry.lower,
        outer_boundary,
        wake_upper: upper_wake.sampled_points(configuration.mesh.wake_cells),
        wake_lower: lower_wake.sampled_points(configuration.mesh.wake_cells),
        wake_half_thickness,
        maximum_radial_growth_ratio,
        radial_grading_sections,
        wall_layer_cells,
        wall_layer_thickness,
        wall_spacing,
    })
}

fn ensure_valid(configuration: &MeshConfiguration) -> Result<(), MeshError> {
    let report = validate_configuration(configuration);
    if report.valid {
        return Ok(());
    }
    Err(MeshError::InvalidConfiguration(
        report
            .issues
            .into_iter()
            .filter(|issue| issue.severity == ValidationSeverity::Error)
            .collect(),
    ))
}

#[allow(clippy::too_many_arguments)]
fn blended_wake_curves(
    center_start: Point2,
    upper_start: Point2,
    lower_start: Point2,
    upper_end: Point2,
    lower_end: Point2,
    outlet_x: f64,
    initial_angle: f64,
    blend_length: f64,
) -> (CurveSpec, CurveSpec) {
    let center_end = Point2::new(outlet_x, f64::midpoint(upper_end.y, lower_end.y));
    let upper_start_offset = Point2::new(
        upper_start.x - center_start.x,
        upper_start.y - center_start.y,
    );
    let lower_start_offset = Point2::new(
        lower_start.x - center_start.x,
        lower_start.y - center_start.y,
    );
    let upper_end_offset = Point2::new(0.0, upper_end.y - center_end.y);
    let lower_end_offset = Point2::new(0.0, lower_end.y - center_end.y);
    let streamwise_length = outlet_x - center_start.x;
    let slope = initial_angle.tan();

    let mut upper_points = Vec::with_capacity(WAKE_CURVE_SAMPLE_COUNT);
    let mut lower_points = Vec::with_capacity(WAKE_CURVE_SAMPLE_COUNT);
    for index in 0..WAKE_CURVE_SAMPLE_COUNT {
        #[allow(clippy::cast_precision_loss)]
        let parameter = index as f64 / (WAKE_CURVE_SAMPLE_COUNT - 1) as f64;
        let streamwise_position = parameter * streamwise_length;
        let center_y = if blend_length <= f64::EPSILON {
            center_start.y
        } else if streamwise_position < blend_length {
            let normalized = streamwise_position / blend_length;
            center_start.y
                + slope
                    * blend_length
                    * (normalized - normalized.powi(2) + normalized.powi(3) / 3.0)
        } else {
            center_start.y + slope * blend_length / 3.0
        };
        let center = Point2::new(center_start.x + streamwise_position, center_y);
        let upper_offset = interpolate_point(upper_start_offset, upper_end_offset, parameter);
        let lower_offset = interpolate_point(lower_start_offset, lower_end_offset, parameter);
        upper_points.push(Point2::new(
            center.x + upper_offset.x,
            center.y + upper_offset.y,
        ));
        lower_points.push(Point2::new(
            center.x + lower_offset.x,
            center.y + lower_offset.y,
        ));
    }
    upper_points[0] = upper_start;
    lower_points[0] = lower_start;
    upper_points[WAKE_CURVE_SAMPLE_COUNT - 1] = upper_end;
    lower_points[WAKE_CURVE_SAMPLE_COUNT - 1] = lower_end;

    (
        CurveSpec::Sampled(SampledCurve {
            points: upper_points,
        }),
        CurveSpec::Sampled(SampledCurve {
            points: lower_points,
        }),
    )
}

fn interpolate_point(start: Point2, end: Point2, parameter: f64) -> Point2 {
    Point2::new(
        start.x + parameter * (end.x - start.x),
        start.y + parameter * (end.y - start.y),
    )
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

fn replace_last(points: &mut [Point2], replacement: Point2) -> Result<(), MeshError> {
    let last = points
        .last_mut()
        .ok_or_else(|| MeshError::TopologyError("cannot regularize an empty surface".to_owned()))?;
    *last = replacement;
    Ok(())
}

fn replace_first(points: &mut [Point2], replacement: Point2) -> Result<(), MeshError> {
    let first = points
        .first_mut()
        .ok_or_else(|| MeshError::TopologyError("cannot regularize an empty surface".to_owned()))?;
    *first = replacement;
    Ok(())
}

fn elliptical_arc_samples(
    join_x: f64,
    upstream_x: f64,
    vertical_radius: f64,
    start_angle: f64,
    end_angle: f64,
) -> Vec<Point2> {
    let horizontal_radius = join_x - upstream_x;
    (0..FARFIELD_CURVE_SAMPLE_COUNT)
        .map(|index| {
            #[allow(clippy::cast_precision_loss)]
            let parameter = index as f64 / (FARFIELD_CURVE_SAMPLE_COUNT - 1) as f64;
            let angle = (1.0 - parameter) * start_angle + parameter * end_angle;
            Point2::new(
                join_x + horizontal_radius * angle.cos(),
                vertical_radius * angle.sin(),
            )
        })
        .collect()
}

fn radial_positions(configuration: &MeshConfiguration) -> Result<RadialPositions, MeshError> {
    let wall_spacing = resolve_wall_spacing(configuration)?;
    let radial_extent = configuration.airfoil.chord_length
        * configuration
            .domain
            .upper_radius_chords
            .min(configuration.domain.lower_radius_chords);
    let wall_layer_thickness = configuration.airfoil.chord_length
        * configuration.les.reference_boundary_layer_thickness_chords;
    let solution = solve_layered_stretching(
        wall_spacing.first_cell_height,
        radial_extent,
        configuration.mesh.radial_cells,
        wall_layer_thickness,
        configuration.les.minimum_cells_across_boundary_layer,
        configuration.les.max_radial_growth_ratio,
    )?;
    let normalized = solution
        .point_positions
        .iter()
        .map(|position| position / radial_extent)
        .collect();
    Ok((
        normalized,
        solution.maximum_ratio,
        Some(wall_spacing),
        solution.grading_sections,
        solution.wall_layer_cells,
        solution.wall_layer_thickness,
    ))
}

fn graded_positions(cell_count: usize, total_grading: f64, reverse: bool) -> Vec<f64> {
    if cell_count == 1 || (total_grading - 1.0).abs() <= f64::EPSILON {
        return uniform_positions(cell_count);
    }
    #[allow(clippy::cast_precision_loss)]
    let cell_ratio = total_grading.powf(1.0 / (cell_count - 1) as f64);
    let mut sizes = Vec::with_capacity(cell_count);
    let mut size = 1.0;
    for _ in 0..cell_count {
        sizes.push(size);
        size *= cell_ratio;
    }
    if reverse {
        sizes.reverse();
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

fn uniform_positions(cell_count: usize) -> Vec<f64> {
    (0..=cell_count)
        .map(|index| {
            #[allow(clippy::cast_precision_loss)]
            let value = index as f64 / cell_count as f64;
            value
        })
        .collect()
}

fn make_block(
    id: usize,
    name: &str,
    xi_positions: Vec<f64>,
    eta_positions: Vec<f64>,
    bottom: CurveSpec,
    top: CurveSpec,
    boundaries: BlockBoundaryAssignments,
) -> LogicalBlock2D {
    let left = CurveSpec::Line(LineCurve::new(bottom.point(0.0), top.point(0.0)));
    let right = CurveSpec::Line(LineCurve::new(bottom.point(1.0), top.point(1.0)));
    LogicalBlock2D {
        id,
        name: name.to_owned(),
        ni: xi_positions.len() - 1,
        nj: eta_positions.len() - 1,
        xi_positions,
        eta_positions,
        bottom,
        top,
        left,
        right,
        boundaries,
    }
}

const fn interface(
    block_a: usize,
    side_a: BlockSide,
    block_b: usize,
    side_b: BlockSide,
    reversed: bool,
) -> BlockInterface {
    BlockInterface {
        block_a,
        side_a,
        block_b,
        side_b,
        reversed,
    }
}

fn concatenate_curves(curves: &[&CurveSpec]) -> Vec<Point2> {
    let mut points = Vec::new();
    for curve in curves {
        let samples = curve.sampled_points(FARFIELD_CURVE_SAMPLE_COUNT - 1);
        if points.is_empty() {
            points.extend(samples);
        } else {
            points.extend(samples.into_iter().skip(1));
        }
    }
    points
}

#[cfg(test)]
mod tests {
    use super::build_cgrid_topology;
    use crate::{
        config::WakeAlignment,
        test_support::example_configuration,
        topology::{BlockSide, BoundaryRole, ParametricCurve},
    };

    #[test]
    fn sharp_trailing_edge_uses_a_six_block_wake_cut() {
        let topology = build_cgrid_topology(&example_configuration()).expect("valid topology");
        assert_eq!(topology.blocks.len(), 6);
        assert_eq!(topology.interfaces.len(), 6);
        assert_eq!(topology.blocks[0].name, "upper_leading_farfield");
        assert_eq!(topology.blocks[4].name, "upper_wake_band");
        assert!(topology.wake_half_thickness.abs() <= f64::EPSILON);
        for (upper, lower) in topology.wake_upper.iter().zip(&topology.wake_lower) {
            assert!(upper.distance_to(*lower) < 1.0e-12);
        }
    }

    #[test]
    fn finite_trailing_edge_retains_the_physical_center_wake_block() {
        let mut configuration = example_configuration();
        configuration.airfoil.finite_trailing_edge = true;
        let topology = build_cgrid_topology(&configuration).expect("valid topology");
        assert_eq!(topology.blocks.len(), 7);
        assert_eq!(topology.interfaces.len(), 7);
        assert_eq!(topology.blocks[4].name, "downstream_wake");
        assert!(topology.wake_half_thickness > 1.0e-5);
    }

    #[test]
    fn complete_curved_c_boundary_is_inlet_and_straight_legs_are_farfield() {
        for finite_trailing_edge in [false, true] {
            let mut configuration = example_configuration();
            configuration.airfoil.finite_trailing_edge = finite_trailing_edge;
            let topology = build_cgrid_topology(&configuration).expect("valid topology");

            for block in &topology.blocks[..4] {
                assert_eq!(
                    block.boundaries.top,
                    Some(BoundaryRole::Inlet),
                    "{} outer arc must be inlet",
                    block.name
                );
            }

            let upper_wake_block = if finite_trailing_edge { 5 } else { 4 };
            let lower_wake_block = if finite_trailing_edge { 6 } else { 5 };
            assert_eq!(
                topology.blocks[upper_wake_block].boundaries.top,
                Some(BoundaryRole::UpperFarfield)
            );
            assert_eq!(
                topology.blocks[lower_wake_block].boundaries.top,
                Some(BoundaryRole::LowerFarfield)
            );
        }
    }

    #[test]
    fn blended_wake_leaves_at_chord_angle_and_relaxes_to_freestream() {
        let mut configuration = example_configuration();
        configuration.airfoil.angle_of_attack_deg = 10.0;
        configuration.wake.alignment = WakeAlignment::Blended;
        configuration.wake.blend_length_chords = 2.0;
        let topology = build_cgrid_topology(&configuration).expect("valid blended wake");
        let first_slope = segment_slope(&topology.wake_upper, 0);
        let last_slope = segment_slope(&topology.wake_upper, topology.wake_upper.len() - 2);
        assert!((first_slope + 10.0_f64.to_radians().tan()).abs() < 0.02);
        assert!(last_slope.abs() < 1.0e-8);
    }

    #[test]
    fn freestream_wake_is_horizontal_after_rotated_trailing_edge() {
        let mut configuration = example_configuration();
        configuration.airfoil.angle_of_attack_deg = 10.0;
        configuration.wake.alignment = WakeAlignment::Freestream;
        let topology = build_cgrid_topology(&configuration).expect("valid freestream wake");
        let start_y = topology.wake_upper[0].y;
        assert!(
            topology
                .wake_upper
                .iter()
                .all(|point| (point.y - start_y).abs() < 1.0e-12)
        );
    }

    #[test]
    fn every_interface_has_matching_counts_and_coordinates() {
        let topology = build_cgrid_topology(&example_configuration()).expect("valid topology");
        for interface in &topology.interfaces {
            let block_a = &topology.blocks[interface.block_a];
            let block_b = &topology.blocks[interface.block_b];
            let count_a = side_count(block_a, interface.side_a);
            let count_b = side_count(block_b, interface.side_b);
            assert_eq!(count_a, count_b);
            for index in 0..count_a {
                let index_b = if interface.reversed {
                    count_b - 1 - index
                } else {
                    index
                };
                let a = side_point(block_a, interface.side_a, index);
                let b = side_point(block_b, interface.side_b, index_b);
                assert!(a.distance_to(b) < 1.0e-10, "{a:?} != {b:?}");
            }
        }
    }

    fn side_count(block: &crate::topology::LogicalBlock2D, side: BlockSide) -> usize {
        match side {
            BlockSide::Bottom | BlockSide::Top => block.ni + 1,
            BlockSide::Left | BlockSide::Right => block.nj + 1,
        }
    }

    fn segment_slope(points: &[crate::geometry::Point2], index: usize) -> f64 {
        let first = points[index];
        let second = points[index + 1];
        (second.y - first.y) / (second.x - first.x)
    }

    fn side_point(
        block: &crate::topology::LogicalBlock2D,
        side: BlockSide,
        index: usize,
    ) -> crate::geometry::Point2 {
        match side {
            BlockSide::Bottom => block.bottom.point(block.xi_positions[index]),
            BlockSide::Top => block.top.point(block.xi_positions[index]),
            BlockSide::Left => block.left.point(block.eta_positions[index]),
            BlockSide::Right => block.right.point(block.eta_positions[index]),
        }
    }
}
