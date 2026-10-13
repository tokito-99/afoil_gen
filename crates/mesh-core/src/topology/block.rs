//! Logical quadrilateral block metadata and conformal interface declarations.

use serde::Serialize;

use crate::topology::{BoundaryRole, CurveSpec};

/// One side of a logical quadrilateral.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BlockSide {
    Bottom,
    Top,
    Left,
    Right,
}

/// Physical-boundary assignments for any non-interface block side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockBoundaryAssignments {
    pub bottom: Option<BoundaryRole>,
    pub top: Option<BoundaryRole>,
    pub left: Option<BoundaryRole>,
    pub right: Option<BoundaryRole>,
}

impl BlockBoundaryAssignments {
    /// Returns the role assigned to a side.
    #[must_use]
    pub const fn get(self, side: BlockSide) -> Option<BoundaryRole> {
        match side {
            BlockSide::Bottom => self.bottom,
            BlockSide::Top => self.top,
            BlockSide::Left => self.left,
            BlockSide::Right => self.right,
        }
    }
}

/// A deterministic Gordon–Hall quadrilateral.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogicalBlock2D {
    pub id: usize,
    pub name: String,
    pub ni: usize,
    pub nj: usize,
    pub xi_positions: Vec<f64>,
    pub eta_positions: Vec<f64>,
    pub bottom: CurveSpec,
    pub top: CurveSpec,
    pub left: CurveSpec,
    pub right: CurveSpec,
    pub boundaries: BlockBoundaryAssignments,
}

/// Explicit topological equivalence between complete block sides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockInterface {
    pub block_a: usize,
    pub side_a: BlockSide,
    pub block_b: usize,
    pub side_b: BlockSide,
    pub reversed: bool,
}
