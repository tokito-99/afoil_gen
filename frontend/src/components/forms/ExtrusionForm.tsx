import type { ReactNode } from 'react';

import { useMeshWorkspace } from '../../state/meshStore.js';
import type { FrontBackPatchType, PatchType } from '../../types/mesh.js';
import {
  NumberField,
  SectionTitle,
  SelectField,
  Toggle,
} from '../FormControls.js';

const PATCH_TYPE_BY_MODE: Record<FrontBackPatchType, PatchType> = {
  EMPTY: 'EMPTY',
  SYMMETRY: 'SYMMETRY_PLANE',
  CYCLIC: 'CYCLIC',
  WALL: 'WALL',
};

export function ExtrusionForm(): ReactNode {
  const { configuration, report, updateConfiguration } = useMeshWorkspace();
  const extrusion = configuration.extrusion;
  const isLes =
    configuration.meshIntent === 'WRLES' ||
    configuration.meshIntent === 'WMLES';
  const issue = (field: string) =>
    report.issues.find((candidate) => candidate.field === field);

  return (
    <section className="form-section">
      <SectionTitle
        description={
          isLes
            ? 'Periodic span required for resolved turbulence'
            : 'Spanwise planes and front/back classification'
        }
      >
        {isLes ? '3D periodic span' : '2D thickness'}
      </SectionTitle>
      <Toggle
        checked={extrusion.enabled}
        description="Create right-handed hexahedra from the 2D C-grid"
        label="Extrude mesh"
        onChange={(enabled) => {
          updateConfiguration({
            ...configuration,
            extrusion: { ...extrusion, enabled },
          });
        }}
      />
      <div className="field-grid field-grid--two">
        <NumberField
          issue={issue('extrusion.spanwiseThickness')}
          label="Thickness"
          min={0}
          onChange={(spanwiseThickness) => {
            updateConfiguration({
              ...configuration,
              extrusion: { ...extrusion, spanwiseThickness },
            });
          }}
          unit="m"
          value={extrusion.spanwiseThickness}
        />
        <SelectField
          hint={isLes ? 'Use cyclic for LES' : 'Use empty for 2D RANS'}
          issue={issue('extrusion.frontBackPatchType') ?? issue('boundaries.front')}
          label="Front / back"
          onChange={(frontBackPatchType) => {
            const patchType = PATCH_TYPE_BY_MODE[frontBackPatchType];
            updateConfiguration({
              ...configuration,
              extrusion: { ...extrusion, frontBackPatchType },
              boundaries: {
                ...configuration.boundaries,
                front: { ...configuration.boundaries.front, patchType },
                back: { ...configuration.boundaries.back, patchType },
              },
            });
          }}
          options={[
            { label: 'Empty · 2D', value: 'EMPTY' },
            { label: 'Symmetry', value: 'SYMMETRY' },
            { label: 'Cyclic pair', value: 'CYCLIC' },
            { label: 'Wall', value: 'WALL' },
          ]}
          value={extrusion.frontBackPatchType}
        />
      </div>
    </section>
  );
}
