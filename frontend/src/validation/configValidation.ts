import type {
  BoundaryConfiguration,
  LesSizingEstimate,
  MeshConfiguration,
  ValidationIssue,
  ValidationReport,
  WallSpacingEstimate,
} from '../types/mesh.js';
import { calculateReynoldsNumber } from '../utils/flow.js';

const CFD_NAME_PATTERN = /^[A-Za-z_][A-Za-z0-9_]*$/;
const NACA_PATTERN = /^[0-9]{4}$/;
const RESERVED_CFD_NAMES = new Set([
  'FoamFile',
  'boundary',
  'vertices',
  'blocks',
  'edges',
  'mergePatchPairs',
  'defaultFaces',
]);

type BoundaryKey = keyof BoundaryConfiguration;

const BOUNDARY_KEYS: readonly BoundaryKey[] = [
  'inlet',
  'outlet',
  'airfoil',
  'upperFarfield',
  'lowerFarfield',
  'front',
  'back',
];

export function sanitizeCfdName(value: string): string {
  const replaced = value
    .trim()
    .replace(/[^A-Za-z0-9_]+/g, '_')
    .replace(/_+/g, '_');

  if (/^[0-9]/.test(replaced)) {
    return `patch_${replaced}`;
  }
  return replaced || 'patch';
}

export function estimateWallSpacing(
  reynoldsNumber: number,
  freestreamVelocity: number,
  density: number,
  dynamicViscosity: number,
  targetYPlus: number,
): WallSpacingEstimate | undefined {
  const values = [
    reynoldsNumber,
    freestreamVelocity,
    density,
    dynamicViscosity,
    targetYPlus,
  ];
  if (values.some((value) => !isPositive(value))) {
    return undefined;
  }

  const skinFrictionCoefficient = 0.058 * reynoldsNumber ** -0.2;
  const wallShearStress =
    0.5 * skinFrictionCoefficient * density * freestreamVelocity ** 2;
  const frictionVelocity = Math.sqrt(wallShearStress / density);
  const firstCellHeight =
    (targetYPlus * dynamicViscosity) / (density * frictionVelocity);

  return {
    skinFrictionCoefficient,
    wallShearStress,
    frictionVelocity,
    firstCellHeight,
  };
}

export function estimateWallSpacingFromWallShear(
  wallShearStress: number,
  freestreamVelocity: number,
  density: number,
  dynamicViscosity: number,
  targetYPlus: number,
): WallSpacingEstimate | undefined {
  const values = [
    wallShearStress,
    freestreamVelocity,
    density,
    dynamicViscosity,
    targetYPlus,
  ];
  if (values.some((value) => !isPositive(value))) {
    return undefined;
  }

  const skinFrictionCoefficient =
    (2 * wallShearStress) / (density * freestreamVelocity ** 2);
  const frictionVelocity = Math.sqrt(wallShearStress / density);
  const firstCellHeight =
    (targetYPlus * dynamicViscosity) / (density * frictionVelocity);
  return {
    skinFrictionCoefficient,
    wallShearStress,
    frictionVelocity,
    firstCellHeight,
  };
}

function resolvedWallSpacing(
  configuration: MeshConfiguration,
  targetYPlus: number,
): WallSpacingEstimate | undefined {
  const { flow, les } = configuration;
  const spacingTarget = configuration.optimization?.targetCellCenterYPlus === true
    ? 2 * targetYPlus
    : targetYPlus;
  if (les.solverWallShearStress !== undefined) {
    return estimateWallSpacingFromWallShear(
      les.solverWallShearStress,
      flow.freestreamVelocity,
      flow.density,
      flow.dynamicViscosity,
      spacingTarget,
    );
  }
  return estimateWallSpacing(
    calculateReynoldsNumber(
      configuration.airfoil.chordLength,
      flow.freestreamVelocity,
      flow.density,
      flow.dynamicViscosity,
    ),
    flow.freestreamVelocity,
    flow.density,
    flow.dynamicViscosity,
    spacingTarget,
  );
}

