import type { ReactNode } from 'react';

import { useMeshWorkspace } from '../../state/meshStore.js';
import type {
  BoundaryConfiguration,
  PatchType,
} from '../../types/mesh.js';
import { SectionTitle, SelectField, TextField } from '../FormControls.js';

type BoundaryKey = keyof BoundaryConfiguration;

const BOUNDARIES: readonly {
  key: BoundaryKey;
  label: string;
  lockedType?: boolean;
}[] = [
  { key: 'inlet', label: 'Inlet' },
  { key: 'outlet', label: 'Outlet' },
  { key: 'airfoil', label: 'Airfoil wall' },
  { key: 'upperFarfield', label: 'Upper farfield' },
  { key: 'lowerFarfield', label: 'Lower farfield' },
  { key: 'front', label: 'Front plane', lockedType: true },
  { key: 'back', label: 'Back plane', lockedType: true },
];

const PATCH_OPTIONS: readonly { label: string; value: PatchType }[] = [
  { label: 'Patch', value: 'PATCH' },
  { label: 'Wall', value: 'WALL' },
  { label: 'Empty', value: 'EMPTY' },
  { label: 'Symmetry plane', value: 'SYMMETRY_PLANE' },
  { label: 'Cyclic', value: 'CYCLIC' },
];

export function BoundaryForm(): ReactNode {
  const { configuration, report, updateConfiguration } = useMeshWorkspace();
  const issue = (field: string) =>
    report.issues.find((candidate) => candidate.field === field);
  const updateBoundary = (
    key: BoundaryKey,
    values: Partial<BoundaryConfiguration[BoundaryKey]>,
  ) => {
    updateConfiguration({
      ...configuration,
      boundaries: {
        ...configuration.boundaries,
        [key]: { ...configuration.boundaries[key], ...values },
      },
    });
  };

  return (
    <section className="form-section">
      <SectionTitle description="Stable names and mesh patch classifications">
        Boundaries
      </SectionTitle>
      <div className="boundary-list">
        {BOUNDARIES.map(({ key, label, lockedType }) => {
          const boundary = configuration.boundaries[key];
          return (
            <div className="boundary-row" key={key}>
              <TextField
                issue={issue(`boundaries.${key}.name`)}
                label={label}
                onChange={(name) => {
                  updateBoundary(key, { name });
                }}
                value={boundary.name}
              />
              {lockedType === true ? (
                <span className="boundary-type">{boundary.patchType}</span>
              ) : (
                <SelectField
                  issue={issue(`boundaries.${key}.patchType`)}
                  label="Type"
                  onChange={(patchType) => {
                    updateBoundary(key, { patchType });
                  }}
                  options={PATCH_OPTIONS}
                  value={boundary.patchType}
                />
              )}
            </div>
          );
        })}
      </div>
      <p className="form-note">
        Patch types classify the mesh only. Solver field values are configured
        in the downstream case.
      </p>
    </section>
  );
}
