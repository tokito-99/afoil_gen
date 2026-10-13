import { useState, type ReactNode } from 'react';

import { errorMessage, planCellBudget } from '../../api/meshEngine.js';
import { useMeshWorkspace } from '../../state/meshStore.js';
import { formatCount, estimateCellCount } from '../../utils/format.js';
import { NumberField, SectionTitle, Toggle } from '../FormControls.js';

export function MeshForm(): ReactNode {
  const { configuration, report, updateConfiguration } = useMeshWorkspace();
  const mesh = configuration.mesh;
  const optimization = configuration.optimization;
  const [cellBudget, setCellBudget] = useState(999_000);
  const [planning, setPlanning] = useState(false);
  const [planMessage, setPlanMessage] = useState('');
  const issue = (field: string) =>
    report.issues.find((candidate) => candidate.field === field);
  const update = (values: Partial<typeof mesh>) => {
    updateConfiguration({
      ...configuration,
      mesh: { ...mesh, ...values },
    });
  };
  const updateOptimization = (values: Partial<typeof optimization>) => {
    updateConfiguration({
      ...configuration,
      optimization: { ...optimization, ...values },
    });
  };
  const runBudgetPlanner = async (): Promise<void> => {
    setPlanning(true);
    setPlanMessage('');
    try {
      await new Promise<void>((resolve) => {
        window.setTimeout(resolve, 0);
      });
      const plan = await planCellBudget(configuration, cellBudget);
      if (plan.feasible && plan.configuration !== undefined) {
        updateConfiguration(plan.configuration);
        const count = plan.counts?.hexCells ?? plan.counts?.quadCells;
        setPlanMessage(
          `Applied verified candidate${count === undefined ? '' : ` with ${formatCount(count)} cells`}. Regenerate the preview to inspect it.`,
        );
      } else {
        setPlanMessage(
          plan.reasons.at(-1) ?? 'No candidate met the unchanged targets.',
        );
      }
    } catch (error) {
      setPlanMessage(errorMessage(error));
    } finally {
      setPlanning(false);
    }
  };
  const cells = estimateCellCount(
    configuration.airfoil.finiteTrailingEdge,
    mesh.leadingEdgeCells,
    mesh.upperSurfaceCells,
    mesh.lowerSurfaceCells,
    mesh.wakeCells,
    mesh.radialCells,
    mesh.spanwiseCells,
  );

  return (
    <section className="form-section">
      <SectionTitle description={`${formatCount(cells.quads)} quads · ${formatCount(cells.hexes)} hexes`}>
        Resolution
      </SectionTitle>
      <div className="field-grid field-grid--two">
        <NumberField
          integer
          issue={issue('mesh.upperSurfaceCells')}
          label="Upper surface"
          min={1}
          onChange={(upperSurfaceCells) => {
            update({ upperSurfaceCells });
          }}
          step={1}
          value={mesh.upperSurfaceCells}
        />
        <NumberField
          integer
          issue={issue('mesh.lowerSurfaceCells')}
          label="Lower surface"
          min={1}
          onChange={(lowerSurfaceCells) => {
            update({ lowerSurfaceCells });
          }}
          step={1}
          value={mesh.lowerSurfaceCells}
        />
        <NumberField
          integer
          issue={issue('mesh.leadingEdgeCells')}
          label="Leading edge"
          min={1}
          onChange={(leadingEdgeCells) => {
            update({ leadingEdgeCells });
          }}
          step={1}
          value={mesh.leadingEdgeCells}
        />
        <NumberField
          integer
          issue={issue('mesh.wakeCells')}
          label="Wake"
          min={4}
          onChange={(wakeCells) => {
            update({ wakeCells });
          }}
          step={1}
          value={mesh.wakeCells}
        />
        <NumberField
          integer
          issue={issue('mesh.radialCells')}
          label="Radial"
          min={10}
          onChange={(radialCells) => {
            update({ radialCells });
          }}
          step={1}
          value={mesh.radialCells}
        />
        <NumberField
          integer
          issue={issue('mesh.spanwiseCells')}
          label="Spanwise"
          min={1}
          onChange={(spanwiseCells) => {
            update({ spanwiseCells });
          }}
          step={1}
          value={mesh.spanwiseCells}
        />
      </div>
      <details className="form-details">
        <summary>Directional grading</summary>
        <div className="field-grid field-grid--two">
          <NumberField
            issue={issue('mesh.surfaceGrading')}
            label="Surface"
            min={0}
            onChange={(surfaceGrading) => {
              update({ surfaceGrading });
            }}
            step={0.05}
            value={mesh.surfaceGrading}
          />
          <NumberField
            issue={issue('mesh.wakeGrading')}
            label="Wake"
            min={0}
            onChange={(wakeGrading) => {
              update({ wakeGrading });
            }}
            step={0.05}
            value={mesh.wakeGrading}
          />
          <NumberField
            issue={issue('mesh.spanwiseGrading')}
            label="Spanwise"
            min={0}
            onChange={(spanwiseGrading) => {
              update({ spanwiseGrading });
            }}
            step={0.05}
            value={mesh.spanwiseGrading}
          />
        </div>
      </details>
      <details className="form-details">
        <summary>Wall coordinates and smoothing</summary>
        <Toggle
          checked={optimization.targetCellCenterYPlus}
          description="Treat y+ as the distance to the first cell center"
          label="Cell-center y+"
          onChange={(targetCellCenterYPlus) => {
            updateOptimization({ targetCellCenterYPlus });
          }}
        />
        <Toggle
          checked={optimization.orthogonalizeWall}
          description="Move interior points only, with quality safeguards"
          label="Wall-normal adjustment"
          onChange={(orthogonalizeWall) => {
            updateOptimization({ orthogonalizeWall });
          }}
        />
        <NumberField
          integer
          issue={issue('optimization.smoothingIterations')}
          label="Smoothing passes"
          max={50}
          min={0}
          onChange={(smoothingIterations) => {
            updateOptimization({ smoothingIterations });
          }}
          step={1}
          value={optimization.smoothingIterations}
        />
        <p className="form-note">
          The optimizer accepts a move only when positivity, orthogonal quality,
          skewness, Jacobian, and growth do not regress.
        </p>
      </details>
      <details className="form-details">
        <summary>Fit a cell limit</summary>
        <NumberField
          integer
          issue={undefined}
          label="Maximum cells"
          min={1}
          onChange={setCellBudget}
          step={1_000}
          value={cellBudget}
        />
        <div className="preset-strip">
          <span>Targets and domain dimensions stay unchanged.</span>
          <button
            disabled={planning}
            onClick={() => void runBudgetPlanner()}
            type="button"
          >
            {planning ? 'Checking…' : 'Find mesh'}
          </button>
        </div>
        {planMessage ? <p className="form-note" role="status">{planMessage}</p> : null}
      </details>
    </section>
  );
}
