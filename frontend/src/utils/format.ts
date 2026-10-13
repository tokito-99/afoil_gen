export function formatCount(value: number): string {
  return new Intl.NumberFormat('en-US').format(value);
}

export function formatMetric(
  value: number | null | undefined,
  digits = 3,
): string {
  if (value === null || value === undefined || !Number.isFinite(value)) {
    return '—';
  }
  if (value === 0) {
    return '0';
  }
  const magnitude = Math.abs(value);
  if (magnitude >= 10_000 || magnitude < 0.001) {
    return value.toExponential(digits);
  }
  return value.toLocaleString('en-US', {
    maximumFractionDigits: digits,
  });
}

export function estimateCellCount(
  finiteTrailingEdge: boolean,
  leadingEdgeCells: number,
  upperSurfaceCells: number,
  lowerSurfaceCells: number,
  wakeCells: number,
  radialCells: number,
  spanwiseCells: number,
): { quads: number; hexes: number } {
  const streamwise =
    2 * leadingEdgeCells +
    upperSurfaceCells +
    lowerSurfaceCells +
    (finiteTrailingEdge ? 3 : 2) * wakeCells;
  const quads = Math.max(0, streamwise * radialCells);
  return {
    quads,
    hexes: Math.max(0, quads * spanwiseCells),
  };
}
