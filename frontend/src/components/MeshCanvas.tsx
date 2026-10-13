import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
  type WheelEvent as ReactWheelEvent,
} from 'react';

import { useMeshWorkspace } from '../state/meshStore.js';
import type {
  BoundaryRole,
  MeshPreview,
  Point2,
  PreviewLevel,
} from '../types/mesh.js';
import { formatMetric } from '../utils/format.js';
import { Icon } from './Icons.js';

interface ViewBox {
  height: number;
  width: number;
  x: number;
  y: number;
}

interface DragState {
  clientX: number;
  clientY: number;
  viewBox: ViewBox;
}

export function MeshCanvas(): ReactNode {
  const {
    dirty,
    engineError,
    engineStatus,
    generate,
    preview,
    previewLevel,
    report,
    setPreviewLevel,
  } = useMeshWorkspace();
  const svgRef = useRef<SVGSVGElement>(null);
  const drag = useRef<DragState | undefined>(undefined);
  const domainView = useMemo(() => fitDomainView(preview), [preview]);
  const nearfieldView = useMemo(
    () => fitNearfieldView(preview, domainView),
    [domainView, preview],
  );
  const [viewBox, setViewBox] = useState<ViewBox>(nearfieldView);
  const [showMesh, setShowMesh] = useState(true);
  const [showLabels, setShowLabels] = useState(false);
  const [developerMode, setDeveloperMode] = useState(false);

  useEffect(() => {
    setViewBox(nearfieldView);
  }, [nearfieldView]);

  const zoom = (factor: number) => {
    setViewBox((current) => zoomView(current, factor, 0.5, 0.5));
  };

  const onPointerDown = (event: ReactPointerEvent<SVGSVGElement>) => {
    event.currentTarget.setPointerCapture(event.pointerId);
    drag.current = {
      clientX: event.clientX,
      clientY: event.clientY,
      viewBox,
    };
  };
  const onPointerMove = (event: ReactPointerEvent<SVGSVGElement>) => {
    if (drag.current === undefined) {
      return;
    }
    const bounds = event.currentTarget.getBoundingClientRect();
    const dx =
      ((event.clientX - drag.current.clientX) / bounds.width) *
      drag.current.viewBox.width;
    const dy =
      ((event.clientY - drag.current.clientY) / bounds.height) *
      drag.current.viewBox.height;
    setViewBox({
      ...drag.current.viewBox,
      x: drag.current.viewBox.x - dx,
      y: drag.current.viewBox.y - dy,
    });
  };
  const onPointerUp = (event: ReactPointerEvent<SVGSVGElement>) => {
    event.currentTarget.releasePointerCapture(event.pointerId);
    drag.current = undefined;
  };
  const onWheel = (event: ReactWheelEvent<SVGSVGElement>) => {
    event.preventDefault();
    const bounds = event.currentTarget.getBoundingClientRect();
    const focusX = (event.clientX - bounds.left) / bounds.width;
    const focusY = (event.clientY - bounds.top) / bounds.height;
    setViewBox((current) =>
      zoomView(current, event.deltaY > 0 ? 1.12 : 0.89, focusX, focusY),
    );
  };

  return (
    <section className="mesh-workspace panel">
      <div className="mesh-toolbar">
        <div className="mesh-toolbar__title">
          <div>
            <span>Preview</span>
            <strong>
              {preview === undefined
                ? 'Loading mesh'
                : `${preview.quality.blockCount} conformal blocks`}
            </strong>
          </div>
          {dirty && preview !== undefined ? (
            <span className="stale-pill">Update required</span>
          ) : null}
        </div>
        <div className="mesh-toolbar__controls">
          <label className="toolbar-select">
            <span>Detail</span>
            <select
              onChange={(event) => {
                setPreviewLevel(event.currentTarget.value as PreviewLevel);
              }}
              value={previewLevel}
            >
              <option value="GEOMETRY_ONLY">Geometry</option>
              <option value="BLOCK_TOPOLOGY">Blocks</option>
              <option value="COARSE_MESH">Coarse mesh</option>
              <option value="FULL_MESH">Full mesh</option>
            </select>
          </label>
          <button
            aria-pressed={showMesh}
            className={`layer-button ${showMesh ? 'is-active' : ''}`}
            onClick={() => {
              setShowMesh((value) => !value);
            }}
            type="button"
          >
            Grid
          </button>
          <button
            aria-pressed={showLabels}
            className={`layer-button ${showLabels ? 'is-active' : ''}`}
            onClick={() => {
              setShowLabels((value) => !value);
            }}
            type="button"
          >
            Labels
          </button>
          <button
            aria-pressed={developerMode}
            className={`layer-button ${developerMode ? 'is-active' : ''}`}
            onClick={() => {
              setDeveloperMode((value) => !value);
            }}
            type="button"
          >
            Point IDs
          </button>
          <button
            className="button button--primary generate-button"
            disabled={!report.valid || engineStatus !== 'READY'}
            onClick={() => {
              void generate();
            }}
            type="button"
          >
            <Icon name="play" size={15} />
            Update mesh
          </button>
        </div>
      </div>

      <div
        className={`mesh-stage ${drag.current === undefined ? '' : 'is-dragging'}`}
      >
        {preview === undefined ? (
          <EmptyViewport
            error={engineError}
            loading={engineStatus !== 'ERROR'}
          />
        ) : (
          <svg
            aria-label="Interactive structured C-grid preview"
            onPointerDown={onPointerDown}
            onPointerMove={onPointerMove}
            onPointerUp={onPointerUp}
            onWheel={onWheel}
            ref={svgRef}
            role="img"
            viewBox={`${viewBox.x} ${viewBox.y} ${viewBox.width} ${viewBox.height}`}
          >
            <defs>
              <pattern
                height={viewBox.height / 18}
                id="minor-grid"
                patternUnits="userSpaceOnUse"
                width={viewBox.width / 18}
              >
                <path
                  className="viewport-grid"
                  d={`M ${viewBox.width / 18} 0 L 0 0 0 ${viewBox.height / 18}`}
                  vectorEffect="non-scaling-stroke"
                />
              </pattern>
            </defs>
            <rect
              fill="url(#minor-grid)"
              height={viewBox.height}
              width={viewBox.width}
              x={viewBox.x}
              y={viewBox.y}
            />
            <g className="viewport-axes">
              <line
                vectorEffect="non-scaling-stroke"
                x1={viewBox.x}
                x2={viewBox.x + viewBox.width}
                y1={0}
                y2={0}
              />
              <line
                vectorEffect="non-scaling-stroke"
                x1={0}
                x2={0}
                y1={viewBox.y}
                y2={viewBox.y + viewBox.height}
              />
            </g>
            <PreviewGeometry
              developerMode={developerMode}
              preview={preview}
              showLabels={showLabels}
              showMesh={showMesh}
              viewBox={viewBox}
            />
          </svg>
        )}
        <div className="viewport-legend">
          <span>
            <i className="legend-line legend-line--mesh" />
            mesh
          </span>
          <span>
            <i className="legend-line legend-line--inlet" />
            inlet
          </span>
          <span>
            <i className="legend-line legend-line--outlet" />
            outlet
          </span>
          <span>
            <i className="legend-line legend-line--airfoil" />
            airfoil
          </span>
          <span>
            <i className="legend-line legend-line--farfield" />
            farfield
          </span>
        </div>
        <div className="zoom-controls">
          <button
            aria-label="Zoom in"
            onClick={() => {
              zoom(0.8);
            }}
            title="Zoom in"
            type="button"
          >
            <Icon name="zoom-in" size={17} />
          </button>
          <button
            aria-label="Zoom out"
            onClick={() => {
              zoom(1.25);
            }}
            title="Zoom out"
            type="button"
          >
            <Icon name="zoom-out" size={17} />
          </button>
          <button
            aria-label="Focus airfoil and near wake"
            onClick={() => {
              setViewBox(nearfieldView);
            }}
            title="Focus airfoil"
            type="button"
          >
            <Icon name="airfoil" size={17} />
          </button>
          <button
            aria-label="Fit full domain"
            onClick={() => {
              setViewBox(domainView);
            }}
            title="Fit full domain"
            type="button"
          >
            <Icon name="fit" size={17} />
          </button>
        </div>
        <div className="viewport-hint">Scroll to zoom · drag to pan</div>
      </div>
    </section>
  );
}

