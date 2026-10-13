import { useState, type ReactNode } from 'react';

import { meshIntentLabel } from '../config/meshIntentPresets.js';
import { useMeshWorkspace } from '../state/meshStore.js';
import { estimateCellCount, formatCount } from '../utils/format.js';
import { Icon } from './Icons.js';
import { AirfoilForm } from './forms/AirfoilForm.js';
import { BoundaryForm } from './forms/BoundaryForm.js';
import { DomainForm } from './forms/DomainForm.js';
import { ExtrusionForm } from './forms/ExtrusionForm.js';
import { FlowForm } from './forms/FlowForm.js';
import { LesForm } from './forms/LesForm.js';
import { MeshForm } from './forms/MeshForm.js';

type Tab = 'GEOMETRY' | 'MESH' | 'PHYSICS' | 'SETUP';

const TABS: readonly {
  icon: 'airfoil' | 'mesh' | 'flow' | 'setup';
  id: Tab;
  label: string;
}[] = [
  { id: 'GEOMETRY', label: 'Geometry', icon: 'airfoil' },
  { id: 'MESH', label: 'Resolution', icon: 'mesh' },
  { id: 'PHYSICS', label: 'Flow', icon: 'flow' },
  { id: 'SETUP', label: 'Boundaries', icon: 'setup' },
];

export function ControlRail(): ReactNode {
  const [activeTab, setActiveTab] = useState<Tab>('GEOMETRY');
  const { configuration, report } = useMeshWorkspace();
  const cells = estimateCellCount(
    configuration.airfoil.finiteTrailingEdge,
    configuration.mesh.leadingEdgeCells,
    configuration.mesh.upperSurfaceCells,
    configuration.mesh.lowerSurfaceCells,
    configuration.mesh.wakeCells,
    configuration.mesh.radialCells,
    configuration.mesh.spanwiseCells,
  );
  const errorCount = report.issues.filter(
    (issue) => issue.severity === 'ERROR',
  ).length;

  return (
    <aside className="control-rail panel">
      <div className="control-summary">
        <div className="profile-token">
          <span>NACA</span>
          <strong>{configuration.airfoil.nacaCode}</strong>
        </div>
        <div>
          <p>
            {meshIntentLabel(configuration.meshIntent)} C-grid
          </p>
          <span>
            {formatCount(cells.hexes)} cells
            {errorCount > 0 ? ` · ${errorCount} to resolve` : ' · ready'}
          </span>
        </div>
      </div>
      <nav aria-label="Configuration sections" className="config-tabs">
        {TABS.map((tab) => (
          <button
            aria-current={activeTab === tab.id ? 'page' : undefined}
            className={activeTab === tab.id ? 'is-active' : ''}
            key={tab.id}
            onClick={() => {
              setActiveTab(tab.id);
            }}
            type="button"
          >
            <Icon name={tab.icon} size={16} />
            {tab.label}
          </button>
        ))}
      </nav>
      <div className="control-scroll">
        {activeTab === 'GEOMETRY' ? (
          <>
            <AirfoilForm />
            <DomainForm />
          </>
        ) : null}
        {activeTab === 'MESH' ? (
          <>
            <MeshForm />
            <LesForm />
          </>
        ) : null}
        {activeTab === 'PHYSICS' ? (
          <>
            <FlowForm />
            <ExtrusionForm />
          </>
        ) : null}
        {activeTab === 'SETUP' ? <BoundaryForm /> : null}
      </div>
    </aside>
  );
}
