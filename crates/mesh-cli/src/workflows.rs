//! File-oriented workflows; numerical decisions stay in mesh-core.
use super::write_output;
use mesh_core::{MeshError, PreviewLevel, generate_mesh_preview, load_configuration};
use std::{fs, path::Path, process::ExitCode};

pub fn optimize(
    config: &Path,
    output: &Path,
    iterations: usize,
    force: bool,
) -> Result<ExitCode, MeshError> {
    let mut configuration = load_configuration(config)?;
    configuration.optimization.orthogonalize_wall = true;
    configuration.optimization.target_cell_center_y_plus = true;
    configuration.optimization.smoothing_iterations = iterations;
    let preview = generate_mesh_preview(&configuration, PreviewLevel::GeometryOnly)?;
    write_output(
        output,
        serde_json::to_string_pretty(&configuration)?.as_bytes(),
        force,
    )?;
    println!("{}", serde_json::to_string_pretty(&preview.optimization)?);
    eprintln!(
        "Saved optimized configuration. Export the actual Gmsh/Fluent coordinates, not blockMeshDict."
    );
    Ok(ExitCode::SUCCESS)
}

pub fn profile(
    config: &Path,
    csv: &Path,
    output: &Path,
    force: bool,
) -> Result<ExitCode, MeshError> {
    let configuration = mesh_core::import_wall_shear_profile(
        &load_configuration(config)?,
        &fs::read_to_string(csv)?,
    )?;
    let preview = generate_mesh_preview(&configuration, PreviewLevel::GeometryOnly)?;
    write_output(
        output,
        serde_json::to_string_pretty(&configuration)?.as_bytes(),
        force,
    )?;
    println!(
        "{}",
        serde_json::to_string_pretty(&preview.wall_resolution)?
    );
    Ok(ExitCode::SUCCESS)
}

pub fn budget(
    config: &Path,
    max_cells: usize,
    max_points: Option<usize>,
    output: Option<&Path>,
    force: bool,
) -> Result<ExitCode, MeshError> {
    let plan = mesh_core::plan_cell_budget(
        &load_configuration(config)?,
        &mesh_core::BudgetConstraints {
            max_cells,
            max_points,
        },
    )?;
    if let (Some(configuration), Some(output)) = (&plan.configuration, output) {
        write_output(
            output,
            serde_json::to_string_pretty(configuration)?.as_bytes(),
            force,
        )?;
    }
    println!("{}", serde_json::to_string_pretty(&plan)?);
    Ok(if plan.feasible {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(2)
    })
}

pub fn study(
    config: &Path,
    directory: &Path,
    time_step: f64,
    ratio: f64,
    max_cells: Option<usize>,
    force: bool,
) -> Result<ExitCode, MeshError> {
    let plan = mesh_core::studies::prepare_study(
        &load_configuration(config)?,
        time_step,
        ratio,
        max_cells,
    )?;
    let mut outputs = vec![(
        directory.join("study.json"),
        serde_json::to_string_pretty(&plan)?,
    )];
    outputs.push((
        directory.join("results.csv"),
        "case,metric,value,uncertainty\n".to_owned(),
    ));
    for case in &plan.cases {
        outputs.push((
            directory.join(&case.name).join("config.json"),
            serde_json::to_string_pretty(&case.configuration)?,
        ));
    }
    // Measurements are user data, never regenerate them even with --force.
    outputs.retain(|(path, _)| path != &directory.join("results.csv") || !path.exists());
    for (path, _) in &outputs {
        if path.exists() && !force {
            return Err(MeshError::ExportError(format!(
                "{} exists; use a new directory or --force",
                path.display()
            )));
        }
    }
    for (path, contents) in outputs {
        write_output(&path, contents.as_bytes(), force)?;
    }
    println!("{}", serde_json::to_string_pretty(&plan)?);
    Ok(ExitCode::SUCCESS)
}

pub fn analyze(manifest: &Path, results: &Path) -> Result<ExitCode, MeshError> {
    let plan = serde_json::from_str(&fs::read_to_string(manifest)?)?;
    let analysis = mesh_core::studies::analyze_study(&plan, &fs::read_to_string(results)?)?;
    let complete = analysis.missing_cases.is_empty() && analysis.series.iter().all(|s| s.complete);
    println!("{}", serde_json::to_string_pretty(&analysis)?);
    Ok(if complete {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(2)
    })
}
