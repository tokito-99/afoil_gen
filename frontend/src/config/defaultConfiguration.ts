import type { MeshConfiguration } from '../types/mesh.js';

export const DEFAULT_CONFIGURATION: MeshConfiguration = {
  meshIntent: 'WRLES',
  airfoil: {
    nacaCode: '0012',
    chordLength: 1,
    angleOfAttackDeg: 0,
    finiteTrailingEdge: false,
    surfacePointCount: 161,
  },
  flow: {
    reynoldsNumber: 1_000_000,
    freestreamVelocity: 14.775_510_204_081_63,
    density: 1.225,
    dynamicViscosity: 0.000_018_1,
  },
  domain: {
    upstreamRadiusChords: 10,
    upperRadiusChords: 10,
    lowerRadiusChords: 10,
    wakeLengthChords: 20,
    wakeHalfHeightChords: 3,
  },
  mesh: {
    upperSurfaceCells: 160,
    lowerSurfaceCells: 160,
    leadingEdgeCells: 40,
    wakeCells: 240,
    radialCells: 120,
    spanwiseCells: 16,
    surfaceGrading: 1,
    radialGrading: 1.15,
    wakeGrading: 1.05,
    spanwiseGrading: 1,
  },
  les: {
    enabled: true,
    mode: 'WRLES',
    targetYPlus: 1,
    targetStreamwisePlus: 100,
    targetSpanwisePlus: 20,
    referenceBoundaryLayerThicknessChords: 0.02,
    minimumCellsAcrossBoundaryLayer: 30,
    maxRadialGrowthRatio: 1.1,
    estimateFirstCellHeight: true,
  },
  wake: {
    alignment: 'BLENDED',
    blendLengthChords: 2,
    customAngleDeg: 0,
  },
  extrusion: {
    enabled: true,
    spanwiseThickness: 0.01,
    frontBackPatchType: 'CYCLIC',
  },
  boundaries: {
    inlet: { name: 'inlet', patchType: 'PATCH' },
    outlet: { name: 'outlet', patchType: 'PATCH' },
    airfoil: { name: 'airfoil_wall', patchType: 'WALL' },
    upperFarfield: { name: 'upper_farfield', patchType: 'PATCH' },
    lowerFarfield: { name: 'lower_farfield', patchType: 'PATCH' },
    front: { name: 'front_plane', patchType: 'CYCLIC' },
    back: { name: 'back_plane', patchType: 'CYCLIC' },
  },
  output: {
    writeOpenfoam: true,
    writeGmsh: true,
    writeGeometryJson: true,
    gmshVersion: '2.2',
  },
  optimization: {
    targetCellCenterYPlus: false,
    orthogonalizeWall: false,
    smoothingIterations: 0,
  },
};

export function createDefaultConfiguration(): MeshConfiguration {
  return structuredClone(DEFAULT_CONFIGURATION);
}