export function estimateLesSizing(
  configuration: MeshConfiguration,
): LesSizingEstimate | undefined {
  const { airfoil, extrusion, flow, les, mesh } = configuration;
  const propertyReynoldsNumber = calculateReynoldsNumber(
    airfoil.chordLength,
    flow.freestreamVelocity,
    flow.density,
    flow.dynamicViscosity,
  );
  const unitEstimate = resolvedWallSpacing(configuration, 1);
  const firstCellHeight = les.estimateFirstCellHeight
    ? resolvedWallSpacing(configuration, les.targetYPlus)?.firstCellHeight
    : les.manualFirstCellHeight;
  if (
    unitEstimate === undefined ||
    firstCellHeight === undefined ||
    !isPositive(firstCellHeight) ||
    !isPositive(airfoil.chordLength) ||
    !isPositive(extrusion.spanwiseThickness) ||
    !Number.isInteger(mesh.spanwiseCells) ||
    mesh.spanwiseCells < 1 ||
    !isPositive(mesh.spanwiseGrading) ||
    !isPositive(les.targetStreamwisePlus) ||
    !isPositive(les.targetSpanwisePlus)
  ) {
    return undefined;
  }

  const viscousLength = unitEstimate.firstCellHeight;
  const targetStreamwiseSpacing = les.targetStreamwisePlus * viscousLength;
  const targetSpanwiseSpacing = les.targetSpanwisePlus * viscousLength;
  // The live editor uses conservative chord fractions. A generated preview
  // replaces these with exact Rust-computed NACA surface arc lengths.
  const leadingLength = 0.25 * airfoil.chordLength;
  const aftSurfaceLength = 0.85 * airfoil.chordLength;
  const maximumSurfaceStreamwiseSpacing = Math.max(
    maximumGradedCellSize(
      leadingLength,
      mesh.leadingEdgeCells,
      mesh.surfaceGrading,
    ),
    maximumGradedCellSize(
      aftSurfaceLength,
      mesh.upperSurfaceCells,
      mesh.surfaceGrading,
    ),
    maximumGradedCellSize(
      aftSurfaceLength,
      mesh.lowerSurfaceCells,
      mesh.surfaceGrading,
    ),
  );
  const referenceBoundaryLayerThickness =
    airfoil.chordLength * les.referenceBoundaryLayerThicknessChords;
  const radialExtent =
    airfoil.chordLength *
    Math.min(
      configuration.domain.upperRadiusChords,
      configuration.domain.lowerRadiusChords,
    );
  const layeredRatios = solveLayeredRatios(
    firstCellHeight,
    radialExtent,
    mesh.radialCells,
    referenceBoundaryLayerThickness,
    les.minimumCellsAcrossBoundaryLayer,
    les.maxRadialGrowthRatio,
  );
  const radialCellsWithinBoundaryLayer =
    layeredRatios === undefined ? 0 : layeredRatios.wallCells;
  const maximumSpanwiseSpacing = maximumGradedCellSize(
    extrusion.spanwiseThickness,
    mesh.spanwiseCells,
    mesh.spanwiseGrading,
  );
  return {
    mode: les.mode,
    propertyReynoldsNumber,
    reynoldsMismatchPercent: 0,
    viscousLength,
    firstCellHeight,
    targetStreamwiseSpacing,
    maximumSurfaceStreamwiseSpacing,
    estimatedMaximumSurfaceStreamwisePlus:
      maximumSurfaceStreamwiseSpacing / viscousLength,
    recommendedLeadingEdgeCells: recommendedCellCount(
      leadingLength,
      targetStreamwiseSpacing,
      mesh.surfaceGrading,
    ),
    recommendedUpperSurfaceCells: recommendedCellCount(
      aftSurfaceLength,
      targetStreamwiseSpacing,
      mesh.surfaceGrading,
    ),
    recommendedLowerSurfaceCells: recommendedCellCount(
      aftSurfaceLength,
      targetStreamwiseSpacing,
      mesh.surfaceGrading,
    ),
    targetSpanwiseSpacing,
    maximumSpanwiseSpacing,
    estimatedMaximumSpanwisePlus: maximumSpanwiseSpacing / viscousLength,
    recommendedSpanwiseCells: recommendedCellCount(
      extrusion.spanwiseThickness,
      targetSpanwiseSpacing,
      mesh.spanwiseGrading,
    ),
    spanwiseExtentChords:
      extrusion.spanwiseThickness / airfoil.chordLength,
    referenceBoundaryLayerThickness,
    radialCellsWithinBoundaryLayer,
    periodicThreeDimensional:
      extrusion.enabled &&
      extrusion.frontBackPatchType === 'CYCLIC' &&
      mesh.spanwiseCells > 1,
  };
}

