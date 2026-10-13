use std::{collections::BTreeMap, fmt::Write as _, path::PathBuf};

use mesh_core::{
    export_gmsh_from_generated, export_openfoam_from_generated, generate_cgrid, load_configuration,
};

const PATCH_NAMES: [&str; 7] = [
    "inlet",
    "outlet",
    "airfoil_wall",
    "upper_farfield",
    "lower_farfield",
    "front_plane",
    "back_plane",
];

fn example_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(name)
}

#[test]
fn default_phase_7_8_exports_match_the_topology_fixture() {
    let configuration =
        load_configuration(example_path("naca0012-les.json")).expect("example must load");
    let generated = generate_cgrid(&configuration).expect("default mesh must generate");
    let openfoam = export_openfoam_from_generated(&configuration, &generated)
        .expect("OpenFOAM export must succeed");
    let gmsh =
        export_gmsh_from_generated(&configuration, &generated).expect("Gmsh export must succeed");

    assert_eq!(
        topology_snapshot(&openfoam, &gmsh),
        include_str!("../../../tests/fixtures/phase_7_8-topology.snapshot")
    );
    assert_eq!(
        openfoam,
        export_openfoam_from_generated(&configuration, &generated)
            .expect("repeated OpenFOAM export")
    );
    assert_eq!(
        gmsh,
        export_gmsh_from_generated(&configuration, &generated).expect("repeated Gmsh export")
    );
    for output in [&openfoam, &gmsh] {
        assert!(!output.contains("NaN"));
        assert!(!output.contains("inf"));
    }
}

fn topology_snapshot(openfoam: &str, gmsh: &str) -> String {
    let mut snapshot = String::new();
    let vertex_count = openfoam
        .lines()
        .filter(|line| {
            line.trim_start().starts_with('(')
                && line
                    .rsplit_once("// ")
                    .is_some_and(|(_, id)| id.parse::<usize>().is_ok())
        })
        .count();
    let block_lines = openfoam
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            (trimmed.starts_with("// Block ") && trimmed.contains(':'))
                || trimmed.starts_with("hex ")
        })
        .collect::<Vec<_>>();
    let spline_count = openfoam
        .lines()
        .filter(|line| line.trim_start().starts_with("spline "))
        .count();
    writeln!(snapshot, "blockMeshDict").expect("writing to a String cannot fail");
    writeln!(snapshot, "vertices {vertex_count}").expect("writing to a String cannot fail");
    writeln!(snapshot, "blocks {}", block_lines.len() / 2)
        .expect("writing to a String cannot fail");
    writeln!(snapshot, "splines {spline_count}").expect("writing to a String cannot fail");
    for line in block_lines {
        writeln!(snapshot, "{}", line.trim()).expect("writing to a String cannot fail");
    }
    for line in openfoam.lines() {
        let trimmed = line.trim();
        if PATCH_NAMES.contains(&trimmed) {
            writeln!(snapshot, "patch {trimmed}").expect("writing to a String cannot fail");
        }
    }

    let node_count = count_after_marker(gmsh, "$Nodes");
    let element_count = count_after_marker(gmsh, "$Elements");
    let (element_types, physical_groups) = element_statistics(gmsh);
    writeln!(snapshot, "gmsh2.2").expect("writing to a String cannot fail");
    writeln!(snapshot, "nodes {node_count}").expect("writing to a String cannot fail");
    writeln!(snapshot, "elements {element_count}").expect("writing to a String cannot fail");
    for (element_type, count) in element_types {
        writeln!(snapshot, "elementType {element_type} {count}")
            .expect("writing to a String cannot fail");
    }
    for (physical_group, count) in physical_groups {
        writeln!(snapshot, "physicalElements {physical_group} {count}")
            .expect("writing to a String cannot fail");
    }
    snapshot.push_str(section(gmsh, "$PhysicalNames", "$EndPhysicalNames"));
    snapshot
}

fn count_after_marker(contents: &str, marker: &str) -> usize {
    let mut lines = contents.lines();
    lines
        .find(|line| *line == marker)
        .and_then(|_| lines.next())
        .and_then(|line| line.parse().ok())
        .expect("export section must contain a numeric count")
}

fn element_statistics(contents: &str) -> (BTreeMap<usize, usize>, BTreeMap<usize, usize>) {
    let mut element_types = BTreeMap::new();
    let mut physical_groups = BTreeMap::new();
    let mut in_elements = false;
    for line in contents.lines() {
        if line == "$Elements" {
            in_elements = true;
            continue;
        }
        if line == "$EndElements" {
            break;
        }
        if !in_elements || !line.contains(' ') {
            continue;
        }
        let mut fields = line.split_ascii_whitespace();
        let _element_id = fields.next().expect("element ID");
        let element_type = fields
            .next()
            .expect("element type")
            .parse::<usize>()
            .expect("numeric element type");
        let tag_count = fields
            .next()
            .expect("tag count")
            .parse::<usize>()
            .expect("numeric tag count");
        assert_eq!(tag_count, 2);
        let physical_group = fields
            .next()
            .expect("physical group")
            .parse::<usize>()
            .expect("numeric physical group");
        *element_types.entry(element_type).or_default() += 1;
        *physical_groups.entry(physical_group).or_default() += 1;
    }
    (element_types, physical_groups)
}

fn section<'a>(contents: &'a str, start: &str, end: &str) -> &'a str {
    let start_index = contents.find(start).expect("section start");
    let end_index = contents[start_index..]
        .find(end)
        .map(|offset| start_index + offset + end.len())
        .expect("section end");
    contents
        .get(start_index..=end_index)
        .expect("valid section boundaries")
}
