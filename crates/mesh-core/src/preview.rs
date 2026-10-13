//! Browser-oriented geometry and structured-mesh preview data.

use serde::{Deserialize, Serialize};

use crate::{
    config::MeshConfiguration,
    error::{MeshError, ValidationError, ValidationSeverity},
    geometry::{Point2, naca4::generate_airfoil},
    les::{
        sizing::{LesSizingEstimate, estimate_les_sizing},
        stretching::{StretchingSolution, solve_layered_stretching},
        wall_spacing::{ResolvedWallSpacing, resolve_wall_spacing},
    },
    mesh::{
        StructuredBlock2D,
        extrusion::spanwise_planes,
        generation::GeneratedCGrid,
        quality::{MeshQualityReport, evaluate_extruded_quality_analytically},
        structured::generate_structured_mesh,
        validation::require_valid_mesh_2d,
        wall_resolution::{WallResolutionAudit, audit_wall_resolution},
    },
    topology::{BoundaryRole, build_cgrid_topology},
    validation::validate_configuration,
};

/// Deterministic geometry-only preview produced before C-grid implementation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeometryPreview {
    /// Explicit scope marker so consumers do not mistake this for a completed mesh.
    pub phase: String,
    pub airfoil_upper: Vec<Point2>,
    pub airfoil_lower: Vec<Point2>,
    pub airfoil_perimeter: Vec<Point2>,
    pub rotation_pivot: Point2,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wall_spacing: Option<ResolvedWallSpacing>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub radial_stretching: Option<StretchingSolution>,
    pub warnings: Vec<ValidationError>,
}

/// Visualization detail without changing generated mesh coordinates or cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PreviewLevel {
    GeometryOnly,
    BlockTopology,
    CoarseMesh,
    FullMesh,
}

/// Visual role assigned to a polyline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PreviewPolylineKind {
    BlockBoundary,
    MeshLine,
}

/// Named polyline for SVG or Canvas rendering.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewPolyline {
    pub label: String,
    pub kind: PreviewPolylineKind,
    pub points: Vec<Point2>,
}

/// Patch label location computed in physical coordinates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoundaryLabel {
    pub role: BoundaryRole,
    pub name: String,
    pub position: Point2,
}

/// Physical boundary segments grouped by their stable semantic role.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewBoundaryPatch {
    pub role: BoundaryRole,
    pub name: String,
    pub segments: Vec<[Point2; 2]>,
}

/// Phase 4–6 preview with C-grid boundaries, sampled mesh lines, and quality.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeshPreview {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub optimization: Option<crate::mesh::optimization::OptimizationReport>,
    pub phase: String,
    pub preview_level: PreviewLevel,
    pub airfoil_upper: Vec<Point2>,
    pub airfoil_lower: Vec<Point2>,
    pub outer_boundary: Vec<Point2>,
    pub wake_upper: Vec<Point2>,
    pub wake_lower: Vec<Point2>,
    pub block_boundaries: Vec<PreviewPolyline>,
    pub mesh_lines: Vec<PreviewPolyline>,
    pub boundary_patches: Vec<PreviewBoundaryPatch>,
    pub boundary_labels: Vec<BoundaryLabel>,
    pub quality: MeshQualityReport,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality_3d: Option<MeshQualityReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wall_spacing: Option<ResolvedWallSpacing>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wall_resolution: Option<WallResolutionAudit>,
    pub wall_layer_cells: usize,
    pub wall_layer_thickness: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub les_sizing: Option<LesSizingEstimate>,
    pub warnings: Vec<ValidationError>,
}

/// Produces an analytical airfoil and LES-spacing preview.
///
/// C-grid block topology, cells, boundary faces, and mesh quality are intentionally
/// absent until Phase 4 and later.
///
/// # Errors
///
/// Returns [`MeshError::InvalidConfiguration`] for fatal validation issues and
/// propagates geometry or stretching failures.
pub fn generate_geometry_preview(
    configuration: &MeshConfiguration,
) -> Result<GeometryPreview, MeshError> {
    let report = validate_configuration(configuration);
    if !report.valid {
        let errors = report
            .issues
            .into_iter()
            .filter(|issue| issue.severity == ValidationSeverity::Error)
            .collect();
        return Err(MeshError::InvalidConfiguration(errors));
    }
    let warnings = report
        .issues
        .into_iter()
        .filter(|issue| issue.severity == ValidationSeverity::Warning)
        .collect();

    let geometry = generate_airfoil(&configuration.airfoil)?;
    let spacing = resolve_wall_spacing(configuration)?;
    let radial_extent = configuration.airfoil.chord_length
        * configuration
            .domain
            .upper_radius_chords
            .min(configuration.domain.lower_radius_chords);
    let wall_layer_thickness = configuration.airfoil.chord_length
        * configuration.les.reference_boundary_layer_thickness_chords;
    let layered = solve_layered_stretching(
        spacing.first_cell_height,
        radial_extent,
        configuration.mesh.radial_cells,
        wall_layer_thickness,
        configuration.les.minimum_cells_across_boundary_layer,
        configuration.les.max_radial_growth_ratio,
    )?;
    let stretching = StretchingSolution {
        ratio: layered.maximum_ratio,
        cell_sizes: layered.cell_sizes,
        point_positions: layered.point_positions,
        total_length: layered.total_length,
    };
    let (wall_spacing, radial_stretching) = (Some(spacing), Some(stretching));

    Ok(GeometryPreview {
        phase: "geometry-and-les-spacing".to_owned(),
        airfoil_upper: geometry.upper,
        airfoil_lower: geometry.lower,
        airfoil_perimeter: geometry.perimeter,
        rotation_pivot: geometry.rotation_pivot,
        wall_spacing,
        radial_stretching,
        warnings,
    })
}

