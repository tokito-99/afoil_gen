import { useRef, useState, type ReactNode } from 'react';

import {
  errorMessage,
  importWallShear,
  importWallShearProfile,
} from '../../api/meshEngine.js';

import {
  applyMeshIntentPreset,
  applyStudentWrlesPreset,
  studentWrlesCellCount,
} from '../../config/meshIntentPresets.js';
import { useMeshWorkspace } from '../../state/meshStore.js';
import type { MeshIntent } from '../../types/mesh.js';
import {
  estimateLesSizing,
  estimateWallSpacing,
  estimateWallSpacingFromWallShear,
} from '../../validation/configValidation.js';
import { formatCount, formatMetric } from '../../utils/format.js';
import { calculateReynoldsNumber } from '../../utils/flow.js';
import {
  NumberField,
  SectionTitle,
  SelectField,
} from '../FormControls.js';

export function LesForm(): ReactNode {
  const { configuration, report, updateConfiguration } = useMeshWorkspace();
  const currentConfiguration = useRef(configuration);
  currentConfiguration.current = configuration;
  const [importStatus, setImportStatus] = useState('');
  const [importError, setImportError] = useState('');
  const [importing, setImporting] = useState(false);
  const [profileImportStatus, setProfileImportStatus] = useState('');
  const [profileImportError, setProfileImportError] = useState('');
  const [profileImporting, setProfileImporting] = useState(false);
  const importCsv = async (file: File): Promise<void> => {
    setImportError('');
    setImportStatus('');
    setImporting(true);
    try {
      if (file.size > 10 * 1024 * 1024) {
        throw new Error('CSV exceeds 10 MB. Use the CLI for large wall-shear tables.');
      }
      const feedback = await importWallShear(await file.text());
      const latest = currentConfiguration.current;
      const calibratedLes = { ...latest.les, estimateFirstCellHeight: true,
        solverWallShearStress: feedback.selectedPa };
      delete calibratedLes.manualFirstCellHeight;
      delete calibratedLes.wallShearProfile;
      updateConfiguration({
        ...latest,
        les: calibratedLes,
      });
      setImportStatus(`Imported ${formatCount(feedback.sampleCount)} samples. Using maximum ${formatMetric(feedback.selectedPa, 4)} Pa.`);
    } catch (error) {
      setImportError(errorMessage(error));
    } finally {
      setImporting(false);
    }
  };
  const importProfileCsv = async (file: File): Promise<void> => {
    setProfileImportError('');
    setProfileImportStatus('');
    setProfileImporting(true);
    try {
      if (file.size > 10 * 1024 * 1024) {
        throw new Error('CSV exceeds 10 MB. Use the CLI for large wall-shear tables.');
      }
      const calibrated = await importWallShearProfile(
        currentConfiguration.current,
        await file.text(),
      );
      updateConfiguration(calibrated);
      setProfileImportStatus(
        `Imported ${formatCount(calibrated.les.wallShearProfile?.samples.length ?? 0)} spatial samples with matching flow provenance.`,
      );
    } catch (error) {
      setProfileImportError(errorMessage(error));
    } finally {
      setProfileImporting(false);
    }
  };
  const les = configuration.les;
  const isLes =
    configuration.meshIntent === 'WRLES' ||
    configuration.meshIntent === 'WMLES';
  const issue = (field: string) =>
    report.issues.find((candidate) => candidate.field === field);
  const update = (values: Partial<typeof les>) => {
    updateConfiguration({
      ...configuration,
      les: { ...les, ...values },
    });
  };
  const spacingYPlus = configuration.optimization.targetCellCenterYPlus
    ? 2 * les.targetYPlus
    : les.targetYPlus;
  const estimate =
    les.solverWallShearStress === undefined
      ? estimateWallSpacing(
          calculateReynoldsNumber(
            configuration.airfoil.chordLength,
            configuration.flow.freestreamVelocity,
            configuration.flow.density,
            configuration.flow.dynamicViscosity,
          ),
          configuration.flow.freestreamVelocity,
          configuration.flow.density,
          configuration.flow.dynamicViscosity,
          spacingYPlus,
        )
      : estimateWallSpacingFromWallShear(
          les.solverWallShearStress,
          configuration.flow.freestreamVelocity,
          configuration.flow.density,
          configuration.flow.dynamicViscosity,
          spacingYPlus,
        );
  const firstCellHeight = les.estimateFirstCellHeight
    ? estimate?.firstCellHeight
    : les.manualFirstCellHeight;
  const sizing = isLes ? estimateLesSizing(configuration) : undefined;
  const firstLayerSource: FirstLayerSource = !les.estimateFirstCellHeight
    ? 'MANUAL'
    : les.wallShearProfile !== undefined
      ? 'SPATIAL_PROFILE'
      : les.solverWallShearStress === undefined
      ? 'PRE_SOLVE'
      : 'SOLVER_FEEDBACK';

  return (
    <section className="form-section">
      <SectionTitle description="Cost and near-wall resolution for the intended solver">
        Mesh strategy
      </SectionTitle>
      <SelectField<MeshIntent>
        hint="Changing mode applies a safe starting mesh"
        issue={issue('meshIntent')}
        label="Analysis mode"
        onChange={(meshIntent) => {
          updateConfiguration(applyMeshIntentPreset(configuration, meshIntent));
        }}
        options={[
          { label: 'RANS · wall resolved', value: 'RANS_WALL_RESOLVED' },
          { label: 'RANS · wall function', value: 'RANS_WALL_FUNCTION' },
          { label: 'Wall-modelled LES', value: 'WMLES' },
          { label: 'Wall-resolved LES', value: 'WRLES' },
        ]}
        value={configuration.meshIntent}
      />
      <p className="form-note">
        {meshIntentDescription(configuration.meshIntent)}
      </p>
      {configuration.meshIntent === 'WRLES' ? (
        <div className="preset-strip">
          <span>
            ANSYS Student starter ·{' '}
            {formatCount(
              studentWrlesCellCount(
                configuration.airfoil.finiteTrailingEdge,
              ),
            )}{' '}
            cells
          </span>
          <button
            onClick={() => {
              updateConfiguration(applyStudentWrlesPreset(configuration));
            }}
            type="button"
          >
            Apply
          </button>
        </div>
      ) : null}
      <SectionTitle description="Structured wall-normal cells; no separate prism layer">
        Wall layers
      </SectionTitle>
      <div className="calculated-strip">
        <span>
          {firstLayerSource === 'SPATIAL_PROFILE'
            ? 'Spatially calibrated first layer'
            : firstLayerSource === 'SOLVER_FEEDBACK'
              ? 'Solver-calibrated first layer'
            : firstLayerSource === 'MANUAL'
              ? 'Manual first layer'
              : 'Estimated first layer'}
        </span>
        <strong>{formatMetric(firstCellHeight, 5)} m</strong>
      </div>
      <div className="field-grid field-grid--two">
        <NumberField
          issue={issue('les.targetYPlus')}
          label="Target y+"
          min={0}
          onChange={(targetYPlus) => {
            update({ targetYPlus });
          }}
          step={0.5}
          value={les.targetYPlus}
        />
        {isLes ? (
          <NumberField
            issue={issue('les.targetStreamwisePlus')}
            label="Target Δs+"
            min={0}
            onChange={(targetStreamwisePlus) => {
              update({ targetStreamwisePlus });
            }}
            step={5}
            value={les.targetStreamwisePlus}
          />
        ) : null}
        {isLes ? (
          <NumberField
            issue={issue('les.targetSpanwisePlus')}
            label="Target Δz+"
            min={0}
            onChange={(targetSpanwisePlus) => {
              update({ targetSpanwisePlus });
            }}
            step={2}
            value={les.targetSpanwisePlus}
          />
        ) : null}
        <NumberField
          hint={
            configuration.meshIntent === 'WRLES'
              ? 'Prefer ≤ 1.1 near the wall'
              : 'Maximum adjacent-cell ratio'
          }
          issue={issue('les.maxRadialGrowthRatio')}
          label="Max growth"
          min={1}
          onChange={(maxRadialGrowthRatio) => {
            update({ maxRadialGrowthRatio });
          }}
          step={0.01}
          value={les.maxRadialGrowthRatio}
        />
        <NumberField
          issue={issue('les.referenceBoundaryLayerThicknessChords')}
          label="Layer thickness"
          min={0}
          onChange={(referenceBoundaryLayerThicknessChords) => {
            update({ referenceBoundaryLayerThicknessChords });
          }}
          step={0.005}
          unit="c"
          value={les.referenceBoundaryLayerThicknessChords}
        />
        <NumberField
          integer
          issue={issue('les.minimumCellsAcrossBoundaryLayer')}
          label="Minimum layers"
          min={8}
          onChange={(minimumCellsAcrossBoundaryLayer) => {
            update({ minimumCellsAcrossBoundaryLayer });
          }}
          step={1}
          value={les.minimumCellsAcrossBoundaryLayer}
        />
      </div>
      <SelectField<FirstLayerSource>
        hint="Use solver feedback after the pilot run"
        issue={undefined}
        label="First-layer source"
        onChange={(source) => {
          if (source === 'PRE_SOLVE') {
            const nextLes = { ...les,
              estimateFirstCellHeight: true,
              solverWallShearStress: undefined,
            };
            delete nextLes.wallShearProfile;
            updateConfiguration({ ...configuration, les: nextLes });
          } else if (source === 'SOLVER_FEEDBACK') {
            const nextLes = { ...les,
              estimateFirstCellHeight: true,
              solverWallShearStress:
                les.solverWallShearStress ?? Number.NaN,
            };
            delete nextLes.wallShearProfile;
            updateConfiguration({ ...configuration, les: nextLes });
          } else if (source === 'MANUAL') {
            const nextLes = { ...les,
              estimateFirstCellHeight: false,
              solverWallShearStress: undefined,
              manualFirstCellHeight:
                les.manualFirstCellHeight ??
                estimate?.firstCellHeight ??
                0.000_01,
            };
            delete nextLes.wallShearProfile;
            updateConfiguration({ ...configuration, les: nextLes });
          } else if (les.wallShearProfile === undefined) {
            setProfileImportStatus('Choose a spatial-profile CSV below.');
          }
        }}
        options={[
          { label: 'Pre-solve estimate', value: 'PRE_SOLVE' },
          { label: 'Fluent wall shear', value: 'SOLVER_FEEDBACK' },
          ...(les.wallShearProfile === undefined
            ? []
            : [
                {
                  label: 'Spatial solver profile',
                  value: 'SPATIAL_PROFILE' as const,
                },
              ]),
          { label: 'Manual height', value: 'MANUAL' },
        ]}
        value={firstLayerSource}
      />
      {firstLayerSource === 'SOLVER_FEEDBACK' ? (
        <>
        <NumberField
          hint="Maximum time-averaged wall shear on airfoil_wall"
          issue={issue('les.solverWallShearStress')}
          label="Solved max wall shear"
          min={0}
          onChange={(solverWallShearStress) => {
            update({ solverWallShearStress });
          }}
          unit="Pa"
          value={les.solverWallShearStress ?? Number.NaN}
        />
        <details className="quality-details wall-shear-import">
          <summary>Import pilot-run wall shear</summary>
          <p className="form-note">
            CSV column: wallShearStressPa. Use nonnegative magnitudes in Pa
            from the airfoil wall of a matching pilot run. The maximum is used;
            no samples are silently dropped.
          </p>
          <label className="field-label" htmlFor="wall-shear-csv">Wall-shear CSV</label>
          <input
            id="wall-shear-csv"
            type="file"
            accept=".csv,text/csv"
            disabled={importing}
            onChange={(event) => {
              const file = event.currentTarget.files?.[0];
              event.currentTarget.value = '';
              if (file !== undefined) void importCsv(file);
            }}
          />
          <p className="form-note" role="status">{importing ? 'Reading wall shear…' : importStatus}</p>
          {importError ? <p className="form-note" role="alert">{importError}</p> : null}
        </details>
        </>
      ) : null}
      {firstLayerSource === 'MANUAL' ? (
        <NumberField
          issue={issue('les.manualFirstCellHeight')}
          label="Manual first cell"
          min={0}
          onChange={(manualFirstCellHeight) => {
            update({ manualFirstCellHeight });
          }}
          unit="m"
          value={les.manualFirstCellHeight ?? 0}
        />
      ) : null}
      <details className="quality-details wall-shear-import">
        <summary>Import spatial wall data</summary>
        <p className="form-note">
          Matching pilot data can vary spacing over each airfoil side. Required
          columns are side, xOverChord, and wallShearStressPa. Optional signed
          shear, intermittency, and boundary-layer thickness are used only when
          supplied by the solver.
        </p>
        <label className="field-label" htmlFor="wall-shear-profile-csv">
          Spatial profile CSV
        </label>
        <input
          id="wall-shear-profile-csv"
          type="file"
          accept=".csv,text/csv"
          disabled={profileImporting}
          onChange={(event) => {
            const file = event.currentTarget.files?.[0];
            event.currentTarget.value = '';
            if (file !== undefined) void importProfileCsv(file);
          }}
        />
        <p className="form-note" role="status">
          {profileImporting ? 'Reading wall data…' : profileImportStatus}
        </p>
        {profileImportError ? <p className="form-note" role="alert">{profileImportError}</p> : null}
      </details>
      {isLes && sizing !== undefined ? (
        <div className="les-sizing-summary">
          <div>
            <span>Target Δs</span>
            <strong>{formatMetric(sizing.targetStreamwiseSpacing, 3)} m</strong>
          </div>
          <div>
            <span>Target Δz</span>
            <strong>{formatMetric(sizing.targetSpanwiseSpacing, 3)} m</strong>
          </div>
          <div>
            <span>Suggested Nz</span>
            <strong>{sizing.recommendedSpanwiseCells}</strong>
          </div>
        </div>
      ) : null}
    </section>
  );
}

type FirstLayerSource =
  | 'PRE_SOLVE'
  | 'SOLVER_FEEDBACK'
  | 'SPATIAL_PROFILE'
  | 'MANUAL';

function meshIntentDescription(intent: MeshIntent): string {
  switch (intent) {
    case 'RANS_WALL_RESOLVED':
      return '2D low-y+ mesh for SA, SST, or transition-sensitive RANS.';
    case 'RANS_WALL_FUNCTION':
      return 'Lower-cost 2D mesh for a solver wall function matched to the resulting y+.';
    case 'WMLES':
      return 'Periodic 3D mesh with wall-model exchange-height and spanwise checks.';
    case 'WRLES':
      return 'High-resolution periodic 3D mesh with wall-unit checks in all directions.';
  }
}
