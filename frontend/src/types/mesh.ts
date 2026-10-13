export type PatchType =
  | 'PATCH'
  | 'WALL'
  | 'EMPTY'
  | 'SYMMETRY_PLANE'
  | 'CYCLIC';

export type FrontBackPatchType =
  | 'EMPTY'
  | 'SYMMETRY'
  | 'CYCLIC'
  | 'WALL';

export type LesMode = 'WRLES' | 'WMLES';
export type MeshIntent =
  | 'RANS_WALL_RESOLVED'
  | 'RANS_WALL_FUNCTION'
  | 'WMLES'
  | 'WRLES';
export type WakeAlignment = 'BLENDED' | 'CHORD' | 'FREESTREAM' | 'CUSTOM';

export interface BoundaryDefinition {
  name: string;
  patchType: PatchType;
}

export interface AirfoilConfiguration {
  nacaCode: string;
  chordLength: number;
  angleOfAttackDeg: number;
  finiteTrailingEdge: boolean;
  surfacePointCount: number;
}

export interface FlowConfiguration {
  reynoldsNumber: number;
  freestreamVelocity: number;
  density: number;
  dynamicViscosity: number;
}

export interface DomainConfiguration {
  upstreamRadiusChords: number;
  upperRadiusChords: number;
  lowerRadiusChords: number;
  wakeLengthChords: number;
  wakeHalfHeightChords: number;
}

export interface MeshResolutionConfiguration {
  upperSurfaceCells: number;
  lowerSurfaceCells: number;
  leadingEdgeCells: number;
  wakeCells: number;
  radialCells: number;
  spanwiseCells: number;
  surfaceGrading: number;
  radialGrading: number;
  wakeGrading: number;
  spanwiseGrading: number;
}

export interface OptimizationConfiguration {
  /** Interpret target y+ at the first-cell centroid rather than its full height. */
  targetCellCenterYPlus: boolean;
  /** Attempt a safeguarded wall-normal coordinate adjustment. */
  orthogonalizeWall: boolean;
  /** Quality-monotone algebraic smoothing sweeps. */
  smoothingIterations: number;
}

export type SurfaceSide = 'UPPER' | 'LOWER';

export interface WallShearSample {
  side: SurfaceSide;
  xOverChord: number;
  wallShearStressPa: number;
  signedWallShearStressPa?: number;
  intermittency?: number;
  boundaryLayerThicknessChords?: number;
}

export interface WallShearProfile {
  reference: {
    airfoil: AirfoilConfiguration;
    flow: FlowConfiguration;
  };
  samples: WallShearSample[];
}

export interface LesConfiguration {
  enabled: boolean;
  mode: LesMode;
  targetYPlus: number;
  targetStreamwisePlus: number;
  targetSpanwisePlus: number;
  referenceBoundaryLayerThicknessChords: number;
  minimumCellsAcrossBoundaryLayer: number;
  maxRadialGrowthRatio: number;
  estimateFirstCellHeight: boolean;
  solverWallShearStress?: number | undefined;
  manualFirstCellHeight?: number;
  wallShearProfile?: WallShearProfile;
}

export interface WakeConfiguration {
  alignment: WakeAlignment;
  blendLengthChords: number;
  customAngleDeg: number;
}

export interface ExtrusionConfiguration {
  enabled: boolean;
  spanwiseThickness: number;
  frontBackPatchType: FrontBackPatchType;
}

export interface BoundaryConfiguration {
  inlet: BoundaryDefinition;
  outlet: BoundaryDefinition;
  airfoil: BoundaryDefinition;
  upperFarfield: BoundaryDefinition;
  lowerFarfield: BoundaryDefinition;
  front: BoundaryDefinition;
  back: BoundaryDefinition;
}

export type GmshVersion = '2.2' | '4.1';

export interface OutputConfiguration {
  writeOpenfoam: boolean;
  writeGmsh: boolean;
  writeGeometryJson: boolean;
  gmshVersion: GmshVersion;
}

export interface MeshConfiguration {
  meshIntent: MeshIntent;
  airfoil: AirfoilConfiguration;
  flow: FlowConfiguration;
  domain: DomainConfiguration;
  mesh: MeshResolutionConfiguration;
  les: LesConfiguration;
  wake: WakeConfiguration;
  extrusion: ExtrusionConfiguration;
  boundaries: BoundaryConfiguration;
  output: OutputConfiguration;
  optimization: OptimizationConfiguration;
}

export interface Point2 {
  x: number;
  y: number;
}

export type ValidationSeverity = 'ERROR' | 'WARNING';

export interface ValidationIssue {
  field: string;
  code: string;
  message: string;
  severity: ValidationSeverity;
}

export interface ValidationReport {
  valid: boolean;
  issues: ValidationIssue[];
}

export interface WallSpacingEstimate {
  skinFrictionCoefficient: number;
  wallShearStress: number;
  frictionVelocity: number;
  firstCellHeight: number;
}

export interface ResolvedWallSpacing {
  firstCellHeight: number;
  estimate?: WallSpacingEstimate;
}

