import type { ReactNode } from 'react';

import { AppHeader } from './components/AppHeader.js';
import { ControlRail } from './components/ControlRail.js';
import { ExportPanel } from './components/ExportPanel.js';
import { MeshCanvas } from './components/MeshCanvas.js';
import { QualityPanel } from './components/QualityPanel.js';

export function App(): ReactNode {
  return (
    <div className="app-shell">
      <AppHeader />
      <main className="workspace-grid">
        <ControlRail />
        <MeshCanvas />
        <aside className="insight-rail">
          <QualityPanel />
          <ExportPanel />
        </aside>
      </main>
    </div>
  );
}
