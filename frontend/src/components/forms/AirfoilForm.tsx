import type { ReactNode } from 'react';

import { useMeshWorkspace } from '../../state/meshStore.js';
import { NumberField, SectionTitle, TextField, Toggle } from '../FormControls.js';

export function AirfoilForm(): ReactNode {
  const { configuration, report, updateConfiguration } = useMeshWorkspace();
  const issues = issueMap(report.issues);
  const airfoil = configuration.airfoil;

  return (
    <section className="form-section">
      <SectionTitle description="Analytical NACA four-digit geometry">
        Airfoil
      </SectionTitle>
      <div className="field-grid field-grid--two">
        <TextField
          hint="Four digits, e.g. 0012"
          issue={issues.get('airfoil.nacaCode')}
          label="NACA profile"
          maxLength={4}
          onChange={(nacaCode) => {
            updateConfiguration({
              ...configuration,
              airfoil: { ...airfoil, nacaCode },
            });
          }}
          value={airfoil.nacaCode}
        />
        <NumberField
          issue={issues.get('airfoil.angleOfAttackDeg')}
          label="Angle of attack"
          onChange={(angleOfAttackDeg) => {
            updateConfiguration({
              ...configuration,
              airfoil: { ...airfoil, angleOfAttackDeg },
            });
          }}
          step={0.25}
          unit="deg"
          value={airfoil.angleOfAttackDeg}
        />
        <NumberField
          issue={issues.get('airfoil.chordLength')}
          label="Chord"
          min={0}
          onChange={(chordLength) => {
            updateConfiguration({
              ...configuration,
              airfoil: { ...airfoil, chordLength },
            });
          }}
          unit="m"
          value={airfoil.chordLength}
        />
      </div>
      <Toggle
        checked={airfoil.finiteTrailingEdge}
        description="Keep the analytical trailing-edge thickness"
        label="Finite trailing edge"
        onChange={(finiteTrailingEdge) => {
          updateConfiguration({
            ...configuration,
            airfoil: { ...airfoil, finiteTrailingEdge },
          });
        }}
      />
      <details className="form-details">
        <summary>Geometry sampling</summary>
        <NumberField
          integer
          issue={issues.get('airfoil.surfacePointCount')}
          label="Surface points"
          min={41}
          onChange={(surfacePointCount) => {
            updateConfiguration({
              ...configuration,
              airfoil: { ...airfoil, surfacePointCount },
            });
          }}
          step={2}
          value={airfoil.surfacePointCount}
        />
      </details>
    </section>
  );
}

function issueMap(
  issues: ReturnType<typeof useMeshWorkspace>['report']['issues'],
): Map<string, (typeof issues)[number]> {
  return new Map(issues.map((issue) => [issue.field, issue]));
}
