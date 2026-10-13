//! JSON preview export used by CLI generation bundles.

use crate::{
    config::MeshConfiguration,
    error::MeshError,
    mesh::generation::GeneratedCGrid,
    preview::{PreviewLevel, generate_mesh_preview, generate_mesh_preview_from_generated},
};

/// Generates pretty, deterministic preview JSON with a trailing newline.
///
/// # Errors
///
/// Returns generation/quality or JSON serialization errors.
pub fn export_preview_json(
    configuration: &MeshConfiguration,
    level: PreviewLevel,
) -> Result<String, MeshError> {
    let preview = generate_mesh_preview(configuration, level)?;
    let mut output = serde_json::to_string_pretty(&preview)?;
    output.push('\n');
    Ok(output)
}

/// Generates preview JSON from an existing validated mesh.
///
/// # Errors
///
/// Returns a JSON serialization error if formatting fails.
pub fn export_preview_json_from_generated(
    configuration: &MeshConfiguration,
    level: PreviewLevel,
    generated: &GeneratedCGrid,
) -> Result<String, MeshError> {
    let preview = generate_mesh_preview_from_generated(configuration, level, generated);
    let mut output = serde_json::to_string_pretty(&preview)?;
    output.push('\n');
    Ok(output)
}
