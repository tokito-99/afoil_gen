//! Thin WebAssembly boundary for the `afoil_gen` Rust engine.
//!
//! Parsing, validation, geometry, topology, quality, and export logic remain in
//! `mesh-core`. This crate only translates JSON and JavaScript values.

use mesh_core::{
    MeshError, PreviewLevel, configuration_from_json, export_fluent, export_gmsh, export_openfoam,
    generate_mesh_preview,
};
use serde::Serialize;
use wasm_bindgen::prelude::*;

/// Parses pilot-solver wall-shear magnitudes in Pa, without changing settings.
///
/// # Errors
/// Returns a JavaScript error for invalid or ambiguous CSV data.
#[wasm_bindgen]
pub fn import_wall_shear(csv: &str, column: &str, percentile: f64) -> Result<JsValue, JsValue> {
    let report = mesh_core::parse_wall_shear_csv(csv, column, percentile)
        .map_err(|error| to_js_error(&error))?;
    to_js_value(&report)
}

/// Imports a side-resolved pilot-solver profile and returns a configuration
/// carrying the profile, its provenance, and safe optimization defaults.
///
/// # Errors
/// Returns a JavaScript error for invalid configuration or incomplete profile data.
#[wasm_bindgen]
pub fn import_wall_shear_profile(config_json: &str, csv: &str) -> Result<JsValue, JsValue> {
    let configuration =
        configuration_from_json(config_json).map_err(|error| to_js_error(&error))?;
    let calibrated = mesh_core::import_wall_shear_profile(&configuration, csv)
        .map_err(|error| to_js_error(&error))?;
    to_js_value(&calibrated)
}

/// Runs the bounded cell-budget planner without relaxing physical dimensions
/// or wall-unit targets. A zero point budget means no point limit.
///
/// # Errors
/// Returns a JavaScript error for invalid limits or configuration.
#[wasm_bindgen]
pub fn plan_cell_budget(
    config_json: &str,
    max_cells: f64,
    max_points: f64,
) -> Result<JsValue, JsValue> {
    let configuration =
        configuration_from_json(config_json).map_err(|error| to_js_error(&error))?;
    let cells = numeric_limit(max_cells, "cell").map_err(|error| to_js_error(&error))?;
    let points = if max_points == 0.0 {
        None
    } else {
        Some(numeric_limit(max_points, "point").map_err(|error| to_js_error(&error))?)
    };
    let plan = mesh_core::plan_cell_budget(
        &configuration,
        &mesh_core::BudgetConstraints {
            max_cells: cells,
            max_points: points,
        },
    )
    .map_err(|error| to_js_error(&error))?;
    to_js_value(&plan)
}

/// Validates configuration JSON and returns the structured report to JavaScript.
///
/// # Errors
///
/// Returns a JavaScript error value when JSON parsing or serialization fails.
#[wasm_bindgen]
pub fn validate_configuration(config_json: &str) -> Result<JsValue, JsValue> {
    let configuration =
        configuration_from_json(config_json).map_err(|error| to_js_error(&error))?;
    let report = mesh_core::validate_configuration(&configuration);
    to_js_value(&report)
}

/// Generates a coarse structured preview for interactive visualization.
///
/// # Errors
///
/// Returns a JavaScript error value for invalid configuration or mesh failures.
#[wasm_bindgen]
pub fn generate_preview(config_json: &str) -> Result<JsValue, JsValue> {
    generate_preview_at_level(config_json, "COARSE_MESH")
}

/// Generates a structured preview at a named visualization level.
///
/// # Errors
///
/// Returns a JavaScript error value for an unknown level, invalid
/// configuration, or mesh generation failure.
#[wasm_bindgen]
pub fn generate_preview_at_level(
    config_json: &str,
    preview_level: &str,
) -> Result<JsValue, JsValue> {
    let configuration =
        configuration_from_json(config_json).map_err(|error| to_js_error(&error))?;
    let level = parse_preview_level(preview_level).map_err(|error| to_js_error(&error))?;
    let preview =
        generate_mesh_preview(&configuration, level).map_err(|error| to_js_error(&error))?;
    to_js_value(&preview)
}

/// Generates the authoritative native `OpenFOAM` `blockMeshDict`.
///
/// # Errors
///
/// Returns a JavaScript error value for invalid configuration or export
/// failures.
#[wasm_bindgen]
pub fn generate_openfoam(config_json: &str) -> Result<String, JsValue> {
    let configuration =
        configuration_from_json(config_json).map_err(|error| to_js_error(&error))?;
    export_openfoam(&configuration).map_err(|error| to_js_error(&error))
}

