//! Gordon–Hall interpolation and deterministic global point indexing.

use std::collections::{BTreeSet, HashMap};

use crate::{
    config::MeshConfiguration,
    error::MeshError,
    geometry::Point2,
    mesh::{BoundaryPatch2D, QuadCell, StructuredBlock2D, StructuredMesh2D},
    topology::{BlockSide, BoundaryRole, CGridTopology, LogicalBlock2D, ParametricCurve},
};

const INTERFACE_RELATIVE_TOLERANCE: f64 = 1.0e-10;

/// Generates every block, verifies complete interfaces, and merges shared points
/// using explicit interface declarations.
///
/// # Errors
///
/// Returns a topology error for corner mismatches, incomplete side assignments,
/// nonmatching interfaces, or duplicate interface declarations.
pub fn generate_structured_mesh(
    topology: &CGridTopology,
    configuration: &MeshConfiguration,
) -> Result<StructuredMesh2D, MeshError> {
    validate_block_ids(topology)?;

    let mut blocks = Vec::with_capacity(topology.blocks.len());
    let mut offsets = Vec::with_capacity(topology.blocks.len());
    let mut all_local_points = Vec::new();
    for logical in &topology.blocks {
        offsets.push(all_local_points.len());
        let points = generate_block_points(logical)?;
        all_local_points.extend_from_slice(&points);
        blocks.push(StructuredBlock2D {
            id: logical.id,
            name: logical.name.clone(),
            ni: logical.ni,
            nj: logical.nj,
            global_point_indices: vec![0; points.len()],
            points,
        });
    }

    let coordinate_scale = all_local_points
        .iter()
        .map(|point| point.x.abs().max(point.y.abs()))
        .fold(1.0, f64::max);
    let tolerance = INTERFACE_RELATIVE_TOLERANCE * coordinate_scale;
    let mut union_find = UnionFind::new(all_local_points.len());
    validate_and_merge_interfaces(
        topology,
        &blocks,
        &offsets,
        &all_local_points,
        tolerance,
        &mut union_find,
    )?;
    validate_side_completeness(topology)?;

    let mut root_to_global = HashMap::new();
    let mut global_points = Vec::new();
    for (local_id, local_point) in all_local_points.iter().enumerate() {
        let root = union_find.find(local_id);
        let global_id = if let Some(existing) = root_to_global.get(&root) {
            *existing
        } else {
            let new_id = global_points.len();
            root_to_global.insert(root, new_id);
            global_points.push(*local_point);
            new_id
        };
        let block_id = block_for_flat_local(local_id, &blocks, &offsets);
        let within_block = local_id - offsets[block_id];
        blocks[block_id].global_point_indices[within_block] = global_id;
    }

    let cells = build_cells(&blocks);
    let boundaries = build_boundaries(topology, &blocks, configuration);

    let mut mesh = StructuredMesh2D {
        optimization_report: None,
        points: global_points,
        cells,
        blocks,
        boundaries,
        maximum_radial_growth_ratio: topology.maximum_radial_growth_ratio,
    };
    crate::mesh::optimization::optimize(&mut mesh, topology, configuration)?;
    Ok(mesh)
}

/// Evaluates the complete Gordon–Hall transfinite interpolation expression.
#[must_use]
pub fn gordon_hall_point(block: &LogicalBlock2D, xi: f64, eta: f64) -> Point2 {
    let bottom = block.bottom.point(xi);
    let top = block.top.point(xi);
    let left = block.left.point(eta);
    let right = block.right.point(eta);
    let p00 = block.bottom.point(0.0);
    let p10 = block.bottom.point(1.0);
    let p01 = block.top.point(0.0);
    let p11 = block.top.point(1.0);

    let x = (1.0 - eta) * bottom.x + eta * top.x + (1.0 - xi) * left.x + xi * right.x
        - ((1.0 - xi) * (1.0 - eta) * p00.x
            + xi * (1.0 - eta) * p10.x
            + (1.0 - xi) * eta * p01.x
            + xi * eta * p11.x);
    let y = (1.0 - eta) * bottom.y + eta * top.y + (1.0 - xi) * left.y + xi * right.y
        - ((1.0 - xi) * (1.0 - eta) * p00.y
            + xi * (1.0 - eta) * p10.y
            + (1.0 - xi) * eta * p01.y
            + xi * eta * p11.y);
    Point2::new(x, y)
}

