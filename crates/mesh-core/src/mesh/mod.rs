//! Structured 2D/3D mesh data, generation, extrusion, and quality checks.

pub mod extrusion;
pub mod generation;
pub mod optimization;
pub mod quality;
pub mod structured;
pub mod validation;
pub mod wall_resolution;

use serde::{Deserialize, Serialize};

use crate::{config::PatchType, geometry::Point2, topology::BoundaryRole};

/// A three-dimensional physical point.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Point3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Point3 {
    /// Creates a point.
    #[must_use]
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    /// Returns the Euclidean distance to another point.
    #[must_use]
    pub fn distance_to(self, other: Self) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        (dx.mul_add(dx, dy.mul_add(dy, dz * dz))).sqrt()
    }
}

/// Counter-clockwise quadrilateral connectivity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuadCell {
    pub vertices: [usize; 4],
    pub block_id: usize,
}

/// Right-handed hexahedral connectivity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HexCell {
    pub vertices: [usize; 8],
    pub block_id: usize,
}

/// Block-local points plus their deterministic global indices.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructuredBlock2D {
    pub id: usize,
    pub name: String,
    pub ni: usize,
    pub nj: usize,
    pub points: Vec<Point2>,
    pub global_point_indices: Vec<usize>,
}

impl StructuredBlock2D {
    /// Returns the row-major local point index.
    #[must_use]
    pub const fn local_index(&self, i: usize, j: usize) -> usize {
        j * (self.ni + 1) + i
    }

    /// Returns a global index for block coordinates.
    #[must_use]
    pub fn global_index(&self, i: usize, j: usize) -> usize {
        self.global_point_indices[self.local_index(i, j)]
    }
}

/// Named 2D exterior edges.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoundaryPatch2D {
    pub role: BoundaryRole,
    pub name: String,
    pub patch_type: PatchType,
    pub edges: Vec<[usize; 2]>,
}

/// Global, conformal structured mesh.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructuredMesh2D {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub optimization_report: Option<optimization::OptimizationReport>,
    pub points: Vec<Point2>,
    pub cells: Vec<QuadCell>,
    pub blocks: Vec<StructuredBlock2D>,
    pub boundaries: Vec<BoundaryPatch2D>,
    pub maximum_radial_growth_ratio: f64,
}

/// Named 3D exterior quadrilateral faces.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoundaryPatch3D {
    pub name: String,
    pub patch_type: PatchType,
    pub faces: Vec<[usize; 4]>,
}

/// Extruded, global hexahedral mesh.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructuredMesh3D {
    pub points: Vec<Point3>,
    pub cells: Vec<HexCell>,
    pub boundaries: Vec<BoundaryPatch3D>,
    pub spanwise_planes: Vec<f64>,
    pub source_2d_point_count: usize,
    pub block_count: usize,
    pub maximum_radial_growth_ratio: f64,
}