function PreviewGeometry({
  developerMode,
  preview,
  showLabels,
  showMesh,
  viewBox,
}: {
  developerMode: boolean;
  preview: MeshPreview;
  showLabels: boolean;
  showMesh: boolean;
  viewBox: ViewBox;
}): ReactNode {
  const airfoilPath = [
    ...preview.airfoilUpper,
    ...[...preview.airfoilLower].reverse(),
  ];
  const firstCell = showLabels
    ? firstCellIndicator(preview, viewBox)
    : undefined;

  return (
    <>
      <polyline
        className="outer-boundary"
        points={pointList(preview.outerBoundary)}
        vectorEffect="non-scaling-stroke"
      />
      {showMesh
        ? preview.meshLines.map((line, index) => (
            <polyline
              className="mesh-line"
              key={`${line.label}-${index}`}
              points={pointList(line.points)}
              vectorEffect="non-scaling-stroke"
            />
          ))
        : null}
      {preview.blockBoundaries.map((line, index) => (
        <polyline
          className="block-line"
          key={`${line.label}-${index}`}
          points={pointList(line.points)}
          vectorEffect="non-scaling-stroke"
        />
      ))}
      <polyline
        className="wake-line"
        points={pointList(preview.wakeUpper)}
        vectorEffect="non-scaling-stroke"
      />
      <polyline
        className="wake-line"
        points={pointList(preview.wakeLower)}
        vectorEffect="non-scaling-stroke"
      />
      <polygon
        className="airfoil-shape"
        points={pointList(airfoilPath)}
        vectorEffect="non-scaling-stroke"
      />
      {preview.boundaryPatches.map((patch) => (
        <path
          className={`boundary-patch ${boundaryRoleClass(patch.role)}`}
          d={segmentPath(patch.segments)}
          key={patch.role}
          vectorEffect="non-scaling-stroke"
        />
      ))}
      {firstCell === undefined ? null : (
        <g className="first-cell-indicator">
          <line
            vectorEffect="non-scaling-stroke"
            x1={firstCell.start.x}
            x2={firstCell.end.x}
            y1={firstCell.start.y}
            y2={firstCell.end.y}
          />
          <circle
            cx={firstCell.end.x}
            cy={firstCell.end.y}
            r={viewBox.width * 0.002}
          />
          <text
            fontSize={viewBox.width * 0.0075}
            x={firstCell.end.x + viewBox.width * 0.0045}
            y={firstCell.end.y - viewBox.height * 0.008}
          >
            δ₁ {formatMetric(firstCell.actual, 3)} m · exaggerated
          </text>
        </g>
      )}
      {showLabels
        ? preview.boundaryLabels.map((label) => {
            const point = toSvg(label.position);
            return (
              <g
                className={`boundary-label ${boundaryRoleClass(label.role)}`}
                key={label.role}
              >
                <circle
                  cx={point.x}
                  cy={point.y}
                  r={viewBox.width * 0.0025}
                />
                <text
                  fontSize={viewBox.width * 0.0075}
                  x={point.x + viewBox.width * 0.0045}
                  y={point.y - viewBox.height * 0.006}
                >
                  {label.name}
                </text>
              </g>
            );
          })
        : null}
      {developerMode
        ? sampledIndices(preview.airfoilUpper).map(({ index, point }) => {
            const mapped = toSvg(point);
            return (
              <text
                className="point-index"
                fontSize={viewBox.width * 0.0065}
                key={`upper-${index}`}
                x={mapped.x}
                y={mapped.y - viewBox.height * 0.006}
              >
                {index}
              </text>
            );
          })
        : null}
      {preview.quality.errors.length > 0 ? (
        <rect
          className="invalid-overlay"
          height={viewBox.height}
          width={viewBox.width}
          x={viewBox.x}
          y={viewBox.y}
        />
      ) : null}
    </>
  );
}