fn generate_block_points(block: &LogicalBlock2D) -> Result<Vec<Point2>, MeshError> {
    if block.xi_positions.len() != block.ni + 1 || block.eta_positions.len() != block.nj + 1 {
        return Err(MeshError::TopologyError(format!(
            "block {} position-array dimensions do not match ni/nj",
            block.id
        )));
    }
    validate_curve_corners(block)?;
    validate_positions(&block.xi_positions, block.id, "xi")?;
    validate_positions(&block.eta_positions, block.id, "eta")?;

    let mut points = Vec::with_capacity((block.ni + 1) * (block.nj + 1));
    for &eta in &block.eta_positions {
        for &xi in &block.xi_positions {
            points.push(gordon_hall_point(block, xi, eta));
        }
    }
    Ok(points)
}

fn validate_curve_corners(block: &LogicalBlock2D) -> Result<(), MeshError> {
    let pairs = [
        (block.bottom.point(0.0), block.left.point(0.0), "P00"),
        (block.bottom.point(1.0), block.right.point(0.0), "P10"),
        (block.top.point(0.0), block.left.point(1.0), "P01"),
        (block.top.point(1.0), block.right.point(1.0), "P11"),
    ];
    let scale = pairs
        .iter()
        .flat_map(|(a, b, _)| [a.x.abs(), a.y.abs(), b.x.abs(), b.y.abs()])
        .fold(1.0, f64::max);
    let tolerance = INTERFACE_RELATIVE_TOLERANCE * scale;
    for (first, second, name) in pairs {
        if !first.x.is_finite()
            || !first.y.is_finite()
            || !second.x.is_finite()
            || !second.y.is_finite()
        {
            return Err(MeshError::TopologyError(format!(
                "block {} has non-finite {name} corner coordinates",
                block.id
            )));
        }
        if first.distance_to(second) > tolerance {
            return Err(MeshError::TopologyError(format!(
                "block {} has inconsistent {name} corner",
                block.id
            )));
        }
    }
    Ok(())
}

fn validate_positions(
    positions: &[f64],
    block_id: usize,
    coordinate_name: &str,
) -> Result<(), MeshError> {
    if positions
        .first()
        .is_none_or(|value| value.abs() > f64::EPSILON)
        || positions
            .last()
            .is_none_or(|value| (*value - 1.0).abs() > f64::EPSILON)
        || positions
            .windows(2)
            .any(|pair| !pair[0].is_finite() || pair[1] <= pair[0])
    {
        return Err(MeshError::TopologyError(format!(
            "block {block_id} has invalid {coordinate_name} positions"
        )));
    }
    Ok(())
}

fn validate_block_ids(topology: &CGridTopology) -> Result<(), MeshError> {
    if topology
        .blocks
        .iter()
        .enumerate()
        .any(|(index, block)| index != block.id)
    {
        return Err(MeshError::TopologyError(
            "block IDs must be contiguous and match deterministic vector order".to_owned(),
        ));
    }
    Ok(())
}

fn validate_and_merge_interfaces(
    topology: &CGridTopology,
    blocks: &[StructuredBlock2D],
    offsets: &[usize],
    points: &[Point2],
    tolerance: f64,
    union_find: &mut UnionFind,
) -> Result<(), MeshError> {
    let mut declared_sides = BTreeSet::new();
    for interface in &topology.interfaces {
        if !declared_sides.insert((interface.block_a, interface.side_a))
            || !declared_sides.insert((interface.block_b, interface.side_b))
        {
            return Err(MeshError::TopologyError(
                "a block side appears in more than one interface".to_owned(),
            ));
        }
        let block_a = blocks.get(interface.block_a).ok_or_else(|| {
            MeshError::TopologyError("interface references an unknown block".to_owned())
        })?;
        let block_b = blocks.get(interface.block_b).ok_or_else(|| {
            MeshError::TopologyError("interface references an unknown block".to_owned())
        })?;
        let nodes_a = side_local_indices(block_a, interface.side_a);
        let mut nodes_b = side_local_indices(block_b, interface.side_b);
        if interface.reversed {
            nodes_b.reverse();
        }
        if nodes_a.len() != nodes_b.len() {
            return Err(MeshError::TopologyError(format!(
                "nonmatching interface counts between blocks {} and {}",
                interface.block_a, interface.block_b
            )));
        }
        for (local_a, local_b) in nodes_a.into_iter().zip(nodes_b) {
            let flat_a = offsets[interface.block_a] + local_a;
            let flat_b = offsets[interface.block_b] + local_b;
            if points[flat_a].distance_to(points[flat_b]) > tolerance {
                return Err(MeshError::TopologyError(format!(
                    "nonmatching interface coordinates between blocks {} and {}",
                    interface.block_a, interface.block_b
                )));
            }
            union_find.union(flat_a, flat_b);
        }
    }
    Ok(())
}