export interface LesSizingEstimate {
  mode: LesMode;
  propertyReynoldsNumber: number;
  reynoldsMismatchPercent: number;
  viscousLength: number;
  firstCellHeight: number;
  targetStreamwiseSpacing: number;
  maximumSurfaceStreamwiseSpacing: number;
  estimatedMaximumSurfaceStreamwisePlus: number;
  recommendedLeadingEdgeCells: number;
  recommendedUpperSurfaceCells: number;
  recommendedLowerSurfaceCells: number;
  targetSpanwiseSpacing: number;
  maximumSpanwiseSpacing: number;
  estimatedMaximumSpanwisePlus: number;
  recommendedSpanwiseCells: number;
  spanwiseExtentChords: number;
  referenceBoundaryLayerThickness: number;
  radialCellsWithinBoundaryLayer: number;
  periodicThreeDimensional: boolean;
}

export interface StretchingSolution {
  ratio: number;
  cellSizes: number[];
  pointPositions: number[];
  totalLength: number;
}

/** Legacy Phase 1–3 geometry/spacing response. */
export interface GeometryPreview {
  phase: 'geometry-and-les-spacing';
  airfoilUpper: Point2[];
  airfoilLower: Point2[];
  airfoilPerimeter: Point2[];
  rotationPivot: Point2;
  wallSpacing?: ResolvedWallSpacing;
  radialStretching?: StretchingSolution;
  warnings: ValidationIssue[];
}

export type PreviewLevel =
  | 'GEOMETRY_ONLY'
  | 'BLOCK_TOPOLOGY'
  | 'COARSE_MESH'
  | 'FULL_MESH';

export type PreviewPolylineKind = 'BLOCK_BOUNDARY' | 'MESH_LINE';

export interface PreviewPolyline {
  label: string;
  kind: PreviewPolylineKind;
  points: Point2[];
}

export type BoundaryRole =
  | 'INLET'
  | 'OUTLET'
  | 'AIRFOIL'
  | 'UPPER_FARFIELD'
  | 'LOWER_FARFIELD';

export interface BoundaryLabel {
  role: BoundaryRole;
  name: string;
  position: Point2;
}

export interface PreviewBoundaryPatch {
  role: BoundaryRole;
  name: string;
  segments: [Point2, Point2][];
}

export interface MeshQualityReport {
  valid: boolean;
  pointCount: number;
  cellCount: number;
  blockCount: number;
  minimumCellArea: number | null;
  maximumCellArea: number | null;
  minimumCellVolume: number | null;
  maximumCellVolume: number | null;
  maximumAspectRatio: number;
  minimumOrthogonalQuality: number;
  maximumEquiangleSkewness: number;
  minimumScaledJacobian: number;
  worstAspectCell?: {
    cellIndex: number;
    blockId: number;
    blockName?: string;
    aspectRatio: number;
  };
  minimumEdgeLength: number;
  maximumEdgeLength: number;
  maximumRadialGrowthRatio: number;
  warnings: string[];
  errors: string[];
}

export interface MeshPreview {
  phase: 'structured-cgrid-and-extrusion';
  previewLevel: PreviewLevel;
  airfoilUpper: Point2[];
  airfoilLower: Point2[];
  outerBoundary: Point2[];
  wakeUpper: Point2[];
  wakeLower: Point2[];
  blockBoundaries: PreviewPolyline[];
  meshLines: PreviewPolyline[];
  boundaryPatches: PreviewBoundaryPatch[];
  boundaryLabels: BoundaryLabel[];
  quality: MeshQualityReport;
  quality3d?: MeshQualityReport;
  wallSpacing?: ResolvedWallSpacing;
  wallResolution?: WallResolutionAudit;
  wallLayerCells: number;
  wallLayerThickness: number;
  lesSizing?: LesSizingEstimate;
  warnings: ValidationIssue[];
  optimization?: OptimizationReport;
}

export interface OptimizationReport {
  requestedIterations: number;
  acceptedIterations: number;
  wallStepAccepted: boolean;
  initialMinimumOrthogonalQuality: number;
  finalMinimumOrthogonalQuality: number;
  initialMinimumScaledJacobian: number;
  finalMinimumScaledJacobian: number;
  maximumDisplacement: number;
  measuredMaximumRadialGrowth: number;
  warnings: string[];
}

export interface WallResolutionAudit {
  wallFaceCount: number;
  referenceFirstCellHeight: number;
  minimumNormalHeight: number;
  maximumNormalHeight: number;
  minimumCentroidDistance: number;
  maximumCentroidDistance: number;
  maximumNonorthogonalityDeg: number;
  minimumPredictedYPlus: number;
  maximumPredictedYPlus: number;
  maximumPredictedStreamwisePlus: number;
  minimumCellsInReferenceLayer: number;
  reverseShearFaceCount: number;
  transitionalFaceCount: number;
  referenceWallShearPa: number;
  shearSource:
    | 'SPATIAL_SOLVER_PROFILE'
    | 'SOLVER_FEEDBACK'
    | 'FLAT_PLATE_CORRELATION';
  warnings: string[];
}

export interface WallShearFeedback {
  column: string;
  sampleCount: number;
  minimumPa: number;
  maximumPa: number;
  meanPa: number;
  selectedPa: number;
  percentile: number;
}

export interface MeshCounts {
  quadCells: number;
  hexCells: number;
  points2d: number;
  points: number;
}

export interface BudgetPlan {
  feasible: boolean;
  candidatesEvaluated: number;
  configuration?: MeshConfiguration;
  counts?: MeshCounts;
  wallAudit?: WallResolutionAudit;
  reasons: string[];
}