export function validateConfiguration(
  configuration: MeshConfiguration,
): ValidationReport {
  const issues: ValidationIssue[] = [];
  validateAirfoil(configuration, issues);
  validateFlow(configuration, issues);
  validateDomain(configuration, issues);
  validateWake(configuration, issues);
  validateMesh(configuration, issues);
  validateOptimization(configuration, issues);
  validateLes(configuration, issues);
  validateExtrusion(configuration, issues);
  validateBoundaries(configuration, issues);

  return {
    valid: !issues.some((issue) => issue.severity === 'ERROR'),
    issues,
  };
}

function validateOptimization(
  configuration: MeshConfiguration,
  issues: ValidationIssue[],
): void {
  const smoothingIterations =
    configuration.optimization?.smoothingIterations ?? 0;
  requireIntegerAtLeast(
    issues,
    'optimization.smoothingIterations',
    smoothingIterations,
    0,
    'INVALID_SMOOTHING_ITERATIONS',
  );
  if (smoothingIterations > 50) {
    error(
      issues,
      'optimization.smoothingIterations',
      'TOO_MANY_SMOOTHING_ITERATIONS',
      'Smoothing passes must not exceed 50',
    );
  }
  const profile = configuration.les.wallShearProfile;
  if (profile === undefined) {
    return;
  }
  const reference = profile.reference;
  const airfoil = configuration.airfoil;
  const flow = configuration.flow;
  const provenanceMatches =
    reference.airfoil.nacaCode === airfoil.nacaCode &&
    reference.airfoil.chordLength === airfoil.chordLength &&
    reference.airfoil.angleOfAttackDeg === airfoil.angleOfAttackDeg &&
    reference.airfoil.finiteTrailingEdge === airfoil.finiteTrailingEdge &&
    reference.flow.freestreamVelocity === flow.freestreamVelocity &&
    reference.flow.density === flow.density &&
    reference.flow.dynamicViscosity === flow.dynamicViscosity;
  if (!provenanceMatches) {
    error(
      issues,
      'les.wallShearProfile',
      'INVALID_WALL_PROFILE',
      'Spatial wall data no longer matches this airfoil and flow; import a matching pilot profile again',
    );
  }
  const maximumShear = profile.samples.reduce(
    (maximum, sample) => Math.max(maximum, sample.wallShearStressPa),
    0,
  );
  if (
    !configuration.les.estimateFirstCellHeight ||
    configuration.les.solverWallShearStress === undefined ||
    Math.abs(configuration.les.solverWallShearStress - maximumShear) >
      1e-9 * Math.max(maximumShear, 1)
  ) {
    error(
      issues,
      'les.wallShearProfile',
      'PROFILE_REFERENCE_MISMATCH',
      'Spatial profile mode requires estimated spacing and its maximum wall shear',
    );
  }
  if (configuration.optimization?.orthogonalizeWall !== true) {
    error(
      issues,
      'optimization.orthogonalizeWall',
      'PROFILE_REQUIRES_WALL_OPTIMIZATION',
      'Enable wall-normal adjustment for spatial feedback',
    );
  }
}

function validateAirfoil(
  configuration: MeshConfiguration,
  issues: ValidationIssue[],
): void {
  const code = configuration.airfoil.nacaCode;
  if (!NACA_PATTERN.test(code)) {
    error(
      issues,
      'airfoil.nacaCode',
      'INVALID_NACA_CODE',
      'NACA code must contain exactly four ASCII digits',
    );
  } else {
    const maximumCamberDigit = Number.parseInt(code.slice(0, 1), 10);
    const camberPositionDigit = Number.parseInt(code.slice(1, 2), 10);
    const thicknessDigits = Number.parseInt(code.slice(2), 10);
    if (thicknessDigits === 0) {
      error(
        issues,
        'airfoil.nacaCode',
        'INVALID_NACA_CODE',
        'NACA thickness digits must be greater than zero',
      );
    }
    if (maximumCamberDigit > 0 && camberPositionDigit === 0) {
      error(
        issues,
        'airfoil.nacaCode',
        'INVALID_NACA_CODE',
        'A cambered airfoil requires a nonzero camber position',
      );
    }
  }

  requirePositive(issues, 'airfoil.chordLength', configuration.airfoil.chordLength);
  requireFinite(
    issues,
    'airfoil.angleOfAttackDeg',
    configuration.airfoil.angleOfAttackDeg,
  );
  if (
    !Number.isInteger(configuration.airfoil.surfacePointCount) ||
    configuration.airfoil.surfacePointCount < 41
  ) {
    error(
      issues,
      'airfoil.surfacePointCount',
      'TOO_FEW_SURFACE_POINTS',
      'Surface point count must be an integer of at least 41',
    );
  }
}

