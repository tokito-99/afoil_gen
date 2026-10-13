import type { ReactNode } from 'react';

import { useMeshWorkspace } from '../state/meshStore.js';
import { estimateCellCount, formatCount, formatMetric } from '../utils/format.js';
import { estimateLesSizing } from '../validation/configValidation.js';
import { Icon } from './Icons.js';

export function QualityPanel(): ReactNode {
  const { configuration, dirty, engineError, engineStatus, preview, report } =
    useMeshWorkspace();
  const errors = report.issues.filter((issue) => issue.severity === 'ERROR');
  const warnings = report.issues.filter(
    (issue) => issue.severity === 'WARNING',
  );
  const qualityWarnings = preview?.quality.warnings ?? [];
  const estimated = estimateCellCount(
    configuration.airfoil.finiteTrailingEdge,
    configuration.mesh.leadingEdgeCells,
    configuration.mesh.upperSurfaceCells,
    configuration.mesh.lowerSurfaceCells,
    configuration.mesh.wakeCells,
    configuration.mesh.radialCells,
    configuration.mesh.spanwiseCells,
  );
  const isLes =
    configuration.meshIntent === 'WRLES' ||
    configuration.meshIntent === 'WMLES';
  const lesSizing = isLes
    ? !dirty && preview?.lesSizing !== undefined
      ? preview.lesSizing
      : estimateLesSizing(configuration)
    : undefined;
  const valid = report.valid && (preview?.quality.valid ?? true);
  const pending = engineStatus === 'LOADING' || engineStatus === 'GENERATING';

  return (
    <section className="quality-card panel-card">
      <div className="card-heading">
        <div>
          <span>Status</span>
          <h2>
            {pending
              ? 'Preparing preview'
              : valid
                ? 'Mesh ready'
                : 'Review settings'}
          </h2>
        </div>
        <span
          className={`health-mark ${pending ? 'is-pending' : valid ? 'is-valid' : 'is-invalid'}`}
        >
          <Icon name={pending ? 'mesh' : valid ? 'check' : 'error'} size={18} />
        </span>
      </div>
      <div className="quality-grid">
        <Metric
          caution={
            configuration.extrusion.enabled && estimated.hexes >= 1_000_000
          }
          label={configuration.extrusion.enabled ? 'Hex cells' : 'Quad cells'}
          value={formatCount(
            configuration.extrusion.enabled ? estimated.hexes : estimated.quads,
          )}
        />
        <Metric
          label="Reference layer"
          value={`${formatMetric(preview?.wallSpacing?.firstCellHeight, 3)} m`}
        />
        <Metric
          label="Radial growth"
          value={formatMetric(preview?.quality.maximumRadialGrowthRatio, 4)}
        />
        <Metric
          caution={(preview?.quality.maximumAspectRatio ?? 0) > 1_000}
          label="Max aspect"
          value={formatMetric(preview?.quality.maximumAspectRatio, 3)}
        />
      </div>
      {dirty && preview !== undefined ? (
        <p className="quality-note">Values reflect the last mesh update.</p>
      ) : null}
      {preview?.wallResolution === undefined ? null : (
        <details className="quality-details">
          <summary>Wall resolution audit{preview.wallResolution.warnings.length > 0 ? ' · review' : ''}</summary>
          <dl>
            <div><dt>Airfoil wall faces</dt><dd>{formatCount(preview.wallResolution.wallFaceCount)}</dd></div>
            <div><dt>Measured normal height</dt><dd>{formatMetric(preview.wallResolution.minimumNormalHeight, 3)}–{formatMetric(preview.wallResolution.maximumNormalHeight, 3)} m</dd></div>
            <div><dt>Cell-center wall distance</dt><dd>{formatMetric(preview.wallResolution.minimumCentroidDistance, 3)}–{formatMetric(preview.wallResolution.maximumCentroidDistance, 3)} m</dd></div>
            <div><dt>Predicted cell-center y+</dt><dd>{formatMetric(preview.wallResolution.minimumPredictedYPlus, 2)}–{formatMetric(preview.wallResolution.maximumPredictedYPlus, 2)}</dd></div>
            <div><dt>Maximum predicted Δs+</dt><dd>{formatMetric(preview.wallResolution.maximumPredictedStreamwisePlus, 2)}</dd></div>
            <div><dt>Cells in measured layer</dt><dd>{preview.wallResolution.minimumCellsInReferenceLayer}</dd></div>
            <div><dt>Solver-marked transition faces</dt><dd>{preview.wallResolution.transitionalFaceCount}</dd></div>
            <div><dt>Solver-marked reverse-shear faces</dt><dd>{preview.wallResolution.reverseShearFaceCount}</dd></div>
            <div><dt>Worst wall nonorthogonality</dt><dd>{formatMetric(preview.wallResolution.maximumNonorthogonalityDeg, 1)}°</dd></div>
            <div><dt>Reference wall shear</dt><dd>{formatMetric(preview.wallResolution.referenceWallShearPa, 3)} Pa</dd></div>
          </dl>
          <p className="quality-note">
            {shearSourceLabel(preview.wallResolution.shearSource)}.
            {' '}{preview.wallResolution.shearSource === 'SPATIAL_SOLVER_PROFILE'
              ? 'Predictions use the imported local profile.'
              : 'Predictions use one reference shear, not a solved local y+ field.'}
            Layer height is not cell-center wall distance. Verify wall y+ in Fluent.
          </p>
          {preview.wallResolution.warnings.map((message) => (
            <Issue key={message} message={message} severity="WARNING" />
          ))}
        </details>
      )}
      <details className="quality-details">
        <summary>Detailed metrics</summary>
        <dl>
          <div>
            <dt>Minimum area</dt>
            <dd>{formatMetric(preview?.quality.minimumCellArea, 4)}</dd>
          </div>
          <div>
            <dt>Minimum volume</dt>
            <dd>{formatMetric(preview?.quality3d?.minimumCellVolume, 4)}</dd>
          </div>
          <div>
            <dt>Minimum orthogonal quality</dt>
            <dd>
              {formatMetric((preview?.quality3d ?? preview?.quality)?.minimumOrthogonalQuality, 4)}
            </dd>
          </div>
          <div>
            <dt>Maximum equiangle skewness</dt>
            <dd>
              {formatMetric((preview?.quality3d ?? preview?.quality)?.maximumEquiangleSkewness, 4)}
            </dd>
          </div>
          <div>
            <dt>Minimum scaled Jacobian</dt>
            <dd>{formatMetric((preview?.quality3d ?? preview?.quality)?.minimumScaledJacobian, 4)}</dd>
          </div>
          <div>
            <dt>Shortest edge</dt>
            <dd>{formatMetric(preview?.quality.minimumEdgeLength, 4)} m</dd>
          </div>
          <div>
            <dt>Wall-layer region</dt>
            <dd>
              {preview === undefined
                ? '—'
                : `${preview.wallLayerCells} cells · ${formatMetric(preview.wallLayerThickness, 3)} m`}
            </dd>
          </div>
          <div>
            <dt>Longest edge</dt>
            <dd>{formatMetric(preview?.quality.maximumEdgeLength, 4)} m</dd>
          </div>
          <div>
            <dt>Worst-aspect block</dt>
            <dd>
              {preview?.quality.worstAspectCell?.blockName ??
                (preview?.quality.worstAspectCell === undefined
                  ? '—'
                  : `Block ${preview.quality.worstAspectCell.blockId}`)}
            </dd>
          </div>
        </dl>
      </details>
      {preview?.optimization === undefined ? null : (
        <details className="quality-details">
          <summary>Coordinate optimization</summary>
          <dl>
            <div><dt>Wall step</dt><dd>{preview.optimization.wallStepAccepted ? 'Accepted' : 'Retained original'}</dd></div>
            <div><dt>Smoothing passes</dt><dd>{preview.optimization.acceptedIterations} / {preview.optimization.requestedIterations}</dd></div>
            <div><dt>Orthogonal quality</dt><dd>{formatMetric(preview.optimization.initialMinimumOrthogonalQuality, 3)} → {formatMetric(preview.optimization.finalMinimumOrthogonalQuality, 3)}</dd></div>
            <div><dt>Scaled Jacobian</dt><dd>{formatMetric(preview.optimization.initialMinimumScaledJacobian, 3)} → {formatMetric(preview.optimization.finalMinimumScaledJacobian, 3)}</dd></div>
            <div><dt>Maximum displacement</dt><dd>{formatMetric(preview.optimization.maximumDisplacement, 3)} m</dd></div>
          </dl>
          {preview.optimization.warnings.map((message) => (
            <Issue key={message} message={message} severity="WARNING" />
          ))}
        </details>
      )}
      {lesSizing === undefined ? null : (
        <div className="les-readiness">
          <div className="les-readiness__heading">
            <div>
              <span>{configuration.meshIntent}</span>
              <strong>LES readiness</strong>
            </div>
            <span
              className={
                lesSizing.periodicThreeDimensional
                  ? 'readiness-pill is-ready'
                  : 'readiness-pill'
              }
            >
              {lesSizing.periodicThreeDimensional ? '3D periodic' : 'Needs 3D'}
            </span>
          </div>
          <dl>
            <div>
              <dt>Maximum Δs+</dt>
              <dd>
                {formatMetric(
                  lesSizing.estimatedMaximumSurfaceStreamwisePlus,
                  1,
                )}
                <small> / {configuration.les.targetStreamwisePlus}</small>
              </dd>
            </div>
            <div>
              <dt>Maximum Δz+</dt>
              <dd>
                {formatMetric(lesSizing.estimatedMaximumSpanwisePlus, 1)}
                <small> / {configuration.les.targetSpanwisePlus}</small>
              </dd>
            </div>
            <div>
              <dt>Spanwise cells</dt>
              <dd>
                {configuration.mesh.spanwiseCells}
                <small> / ~{lesSizing.recommendedSpanwiseCells}</small>
              </dd>
            </div>
            <div>
              <dt>Periodic span</dt>
              <dd>
                {formatMetric(lesSizing.spanwiseExtentChords, 3)}c
                <small> / ≥ 0.1c screening</small>
              </dd>
            </div>
            <div>
              <dt>Cells in reference layer</dt>
              <dd>
                {lesSizing.radialCellsWithinBoundaryLayer}
                <small>
                  {' '}
                  / {configuration.les.minimumCellsAcrossBoundaryLayer}
                </small>
              </dd>
            </div>
            <div>
              <dt>Reynolds number</dt>
              <dd>{formatMetric(lesSizing.propertyReynoldsNumber, 2)}</dd>
            </div>
          </dl>
        </div>
      )}
      <div className="issue-stack">
        {preview?.warnings.filter((issue) => issue.code === 'WALL_AUDIT_FAILED').map((issue) => (
          <Issue key={issue.code} message={issue.message} severity="WARNING" />
        ))}
        {engineError === undefined ? null : (
          <Issue message={engineError} severity="ERROR" />
        )}
        {errors.slice(0, 3).map((issue) => (
          <Issue key={issue.code + issue.field} message={issue.message} severity="ERROR" />
        ))}
        {warnings.slice(0, 2).map((issue) => (
          <Issue
            key={issue.code + issue.field}
            message={issue.message}
            severity="WARNING"
          />
        ))}
        {qualityWarnings.slice(0, 2).map((message) => (
          <Issue key={message} message={message} severity="WARNING" />
        ))}
      </div>
    </section>
  );
}

function shearSourceLabel(
  source: 'SPATIAL_SOLVER_PROFILE' | 'SOLVER_FEEDBACK' | 'FLAT_PLATE_CORRELATION',
): string {
  switch (source) {
    case 'SPATIAL_SOLVER_PROFILE':
      return 'Spatial pilot-solver shear';
    case 'SOLVER_FEEDBACK':
      return 'Pilot-solver maximum shear';
    case 'FLAT_PLATE_CORRELATION':
      return 'Flat-plate pre-solve fallback';
  }
}

function Metric({
  caution = false,
  label,
  value,
}: {
  caution?: boolean;
  label: string;
  value: string;
}): ReactNode {
  return (
    <div className={caution ? 'metric is-caution' : 'metric'}>
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

function Issue({
  message,
  severity,
}: {
  message: string;
  severity: 'ERROR' | 'WARNING';
}): ReactNode {
  return (
    <div className={`issue issue--${severity.toLowerCase()}`}>
      <Icon name={severity === 'ERROR' ? 'error' : 'warning'} size={15} />
      <p>{message}</p>
    </div>
  );
}
