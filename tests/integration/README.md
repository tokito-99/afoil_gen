# External integration checks

The ordinary Rust tests validate exporters and the isolated verification
workflow without requiring commercial or system CFD tools. A mock Gmsh
round-trip locks count, physical-name, overwrite, and report behavior.

When installed, run the real target adapters with `mesh-cli verify --solver`
and select `gmsh`, `openfoam`, or `fluent`. Gmsh uses `-check` and a format
round trip. OpenFOAM
uses `gmshToFoam`, restores configured patch types with `createPatch`, and runs
`checkMesh -allGeometry -allTopology`. Fluent imports the native mesh and runs
its mesh-check journal. These external checks stay optional because solver
availability and licensing vary across CI environments.

Every verification run uses a new or empty output directory, preserves logs,
and emits `verification.json`. A passed mesh check confirms interoperability
for that tool invocation, not solution accuracy or LES adequacy.
