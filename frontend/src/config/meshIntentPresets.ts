import type { MeshConfiguration, MeshIntent, PatchType } from '../types/mesh.js';

interface IntentPreset {
  leadingEdgeCells: number;
  minimumWallLayerCells: number;
  radialCells: number;
  lowerSurfaceCells: number;
  maxRadialGrowthRatio: number;
  spanwiseCells: number;
  targetSpanwisePlus: number;
  targetStreamwisePlus: number;
  targetYPlus: number;
  upperSurfaceCells: number;
  wakeCells: number;
}

const PRESETS: Record<MeshIntent, IntentPreset> = {
  RANS_WALL_RESOLVED: {
    leadingEdgeCells: 28,
    upperSurfaceCells: 110,
    lowerSurfaceCells: 110,
    wakeCells: 160,
    radialCells: 96,
    spanwiseCells: 1,
    targetYPlus: 1,
    targetStreamwisePlus: 400,
    targetSpanwisePlus: 100,
    minimumWallLayerCells: 32,
    maxRadialGrowthRatio: 1.15,
  },
  RANS_WALL_FUNCTION: {
    leadingEdgeCells: 20,
    upperSurfaceCells: 80,
    lowerSurfaceCells: 80,
    wakeCells: 120,
    radialCells: 72,
    spanwiseCells: 1,
    targetYPlus: 50,
    targetStreamwisePlus: 500,
    targetSpanwisePlus: 150,
    minimumWallLayerCells: 24,
    maxRadialGrowthRatio: 1.2,
  },
  WMLES: {
    leadingEdgeCells: 32,
    upperSurfaceCells: 120,
    lowerSurfaceCells: 120,
    wakeCells: 180,
    radialCells: 96,
    spanwiseCells: 12,
    targetYPlus: 30,
    targetStreamwisePlus: 400,
    targetSpanwisePlus: 100,
    minimumWallLayerCells: 24,
    maxRadialGrowthRatio: 1.15,
  },
  WRLES: {
    leadingEdgeCells: 40,
    upperSurfaceCells: 160,
    lowerSurfaceCells: 160,
    wakeCells: 240,
    radialCells: 120,
    spanwiseCells: 16,
    targetYPlus: 1,
    targetStreamwisePlus: 100,
    targetSpanwisePlus: 20,
    minimumWallLayerCells: 30,
    maxRadialGrowthRatio: 1.1,
  },
};

export function applyMeshIntentPreset(
  configuration: MeshConfiguration,
  meshIntent: MeshIntent,
): MeshConfiguration {
  const preset = PRESETS[meshIntent];
  const isLes = meshIntent === 'WMLES' || meshIntent === 'WRLES';
  const frontBackPatchType = isLes ? 'CYCLIC' : 'EMPTY';
  const patchType: PatchType = isLes ? 'CYCLIC' : 'EMPTY';

  return {
    ...configuration,
    meshIntent,
    mesh: {
      ...configuration.mesh,
      leadingEdgeCells: preset.leadingEdgeCells,
      upperSurfaceCells: preset.upperSurfaceCells,
      lowerSurfaceCells: preset.lowerSurfaceCells,
      wakeCells: preset.wakeCells,
      radialCells: preset.radialCells,
      spanwiseCells: preset.spanwiseCells,
    },
    les: {
      ...configuration.les,
      enabled: isLes,
      mode: meshIntent === 'WMLES' ? 'WMLES' : 'WRLES',
      targetYPlus: preset.targetYPlus,
      targetStreamwisePlus: preset.targetStreamwisePlus,
      targetSpanwisePlus: preset.targetSpanwisePlus,
      minimumCellsAcrossBoundaryLayer: preset.minimumWallLayerCells,
      maxRadialGrowthRatio: preset.maxRadialGrowthRatio,
    },
    extrusion: {
      ...configuration.extrusion,
      enabled: true,
      frontBackPatchType,
    },
    boundaries: {
      ...configuration.boundaries,
      front: { ...configuration.boundaries.front, patchType },
      back: { ...configuration.boundaries.back, patchType },
    },
  };
}

/**
 * Constrained WRLES starter for the ANSYS Student one-million-cell ceiling.
 *
 * Sharp trailing edge: 947,700 hexes.
 * Finite trailing edge: 908,730 hexes.
 */
export function applyStudentWrlesPreset(
  configuration: MeshConfiguration,
): MeshConfiguration {
  const wrles = applyMeshIntentPreset(configuration, 'WRLES');
  const finiteTrailingEdge = configuration.airfoil.finiteTrailingEdge;
  return {
    ...wrles,
    domain: {
      ...wrles.domain,
      wakeLengthChords: 10,
    },
    mesh: {
      ...wrles.mesh,
      leadingEdgeCells: 22,
      upperSurfaceCells: 73,
      lowerSurfaceCells: 73,
      wakeCells: finiteTrailingEdge ? 83 : 100,
      radialCells: 90,
      spanwiseCells: finiteTrailingEdge ? 23 : 27,
      wakeGrading: 1,
    },
    les: {
      ...wrles.les,
      targetYPlus: 1,
      targetStreamwisePlus: 110,
      targetSpanwisePlus: 40,
      referenceBoundaryLayerThicknessChords: 0.02,
      minimumCellsAcrossBoundaryLayer: 30,
      maxRadialGrowthRatio: 1.12,
    },
    extrusion: {
      ...wrles.extrusion,
      spanwiseThickness: 0.1 * configuration.airfoil.chordLength,
    },
  };
}

export function studentWrlesCellCount(finiteTrailingEdge: boolean): number {
  const wakeCells = finiteTrailingEdge ? 83 : 100;
  const radialCells = 90;
  const spanwiseCells = finiteTrailingEdge ? 23 : 27;
  const streamwise =
    2 * 22 + 2 * 73 + (finiteTrailingEdge ? 3 : 2) * wakeCells;
  return streamwise * radialCells * spanwiseCells;
}

export function meshIntentLabel(intent: MeshIntent): string {
  switch (intent) {
    case 'RANS_WALL_RESOLVED':
      return 'RANS · low y+';
    case 'RANS_WALL_FUNCTION':
      return 'RANS · wall function';
    case 'WMLES':
      return 'WMLES';
    case 'WRLES':
      return 'WRLES';
  }
}
