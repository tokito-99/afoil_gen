import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';

import { App } from './App.js';
import { MeshWorkspaceProvider } from './state/meshStore.js';
import './styles.css';

const root = document.getElementById('root');
if (root === null) {
  throw new Error('Missing #root application element');
}

createRoot(root).render(
  <StrictMode>
    <MeshWorkspaceProvider>
      <App />
    </MeshWorkspaceProvider>
  </StrictMode>,
);