/// Produces a structured C-grid preview with selectable line density.
///
/// Preview downsampling only affects visualization polylines. The complete 2-D
/// mesh is validated; extruded counts and vertical-volume/edge metrics are
/// computed analytically without allocating the expanded 3-D mesh.
///
/// # Errors
///
/// Returns configuration, topology, extrusion, or fatal quality errors from
/// the complete Phase 4–6 generation pipeline.
pub fn generate_mesh_preview(
    configuration: &MeshConfiguration,
    level: PreviewLevel,
) -> Result<MeshPreview, MeshError> {
    let topology = build_cgrid_topology(configuration)?;
    let mesh_2d = generate_structured_mesh(&topology, configuration)?;
    let quality_2d = require_valid_mesh_2d(&mesh_2d)?;
    let quality_3d = if configuration.extrusion.enabled {
        let planes = spanwise_planes(configuration)?;
        Some(evaluate_extruded_quality_analytically(
            &mesh_2d,
            &quality_2d,
            &planes,
        )?)
    } else {
        None
    };
    let generated = GeneratedCGrid {
        topology,
        mesh_2d,
        quality_2d,
        mesh_3d: None,
        quality_3d,
    };
    Ok(generate_mesh_preview_from_generated(
        configuration,
        level,
        &generated,
    ))
}

/// Builds visualization data from an existing validated C-grid.
#[must_use]
pub fn generate_mesh_preview_from_generated(
    configuration: &MeshConfiguration,
    level: PreviewLevel,
    generated: &GeneratedCGrid,
) -> MeshPreview {
    let mut warnings: Vec<_> = validate_configuration(configuration)
        .issues
        .into_iter()
        .filter(|issue| issue.severity == ValidationSeverity::Warning)
        .collect();
    let block_boundaries = preview_block_boundaries(generated, level);
    let mesh_lines = match level {
        PreviewLevel::GeometryOnly | PreviewLevel::BlockTopology => Vec::new(),
        PreviewLevel::CoarseMesh => preview_mesh_lines(&generated.mesh_2d.blocks, false),
        PreviewLevel::FullMesh => preview_mesh_lines(&generated.mesh_2d.blocks, true),
    };
    let (boundary_patches, boundary_labels) = preview_boundary_data(generated);
    let wall_resolution = match audit_wall_resolution(&generated.mesh_2d, configuration) {
        Ok(audit) => {
            warnings.extend(audit.warnings.iter().map(|message| {
                ValidationError::warning("mesh.wallResolution", "WALL_RESOLUTION_REVIEW", message)
            }));
            Some(audit)
        }
        Err(error) => {
            warnings.push(ValidationError::warning(
                "mesh.wallResolution",
                "WALL_AUDIT_FAILED",
                error.to_string(),
            ));
            None
        }
    };

    MeshPreview {
        optimization: generated.mesh_2d.optimization_report.clone(),
        phase: "structured-cgrid-and-extrusion".to_owned(),
        preview_level: level,
        airfoil_upper: generated.topology.airfoil_upper.clone(),
        airfoil_lower: generated.topology.airfoil_lower.clone(),
        outer_boundary: generated.topology.outer_boundary.clone(),
        wake_upper: generated.topology.wake_upper.clone(),
        wake_lower: generated.topology.wake_lower.clone(),
        block_boundaries,
        mesh_lines,
        boundary_patches,
        boundary_labels,
        quality: generated.quality_2d.clone(),
        quality_3d: generated.quality_3d.clone(),
        wall_spacing: generated.topology.wall_spacing,
        wall_resolution,
        wall_layer_cells: generated.topology.wall_layer_cells,
        wall_layer_thickness: generated.topology.wall_layer_thickness,
        les_sizing: configuration
            .resolved_mesh_intent()
            .is_les()
            .then(|| estimate_les_sizing(configuration).ok())
            .flatten(),
        warnings,
    }
}

