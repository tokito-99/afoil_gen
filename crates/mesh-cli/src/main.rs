use std::{
    fs::{self, File, OpenOptions},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    process::ExitCode,
};

use clap::{Parser, Subcommand, ValueEnum, ValueHint};
use mesh_core::{
    GeneratedCGrid, MeshError, PreviewLevel, config::GmshVersion, export_gmsh_from_generated,
    export_openfoam_from_generated, export_preview_json_from_generated, generate_cgrid,
    generate_mesh_preview, load_configuration, validate_configuration, write_fluent_streaming,
    write_gmsh_streaming,
};
use serde::Serialize;
mod verification;
mod workflows;

#[derive(Debug, Parser)]
#[command(
    name = "mesh-cli",
    version,
    about = "RANS/LES structured NACA C-grid engine for afoil_gen"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Save a safeguarded wall-normal/smoothed configuration (actual-mesh exports).
    Optimize {
        config: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long, default_value_t = 5)]
        iterations: usize,
        #[arg(long)]
        force: bool,
    },
    /// Import spatial wall shear and optional transition/separation signals.
    Profile {
        config: PathBuf,
        #[arg(long)]
        wall_shear: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long)]
        force: bool,
    },
    /// Plan counts within a cell/point budget without reducing CFD requirements.
    Budget {
        config: PathBuf,
        #[arg(long, default_value_t = 1_000_000)]
        max_cells: usize,
        #[arg(long)]
        max_points: Option<usize>,
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[arg(long)]
        force: bool,
    },
    /// Prepare independent grid, time-step and span study configurations.
    Study {
        config: PathBuf,
        #[arg(long)]
        output_dir: PathBuf,
        #[arg(long)]
        time_step: f64,
        #[arg(long, default_value_t = 1.5)]
        refinement_ratio: f64,
        #[arg(long)]
        max_cells: Option<usize>,
        #[arg(long)]
        force: bool,
    },
    /// Analyse real solver measurements from a prepared study manifest.
    AnalyzeStudy {
        manifest: PathBuf,
        #[arg(long)]
        results: PathBuf,
    },
    /// Export and check in an external solver in a new/empty isolated directory.
    Verify {
        config: PathBuf,
        #[arg(long, value_enum)]
        solver: verification::Solver,
        #[arg(long)]
        output_dir: PathBuf,
        #[arg(long)]
        executable: Option<PathBuf>,
        #[arg(long, default_value_t = 120)]
        timeout_seconds: u64,
    },
    /// Audit generated wall distances and predicted cell-center y+ (not solved y+).
    Audit {
        #[arg(value_hint = ValueHint::FilePath)]
        config: PathBuf,
    },
    /// Remesh from pilot-run wall-shear magnitudes in Pa; maximum by default.
    Calibrate {
        #[arg(value_hint = ValueHint::FilePath)]
        config: PathBuf,
        #[arg(long, value_hint = ValueHint::FilePath)]
        wall_shear: PathBuf,
        #[arg(long, default_value = "wallShearStressPa")]
        column: String,
        /// Nearest-rank percentile. Below 100 deliberately excludes high shear.
        #[arg(long, default_value_t = 100.0)]
        percentile: f64,
        #[arg(short, long, value_hint = ValueHint::FilePath)]
        output: PathBuf,
        #[arg(long)]
        force: bool,
    },
    /// Validate a JSON configuration and print a structured report.
    Validate {
        #[arg(value_hint = ValueHint::FilePath)]
        config: PathBuf,
    },
    /// Write structured C-grid topology, mesh lines, and quality as JSON.
    Preview {
        #[arg(value_hint = ValueHint::FilePath)]
        config: PathBuf,
        #[arg(short, long, value_hint = ValueHint::FilePath)]
        output: PathBuf,
        /// Visualization detail; exported mesh quality always uses the full mesh.
        #[arg(long, value_enum, default_value_t = CliPreviewLevel::CoarseMesh)]
        level: CliPreviewLevel,
        /// Permit replacing an existing output file.
        #[arg(long)]
        force: bool,
    },
    /// Write a native `OpenFOAM` blockMeshDict.
    Openfoam {
        #[arg(value_hint = ValueHint::FilePath)]
        config: PathBuf,
        #[arg(short, long, value_hint = ValueHint::FilePath)]
        output: PathBuf,
        /// Permit replacing an existing output file.
        #[arg(long)]
        force: bool,
    },
    /// Write a Gmsh ASCII mesh using the configured version.
    Gmsh {
        #[arg(value_hint = ValueHint::FilePath)]
        config: PathBuf,
        #[arg(short, long, value_hint = ValueHint::FilePath)]
        output: PathBuf,
        /// Permit replacing an existing output file.
        #[arg(long)]
        force: bool,
    },
    /// Write an ANSYS Fluent legacy ASCII mesh.
    Fluent {
        #[arg(value_hint = ValueHint::FilePath)]
        config: PathBuf,
        #[arg(short, long, value_hint = ValueHint::FilePath)]
        output: PathBuf,
        /// Permit replacing an existing output file.
        #[arg(long)]
        force: bool,
    },
    /// Generate every output enabled by the JSON configuration.
    Generate {
        #[arg(value_hint = ValueHint::FilePath)]
        config: PathBuf,
        #[arg(long, value_hint = ValueHint::DirPath)]
        output_dir: PathBuf,
        /// Permit replacing any existing generated files.
        #[arg(long)]
        force: bool,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliPreviewLevel {
    GeometryOnly,
    BlockTopology,
    CoarseMesh,
    FullMesh,
}

impl From<CliPreviewLevel> for PreviewLevel {
    fn from(value: CliPreviewLevel) -> Self {
        match value {
            CliPreviewLevel::GeometryOnly => Self::GeometryOnly,
            CliPreviewLevel::BlockTopology => Self::BlockTopology,
            CliPreviewLevel::CoarseMesh => Self::CoarseMesh,
            CliPreviewLevel::FullMesh => Self::FullMesh,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorOutput<'a> {
    valid: bool,
    error: &'a str,
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(exit_code) => exit_code,
        Err(error) => {
            print_error(&error);
            ExitCode::from(1)
        }
    }
}

#[allow(clippy::too_many_lines)] // Explicit clap-to-workflow dispatch.
fn run(cli: Cli) -> Result<ExitCode, MeshError> {
    match cli.command {
        Command::Optimize {
            config,
            output,
            iterations,
            force,
        } => workflows::optimize(&config, &output, iterations, force),
        Command::Profile {
            config,
            wall_shear,
            output,
            force,
        } => workflows::profile(&config, &wall_shear, &output, force),
        Command::Budget {
            config,
            max_cells,
            max_points,
            output,
            force,
        } => workflows::budget(&config, max_cells, max_points, output.as_deref(), force),
        Command::Study {
            config,
            output_dir,
            time_step,
            refinement_ratio,
            max_cells,
            force,
        } => workflows::study(
            &config,
            &output_dir,
            time_step,
            refinement_ratio,
            max_cells,
            force,
        ),
        Command::AnalyzeStudy { manifest, results } => workflows::analyze(&manifest, &results),
        Command::Verify {
            config,
            solver,
            output_dir,
            executable,
            timeout_seconds,
        } => verification::verify(
            &config,
            solver,
            &output_dir,
            executable.as_deref(),
            timeout_seconds,
        ),
        Command::Audit { config } => {
            let configuration = load_configuration(config)?;
            let preview = generate_mesh_preview(&configuration, PreviewLevel::GeometryOnly)?;
            let audit = preview.wall_resolution.ok_or_else(|| {
                MeshError::QualityError("wall audit failed; inspect preview warnings".to_owned())
            })?;
            println!("{}", serde_json::to_string_pretty(&audit)?);
            Ok(ExitCode::SUCCESS)
        }
        Command::Calibrate {
            config,
            wall_shear,
            column,
            percentile,
            output,
            force,
        } => {
            let original = load_configuration(config)?;
            let csv = fs::read_to_string(wall_shear)?;
            let result = mesh_core::calibrate_configuration(&original, &csv, &column, percentile)?;
            if !result.validation.valid {
                println!("{}", serde_json::to_string_pretty(&result.validation)?);
                return Ok(ExitCode::from(2));
            }
            // Do not write a candidate whose actual mesh cannot be generated.
            let preview = generate_mesh_preview(&result.configuration, PreviewLevel::GeometryOnly)?;
            let mut contents = serde_json::to_string_pretty(&result.configuration)?;
            contents.push('\n');
            write_output(&output, contents.as_bytes(), force)?;
            println!("{}", serde_json::to_string_pretty(&result.feedback)?);
            eprintln!("Calibrated configuration written: {}", output.display());
            for warning in &preview.warnings {
                eprintln!("warning: {}", warning.message);
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Validate { config } => run_validate(&config),
        Command::Preview {
            config,
            output,
            level,
            force,
        } => run_preview(&config, &output, level, force),
        Command::Openfoam {
            config,
            output,
            force,
        } => run_openfoam(&config, &output, force),
        Command::Gmsh {
            config,
            output,
            force,
        } => run_gmsh(&config, &output, force),
        Command::Fluent {
            config,
            output,
            force,
        } => run_fluent(&config, &output, force),
        Command::Generate {
            config,
            output_dir,
            force,
        } => run_generate(&config, &output_dir, force),
    }
}

fn run_validate(config: &Path) -> Result<ExitCode, MeshError> {
    let configuration = load_configuration(config)?;
    let report = validate_configuration(&configuration);
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(if report.valid {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(2)
    })
}

fn run_preview(
    config: &Path,
    output: &Path,
    level: CliPreviewLevel,
    force: bool,
) -> Result<ExitCode, MeshError> {
    let configuration = load_configuration(config)?;
    let preview = generate_mesh_preview(&configuration, level.into())?;
    let mut contents = serde_json::to_string_pretty(&preview)?;
    contents.push('\n');
    write_output(output, contents.as_bytes(), force)?;

    eprintln!(
        "preview written: {} points, {} cells, {} blocks",
        preview.quality.point_count, preview.quality.cell_count, preview.quality.block_count
    );
    if let Some(wall_spacing) = preview.wall_spacing {
        eprintln!("first-cell height: {:.12e}", wall_spacing.first_cell_height);
    }
    eprintln!(
        "maximum radial growth ratio: {:.10}; minimum cell area: {:.12e}",
        preview.quality.maximum_radial_growth_ratio,
        preview.quality.minimum_cell_area.unwrap_or_default()
    );
    if let Some(quality_3d) = preview.quality_3d {
        eprintln!(
            "extruded mesh: {} points, {} hex cells; minimum volume: {:.12e}",
            quality_3d.point_count,
            quality_3d.cell_count,
            quality_3d.minimum_cell_volume.unwrap_or_default()
        );
    }
    Ok(ExitCode::SUCCESS)
}

fn run_openfoam(config: &Path, output: &Path, force: bool) -> Result<ExitCode, MeshError> {
    let configuration = load_configuration(config)?;
    let generated = generate_cgrid(&configuration)?;
    let dictionary = export_openfoam_from_generated(&configuration, &generated)?;
    write_output(output, dictionary.as_bytes(), force)?;
    eprintln!("OpenFOAM blockMeshDict written: {}", output.display());
    print_mesh_statistics(&generated);
    Ok(ExitCode::SUCCESS)
}

fn run_gmsh(config: &Path, output: &Path, force: bool) -> Result<ExitCode, MeshError> {
    let configuration = load_configuration(config)?;
    if configuration.extrusion.enabled && configuration.output.gmsh_version == GmshVersion::V2_2 {
        let file = open_output(output, force)?;
        let mut writer = BufWriter::new(file);
        let summary = write_gmsh_streaming(&configuration, &mut writer)?;
        writer.flush()?;
        eprintln!("Gmsh mesh written: {}", output.display());
        eprintln!(
            "streamed mesh statistics: {} points, {} hex cells, {} boundary quads; 2D maximum aspect ratio {:.6e}",
            summary.point_count,
            summary.hex_cell_count,
            summary.boundary_element_count,
            summary.quality_2d.maximum_aspect_ratio
        );
        return Ok(ExitCode::SUCCESS);
    }

    let generated = generate_cgrid(&configuration)?;
    let mesh = export_gmsh_from_generated(&configuration, &generated)?;
    write_output(output, mesh.as_bytes(), force)?;
    eprintln!("Gmsh mesh written: {}", output.display());
    print_mesh_statistics(&generated);
    Ok(ExitCode::SUCCESS)
}

fn run_fluent(config: &Path, output: &Path, force: bool) -> Result<ExitCode, MeshError> {
    let configuration = load_configuration(config)?;
    let file = open_output(output, force)?;
    let mut writer = BufWriter::new(file);
    let summary = write_fluent_streaming(&configuration, &mut writer)?;
    writer.flush()?;
    eprintln!("ANSYS Fluent mesh written: {}", output.display());
    eprintln!(
        "streamed mesh statistics: {}D, {} points, {} cells, {} faces; 2D maximum aspect ratio {:.6e}",
        summary.dimension,
        summary.point_count,
        summary.cell_count,
        summary.face_count,
        summary.quality_2d.maximum_aspect_ratio
    );
    Ok(ExitCode::SUCCESS)
}

fn run_generate(config: &Path, output_dir: &Path, force: bool) -> Result<ExitCode, MeshError> {
    let configuration = load_configuration(config)?;
    let generated = generate_cgrid(&configuration)?;
    let mut outputs = Vec::new();
    if configuration.output.write_openfoam {
        outputs.push((
            output_dir.join("system/blockMeshDict"),
            export_openfoam_from_generated(&configuration, &generated)?,
        ));
    }
    if configuration.output.write_gmsh {
        outputs.push((
            output_dir.join("mesh.msh"),
            export_gmsh_from_generated(&configuration, &generated)?,
        ));
    }
    if configuration.output.write_geometry_json {
        outputs.push((
            output_dir.join("preview.json"),
            export_preview_json_from_generated(
                &configuration,
                PreviewLevel::CoarseMesh,
                &generated,
            )?,
        ));
    }
    if outputs.is_empty() {
        return Err(MeshError::ExportError(
            "configuration disables every output".to_owned(),
        ));
    }
    write_outputs(&outputs, force)?;
    for (path, _) in &outputs {
        eprintln!("generated: {}", path.display());
    }
    print_mesh_statistics(&generated);
    Ok(ExitCode::SUCCESS)
}

fn print_mesh_statistics(generated: &GeneratedCGrid) {
    eprintln!(
        "mesh statistics: {} points, {} quads, {} blocks; maximum aspect ratio {:.6e}",
        generated.quality_2d.point_count,
        generated.quality_2d.cell_count,
        generated.quality_2d.block_count,
        generated.quality_2d.maximum_aspect_ratio
    );
    for warning in &generated.quality_2d.warnings {
        eprintln!("mesh warning: {warning}");
    }
    if let Some(quality) = &generated.quality_3d {
        eprintln!(
            "extruded statistics: {} points, {} hex cells; maximum aspect ratio {:.6e}",
            quality.point_count, quality.cell_count, quality.maximum_aspect_ratio
        );
        for warning in &quality.warnings {
            eprintln!("extruded mesh warning: {warning}");
        }
    }
}

fn write_outputs(outputs: &[(PathBuf, String)], force: bool) -> Result<(), MeshError> {
    if !force {
        for (path, _) in outputs {
            if path.exists() {
                return Err(MeshError::ExportError(format!(
                    "{} already exists; pass --force to replace generated files",
                    path.display()
                )));
            }
        }
    }
    for (path, contents) in outputs {
        write_output(path, contents.as_bytes(), force)?;
    }
    Ok(())
}

fn write_output(path: &Path, contents: &[u8], force: bool) -> Result<(), MeshError> {
    let mut file = open_output(path, force)?;
    file.write_all(contents)?;
    Ok(())
}

fn open_output(path: &Path, force: bool) -> Result<File, MeshError> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }

    let mut options = OpenOptions::new();
    options.write(true);
    if force {
        options.create(true).truncate(true);
    } else {
        options.create_new(true);
    }

    options.open(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            MeshError::ExportError(format!(
                "{} already exists; pass --force to replace it",
                path.display()
            ))
        } else {
            MeshError::Io(error)
        }
    })
}

fn print_error(error: &MeshError) {
    if let MeshError::InvalidConfiguration(issues) = error {
        let report = mesh_core::ValidationReport::new(issues.clone());
        if let Ok(json) = serde_json::to_string_pretty(&report) {
            eprintln!("{json}");
            return;
        }
    }

    let output = ErrorOutput {
        valid: false,
        error: &error.to_string(),
    };
    if let Ok(json) = serde_json::to_string_pretty(&output) {
        eprintln!("{json}");
    } else {
        eprintln!("{{\"valid\":false,\"error\":\"failed to serialize error output\"}}");
    }
}
