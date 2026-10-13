import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

import {
  DEFAULT_CONFIGURATION,
  createDefaultConfiguration,
} from '../dist-test/config/defaultConfiguration.js';
import {
  isMeshConfiguration,
  normalizeMeshConfiguration,
} from '../dist-test/utils/configuration.js';
import {
  assessBrowserFluent,
  assessBrowserGmsh,
} from '../dist-test/utils/exportSizing.js';
import { estimateCellCount } from '../dist-test/utils/format.js';

test('frontend default configuration stays byte-compatible with the example', async () => {
  const exampleUrl = new URL(
    '../../examples/naca0012-les.json',
    import.meta.url,
  );
  const example = JSON.parse(await readFile(exampleUrl, 'utf8'));
  assert.deepEqual(DEFAULT_CONFIGURATION, example);
});

test('default sharp-TE cell estimate matches the six-block Rust mesh', () => {
  const mesh = DEFAULT_CONFIGURATION.mesh;
  const estimate = estimateCellCount(
    DEFAULT_CONFIGURATION.airfoil.finiteTrailingEdge,
    mesh.leadingEdgeCells,
    mesh.upperSurfaceCells,
    mesh.lowerSurfaceCells,
    mesh.wakeCells,
    mesh.radialCells,
    mesh.spanwiseCells,
  );
  assert.deepEqual(estimate, { quads: 105_600, hexes: 1_689_600 });
});

test('configuration cloning and load guard do not share mutable state', () => {
  const first = createDefaultConfiguration();
  const second = createDefaultConfiguration();
  first.airfoil.nacaCode = '2412';
  assert.equal(second.airfoil.nacaCode, '0012');
  assert.equal(isMeshConfiguration(second), true);
  assert.equal(isMeshConfiguration({ airfoil: {} }), false);
});

test('legacy LES configurations infer WRLES and receive new defaults', () => {
  const legacy = createDefaultConfiguration();
  delete legacy.meshIntent;
  delete legacy.wake;
  delete legacy.les.mode;

  const normalized = normalizeMeshConfiguration(legacy);
  assert.equal(normalized.meshIntent, 'WRLES');
  assert.equal(normalized.les.mode, 'WRLES');
  assert.equal(normalized.wake.alignment, 'BLENDED');
});

test('large Gmsh exports are kept out of the browser string path', () => {
  const preview = {
    quality: { pointCount: 250_000, cellCount: 240_000, blockCount: 6 },
    quality3d: {
      pointCount: 10_000_000,
      cellCount: 9_500_000,
      blockCount: 6,
    },
  };
  const assessment = assessBrowserGmsh(preview);
  assert.equal(assessment.browserSafe, false);
  assert.equal(assessment.crossSectionBrowserSafe, true);
  assert.ok(assessment.estimatedBytes > 1_000_000_000);
  const fluentAssessment = assessBrowserFluent(preview, true);
  assert.equal(fluentAssessment.browserSafe, false);
  assert.equal(fluentAssessment.dimension, 3);
});