fn preview_block_boundaries(
    generated: &GeneratedCGrid,
    level: PreviewLevel,
) -> Vec<PreviewPolyline> {
    if level == PreviewLevel::GeometryOnly {
        return Vec::new();
    }
    generated
        .topology
        .blocks
        .iter()
        .flat_map(|block| {
            [
                ("bottom", &block.bottom),
                ("top", &block.top),
                ("left", &block.left),
                ("right", &block.right),
            ]
            .into_iter()
            .map(|(side, curve)| PreviewPolyline {
                label: format!("{}:{side}", block.name),
                kind: PreviewPolylineKind::BlockBoundary,
                points: curve.sampled_points(32),
            })
        })
        .collect()
}

fn preview_boundary_data(
    generated: &GeneratedCGrid,
) -> (Vec<PreviewBoundaryPatch>, Vec<BoundaryLabel>) {
    let boundary_patches = generated
        .mesh_2d
        .boundaries
        .iter()
        .map(|patch| PreviewBoundaryPatch {
            role: patch.role,
            name: patch.name.clone(),
            segments: patch
                .edges
                .iter()
                .map(|edge| {
                    [
                        generated.mesh_2d.points[edge[0]],
                        generated.mesh_2d.points[edge[1]],
                    ]
                })
                .collect(),
        })
        .collect();
    let boundary_labels = generated
        .mesh_2d
        .boundaries
        .iter()
        .map(|patch| {
            let mut coordinate_sum = Point2::new(0.0, 0.0);
            let mut sample_count = 0_usize;
            for edge in &patch.edges {
                for point_index in edge {
                    let point = generated.mesh_2d.points[*point_index];
                    coordinate_sum.x += point.x;
                    coordinate_sum.y += point.y;
                    sample_count += 1;
                }
            }
            #[allow(clippy::cast_precision_loss)]
            let denominator = sample_count.max(1) as f64;
            BoundaryLabel {
                role: patch.role,
                name: patch.name.clone(),
                position: Point2::new(
                    coordinate_sum.x / denominator,
                    coordinate_sum.y / denominator,
                ),
            }
        })
        .collect();
    (boundary_patches, boundary_labels)
}

fn preview_mesh_lines(blocks: &[StructuredBlock2D], full_resolution: bool) -> Vec<PreviewPolyline> {
    let mut lines = Vec::new();
    for block in blocks {
        let i_indices = preview_indices(block.ni, full_resolution);
        let j_indices = preview_indices(block.nj, full_resolution);
        for &i in &i_indices {
            lines.push(PreviewPolyline {
                label: format!("{}:i={i}", block.name),
                kind: PreviewPolylineKind::MeshLine,
                points: j_indices
                    .iter()
                    .map(|j| block.points[block.local_index(i, *j)])
                    .collect(),
            });
        }
        for j in j_indices {
            lines.push(PreviewPolyline {
                label: format!("{}:j={j}", block.name),
                kind: PreviewPolylineKind::MeshLine,
                points: i_indices
                    .iter()
                    .map(|i| block.points[block.local_index(*i, j)])
                    .collect(),
            });
        }
    }
    lines
}

fn preview_indices(cell_count: usize, full_resolution: bool) -> Vec<usize> {
    let stride = if full_resolution {
        1
    } else {
        cell_count.div_ceil(20).max(1)
    };
    let mut indices = (0..=cell_count).step_by(stride).collect::<Vec<_>>();
    if indices.last().copied() != Some(cell_count) {
        indices.push(cell_count);
    }
    indices
}

#[cfg(test)]
mod tests {
    use super::{PreviewLevel, generate_mesh_preview};
    use crate::test_support::small_configuration;

    #[test]
    fn structured_preview_exposes_blocks_boundaries_and_quality() {
        let preview = generate_mesh_preview(&small_configuration(), PreviewLevel::CoarseMesh)
            .expect("valid preview");
        assert_eq!(preview.phase, "structured-cgrid-and-extrusion");
        assert!(preview.quality.valid);
        assert_eq!(preview.quality.block_count, 6);
        assert_eq!(preview.block_boundaries.len(), 24);
        assert!(!preview.mesh_lines.is_empty());
        assert_eq!(preview.boundary_patches.len(), 5);
        assert!(
            preview
                .boundary_patches
                .iter()
                .all(|patch| !patch.segments.is_empty())
        );
        assert_eq!(preview.boundary_labels.len(), 5);
        assert!(preview.quality_3d.is_some_and(|quality| quality.valid));
    }

    #[test]
    fn preview_levels_change_only_visual_line_payload() {
        let configuration = small_configuration();
        let topology = generate_mesh_preview(&configuration, PreviewLevel::BlockTopology)
            .expect("topology preview");
        let coarse = generate_mesh_preview(&configuration, PreviewLevel::CoarseMesh)
            .expect("coarse preview");
        let full =
            generate_mesh_preview(&configuration, PreviewLevel::FullMesh).expect("full preview");
        assert!(topology.mesh_lines.is_empty());
        assert!(coarse.mesh_lines.len() < full.mesh_lines.len());
        assert_eq!(coarse.quality, full.quality);
    }
}