fn validate_side_completeness(topology: &CGridTopology) -> Result<(), MeshError> {
    let interface_sides = topology
        .interfaces
        .iter()
        .flat_map(|interface| {
            [
                (interface.block_a, interface.side_a),
                (interface.block_b, interface.side_b),
            ]
        })
        .collect::<BTreeSet<_>>();

    for block in &topology.blocks {
        for side in [
            BlockSide::Bottom,
            BlockSide::Top,
            BlockSide::Left,
            BlockSide::Right,
        ] {
            let is_interface = interface_sides.contains(&(block.id, side));
            let is_boundary = block.boundaries.get(side).is_some();
            if is_interface == is_boundary {
                return Err(MeshError::TopologyError(format!(
                    "block {} side {side:?} must be assigned exactly once as interface or boundary",
                    block.id
                )));
            }
        }
    }
    Ok(())
}

fn side_local_indices(block: &StructuredBlock2D, side: BlockSide) -> Vec<usize> {
    match side {
        BlockSide::Bottom => (0..=block.ni).map(|i| block.local_index(i, 0)).collect(),
        BlockSide::Top => (0..=block.ni)
            .map(|i| block.local_index(i, block.nj))
            .collect(),
        BlockSide::Left => (0..=block.nj).map(|j| block.local_index(0, j)).collect(),
        BlockSide::Right => (0..=block.nj)
            .map(|j| block.local_index(block.ni, j))
            .collect(),
    }
}

fn build_cells(blocks: &[StructuredBlock2D]) -> Vec<QuadCell> {
    let capacity = blocks.iter().map(|block| block.ni * block.nj).sum();
    let mut cells = Vec::with_capacity(capacity);
    for block in blocks {
        for j in 0..block.nj {
            for i in 0..block.ni {
                cells.push(QuadCell {
                    vertices: [
                        block.global_index(i, j),
                        block.global_index(i + 1, j),
                        block.global_index(i + 1, j + 1),
                        block.global_index(i, j + 1),
                    ],
                    block_id: block.id,
                });
            }
        }
    }
    cells
}

fn build_boundaries(
    topology: &CGridTopology,
    blocks: &[StructuredBlock2D],
    configuration: &MeshConfiguration,
) -> Vec<BoundaryPatch2D> {
    let definitions = [
        (BoundaryRole::Inlet, &configuration.boundaries.inlet),
        (BoundaryRole::Outlet, &configuration.boundaries.outlet),
        (BoundaryRole::Airfoil, &configuration.boundaries.airfoil),
        (
            BoundaryRole::UpperFarfield,
            &configuration.boundaries.upper_farfield,
        ),
        (
            BoundaryRole::LowerFarfield,
            &configuration.boundaries.lower_farfield,
        ),
    ];
    let mut patches = definitions
        .into_iter()
        .map(|(role, definition)| BoundaryPatch2D {
            role,
            name: definition.name.clone(),
            patch_type: definition.patch_type,
            edges: Vec::new(),
        })
        .collect::<Vec<_>>();

    for logical in &topology.blocks {
        let block = &blocks[logical.id];
        for side in [
            BlockSide::Bottom,
            BlockSide::Top,
            BlockSide::Left,
            BlockSide::Right,
        ] {
            if let Some(role) = logical.boundaries.get(side) {
                let patch_index = patch_index(role);
                patches[patch_index].edges.extend(side_edges(block, side));
            }
        }
    }
    patches
}

const fn patch_index(role: BoundaryRole) -> usize {
    match role {
        BoundaryRole::Inlet => 0,
        BoundaryRole::Outlet => 1,
        BoundaryRole::Airfoil => 2,
        BoundaryRole::UpperFarfield => 3,
        BoundaryRole::LowerFarfield => 4,
    }
}

fn side_edges(block: &StructuredBlock2D, side: BlockSide) -> Vec<[usize; 2]> {
    match side {
        BlockSide::Bottom => (0..block.ni)
            .map(|i| [block.global_index(i, 0), block.global_index(i + 1, 0)])
            .collect(),
        BlockSide::Top => (0..block.ni)
            .map(|i| {
                [
                    block.global_index(i, block.nj),
                    block.global_index(i + 1, block.nj),
                ]
            })
            .collect(),
        BlockSide::Left => (0..block.nj)
            .map(|j| [block.global_index(0, j), block.global_index(0, j + 1)])
            .collect(),
        BlockSide::Right => (0..block.nj)
            .map(|j| {
                [
                    block.global_index(block.ni, j),
                    block.global_index(block.ni, j + 1),
                ]
            })
            .collect(),
    }
}

