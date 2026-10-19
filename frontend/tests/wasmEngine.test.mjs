import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

import init, {
  generate_fluent,
  generate_gmsh,
  generate_openfoam,
  generate_preview_at_level,
  import_wall_shear,
  import_wall_shear_profile,
  plan_cell_budget,
  validate_configuration,
} from '../src/wasm-pkg/mesh_wasm.js';

const wasmUrl = new URL('../src/wasm-pkg/mesh_wasm_bg.wasm', import.meta.url);
await init({ module_or_path: await readFile(wasmUrl) });

const exampleUrl = new URL(
  '../../examples/naca0012-les.json',
  import.meta.url,
);
const configuration = JSON.parse(await readFile(exampleUrl, 'utf8'));
configuration.airfoil.surfacePointCount = 41;
configuration.domain.upstreamRadiusChords = 4;
configuration.domain.upperRadiusChords = 4;
configuration.domain.lowerRadiusChords = 4;
configuration.domain.wakeLengthChords = 6;
configuration.domain.wakeHalfHeightChords = 1.5;
configuration.mesh.upperSurfaceCells = 6;
configuration.mesh.lowerSurfaceCells = 6;
configuration.mesh.leadingEdgeCells = 3;
configuration.mesh.wakeCells = 16;
configuration.mesh.radialCells = 64;
configuration.mesh.spanwiseCells = 1;
configuration.meshIntent = 'RANS_WALL_RESOLVED';
configuration.les.enabled = false;
configuration.les.referenceBoundaryLayerThicknessChords = 0.01;
configuration.les.minimumCellsAcrossBoundaryLayer = 16;
configuration.les.maxRadialGrowthRatio = 1.2;
configuration.les.estimateFirstCellHeight = false;
configuration.les.manualFirstCellHeight = 0.00012;
configuration.extrusion.frontBackPatchType = 'EMPTY';
configuration.boundaries.front.patchType = 'EMPTY';
configuration.boundaries.back.patchType = 'EMPTY';
const json = JSON.stringify(configuration);

test('WASM validates and previews through the real Rust engine', () => {
  const report = validate_configuration(json);
  const preview = generate_preview_at_level(json, 'COARSE_MESH');
  assert.equal(report.valid, true);
  assert.equal(preview.quality.valid, true);
  assert.equal(preview.wallResolution.wallFaceCount, 18);
  assert.ok(preview.wallResolution.minimumCentroidDistance > 0);
  assert.ok(preview.wallResolution.maximumNormalHeight >= preview.wallResolution.minimumNormalHeight);
  assert.equal(preview.quality.blockCount, 6);
  assert.ok(preview.meshLines.length > 0);
  assert.equal(preview.boundaryPatches.length, 5);
  assert.ok(
    preview.boundaryPatches.every((patch) => patch.segments.length > 0),
  );
});

test('WASM wall-shear import selects the maximum and rejects invalid rows', () => {
  const feedback = import_wall_shear('wallShearStressPa,x\n0,0\n8,1\n2,2', 'wallShearStressPa', 100);
  assert.equal(feedback.sampleCount, 3);
  assert.equal(feedback.selectedPa, 8);
  assert.throws(() => import_wall_shear('wallShearStressPa\n8\nNaN', 'wallShearStressPa', 100));
  const calibrated = structuredClone(configuration);
  calibrated.les.estimateFirstCellHeight = true;
  calibrated.les.solverWallShearStress = feedback.selectedPa;
  delete calibrated.les.manualFirstCellHeight;
  const report = validate_configuration(JSON.stringify(calibrated));
  assert.equal(typeof report.valid, 'boolean');
});

test('WASM exposes spatial feedback and bounded budget planning', () => {
  const csv = [
    'side,xOverChord,wallShearStressPa,signedWallShearStressPa,intermittency',
    'UPPER,0,1,1,0',
    'UPPER,1,2,-1,1',
    'LOWER,0,1,1,0',
    'LOWER,1,1,1,1',
  ].join('\n');
  const calibrated = import_wall_shear_profile(json, csv);
  assert.equal(calibrated.les.wallShearProfile.samples.length, 4);
  assert.equal(calibrated.les.solverWallShearStress, 2);
  assert.equal(calibrated.optimization.orthogonalizeWall, true);
  assert.equal(calibrated.optimization.targetCellCenterYPlus, true);

  const plan = plan_cell_budget(json, 1, 0);
  assert.equal(plan.feasible, false);
  assert.ok(plan.counts.hexCells > 1);
  assert.ok(plan.reasons.length > 0);
});

test('WASM export wrappers return readable deterministic formats', () => {
  const openfoam = generate_openfoam(json);
  const gmsh = generate_gmsh(json);
  const fluent = generate_fluent(json);
  assert.match(openfoam, /object\s+blockMeshDict;/);
  assert.equal((openfoam.match(/hex \(/g) ?? []).length, 6);
  assert.ok(gmsh.startsWith('$MeshFormat\n2.2 0 8\n$EndMeshFormat\n'));
  assert.ok(gmsh.includes('"airfoil_wall"'));
  assert.ok(
    fluent.startsWith(
      '(0 "afoil_gen ANSYS Fluent legacy ASCII mesh")\n',
    ),
  );
  assert.ok(fluent.includes('(2 2)\n'));
  assert.ok(fluent.includes(' wall airfoil_wall 1)())'));
});
