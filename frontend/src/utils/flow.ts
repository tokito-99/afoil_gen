import type { MeshConfiguration } from '../types/mesh.js';

export function calculateReynoldsNumber(
  chordLength: number,
  freestreamVelocity: number,
  density: number,
  dynamicViscosity: number,
): number {
  return (
    (density * freestreamVelocity * chordLength) /
    dynamicViscosity
  );
}

export function synchronizeReynoldsNumber(
  configuration: MeshConfiguration,
): MeshConfiguration {
  const reynoldsNumber = calculateReynoldsNumber(
    configuration.airfoil.chordLength,
    configuration.flow.freestreamVelocity,
    configuration.flow.density,
    configuration.flow.dynamicViscosity,
  );
  return {
    ...configuration,
    flow: {
      ...configuration.flow,
      reynoldsNumber,
    },
  };
}