function validateFlow(
  configuration: MeshConfiguration,
  issues: ValidationIssue[],
): void {
  requirePositive(
    issues,
    'flow.freestreamVelocity',
    configuration.flow.freestreamVelocity,
  );
  requirePositive(issues, 'flow.density', configuration.flow.density);
  requirePositive(
    issues,
    'flow.dynamicViscosity',
    configuration.flow.dynamicViscosity,
  );
}

function validateDomain(
  configuration: MeshConfiguration,
  issues: ValidationIssue[],
): void {
  requireGreaterThan(
    issues,
    'domain.upstreamRadiusChords',
    configuration.domain.upstreamRadiusChords,
    1,
  );
  requireGreaterThan(
    issues,
    'domain.upperRadiusChords',
    configuration.domain.upperRadiusChords,
    1,
  );
  requireGreaterThan(
    issues,
    'domain.lowerRadiusChords',
    configuration.domain.lowerRadiusChords,
    1,
  );
  requireGreaterThan(
    issues,
    'domain.wakeLengthChords',
    configuration.domain.wakeLengthChords,
    1,
  );
  requirePositive(
    issues,
    'domain.wakeHalfHeightChords',
    configuration.domain.wakeHalfHeightChords,
  );
}

function validateWake(
  configuration: MeshConfiguration,
  issues: ValidationIssue[],
): void {
  if (configuration.wake.alignment === 'BLENDED') {
    requirePositive(
      issues,
      'wake.blendLengthChords',
      configuration.wake.blendLengthChords,
    );
  } else {
    requireFinite(
      issues,
      'wake.blendLengthChords',
      configuration.wake.blendLengthChords,
    );
  }
  requireFinite(
    issues,
    'wake.customAngleDeg',
    configuration.wake.customAngleDeg,
  );
  const effectiveAngle =
    configuration.wake.alignment === 'CUSTOM'
      ? configuration.wake.customAngleDeg
      : configuration.wake.alignment === 'FREESTREAM'
        ? 0
        : -configuration.airfoil.angleOfAttackDeg;
  if (Number.isFinite(effectiveAngle) && Math.abs(effectiveAngle) >= 80) {
    error(
      issues,
      'wake.customAngleDeg',
      'WAKE_ANGLE_TOO_STEEP',
      'Wake alignment angle must remain between -80 and 80 degrees',
    );
  }
}

function validateMesh(
  configuration: MeshConfiguration,
  issues: ValidationIssue[],
): void {
  for (const [field, count] of [
    ['mesh.upperSurfaceCells', configuration.mesh.upperSurfaceCells],
    ['mesh.lowerSurfaceCells', configuration.mesh.lowerSurfaceCells],
    ['mesh.leadingEdgeCells', configuration.mesh.leadingEdgeCells],
  ] as const) {
    requireIntegerAtLeast(issues, field, count, 1, 'CELL_COUNT_TOO_SMALL');
  }
  requireIntegerAtLeast(
    issues,
    'mesh.wakeCells',
    configuration.mesh.wakeCells,
    4,
    'WAKE_CELL_COUNT_TOO_SMALL',
  );
  requireIntegerAtLeast(
    issues,
    'mesh.radialCells',
    configuration.mesh.radialCells,
    10,
    'RADIAL_CELL_COUNT_TOO_SMALL',
  );
  requireIntegerAtLeast(
    issues,
    'mesh.spanwiseCells',
    configuration.mesh.spanwiseCells,
    1,
    'SPANWISE_CELL_COUNT_TOO_SMALL',
  );
  for (const [field, value] of [
    ['mesh.surfaceGrading', configuration.mesh.surfaceGrading],
    ['mesh.radialGrading', configuration.mesh.radialGrading],
    ['mesh.wakeGrading', configuration.mesh.wakeGrading],
    ['mesh.spanwiseGrading', configuration.mesh.spanwiseGrading],
  ] as const) {
    requirePositive(issues, field, value);
  }
}

