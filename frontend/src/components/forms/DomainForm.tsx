import type { ReactNode } from 'react';

import { useMeshWorkspace } from '../../state/meshStore.js';
import type { WakeAlignment } from '../../types/mesh.js';
import { NumberField, SectionTitle, SelectField } from '../FormControls.js';

export function DomainForm(): ReactNode {
  const { configuration, report, updateConfiguration } = useMeshWorkspace();
  const domain = configuration.domain;
  const issue = (field: string) =>
    report.issues.find((candidate) => candidate.field === field);
  const update = (values: Partial<typeof domain>) => {
    updateConfiguration({
      ...configuration,
      domain: { ...domain, ...values },
    });
  };
  const updateWake = (values: Partial<typeof configuration.wake>) => {
    updateConfiguration({
      ...configuration,
      wake: { ...configuration.wake, ...values },
    });
  };

  return (
    <section className="form-section">
      <SectionTitle description="Farfield extents normalized by chord">
        Domain
      </SectionTitle>
      <div className="field-grid field-grid--two">
        <NumberField
          issue={issue('domain.upstreamRadiusChords')}
          label="Upstream radius"
          min={1}
          onChange={(upstreamRadiusChords) => {
            update({ upstreamRadiusChords });
          }}
          unit="c"
          value={domain.upstreamRadiusChords}
        />
        <NumberField
          issue={issue('domain.wakeLengthChords')}
          label="Wake length"
          min={1}
          onChange={(wakeLengthChords) => {
            update({ wakeLengthChords });
          }}
          unit="c"
          value={domain.wakeLengthChords}
        />
        <NumberField
          issue={issue('domain.upperRadiusChords')}
          label="Upper radius"
          min={1}
          onChange={(upperRadiusChords) => {
            update({ upperRadiusChords });
          }}
          unit="c"
          value={domain.upperRadiusChords}
        />
        <NumberField
          issue={issue('domain.lowerRadiusChords')}
          label="Lower radius"
          min={1}
          onChange={(lowerRadiusChords) => {
            update({ lowerRadiusChords });
          }}
          unit="c"
          value={domain.lowerRadiusChords}
        />
        {configuration.airfoil.finiteTrailingEdge ? (
          <NumberField
            issue={issue('domain.wakeHalfHeightChords')}
            label="Wake half-height"
            min={0}
            onChange={(wakeHalfHeightChords) => {
              update({ wakeHalfHeightChords });
            }}
            unit="c"
            value={domain.wakeHalfHeightChords}
          />
        ) : null}
      </div>
      {configuration.airfoil.finiteTrailingEdge ? null : (
        <p className="form-note">
          Sharp trailing edge uses a zero-thickness conformal wake cut.
        </p>
      )}
      <SectionTitle description="Direction of the conformal wake cut">
        Wake path
      </SectionTitle>
      <SelectField<WakeAlignment>
        issue={issue('wake.alignment')}
        label="Alignment"
        onChange={(alignment) => {
          updateWake({ alignment });
        }}
        options={[
          { label: 'Blended · recommended', value: 'BLENDED' },
          { label: 'Chord aligned', value: 'CHORD' },
          { label: 'Freestream aligned', value: 'FREESTREAM' },
          { label: 'Custom angle', value: 'CUSTOM' },
        ]}
        value={configuration.wake.alignment}
      />
      {configuration.wake.alignment === 'BLENDED' ? (
        <NumberField
          hint="Leaves the trailing edge at the chord angle, then turns parallel"
          issue={issue('wake.blendLengthChords')}
          label="Turn length"
          min={0}
          onChange={(blendLengthChords) => {
            updateWake({ blendLengthChords });
          }}
          step={0.25}
          unit="c"
          value={configuration.wake.blendLengthChords}
        />
      ) : null}
      {configuration.wake.alignment === 'CUSTOM' ? (
        <NumberField
          issue={issue('wake.customAngleDeg')}
          label="Wake angle"
          max={79}
          min={-79}
          onChange={(customAngleDeg) => {
            updateWake({ customAngleDeg });
          }}
          step={0.5}
          unit="deg"
          value={configuration.wake.customAngleDeg}
        />
      ) : null}
    </section>
  );
}
