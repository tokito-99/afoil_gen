import { useState, type ReactNode } from 'react';

import {
  errorMessage,
  generateFluent,
  generateGmsh,
  generateGmshCrossSection,
  generateOpenfoam,
} from '../api/meshEngine.js';
import { useMeshWorkspace } from '../state/meshStore.js';
import type { GmshVersion } from '../types/mesh.js';
import {
  configurationFilename,
  downloadText,
} from '../utils/download.js';
import {
  assessBrowserFluent,
  assessBrowserGmsh,
  formatFileSize,
} from '../utils/exportSizing.js';
import { Icon } from './Icons.js';

type Exporting =
  | 'OPENFOAM'
  | 'FLUENT'
  | 'GMSH'
  | 'GMSH_2D'
  | undefined;

export function ExportPanel(): ReactNode {
  const {
    configuration,
    dirty,
    engineStatus,
    preview,
    report,
    updateConfiguration,
  } = useMeshWorkspace();
  const [exporting, setExporting] = useState<Exporting>();
  const [exportError, setExportError] = useState<string>();
  const canExport =
    report.valid &&
    preview !== undefined &&
    !dirty &&
    engineStatus === 'READY' &&
    exporting === undefined;
  const baseName = `naca${configuration.airfoil.nacaCode.toLowerCase()}-cgrid`;
  const gmshAssessment =
    preview === undefined ? undefined : assessBrowserGmsh(preview);
  const fluentAssessment =
    preview === undefined
      ? undefined
      : assessBrowserFluent(
          preview,
          configuration.extrusion.enabled &&
            configuration.extrusion.frontBackPatchType !== 'EMPTY',
        );
  const browserGmshSafe = gmshAssessment?.browserSafe ?? false;
  const browserFluentSafe = fluentAssessment?.browserSafe ?? false;
  const nativeBlockMeshCompatible =
    !configuration.optimization.orthogonalizeWall &&
    configuration.optimization.smoothingIterations === 0 &&
    configuration.les.wallShearProfile === undefined;

  const runExport = async (format: Exclude<Exporting, undefined>) => {
    setExporting(format);
    setExportError(undefined);
    await new Promise<void>((resolve) => {
      window.setTimeout(resolve, 0);
    });
    try {
      if (format === 'OPENFOAM') {
        const output = await generateOpenfoam(configuration);
        downloadText(output, 'blockMeshDict', 'text/plain;charset=utf-8');
      } else if (format === 'FLUENT') {
        const output = await generateFluent(configuration);
        downloadText(
          output,
          `${baseName}-fluent.msh`,
          'text/plain;charset=utf-8',
        );
      } else if (format === 'GMSH') {
        const output = await generateGmsh(configuration);
        downloadText(
          output,
          `${baseName}-gmsh.msh`,
          'text/plain;charset=utf-8',
        );
      } else {
        const output = await generateGmshCrossSection(configuration);
        downloadText(
          output,
          `${baseName}-2d.msh`,
          'text/plain;charset=utf-8',
        );
      }
    } catch (error: unknown) {
      setExportError(errorMessage(error));
    } finally {
      setExporting(undefined);
    }
  };

  return (
    <section className="export-card panel-card">
      <div className="card-heading">
        <div>
          <span>Outputs</span>
          <h2>Export mesh</h2>
        </div>
        <Icon name="download" size={20} />
      </div>
      <div className="export-actions">
        <button
          className="export-button export-button--primary"
          disabled={
            !canExport ||
            !configuration.extrusion.enabled ||
            !nativeBlockMeshCompatible
          }
          onClick={() => {
            void runExport('OPENFOAM');
          }}
          type="button"
        >
          <span className="export-icon">
            <Icon name="code" size={19} />
          </span>
          <span>
            <strong>
              {exporting === 'OPENFOAM' ? 'Preparing…' : 'OpenFOAM'}
            </strong>
            <small>
              {nativeBlockMeshCompatible
                ? 'Native blockMeshDict'
                : 'Use Gmsh or Fluent for optimized coordinates'}
            </small>
          </span>
          <Icon name="download" size={16} />
        </button>
        <button
          className="export-button"
          disabled={!canExport || !browserFluentSafe}
          onClick={() => {
            void runExport('FLUENT');
          }}
          type="button"
        >
          <span className="export-icon">
            <Icon name="mesh" size={19} />
          </span>
          <span>
            <strong>
              {exporting === 'FLUENT'
                ? 'Preparing…'
                : browserFluentSafe
                  ? 'Ansys Fluent'
                  : 'Fluent · native export'}
            </strong>
            <small>
              {fluentAssessment === undefined
                ? 'Native legacy .msh'
                : `${fluentAssessment.dimension}D · ~${formatFileSize(fluentAssessment.estimatedBytes)}`}
            </small>
          </span>
          <Icon name="download" size={16} />
        </button>
        <button
          className="export-button"
          disabled={!canExport || !browserGmshSafe}
          onClick={() => {
            void runExport('GMSH');
          }}
          type="button"
        >
          <span className="export-icon">
            <Icon name="mesh" size={19} />
          </span>
          <span>
            <strong>
              {exporting === 'GMSH'
                ? 'Preparing…'
                : browserGmshSafe
                  ? 'Gmsh'
                  : 'Gmsh · native export'}
            </strong>
            <small>
              {gmshAssessment === undefined
                ? `ASCII ${configuration.output.gmshVersion}`
                : `~${formatFileSize(gmshAssessment.estimatedBytes)} ASCII`}
            </small>
          </span>
          <Icon name="download" size={16} />
        </button>
        {!browserGmshSafe &&
        gmshAssessment?.crossSectionBrowserSafe === true ? (
          <button
            className="export-button"
            disabled={!canExport}
            onClick={() => {
              void runExport('GMSH_2D');
            }}
            type="button"
          >
            <span className="export-icon">
              <Icon name="mesh" size={19} />
            </span>
            <span>
              <strong>
                {exporting === 'GMSH_2D'
                  ? 'Preparing…'
                  : 'Gmsh 2D cross-section'}
              </strong>
              <small>
                ~{formatFileSize(gmshAssessment.crossSectionEstimatedBytes)}
              </small>
            </span>
            <Icon name="download" size={16} />
          </button>
        ) : null}
        <button
          className="export-button"
          onClick={() => {
            downloadText(
              `${JSON.stringify(configuration, undefined, 2)}\n`,
              configurationFilename(configuration.airfoil.nacaCode),
              'application/json',
            );
          }}
          type="button"
        >
          <span className="export-icon">
            <Icon name="file" size={19} />
          </span>
          <span>
            <strong>Configuration</strong>
            <small>Mesh settings (.json)</small>
          </span>
          <Icon name="download" size={16} />
        </button>
      </div>
      <details className="export-options">
        <summary>Format options</summary>
        <label>
          <span>Gmsh version</span>
          <select
            onChange={(event) => {
              updateConfiguration({
                ...configuration,
                output: {
                  ...configuration.output,
                  gmshVersion: event.currentTarget.value as GmshVersion,
                },
              });
            }}
            value={configuration.output.gmshVersion}
          >
            <option value="2.2">2.2 · compatible</option>
            <option value="4.1">4.1 · entity blocks</option>
          </select>
        </label>
      </details>
      {exportError === undefined ? null : (
        <p className="export-error" role="alert">
          {exportError}
        </p>
      )}
      {!browserGmshSafe && gmshAssessment !== undefined ? (
        <p className="export-guidance">
          Full Gmsh is too large for a browser-safe in-memory download. Save the
          configuration and run the native streaming CLI with Gmsh 2.2.
        </p>
      ) : null}
      {!browserFluentSafe && fluentAssessment !== undefined ? (
        <p className="export-guidance">
          Full Fluent export is too large for a browser-safe download. Save the
          configuration and run the native streaming CLI:
          <code> mesh-cli fluent config.json -o mesh.msh</code>
        </p>
      ) : null}
      <p className="export-caveat">
        Patch classifications are included. Solver field values remain a case
        setup responsibility.
      </p>
    </section>
  );
}