function EmptyViewport({
  error,
  loading,
}: {
  error: string | undefined;
  loading: boolean;
}): ReactNode {
  return (
    <div className="empty-viewport">
      <div className={loading ? 'engine-loader' : 'engine-loader is-error'}>
        <Icon name={loading ? 'mesh' : 'error'} size={28} />
      </div>
      <strong>{loading ? 'Loading mesh preview' : 'Preview unavailable'}</strong>
      <p>
        {loading
          ? 'Preparing the first validated mesh preview.'
          : (error ?? 'Reload the page and try again.')}
      </p>
    </div>
  );
}

function fitDomainView(preview: MeshPreview | undefined): ViewBox {
  const points =
    preview === undefined
      ? []
      : [
          ...preview.outerBoundary,
          ...preview.airfoilUpper,
          ...preview.airfoilLower,
          ...preview.wakeUpper,
          ...preview.wakeLower,
        ].map(toSvg);
  if (points.length === 0) {
    return { x: -11, y: -11, width: 33, height: 22 };
  }
  const xs = points.map((point) => point.x);
  const ys = points.map((point) => point.y);
  const minX = Math.min(...xs);
  const maxX = Math.max(...xs);
  const minY = Math.min(...ys);
  const maxY = Math.max(...ys);
  const width = Math.max(maxX - minX, 1);
  const height = Math.max(maxY - minY, 1);
  const paddingX = width * 0.055;
  const paddingY = height * 0.09;
  return {
    x: minX - paddingX,
    y: minY - paddingY,
    width: width + 2 * paddingX,
    height: height + 2 * paddingY,
  };
}

