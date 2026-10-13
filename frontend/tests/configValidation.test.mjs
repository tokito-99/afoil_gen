import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

import {
  applyMeshIntentPreset,
  applyStudentWrlesPreset,
  studentWrlesCellCount,
} from '../dist-test/config/meshIntentPresets.js';
import {
  calculateReynoldsNumber,
  synchronizeReynoldsNumber,
} from '../dist-test/utils/flow.js';
import {
  estimateWallSpacing,
  estimateWallSpacingFromWallShear,
  sanitizeCfdName,
  validateConfiguration,
} from '../dist-test/validation/configValidation.js';

async function exampleConfiguration() {
  const path = new URL('../../examples/naca0012-les.json', import.meta.url);
  return JSON.parse(await readFile(path, 'utf8'));
}

test('default LES example is valid and clearly warned as an estimate', async () => {
  const configuration = await exampleConfiguration();
  const report = validateConfiguration(configuration);
  assert.equal(report.valid, true);
  assert.ok(
    report.issues.some(
      (issue) =>
        issue.code === 'Y_PLUS_IS_AN_ESTIMATE' &&
        issue.severity === 'WARNING',
    ),
  );
});

test('NACA and boundary validation return stable error codes', async () => {
  const configuration = await exampleConfiguration();
  configuration.airfoil.nacaCode = '0000';
  configuration.boundaries.outlet.name = configuration.boundaries.inlet.name;
  const report = validateConfiguration(configuration);
  assert.equal(report.valid, false);
  assert.ok(report.issues.some((issue) => issue.code === 'INVALID_NACA_CODE'));
  assert.ok(
    report.issues.some((issue) => issue.code === 'DUPLICATE_BOUNDARY_NAME'),
  );
});

test('CFD name sanitizer is deterministic and separate from validation', () => {
  assert.equal(sanitizeCfdName(' 1 bad--name.txt '), 'patch_1_bad_name_txt');
  assert.equal(sanitizeCfdName(''), 'patch');
  assert.equal(sanitizeCfdName('already_safe'), 'already_safe');
});

test('wall estimate is positive and rejects invalid inputs', () => {
  const estimate = estimateWallSpacing(1_000_000, 30, 1.225, 0.0000181, 1);
  assert.ok(estimate);
  assert.ok(estimate.firstCellHeight > 0);
  assert.equal(estimateWallSpacing(0, 30, 1.225, 0.0000181, 1), undefined);
});

test('solver wall shear feedback controls the remeshed first layer', () => {
  const estimate = estimateWallSpacingFromWallShear(
    8,
    25,
    1.225,
    0.0000181,
    1,
  );
  assert.ok(estimate);
  const expected = 0.0000181 / (1.225 * Math.sqrt(8 / 1.225));
  assert.ok(Math.abs(estimate.firstCellHeight - expected) < 1e-15);
});

test('front and back patch classifications must match extrusion mode', async () => {
  const configuration = await exampleConfiguration();
  configuration.extrusion.frontBackPatchType = 'SYMMETRY';
  configuration.mesh.spanwiseCells = 3;
  let report = validateConfiguration(configuration);
  assert.ok(
    report.issues.some(
      (issue) => issue.code === 'FRONT_BACK_PATCH_TYPE_MISMATCH',
    ),
  );

  configuration.boundaries.front.patchType = 'SYMMETRY_PLANE';
  configuration.boundaries.back.patchType = 'SYMMETRY_PLANE';
  report = validateConfiguration(configuration);
  assert.ok(
    !report.issues.some(
      (issue) => issue.code === 'FRONT_BACK_PATCH_TYPE_MISMATCH',
    ),
  );
});

test('RANS and LES intent presets switch dimensional requirements coherently', async () => {
  const configuration = await exampleConfiguration();
  const rans = applyMeshIntentPreset(configuration, 'RANS_WALL_RESOLVED');
  assert.equal(rans.meshIntent, 'RANS_WALL_RESOLVED');
  assert.equal(rans.mesh.spanwiseCells, 1);
  assert.equal(rans.extrusion.frontBackPatchType, 'EMPTY');
  assert.equal(rans.boundaries.front.patchType, 'EMPTY');
  assert.equal(validateConfiguration(rans).valid, true);

  const les = applyMeshIntentPreset(rans, 'WMLES');
  assert.equal(les.meshIntent, 'WMLES');
  assert.ok(les.mesh.spanwiseCells > 1);
  assert.equal(les.extrusion.frontBackPatchType, 'CYCLIC');
  assert.equal(les.boundaries.front.patchType, 'CYCLIC');
  assert.equal(validateConfiguration(les).valid, true);
});

test('Reynolds number follows chord and physical flow properties', async () => {
  const configuration = await exampleConfiguration();
  configuration.airfoil.chordLength = 0.1143;
  configuration.flow.freestreamVelocity = 25;
  const synchronized = synchronizeReynoldsNumber(configuration);
  const expected = calculateReynoldsNumber(
    0.1143,
    25,
    configuration.flow.density,
    configuration.flow.dynamicViscosity,
  );
  assert.equal(synchronized.flow.reynoldsNumber, expected);
  assert.ok(Math.abs(expected - 193_400) < 100);
  assert.ok(
    !validateConfiguration(synchronized).issues.some(
      (issue) => issue.code === 'INCONSISTENT_REYNOLDS_NUMBER',
    ),
  );
});

test('ANSYS Student WRLES preset stays below one million cells', async () => {
  const configuration = await exampleConfiguration();
  const sharp = applyStudentWrlesPreset(configuration);
  assert.equal(studentWrlesCellCount(false), 947_700);
  assert.equal(sharp.mesh.spanwiseCells, 27);
  assert.equal(sharp.extrusion.spanwiseThickness, sharp.airfoil.chordLength * 0.1);
  assert.equal(sharp.extrusion.frontBackPatchType, 'CYCLIC');

  const finite = applyStudentWrlesPreset({
    ...configuration,
    airfoil: { ...configuration.airfoil, finiteTrailingEdge: true },
  });
  assert.equal(studentWrlesCellCount(true), 908_730);
  assert.ok(studentWrlesCellCount(finite.airfoil.finiteTrailingEdge) < 1_000_000);
});
