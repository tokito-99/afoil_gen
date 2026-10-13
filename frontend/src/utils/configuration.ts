import type { MeshConfiguration } from '../types/mesh.js';
import { DEFAULT_CONFIGURATION } from '../config/defaultConfiguration.js';
import { synchronizeReynoldsNumber } from './flow.js';

const ROOT_KEYS = [
  'airfoil',
  'flow',
  'domain',
  'mesh',
  'les',
  'extrusion',
  'boundaries',
  'output',
] as const;

export function isMeshConfiguration(value: unknown): value is MeshConfiguration {
  if (!isRecord(value)) {
    return false;
  }
  return ROOT_KEYS.every((key) => isRecord(value[key]));
}

export function normalizeMeshConfiguration(
  configuration: MeshConfiguration,
): MeshConfiguration {
  const meshIntent =
    configuration.meshIntent ??
    (configuration.les.enabled
      ? configuration.les.mode === 'WMLES'
        ? 'WMLES'
        : 'WRLES'
      : configuration.les.targetYPlus >= 10
        ? 'RANS_WALL_FUNCTION'
        : 'RANS_WALL_RESOLVED');
  return synchronizeReynoldsNumber({
    ...configuration,
    meshIntent,
    les: {
      ...DEFAULT_CONFIGURATION.les,
      ...configuration.les,
      enabled: meshIntent === 'WRLES' || meshIntent === 'WMLES',
      mode: meshIntent === 'WMLES' ? 'WMLES' : 'WRLES',
    },
    wake: {
      ...DEFAULT_CONFIGURATION.wake,
      ...configuration.wake,
    },
    optimization: {
      ...DEFAULT_CONFIGURATION.optimization,
      ...configuration.optimization,
    },
  });
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