function validateLes(
  configuration: MeshConfiguration,
  issues: ValidationIssue[],
): void {
  const intent = configuration.meshIntent;
  const isLes = intent === 'WRLES' || intent === 'WMLES';
  requirePositive(issues, 'les.targetYPlus', configuration.les.targetYPlus);
  requirePositive(
    issues,
    'les.targetStreamwisePlus',
    configuration.les.targetStreamwisePlus,
  );
  requirePositive(
    issues,
    'les.targetSpanwisePlus',
    configuration.les.targetSpanwisePlus,
  );
  requirePositive(
    issues,
    'les.referenceBoundaryLayerThicknessChords',
    configuration.les.referenceBoundaryLayerThicknessChords,
  );
  requireIntegerAtLeast(
    issues,
    'les.minimumCellsAcrossBoundaryLayer',
    configuration.les.minimumCellsAcrossBoundaryLayer,
    8,
    'BOUNDARY_LAYER_CELL_TARGET_TOO_SMALL',
  );
  if (
    configuration.les.minimumCellsAcrossBoundaryLayer + 8 >
    configuration.mesh.radialCells
  ) {
    error(
      issues,
      'les.minimumCellsAcrossBoundaryLayer',
      'WALL_LAYERS_USE_ALL_RADIAL_CELLS',
      'Wall-layer cells must leave at least eight radial cells for the outer field',
    );
  }
  if (
    !Number.isFinite(configuration.les.maxRadialGrowthRatio) ||
    configuration.les.maxRadialGrowthRatio < 1
  ) {
    error(
      issues,
      'les.maxRadialGrowthRatio',
      'VALUE_TOO_SMALL',
      'Maximum radial growth ratio must be finite and at least 1',
    );
  } else if (configuration.les.maxRadialGrowthRatio > 1.2) {
    warning(
      issues,
      'les.maxRadialGrowthRatio',
      'HIGH_WALL_LAYER_GROWTH_RATIO',
      'Growth ratios above 1.2 deserve review for wall-bounded CFD',
    );
  }

  if (isLes && !configuration.extrusion.enabled) {
    error(
      issues,
      'extrusion.enabled',
      'LES_REQUIRES_3D_EXTRUSION',
      'LES requires an enabled three-dimensional extrusion',
    );
  }
  if (isLes && configuration.extrusion.frontBackPatchType !== 'CYCLIC') {
    error(
      issues,
      'extrusion.frontBackPatchType',
      'LES_REQUIRES_PERIODIC_SPAN',
      'LES requires a cyclic front/back pair; empty produces a 2D case',
    );
  }
  if (isLes && configuration.mesh.spanwiseCells < 2) {
    error(
      issues,
      'mesh.spanwiseCells',
      'LES_REQUIRES_MULTIPLE_SPANWISE_CELLS',
      'LES requires at least two spanwise cells',
    );
  }
  if (
    isLes &&
    isPositive(configuration.airfoil.chordLength) &&
    isPositive(configuration.extrusion.spanwiseThickness) &&
    configuration.extrusion.spanwiseThickness /
        configuration.airfoil.chordLength <
      0.1 - 1e-12
  ) {
    warning(
      issues,
      'extrusion.spanwiseThickness',
      'SHORT_LES_SPAN',
      'Periodic LES span is below 0.1 chord; use at least 0.1c as an initial screening width and verify two-point correlations with a wider-span study',
    );
  }
  if (intent === 'WRLES') {
    if (configuration.les.targetYPlus > 2) {
      warning(
        issues,
        'les.targetYPlus',
        'HIGH_WRLES_Y_PLUS',
        'WRLES normally starts with a wall-normal target near y+ 1',
      );
    }
    if (configuration.les.targetStreamwisePlus > 150) {
      warning(
        issues,
        'les.targetStreamwisePlus',
        'COARSE_WRLES_STREAMWISE_TARGET',
        'WRLES streamwise targets above 150 wall units deserve review',
      );
    }
    if (configuration.les.targetSpanwisePlus > 40) {
      warning(
        issues,
        'les.targetSpanwisePlus',
        'COARSE_WRLES_SPANWISE_TARGET',
        'WRLES spanwise targets above 40 wall units deserve review',
      );
    }
  } else if (intent === 'WMLES' && configuration.les.targetYPlus < 10) {
    warning(
      issues,
      'les.targetYPlus',
      'LOW_WMLES_EXCHANGE_HEIGHT',
      'WMLES placement must match the wall model; very low y+ removes much of its cost benefit',
    );
  } else if (
    intent === 'RANS_WALL_RESOLVED' &&
    configuration.les.targetYPlus > 2
  ) {
    warning(
      issues,
      'les.targetYPlus',
      'HIGH_WALL_RESOLVED_RANS_Y_PLUS',
      'Wall-resolved RANS normally targets the first cell near y+ 1',
    );
  } else if (
    intent === 'RANS_WALL_FUNCTION' &&
    (configuration.les.targetYPlus < 30 ||
      configuration.les.targetYPlus > 300)
  ) {
    warning(
      issues,
      'les.targetYPlus',
      'RANS_WALL_FUNCTION_Y_PLUS_RANGE',
      'A conventional wall-function starting point is y+ 30 to 300; verify the solver wall treatment',
    );
  }
  if (!configuration.les.estimateFirstCellHeight) {
    if (configuration.les.manualFirstCellHeight === undefined) {
      error(
        issues,
        'les.manualFirstCellHeight',
        'MANUAL_FIRST_CELL_HEIGHT_REQUIRED',
        'Manual first-cell height is required when estimation is disabled',
      );
    } else {
      requirePositive(
        issues,
        'les.manualFirstCellHeight',
        configuration.les.manualFirstCellHeight,
      );
    }
  }
  if (configuration.les.solverWallShearStress !== undefined) {
    requirePositive(
      issues,
      'les.solverWallShearStress',
      configuration.les.solverWallShearStress,
    );
  }
  warning(
    issues,
    'les.targetYPlus',
    'Y_PLUS_IS_AN_ESTIMATE',
    configuration.les.solverWallShearStress === undefined
      ? 'The pre-solution flat-plate estimate does not guarantee solver-computed y-plus; calibrate it with airfoil-wall shear from a matching solution'
      : 'First-cell sizing uses solver wall-shear feedback; repeat the y-plus check after remeshing',
  );

  const firstCellHeight = configuration.les.estimateFirstCellHeight
    ? resolvedWallSpacing(
        configuration,
        configuration.les.targetYPlus,
      )?.firstCellHeight
    : configuration.les.manualFirstCellHeight;
  if (
    firstCellHeight !== undefined &&
    isPositive(firstCellHeight) &&
    Number.isInteger(configuration.mesh.radialCells) &&
    configuration.mesh.radialCells > 0 &&
    isPositive(configuration.airfoil.chordLength) &&
    isPositive(configuration.domain.upperRadiusChords) &&
    isPositive(configuration.domain.lowerRadiusChords) &&
    Number.isFinite(configuration.les.maxRadialGrowthRatio) &&
    configuration.les.maxRadialGrowthRatio >= 1
  ) {
    const radialExtent =
      configuration.airfoil.chordLength *
      Math.min(
        configuration.domain.upperRadiusChords,
        configuration.domain.lowerRadiusChords,
      );
    const wallLayerThickness =
      configuration.airfoil.chordLength *
      configuration.les.referenceBoundaryLayerThicknessChords;
    const layeredRatios = solveLayeredRatios(
      firstCellHeight,
      radialExtent,
      configuration.mesh.radialCells,
      wallLayerThickness,
      configuration.les.minimumCellsAcrossBoundaryLayer,
      configuration.les.maxRadialGrowthRatio,
    );
    if (layeredRatios === undefined) {
      error(
        issues,
        'mesh.radialCells',
        'INFEASIBLE_RADIAL_STRETCHING',
        'Requested radial extent is infeasible; adjust radial cells, first-cell height, or maximum growth ratio',
      );
    }
  }

  const sizing = isLes ? estimateLesSizing(configuration) : undefined;
  if (sizing !== undefined) {
    if (
      intent === 'WRLES' &&
      sizing.estimatedMaximumSurfaceStreamwisePlus >
        1.1 * configuration.les.targetStreamwisePlus
    ) {
      warning(
        issues,
        'mesh.upperSurfaceCells',
        'STREAMWISE_LES_TARGET_NOT_MET',
        `Estimated maximum wall spacing is s+ ${sizing.estimatedMaximumSurfaceStreamwisePlus.toFixed(1)}; suggested leading/upper/lower counts are ~${sizing.recommendedLeadingEdgeCells}/${sizing.recommendedUpperSurfaceCells}/${sizing.recommendedLowerSurfaceCells}`,
      );
    }
    if (
      intent === 'WRLES' &&
      sizing.estimatedMaximumSpanwisePlus >
        1.1 * configuration.les.targetSpanwisePlus
    ) {
      warning(
        issues,
        'mesh.spanwiseCells',
        'SPANWISE_LES_TARGET_NOT_MET',
        `Estimated maximum spacing is z+ ${sizing.estimatedMaximumSpanwisePlus.toFixed(1)}; approximately ${sizing.recommendedSpanwiseCells} cells meet the target`,
      );
    }
    if (
      sizing.radialCellsWithinBoundaryLayer <
      configuration.les.minimumCellsAcrossBoundaryLayer
    ) {
      warning(
        issues,
        'mesh.radialCells',
        'BOUNDARY_LAYER_CELL_TARGET_NOT_MET',
        `${sizing.radialCellsWithinBoundaryLayer} radial cell centers lie within the reference boundary layer; target is ${configuration.les.minimumCellsAcrossBoundaryLayer}`,
      );
    }
    if (
      configuration.extrusion.spanwiseThickness <
      2 * sizing.referenceBoundaryLayerThickness
    ) {
      warning(
        issues,
        'extrusion.spanwiseThickness',
        'NARROW_PERIODIC_SPAN',
        'Periodic span is less than twice the reference boundary-layer thickness; verify span independence',
      );
    }
  }
}