fn block_for_flat_local(local_id: usize, blocks: &[StructuredBlock2D], offsets: &[usize]) -> usize {
    let insertion = offsets.partition_point(|offset| *offset <= local_id);
    (insertion - 1).min(blocks.len() - 1)
}

#[derive(Debug)]
struct UnionFind {
    parents: Vec<usize>,
    ranks: Vec<u8>,
}

impl UnionFind {
    fn new(size: usize) -> Self {
        Self {
            parents: (0..size).collect(),
            ranks: vec![0; size],
        }
    }

    fn find(&mut self, value: usize) -> usize {
        if self.parents[value] != value {
            self.parents[value] = self.find(self.parents[value]);
        }
        self.parents[value]
    }

    fn union(&mut self, first: usize, second: usize) {
        let first_root = self.find(first);
        let second_root = self.find(second);
        if first_root == second_root {
            return;
        }
        match self.ranks[first_root].cmp(&self.ranks[second_root]) {
            std::cmp::Ordering::Less => self.parents[first_root] = second_root,
            std::cmp::Ordering::Greater => self.parents[second_root] = first_root,
            std::cmp::Ordering::Equal => {
                self.parents[second_root] = first_root;
                self.ranks[first_root] += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{generate_structured_mesh, gordon_hall_point};
    use crate::{
        geometry::Point2,
        test_support::example_configuration,
        topology::{
            BlockBoundaryAssignments, CurveSpec, LineCurve, LogicalBlock2D, build_cgrid_topology,
        },
    };

    #[test]
    fn gordon_hall_reproduces_a_rectangle() {
        let positions = vec![0.0, 0.5, 1.0];
        let block = LogicalBlock2D {
            id: 0,
            name: "rectangle".to_owned(),
            ni: 2,
            nj: 2,
            xi_positions: positions.clone(),
            eta_positions: positions,
            bottom: CurveSpec::Line(LineCurve::new(Point2::new(0.0, 0.0), Point2::new(2.0, 0.0))),
            top: CurveSpec::Line(LineCurve::new(Point2::new(0.0, 1.0), Point2::new(2.0, 1.0))),
            left: CurveSpec::Line(LineCurve::new(Point2::new(0.0, 0.0), Point2::new(0.0, 1.0))),
            right: CurveSpec::Line(LineCurve::new(Point2::new(2.0, 0.0), Point2::new(2.0, 1.0))),
            boundaries: BlockBoundaryAssignments::default(),
        };
        assert_eq!(gordon_hall_point(&block, 0.5, 0.5), Point2::new(1.0, 0.5));
    }

    #[test]
    fn default_topology_merges_every_declared_interface() {
        let configuration = example_configuration();
        let topology = build_cgrid_topology(&configuration).expect("valid topology");
        let local_count: usize = topology
            .blocks
            .iter()
            .map(|block| (block.ni + 1) * (block.nj + 1))
            .sum();
        let shared_nodes: usize = topology
            .interfaces
            .iter()
            .map(|interface| match interface.side_a {
                crate::topology::BlockSide::Bottom | crate::topology::BlockSide::Top => {
                    topology.blocks[interface.block_a].ni + 1
                }
                crate::topology::BlockSide::Left | crate::topology::BlockSide::Right => {
                    topology.blocks[interface.block_a].nj + 1
                }
            })
            .sum();
        let mesh =
            generate_structured_mesh(&topology, &configuration).expect("valid structured mesh");
        assert_eq!(mesh.points.len(), local_count - shared_nodes);
        assert_eq!(mesh.blocks.len(), 6);
        assert!(mesh.boundaries.iter().all(|patch| !patch.edges.is_empty()));
    }

    #[test]
    fn default_surface_first_cell_matches_les_estimate_at_trailing_edge() {
        let configuration = example_configuration();
        let topology = build_cgrid_topology(&configuration).expect("valid topology");
        let expected = topology
            .wall_spacing
            .expect("LES spacing")
            .first_cell_height;
        let mesh = generate_structured_mesh(&topology, &configuration).expect("valid mesh");
        let upper_surface = &mesh.blocks[1];
        let wall = upper_surface.points[upper_surface.local_index(upper_surface.ni, 0)];
        let first = upper_surface.points[upper_surface.local_index(upper_surface.ni, 1)];
        let actual = wall.distance_to(first);
        assert!((actual - expected).abs() / expected < 1.0e-4);
    }
}
