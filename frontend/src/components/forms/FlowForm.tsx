import type { ReactNode } from 'react';

import { useMeshWorkspace } from '../../state/meshStore.js';
import { formatMetric } from '../../utils/format.js';
import { NumberField, SectionTitle } from '../FormControls.js';

export function FlowForm(): ReactNode {
  const { configuration, report, updateConfiguration } = useMeshWorkspace();
  const flow = configuration.flow;
  const issue = (field: string) =>
    report.issues.find((candidate) => candidate.field === field);
  const update = (values: Partial<typeof flow>) => {
    updateConfiguration({
      ...configuration,
      flow: { ...flow, ...values },
    });
  };

  return (
    <section className="form-section">
      <SectionTitle description="Reference flow used for wall-spacing estimates">
        Flow
      </SectionTitle>
      <div className="calculated-strip">
        <span>Calculated Reynolds number</span>
        <strong>{formatMetric(flow.reynoldsNumber, 3)}</strong>
      </div>
      <div className="field-grid field-grid--two">
        <NumberField
          issue={issue('flow.freestreamVelocity')}
          label="Freestream"
          min={0}
          onChange={(freestreamVelocity) => {
            update({ freestreamVelocity });
          }}
          unit="m/s"
          value={flow.freestreamVelocity}
        />
        <NumberField
          issue={issue('flow.density')}
          label="Density"
          min={0}
          onChange={(density) => {
            update({ density });
          }}
          unit="kg/m³"
          value={flow.density}
        />
        <NumberField
          issue={issue('flow.dynamicViscosity')}
          label="Dynamic viscosity"
          min={0}
          onChange={(dynamicViscosity) => {
            update({ dynamicViscosity });
          }}
          unit="Pa·s"
          value={flow.dynamicViscosity}
        />
      </div>
    </section>
  );
}