function validateExtrusion(
  configuration: MeshConfiguration,
  issues: ValidationIssue[],
): void {
  if (configuration.extrusion.enabled) {
    requirePositive(
      issues,
      'extrusion.spanwiseThickness',
      configuration.extrusion.spanwiseThickness,
    );
  }
  if (
    configuration.extrusion.frontBackPatchType === 'EMPTY' &&
    configuration.mesh.spanwiseCells > 1
  ) {
    error(
      issues,
      'mesh.spanwiseCells',
      'EMPTY_REQUIRES_ONE_SPANWISE_CELL',
      'EMPTY front/back patches require exactly one spanwise cell',
    );
  }
  const expectedPatchType = {
    EMPTY: 'EMPTY',
    SYMMETRY: 'SYMMETRY_PLANE',
    CYCLIC: 'CYCLIC',
    WALL: 'WALL',
  }[configuration.extrusion.frontBackPatchType];
  if (
    configuration.boundaries.front.patchType !== expectedPatchType ||
    configuration.boundaries.back.patchType !== expectedPatchType
  ) {
    error(
      issues,
      'boundaries.front',
      'FRONT_BACK_PATCH_TYPE_MISMATCH',
      `Front and back boundary patch types must match frontBackPatchType (${expectedPatchType})`,
    );
  }
}

