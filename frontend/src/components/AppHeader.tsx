import {
  useRef,
  useState,
  type ChangeEvent,
  type ReactNode,
} from 'react';

import { useMeshWorkspace } from '../state/meshStore.js';
import { meshIntentLabel } from '../config/meshIntentPresets.js';
import {
  isMeshConfiguration,
  normalizeMeshConfiguration,
} from '../utils/configuration.js';
import {
  configurationFilename,
  downloadText,
} from '../utils/download.js';
import { Icon } from './Icons.js';

export function AppHeader(): ReactNode {
  const {
    configuration,
    engineStatus,
    loadConfiguration,
    resetConfiguration,
  } = useMeshWorkspace();
  const fileInput = useRef<HTMLInputElement>(null);
  const [fileError, setFileError] = useState<string>();

  const loadFile = async (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.currentTarget.files?.[0];
    event.currentTarget.value = '';
    if (file === undefined) {
      return;
    }
    try {
      const parsed: unknown = JSON.parse(await file.text());
      if (!isMeshConfiguration(parsed)) {
        throw new Error('That file is not an afoil_gen mesh configuration.');
      }
      await loadConfiguration(normalizeMeshConfiguration(parsed));
      setFileError(undefined);
    } catch (error: unknown) {
      setFileError(
        error instanceof Error ? error.message : 'Unable to load configuration.',
      );
    }
  };

  return (
    <header className="app-header">
      <div className="brand">
        <div aria-hidden="true" className="brand-mark">
          AF
        </div>
        <div>
          <strong>afoil_gen</strong>
          <span>Airfoil mesh generator</span>
        </div>
      </div>
      <div className="header-context">
        <span>NACA {configuration.airfoil.nacaCode}</span>
        <i />
        <span>
          {meshIntentLabel(configuration.meshIntent)} ·{' '}
          {configuration.meshIntent === 'WRLES' ||
          configuration.meshIntent === 'WMLES'
            ? configuration.extrusion.frontBackPatchType === 'CYCLIC'
              ? 'periodic span'
              : 'span unresolved'
            : '2D'}
        </span>
      </div>
      <div className="header-actions">
        <span className={`engine-pill engine-pill--${engineStatus.toLowerCase()}`}>
          <i />
          {engineStatus === 'LOADING'
            ? 'Loading'
            : engineStatus === 'GENERATING'
              ? 'Updating'
              : engineStatus === 'ERROR'
                ? 'Unavailable'
                : 'Ready'}
        </span>
        <button
          className="button button--quiet"
          onClick={() => {
            fileInput.current?.click();
          }}
          type="button"
        >
          <Icon name="upload" size={16} />
          Open
        </button>
        <button
          className="button button--quiet"
          onClick={() => {
            downloadText(
              `${JSON.stringify(configuration, undefined, 2)}\n`,
              configurationFilename(configuration.airfoil.nacaCode),
              'application/json',
            );
          }}
          type="button"
        >
          <Icon name="download" size={16} />
          Save
        </button>
        <button
          aria-label="Reset default configuration"
          className="icon-button"
          onClick={resetConfiguration}
          title="Reset default configuration"
          type="button"
        >
          <Icon name="reset" size={17} />
        </button>
        <input
          accept="application/json,.json"
          className="visually-hidden"
          onChange={(event) => {
            void loadFile(event);
          }}
          ref={fileInput}
          type="file"
        />
      </div>
      {fileError === undefined ? null : (
        <div className="header-error" role="alert">
          {fileError}
        </div>
      )}
    </header>
  );
}