function fitNearfieldView(
  preview: MeshPreview | undefined,
  fallback: ViewBox,
): ViewBox {
  const points =
    preview === undefined
      ? []
      : [...preview.airfoilUpper, ...preview.airfoilLower].map(toSvg);
  if (points.length === 0) {
    return fallback;
  }
  const xs = points.map((point) => point.x);
  const ys = points.map((point) => point.y);
  const minX = Math.min(...xs);
  const maxX = Math.max(...xs);
  const minY = Math.min(...ys);
  const maxY = Math.max(...ys);
  const scale = Math.max(maxX - minX, maxY - minY, 1e-6);
  const centerY = (minY + maxY) / 2;

  return {
    x: minX - scale * 0.25,
    y: centerY - scale * 0.62,
    width: scale * 2.8,
    height: scale * 1.24,
  };
}

function zoomView(
  viewBox: ViewBox,
  factor: number,
  focusX: number,
  focusY: number,
): ViewBox {
  const width = viewBox.width * factor;
  const height = viewBox.height * factor;
  return {
    x: viewBox.x + (viewBox.width - width) * focusX,
    y: viewBox.y + (viewBox.height - height) * focusY,
    width,
    height,
  };
}

function pointList(points: Point2[]): string {
  return points.map((point) => `${point.x},${-point.y}`).join(' ');
}

function segmentPath(segments: [Point2, Point2][]): string {
  return segments
    .map(
      ([start, end]) =>
        `M ${start.x} ${-start.y} L ${end.x} ${-end.y}`,
    )
    .join(' ');
}

function boundaryRoleClass(role: BoundaryRole): string {
  switch (role) {
    case 'INLET':
      return 'boundary-role--inlet';
    case 'OUTLET':
      return 'boundary-role--outlet';
    case 'AIRFOIL':
      return 'boundary-role--airfoil';
    case 'UPPER_FARFIELD':
    case 'LOWER_FARFIELD':
      return 'boundary-role--farfield';
  }
}

function toSvg(point: Point2): Point2 {
  return { x: point.x, y: -point.y };
}

function sampledIndices(
  points: Point2[],
): { index: number; point: Point2 }[] {
  const stride = Math.max(1, Math.ceil(points.length / 24));
  return points.flatMap((point, index) =>
    index % stride === 0 ? [{ index, point }] : [],
  );
}

function firstCellIndicator(
  preview: MeshPreview,
  viewBox: ViewBox,
):
  | {
      actual: number;
      end: Point2;
      start: Point2;
    }
  | undefined {
  const actual = preview.wallSpacing?.firstCellHeight;
  const index = Math.floor(preview.airfoilUpper.length * 0.42);
  const point = preview.airfoilUpper[index];
  const before = preview.airfoilUpper[Math.max(0, index - 1)];
  const after = preview.airfoilUpper[Math.min(preview.airfoilUpper.length - 1, index + 1)];
  if (
    actual === undefined ||
    point === undefined ||
    before === undefined ||
    after === undefined
  ) {
    return undefined;
  }
  const tangentX = after.x - before.x;
  const tangentY = after.y - before.y;
  const length = Math.hypot(tangentX, tangentY);
  if (length === 0) {
    return undefined;
  }
  let normalX = -tangentY / length;
  let normalY = tangentX / length;
  if (normalY < 0) {
    normalX *= -1;
    normalY *= -1;
  }
  const visibleDistance = Math.max(actual, viewBox.width * 0.018);
  return {
    actual,
    start: toSvg(point),
    end: toSvg({
      x: point.x + normalX * visibleDistance,
      y: point.y + normalY * visibleDistance,
    }),
  };
}