function validateBoundaries(
  configuration: MeshConfiguration,
  issues: ValidationIssue[],
): void {
  const names = new Set<string>();
  for (const key of BOUNDARY_KEYS) {
    const boundary = configuration.boundaries[key];
    const field = `boundaries.${key}.name`;
    if (!CFD_NAME_PATTERN.test(boundary.name)) {
      error(
        issues,
        field,
        'INVALID_CFD_NAME',
        `Boundary name is unsafe; suggested name: ${sanitizeCfdName(boundary.name)}`,
      );
    } else if (RESERVED_CFD_NAMES.has(boundary.name)) {
      error(
        issues,
        field,
        'RESERVED_CFD_NAME',
        'Boundary name is reserved by the OpenFOAM dictionary format',
      );
    }
    if (names.has(boundary.name)) {
      error(
        issues,
        field,
        'DUPLICATE_BOUNDARY_NAME',
        'Boundary names must be unique',
      );
    }
    names.add(boundary.name);
  }
  if (configuration.boundaries.airfoil.patchType !== 'WALL') {
    warning(
      issues,
      'boundaries.airfoil.patchType',
      'AIRFOIL_NOT_WALL',
      'The airfoil patch is normally classified as WALL',
    );
  }
}

function geometricSum(firstCellHeight: number, count: number, ratio: number): number {
  let sum = 0;
  let size = firstCellHeight;
  for (let index = 0; index < count; index += 1) {
    sum += size;
    size *= ratio;
  }
  return sum;
}

function solveGeometricRatio(
  firstCellHeight: number,
  totalLength: number,
  count: number,
  maximumRatio: number,
): number | undefined {
  if (
    !isPositive(firstCellHeight) ||
    !isPositive(totalLength) ||
    !Number.isInteger(count) ||
    count < 1 ||
    !Number.isFinite(maximumRatio) ||
    maximumRatio < 1 ||
    firstCellHeight * count > totalLength * (1 + 1e-10) ||
    geometricSum(firstCellHeight, count, maximumRatio) < totalLength
  ) {
    return undefined;
  }
  let lower = 1;
  let upper = maximumRatio;
  for (let iteration = 0; iteration < 96; iteration += 1) {
    const ratio = 0.5 * (lower + upper);
    if (geometricSum(firstCellHeight, count, ratio) < totalLength) {
      lower = ratio;
    } else {
      upper = ratio;
    }
  }
  return 0.5 * (lower + upper);
}

