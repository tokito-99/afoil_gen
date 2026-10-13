use serde::{Deserialize, Serialize};

/// Severity assigned to an input-validation issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ValidationSeverity {
    /// The configuration cannot be used.
    Error,
    /// The configuration can be used, but deserves engineering review.
    Warning,
}

/// A field-specific, machine-readable validation issue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationError {
    /// JSON field path associated with the issue.
    pub field: String,
    /// Stable issue identifier.
    pub code: String,
    /// Actionable description.
    pub message: String,
    /// Whether the issue is fatal.
    pub severity: ValidationSeverity,
}

impl ValidationError {
    /// Creates a fatal validation issue.
    #[must_use]
    pub fn error(
        field: impl Into<String>,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            field: field.into(),
            code: code.into(),
            message: message.into(),
            severity: ValidationSeverity::Error,
        }
    }

    /// Creates a non-fatal validation issue.
    #[must_use]
    pub fn warning(
        field: impl Into<String>,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            field: field.into(),
            code: code.into(),
            message: message.into(),
            severity: ValidationSeverity::Warning,
        }
    }
}

/// Result of authoritative configuration validation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationReport {
    /// True when no fatal issue is present.
    pub valid: bool,
    /// Errors and warnings in deterministic field order.
    pub issues: Vec<ValidationError>,
}

impl ValidationReport {
    /// Builds a report and derives validity from issue severities.
    #[must_use]
    pub fn new(issues: Vec<ValidationError>) -> Self {
        let valid = !issues
            .iter()
            .any(|issue| issue.severity == ValidationSeverity::Error);
        Self { valid, issues }
    }
}

/// Errors produced by the geometry and spacing engine.
#[derive(Debug, thiserror::Error)]
pub enum MeshError {
    /// Pilot-solver feedback has invalid data or ambiguous units/columns.
    #[error("wall-shear feedback failed: {0}")]
    FeedbackError(String),
    /// The configuration contains one or more fatal validation issues.
    #[error("invalid configuration")]
    InvalidConfiguration(Vec<ValidationError>),

    /// The requested NACA designation is unsupported or malformed.
    #[error("invalid NACA code: {0}")]
    InvalidNacaCode(String),

    /// The requested geometric progression has no solution in the allowed range.
    #[error("geometric stretching is infeasible: {0}")]
    InfeasibleStretching(String),

    /// A requested geometry operation is undefined.
    #[error("geometry construction failed: {0}")]
    GeometryError(String),

    /// A future topology operation failed.
    #[error("topology construction failed: {0}")]
    TopologyError(String),

    /// A future quality check failed.
    #[error("mesh quality validation failed: {0}")]
    QualityError(String),

    /// A future export operation failed.
    #[error("export failed: {0}")]
    ExportError(String),

    /// File-system operation failed.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON parsing or serialization failed.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}
