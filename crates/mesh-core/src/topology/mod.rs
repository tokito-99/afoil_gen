//! Logical C-grid curves, blocks, interfaces, and patch roles.

pub mod block;
pub mod cgrid;
pub mod curves;
pub mod patch;

pub use block::{BlockBoundaryAssignments, BlockInterface, BlockSide, LogicalBlock2D};
pub use cgrid::{CGridTopology, build_cgrid_topology};
pub use curves::{
    AirfoilSurfaceCurve, CircularArcCurve, CurveSpec, LineCurve, ParametricCurve, PolylineCurve,
    SampledCurve, WakeCenterlineCurve,
};
pub use patch::BoundaryRole;