function solveLayeredRatios(
  firstCellHeight: number,
  totalLength: number,
  totalCells: number,
  wallLayerThickness: number,
  wallLayerCells: number,
  maximumRatio: number,
): { outerRatio: number; wallCells: number; wallRatio: number } | undefined {
  if (
    !isPositive(wallLayerThickness) ||
    wallLayerThickness >= totalLength ||
    !Number.isInteger(wallLayerCells) ||
    wallLayerCells < 1 ||
    wallLayerCells + 8 > totalCells
  ) {
    return undefined;
  }
  for (
    let candidateWallCells = wallLayerCells;
    candidateWallCells <= totalCells - 8;
    candidateWallCells += 1
  ) {
    const wallRatio = solveGeometricRatio(
      firstCellHeight,
      wallLayerThickness,
      candidateWallCells,
      maximumRatio,
    );
    if (wallRatio === undefined) {
      continue;
    }
    const outerFirst =
      firstCellHeight * wallRatio ** Math.max(0, candidateWallCells - 1);
    const outerRatio = solveGeometricRatio(
      outerFirst,
      totalLength - wallLayerThickness,
      totalCells - candidateWallCells,
      maximumRatio,
    );
    if (outerRatio !== undefined) {
      return {
        outerRatio,
        wallCells: candidateWallCells,
        wallRatio,
      };
    }
  }
  return undefined;
}

function maximumGradedCellSize(
  length: number,
  count: number,
  totalGrading: number,
): number {
  if (count <= 1) {
    return length;
  }
  if (Math.abs(totalGrading - 1) <= Number.EPSILON) {
    return length / count;
  }
  const logRatio = Math.log(totalGrading) / (count - 1);
  const weightSum = Math.expm1(count * logRatio) / Math.expm1(logRatio);
  return (length * Math.max(1, totalGrading)) / weightSum;
}

function recommendedCellCount(
  length: number,
  target: number,
  totalGrading: number,
): number {
  let upper = Math.max(2, Math.ceil(length / target));
  while (
    upper < Number.MAX_SAFE_INTEGER / 2 &&
    maximumGradedCellSize(length, upper, totalGrading) > target
  ) {
    upper *= 2;
  }

  let lower = 2;
  while (lower < upper) {
    const middle = lower + Math.floor((upper - lower) / 2);
    if (maximumGradedCellSize(length, middle, totalGrading) <= target) {
      upper = middle;
    } else {
      lower = middle + 1;
    }
  }
  return lower;
}

function isPositive(value: number): boolean {
  return Number.isFinite(value) && value > 0;
}

function requirePositive(
  issues: ValidationIssue[],
  field: string,
  value: number,
): void {
  if (!isPositive(value)) {
    error(
      issues,
      field,
      'MUST_BE_POSITIVE',
      'Value must be finite and greater than zero',
    );
  }
}

function requireFinite(
  issues: ValidationIssue[],
  field: string,
  value: number,
): void {
  if (!Number.isFinite(value)) {
    error(issues, field, 'MUST_BE_FINITE', 'Value must be finite');
  }
}

function requireGreaterThan(
  issues: ValidationIssue[],
  field: string,
  value: number,
  minimum: number,
): void {
  if (!Number.isFinite(value) || value <= minimum) {
    error(
      issues,
      field,
      'VALUE_TOO_SMALL',
      `Value must be finite and greater than ${minimum}`,
    );
  }
}

function requireIntegerAtLeast(
  issues: ValidationIssue[],
  field: string,
  value: number,
  minimum: number,
  code: string,
): void {
  if (!Number.isInteger(value) || value < minimum) {
    error(
      issues,
      field,
      code,
      `Value must be an integer of at least ${minimum}`,
    );
  }
}

function error(
  issues: ValidationIssue[],
  field: string,
  code: string,
  message: string,
): void {
  issues.push({ field, code, message, severity: 'ERROR' });
}

function warning(
  issues: ValidationIssue[],
  field: string,
  code: string,
  message: string,
): void {
  issues.push({ field, code, message, severity: 'WARNING' });
}
