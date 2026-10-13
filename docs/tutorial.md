# Short tutorial

This tutorial uses the supplied example configurations. Commands are run from the repository root.

## 1. Start the visual workspace

Install dependencies and start the local application:

```bash
npm ci
npm run dev
```

Open the URL printed in the terminal. Choose a mesh intent, set the airfoil and flow conditions, then select **Generate mesh**. Use the preview controls to inspect the block topology, mesh lines, boundary patches, and quality summary before downloading a solver file.

The interface runs locally. Configuration and mesh data are not sent to a server.

## 2. Build the CLI

```bash
cargo build --release -p mesh-cli
mkdir -p examples/generated/tutorial
```

The tutorial uses `target/release/mesh-cli` so large exports run with compiler optimizations.

## 3. Generate a 2D RANS mesh

The NACA 2412 example is a wall-resolved RANS setup with `EMPTY` front and back patches:

```bash
target/release/mesh-cli validate examples/naca2412-rans.json
target/release/mesh-cli audit examples/naca2412-rans.json
target/release/mesh-cli generate examples/naca2412-rans.json \
  --output-dir examples/generated/tutorial/rans
```

This produces the outputs enabled in the JSON configuration, including:

- `system/blockMeshDict` for OpenFOAM
- `mesh.msh` for Gmsh
- `preview.json` for structured preview data

To create a Fluent mesh instead:

```bash
target/release/mesh-cli fluent examples/naca2412-rans.json \
  --output examples/generated/tutorial/rans-fluent.msh
```

If Gmsh is installed, view the generated grid with:

```bash
gmsh examples/generated/tutorial/rans/mesh.msh
```

## 4. Generate a periodic 3D WRLES starting mesh

The student example uses a NACA 0012, 0.1143 m chord, 25 m/s freestream, 5 degree angle of attack, and a periodic span. Its configured mesh remains below one million cells and points:

```bash
target/release/mesh-cli validate examples/naca0012-student-wrles.json
target/release/mesh-cli audit examples/naca0012-student-wrles.json
target/release/mesh-cli fluent examples/naca0012-student-wrles.json \
  --output examples/generated/tutorial/naca0012-wrles-fluent.msh
```

For large meshes, prefer the release CLI over browser export because the CLI streams Gmsh 2.2 and Fluent meshes to disk instead of retaining the full text file in browser memory.

The under-one-million preset is a starting point, not proof of LES adequacy. Check solved wall units, spanwise correlations, resolved turbulence content, time-step sensitivity, and grid sensitivity in the solver.

## 5. Adjust a case

Copy an example JSON file and edit these groups:

- `airfoil`: NACA code, chord, angle of attack, and trailing-edge type
- `flow`: velocity, density, and dynamic viscosity; Reynolds number is synchronized from these values and chord
- `meshIntent`: `RANS_WALL_RESOLVED`, `RANS_WALL_FUNCTION`, `WMLES`, or `WRLES`
- `mesh`: surface, wake, radial, and spanwise cell counts and grading
- `les`: target wall units, boundary-layer coverage, and radial growth limit
- `wake`: chord, freestream, blended, or custom alignment
- `extrusion`: span thickness, spanwise cells, and front/back patch behavior

Validate after every meaningful change:

```bash
target/release/mesh-cli validate path/to/config.json
target/release/mesh-cli audit path/to/config.json
```

The audit reports generated wall distance and predicted wall units. These predictions use the configured or imported wall-shear estimate; they are not solved CFD values.

## 6. Work within a cell limit

Ask the bounded planner for a configuration that respects both mesh requirements and a cell limit:

```bash
target/release/mesh-cli budget examples/naca0012-student-wrles.json \
  --max-cells 999000 \
  --max-points 999000 \
  --output examples/generated/tutorial/budgeted.json
```

If the requirements cannot fit, the command reports infeasibility instead of silently weakening wall-unit targets or shrinking the domain.

## 7. Recalibrate from a pilot solution

After a pilot CFD run, import wall-shear stress in pascals to update first-cell sizing. The repository includes synthetic CSV examples that show the required format.

For one conservative scalar value:

```bash
target/release/mesh-cli calibrate examples/naca0012-student-wrles.json \
  --wall-shear examples/pilot-wall-shear.csv \
  --output examples/generated/tutorial/calibrated.json
```

For a spatial upper/lower-surface profile:

```bash
target/release/mesh-cli profile examples/naca0012-student-wrles.json \
  --wall-shear examples/pilot-wall-shear-profile.csv \
  --output examples/generated/tutorial/profiled.json
```

Validate and audit the new configuration before exporting it. Imported solver data is provenance-checked so a profile cannot be reused silently after the airfoil or flow inputs change.

## 8. Optional target-tool verification

When Gmsh or OpenFOAM is installed, export and run its mesh check in a new directory:

```bash
target/release/mesh-cli verify examples/naca2412-rans.json \
  --solver gmsh \
  --output-dir examples/generated/tutorial/verify-gmsh
```

Verification logs and a machine-readable report remain in that directory. Fluent export is supported directly, but final import and mesh checks should still be performed in the Fluent version used for the simulation.