/// Generates the configured Gmsh ASCII mesh.
///
/// # Errors
///
/// Returns a JavaScript error value for invalid configuration or export
/// failures.
#[wasm_bindgen]
pub fn generate_gmsh(config_json: &str) -> Result<String, JsValue> {
    let configuration =
        configuration_from_json(config_json).map_err(|error| to_js_error(&error))?;
    export_gmsh(&configuration).map_err(|error| to_js_error(&error))
}

/// Generates an ANSYS Fluent legacy ASCII mesh.
///
/// # Errors
///
/// Returns a JavaScript error value for invalid configuration or export
/// failures.
#[wasm_bindgen]
pub fn generate_fluent(config_json: &str) -> Result<String, JsValue> {
    let configuration =
        configuration_from_json(config_json).map_err(|error| to_js_error(&error))?;
    export_fluent(&configuration).map_err(|error| to_js_error(&error))
}

/// Native-testable JSON validation adapter.
///
/// # Errors
///
/// Returns parsing or serialization errors from `mesh-core`.
pub fn validate_configuration_json(config_json: &str) -> Result<String, MeshError> {
    let configuration = configuration_from_json(config_json)?;
    let report = mesh_core::validate_configuration(&configuration);
    Ok(serde_json::to_string(&report)?)
}

/// Native-testable preview adapter.
///
/// # Errors
///
/// Returns parsing, validation, generation, or serialization errors.
pub fn generate_preview_json(config_json: &str, preview_level: &str) -> Result<String, MeshError> {
    let configuration = configuration_from_json(config_json)?;
    let level = parse_preview_level(preview_level)?;
    let preview = generate_mesh_preview(&configuration, level)?;
    Ok(serde_json::to_string(&preview)?)
}

fn parse_preview_level(value: &str) -> Result<PreviewLevel, MeshError> {
    match value {
        "GEOMETRY_ONLY" => Ok(PreviewLevel::GeometryOnly),
        "BLOCK_TOPOLOGY" => Ok(PreviewLevel::BlockTopology),
        "COARSE_MESH" => Ok(PreviewLevel::CoarseMesh),
        "FULL_MESH" => Ok(PreviewLevel::FullMesh),
        _ => Err(MeshError::ExportError(format!(
            "unknown preview level {value:?}"
        ))),
    }
}

fn numeric_limit(value: f64, label: &str) -> Result<usize, MeshError> {
    #[allow(clippy::cast_precision_loss)]
    let maximum = usize::MAX as f64;
    if !value.is_finite() || value < 1.0 || value.fract() != 0.0 || value > maximum {
        return Err(MeshError::ExportError(format!(
            "{label} budget must be a positive integer representable by this build"
        )));
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(value as usize)
}

fn to_js_value<T: Serialize>(value: &T) -> Result<JsValue, JsValue> {
    serde_wasm_bindgen::to_value(value).map_err(|error| JsValue::from_str(&error.to_string()))
}

fn to_js_error(error: &MeshError) -> JsValue {
    JsValue::from_str(&error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{generate_preview_json, validate_configuration_json};

    const CONFIGURATION: &str = include_str!("../../../examples/naca0012-les.json");

    #[test]
    fn validation_wrapper_returns_structured_json() {
        let json = validate_configuration_json(CONFIGURATION).expect("valid wrapper response");
        let value: serde_json::Value = serde_json::from_str(&json).expect("JSON response");
        assert_eq!(value["valid"], true);
        assert!(value["issues"].is_array());
    }

    #[test]
    fn preview_wrapper_returns_rust_generated_geometry() {
        let json =
            generate_preview_json(CONFIGURATION, "BLOCK_TOPOLOGY").expect("preview response");
        let value: serde_json::Value = serde_json::from_str(&json).expect("JSON response");
        assert_eq!(value["previewLevel"], "BLOCK_TOPOLOGY");
        assert_eq!(
            value["phase"],
            serde_json::Value::String("structured-cgrid-and-extrusion".to_owned())
        );
        assert_eq!(value["quality"]["blockCount"], 6);
    }

    #[test]
    fn unknown_preview_level_is_rejected() {
        assert!(generate_preview_json(CONFIGURATION, "ULTRA").is_err());
    }
}
