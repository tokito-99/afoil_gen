import init, {
  generate_fluent,
  generate_gmsh,
  generate_openfoam,
  generate_preview_at_level,
  import_wall_shear,
  import_wall_shear_profile,
  plan_cell_budget,
  validate_configuration,
} from '../wasm-pkg/mesh_wasm.js';
import type {
  BudgetPlan,
  MeshConfiguration,
  MeshPreview,
  PreviewLevel,
  ValidationReport,
  WallShearFeedback,
} from '../types/mesh.js';

let initialization: Promise<void> | undefined;

export async function importWallShear(csv: string): Promise<WallShearFeedback> {
  await initializeMeshEngine();
  return import_wall_shear(csv, 'wallShearStressPa', 100) as WallShearFeedback;
}

export async function importWallShearProfile(
  configuration: MeshConfiguration,
  csv: string,
): Promise<MeshConfiguration> {
  await initializeMeshEngine();
  return import_wall_shear_profile(
    serialize(configuration),
    csv,
  ) as MeshConfiguration;
}

export async function planCellBudget(
  configuration: MeshConfiguration,
  maxCells: number,
  maxPoints?: number,
): Promise<BudgetPlan> {
  await initializeMeshEngine();
  return plan_cell_budget(
    serialize(configuration),
    maxCells,
    maxPoints ?? 0,
  ) as BudgetPlan;
}

export function initializeMeshEngine(): Promise<void> {
  initialization ??= init().then(() => undefined);
  return initialization;
}

export async function validateWithEngine(
  configuration: MeshConfiguration,
): Promise<ValidationReport> {
  await initializeMeshEngine();
  return validate_configuration(serialize(configuration)) as ValidationReport;
}

export async function generatePreview(
  configuration: MeshConfiguration,
  level: PreviewLevel,
): Promise<MeshPreview> {
  await initializeMeshEngine();
  return generate_preview_at_level(
    serialize(configuration),
    level,
  ) as MeshPreview;
}

export async function generateOpenfoam(
  configuration: MeshConfiguration,
): Promise<string> {
  await initializeMeshEngine();
  return generate_openfoam(serialize(configuration));
}

export async function generateGmsh(
  configuration: MeshConfiguration,
): Promise<string> {
  await initializeMeshEngine();
  return generate_gmsh(serialize(configuration));
}

export async function generateFluent(
  configuration: MeshConfiguration,
): Promise<string> {
  await initializeMeshEngine();
  return generate_fluent(serialize(configuration));
}

export async function generateGmshCrossSection(
  configuration: MeshConfiguration,
): Promise<string> {
  await initializeMeshEngine();
  const crossSection: MeshConfiguration = {
    ...configuration,
    meshIntent:
      configuration.les.targetYPlus >= 10
        ? 'RANS_WALL_FUNCTION'
        : 'RANS_WALL_RESOLVED',
    mesh: { ...configuration.mesh, spanwiseCells: 1 },
    les: { ...configuration.les, enabled: false },
    optimization: {
      ...configuration.optimization,
      orthogonalizeWall: false,
      smoothingIterations: 0,
    },
    extrusion: {
      ...configuration.extrusion,
      enabled: false,
      frontBackPatchType: 'EMPTY',
    },
    boundaries: {
      ...configuration.boundaries,
      front: { ...configuration.boundaries.front, patchType: 'EMPTY' },
      back: { ...configuration.boundaries.back, patchType: 'EMPTY' },
    },
  };
  return generate_gmsh(serialize(crossSection));
}

export function errorMessage(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  if (typeof error === 'string') {
    return error;
  }
  return 'The mesh engine returned an unknown error.';
}

function serialize(configuration: MeshConfiguration): string {
  return `${JSON.stringify(configuration, undefined, 2)}\n`;
}
