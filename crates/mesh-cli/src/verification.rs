//! Isolated external-tool checks with timeouts and explicit unavailable states.
use clap::ValueEnum;
use mesh_core::{
    MeshConfiguration, MeshError, load_configuration, mesh_counts, write_fluent_streaming,
    write_gmsh_streaming,
};
use serde::Serialize;
use std::{
    collections::BTreeSet,
    fmt::Write as _,
    fs::{self, File},
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Command, ExitCode, Stdio},
    thread,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Copy, ValueEnum, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Solver {
    Gmsh,
    Openfoam,
    Fluent,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct VerificationReport {
    solver: Solver,
    status: String,
    work_directory: PathBuf,
    logs: Vec<String>,
    evidence: Vec<String>,
    warnings: Vec<String>,
}

pub fn verify(
    config: &Path,
    solver: Solver,
    directory: &Path,
    executable: Option<&Path>,
    timeout_seconds: u64,
) -> Result<ExitCode, MeshError> {
    if timeout_seconds == 0 || timeout_seconds > 3600 {
        return Err(fail("timeout must be in 1..=3600 seconds"));
    }
    let configuration = load_configuration(config)?;
    let validation = mesh_core::validate_configuration(&configuration);
    if !validation.valid {
        return Err(MeshError::InvalidConfiguration(validation.issues));
    }
    if directory.exists() && fs::read_dir(directory)?.next().is_some() {
        return Err(fail(
            "verification output directory must be new or empty; existing solver data will not be overwritten",
        ));
    }
    fs::create_dir_all(directory)?;
    let directory = fs::canonicalize(directory)?;
    let executable = executable
        .map(|p| {
            if p.components().count() > 1 {
                fs::canonicalize(p)
            } else {
                Ok(p.to_path_buf())
            }
        })
        .transpose()?;
    let mut report = VerificationReport {
        solver,
        status: "FAILED".to_owned(),
        work_directory: directory.clone(),
        logs: vec![],
        evidence: vec![],
        warnings: vec![
            "A mesh import/check is not a flow validation or an LES convergence study.".to_owned(),
        ],
    };
    match solver {
        Solver::Gmsh => gmsh(
            &configuration,
            &directory,
            executable.as_deref(),
            timeout_seconds,
            &mut report,
        )?,
        Solver::Openfoam => openfoam(
            &configuration,
            &directory,
            executable.as_deref(),
            timeout_seconds,
            &mut report,
        )?,
        Solver::Fluent => fluent(
            &configuration,
            &directory,
            executable.as_deref(),
            timeout_seconds,
            &mut report,
        )?,
    }
    let json = serde_json::to_string_pretty(&report)?;
    super::write_output(&directory.join("verification.json"), json.as_bytes(), false)?;
    println!("{json}");
    Ok(if report.status == "PASSED" {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(2)
    })
}

fn gmsh(
    configuration: &MeshConfiguration,
    directory: &Path,
    executable: Option<&Path>,
    timeout: u64,
    report: &mut VerificationReport,
) -> Result<(), MeshError> {
    let mut writer = File::create(directory.join("input.msh"))?;
    write_gmsh_streaming(configuration, &mut writer)?;
    writer.flush()?;
    if !run(
        executable.unwrap_or_else(|| Path::new("gmsh")),
        &[
            "input.msh",
            "-check",
            "-0",
            "-format",
            "msh2",
            "-o",
            "checked.msh",
            "-v",
            "4",
            "-nopopup",
        ],
        directory,
        "gmsh.log",
        timeout,
        report,
    )? {
        return Ok(());
    }
    let log = read_log(&directory.join("gmsh.log"))?;
    if has_error(&log) || !log.contains("Done reading") || !log.contains("Done writing") {
        report.evidence.push(
            "Gmsh did not complete its read/check/write sequence; inspect gmsh.log.".to_owned(),
        );
        return Ok(());
    }
    let (nodes, quads, hexes, names) = read_gmsh2_counts(&directory.join("checked.msh"))?;
    let expected = mesh_counts(configuration)?;
    let cells_match = if configuration.extrusion.enabled {
        hexes == expected.hex_cells
    } else {
        hexes == 0 && quads == expected.quad_cells
    };
    let required_names = configuration
        .boundaries
        .ordered()
        .into_iter()
        .filter(|(field, _)| {
            configuration.extrusion.enabled
                || (!field.ends_with("front") && !field.ends_with("back"))
        })
        .map(|(_, b)| &b.name);
    if nodes != expected.points
        || !cells_match
        || required_names.into_iter().any(|name| !names.contains(name))
    {
        report.evidence.push("Round-trip node/cell counts or named physical boundaries differ from the generated mesh.".to_owned());
        return Ok(());
    }
    "PASSED".clone_into(&mut report.status);
    report.evidence.push(format!("Gmsh coherence check completed; round-trip preserved {nodes} nodes, {quads} quads, {hexes} hexes and required physical names."));
    report.warnings.push("Gmsh physical groups alone do not establish solver boundary conditions or periodic pairing.".to_owned());
    Ok(())
}

fn openfoam(
    configuration: &MeshConfiguration,
    directory: &Path,
    executable: Option<&Path>,
    timeout: u64,
    report: &mut VerificationReport,
) -> Result<(), MeshError> {
    if !configuration.extrusion.enabled {
        return Err(fail(
            "OpenFOAM conversion requires extrusion, including a one-cell EMPTY slab for 2D",
        ));
    }
    let mut converted = configuration.clone();
    converted.output.gmsh_version = mesh_core::config::GmshVersion::V2_2;
    let mut writer = File::create(directory.join("input.msh"))?;
    write_gmsh_streaming(&converted, &mut writer)?;
    writer.flush()?;
    fs::create_dir_all(directory.join("system"))?;
    fs::create_dir_all(directory.join("constant"))?;
    super::write_output(
        &directory.join("system/controlDict"),
        CONTROL_DICT.as_bytes(),
        false,
    )?;
    super::write_output(
        &directory.join("system/createPatchDict"),
        create_patch_dict(configuration)?.as_bytes(),
        false,
    )?;
    if !run(
        executable.unwrap_or_else(|| Path::new("gmshToFoam")),
        &["input.msh", "-case", "."],
        directory,
        "conversion.log",
        timeout,
        report,
    )? {
        return Ok(());
    }
    if has_error(&read_log(&directory.join("conversion.log"))?) {
        return Ok(());
    }
    if !run(
        Path::new("createPatch"),
        &["-overwrite", "-case", "."],
        directory,
        "patches.log",
        timeout,
        report,
    )? {
        return Ok(());
    }
    if has_error(&read_log(&directory.join("patches.log"))?) {
        return Ok(());
    }
    if !run(
        Path::new("checkMesh"),
        &["-allGeometry", "-allTopology", "-case", "."],
        directory,
        "checkMesh.log",
        timeout,
        report,
    )? {
        return Ok(());
    }
    let log = read_log(&directory.join("checkMesh.log"))?;
    if !has_error(&log) && log.contains("Mesh OK.") {
        "PASSED".clone_into(&mut report.status);
        report.evidence.push("Actual coordinate mesh converted with gmshToFoam, patch types restored with createPatch, and checkMesh reported Mesh OK.".to_owned());
    } else {
        report
            .evidence
            .push("checkMesh did not report Mesh OK; inspect the full solver logs.".to_owned());
    }
    Ok(())
}

fn fluent(
    configuration: &MeshConfiguration,
    directory: &Path,
    executable: Option<&Path>,
    timeout: u64,
    report: &mut VerificationReport,
) -> Result<(), MeshError> {
    let mut writer = File::create(directory.join("input-fluent.msh"))?;
    let summary = write_fluent_streaming(configuration, &mut writer)?;
    writer.flush()?;
    super::write_output(&directory.join("check.jou"),b"/file/read-mesh input-fluent.msh\n/mesh/check\n(display \"AFOIL_GEN_CHECK_COMPLETE\\n\")\n/exit yes\n",false)?;
    let dimension = if configuration.extrusion.enabled
        && configuration.extrusion.front_back_patch_type
            != mesh_core::config::FrontBackPatchType::Empty
    {
        "3ddp"
    } else {
        "2ddp"
    };
    if !run(
        executable.unwrap_or_else(|| Path::new("fluent")),
        &[dimension, "-g", "-t1", "-i", "check.jou"],
        directory,
        "fluent.log",
        timeout,
        report,
    )? {
        return Ok(());
    }
    let log = read_log(&directory.join("fluent.log"))?;
    let minimum_volume = log
        .lines()
        .filter(|line| line.to_lowercase().contains("minimum volume"))
        .filter_map(|line| {
            line.split_once('=')
                .and_then(|(_, value)| value.split_whitespace().next())
                .and_then(|value| value.parse::<f64>().ok())
        })
        .next_back();
    if !has_error(&log)
        && log.contains("AFOIL_GEN_CHECK_COMPLETE")
        && minimum_volume.is_some_and(|v| v.is_finite() && v > 0.0)
    {
        "PASSED".clone_into(&mut report.status);
        report.evidence.push(format!(
            "Fluent imported and checked the native mesh with positive minimum volume; exported {} cells and {} points.",
            summary.cell_count, summary.point_count
        ));
    } else {
        "INCONCLUSIVE".clone_into(&mut report.status);
        report.evidence.push("No unambiguous positive-volume completed Fluent mesh check was found; inspect the journal/log in your Fluent version.".to_owned());
    }
    report.warnings.push("Fluent TUI/log formats are version-dependent. Solver field values and turbulence models still require a separate case setup.".to_owned());
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run(
    program: &Path,
    args: &[&str],
    directory: &Path,
    log_name: &str,
    timeout: u64,
    report: &mut VerificationReport,
) -> Result<bool, MeshError> {
    report.logs.push(log_name.to_owned());
    let output = File::create(directory.join(log_name))?;
    let started = Instant::now();
    let mut child = match Command::new(program)
        .args(args)
        .current_dir(directory)
        .stdin(Stdio::null())
        .stdout(Stdio::from(output.try_clone()?))
        .stderr(Stdio::from(output))
        .spawn()
    {
        Ok(child) => child,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            "UNAVAILABLE".clone_into(&mut report.status);
            report.evidence.push(format!(
                "Executable {} is not installed or not on PATH; this check was not run.",
                program.display()
            ));
            return Ok(false);
        }
        Err(error) => return Err(error.into()),
    };
    loop {
        if let Some(status) = child.try_wait()? {
            if status.success() {
                return Ok(true);
            }
            "FAILED".clone_into(&mut report.status);
            report.evidence.push(format!(
                "{} exited with {status}; inspect {log_name}",
                program.display()
            ));
            return Ok(false);
        }
        if started.elapsed() >= Duration::from_secs(timeout) {
            child.kill()?;
            child.wait()?;
            "TIMED_OUT".clone_into(&mut report.status);
            report.evidence.push(format!(
                "{} exceeded {timeout} seconds and was stopped",
                program.display()
            ));
            return Ok(false);
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn read_log(path: &Path) -> Result<String, MeshError> {
    if fs::metadata(path)?.len() > 16 * 1024 * 1024 {
        return Err(fail(
            "solver log exceeds 16 MB; inspect it directly instead of reporting an automated pass",
        ));
    }
    let mut output = String::new();
    File::open(path)?
        .take(16 * 1024 * 1024)
        .read_to_string(&mut output)?;
    Ok(output)
}
fn has_error(log: &str) -> bool {
    let lower = log.to_lowercase();
    lower.contains("error:")
        || lower.contains("fatal error")
        || lower.contains("failed mesh checks")
}

type GmshCounts = (usize, usize, usize, BTreeSet<String>);
fn read_gmsh2_counts(path: &Path) -> Result<GmshCounts, MeshError> {
    let mut lines = BufReader::new(File::open(path)?).lines();
    let (mut nodes, mut quads, mut hexes) = (0, 0, 0);
    let mut names = BTreeSet::new();
    while let Some(line) = lines.next() {
        match line?.trim() {
            "$Nodes" => {
                nodes = lines
                    .next()
                    .ok_or_else(|| fail("missing node count"))??
                    .trim()
                    .parse()
                    .map_err(|_| fail("invalid node count"))?;
            }
            "$PhysicalNames" => {
                let count: usize = lines
                    .next()
                    .ok_or_else(|| fail("missing physical count"))??
                    .trim()
                    .parse()
                    .map_err(|_| fail("invalid physical count"))?;
                for _ in 0..count {
                    let line = lines
                        .next()
                        .ok_or_else(|| fail("missing physical name"))??;
                    let name = line
                        .split('"')
                        .nth(1)
                        .ok_or_else(|| fail("invalid physical name"))?;
                    names.insert(name.to_owned());
                }
            }
            "$Elements" => {
                let count: usize = lines
                    .next()
                    .ok_or_else(|| fail("missing element count"))??
                    .trim()
                    .parse()
                    .map_err(|_| fail("invalid element count"))?;
                for _ in 0..count {
                    let line = lines.next().ok_or_else(|| fail("missing element"))??;
                    match line.split_whitespace().nth(1) {
                        Some("3") => quads += 1,
                        Some("5") => hexes += 1,
                        Some("1" | "15") => {}
                        _ => return Err(fail("round-trip introduced unsupported element types")),
                    }
                }
            }
            _ => {}
        }
    }
    Ok((nodes, quads, hexes, names))
}

fn create_patch_dict(configuration: &MeshConfiguration) -> Result<String, MeshError> {
    use mesh_core::config::PatchType;
    let mut output="FoamFile {version 2.0; format ascii; class dictionary; object createPatchDict;}\npointSync true;\npatches (\n".to_owned();
    for (field, boundary) in configuration.boundaries.ordered() {
        let patch = match boundary.patch_type {
            PatchType::Patch => "patch",
            PatchType::Wall => "wall",
            PatchType::Empty => "empty",
            PatchType::SymmetryPlane => "symmetryPlane",
            PatchType::Cyclic => "cyclic",
        };
        let pairing = if boundary.patch_type == PatchType::Cyclic {
            let (neighbor, sign) = if field.ends_with("front") {
                (&configuration.boundaries.back.name, 1.0)
            } else if field.ends_with("back") {
                (&configuration.boundaries.front.name, -1.0)
            } else {
                return Err(fail("side cyclic patches have no defined OpenFOAM pairing"));
            };
            format!(
                "neighbourPatch {neighbor}; transform translational; separationVector (0 0 {});",
                sign * configuration.extrusion.spanwise_thickness
            )
        } else {
            String::new()
        };
        writeln!(
            &mut output,
            "{{ name {}; patchInfo {{type {patch}; {pairing}}} constructFrom patches; patches ({}); }}",
            boundary.name, boundary.name
        )
        .map_err(|error| fail(&error.to_string()))?;
    }
    output.push_str(");\n");
    Ok(output)
}
const CONTROL_DICT: &str = "FoamFile {version 2.0; format ascii; class dictionary; object controlDict;}\napplication checkMesh; startFrom startTime; startTime 0; stopAt endTime; endTime 1; deltaT 1; writeControl timeStep; writeInterval 1; writeFormat ascii; writePrecision 17; writeCompression off; timeFormat general; timePrecision 6; runTimeModifiable false;\n";
fn fail(message: &str) -> MeshError {
    MeshError::ExportError(message.to_owned())
}
