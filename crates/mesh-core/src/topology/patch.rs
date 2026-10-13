//! Semantic boundary roles resolved to user-configured patch names and types.

use serde::{Deserialize, Serialize};

/// Stable semantic roles used before names are applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BoundaryRole {
    Inlet,
    Outlet,
    Airfoil,
    UpperFarfield,
    LowerFarfield,
}
