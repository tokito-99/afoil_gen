import { readFile, writeFile } from 'node:fs/promises';
import process from 'node:process';

const [, , inputPath, outputPath] = process.argv;
if (!inputPath || !outputPath) {
  console.error('Usage: node render-preview.mjs <preview.json> <mesh.svg>');
  process.exit(2);
}

const preview = JSON.parse(await readFile(inputPath, 'utf8'));
const width = 1400;
const height = 900;
const mainRect = { x: 48, y: 100, width: 1304, height: 700 };
const insetRect = { x: 790, y: 514, width: 542, height: 260 };

const geometryPoints = [
  ...preview.outerBoundary,
  ...preview.airfoilUpper,
  ...preview.airfoilLower,
  ...preview.wakeUpper,
  ...preview.wakeLower,
];
const mainBounds = boundsFor(geometryPoints, 0.04);
const insetBounds = { minX: -0.15, maxX: 2.2, minY: -0.5, maxY: 0.5 };
const mainTransform = makeTransform(mainBounds, mainRect);
const insetTransform = makeTransform(insetBounds, insetRect);

const layers = [
  {
    values: preview.meshLines,
    stroke: '#2c7187',
    width: 0.7,
    opacity: 0.68,
  },
  {
    values: preview.blockBoundaries,
    stroke: '#f0b64b',
    width: 1.8,
    opacity: 0.95,
  },
];

const svg = [];
svg.push(
  `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}">`,
  '<defs>',
  '<clipPath id="mainClip"><rect x="48" y="100" width="1304" height="700" rx="6"/></clipPath>',
  '<clipPath id="insetClip"><rect x="790" y="514" width="542" height="260" rx="5"/></clipPath>',
  '<filter id="shadow" x="-20%" y="-20%" width="140%" height="140%"><feDropShadow dx="0" dy="5" stdDeviation="8" flood-opacity="0.35"/></filter>',
  '</defs>',
  '<rect width="1400" height="900" fill="#07131e"/>',
  '<text x="48" y="45" fill="#f5f8fa" font-family="Inter,Segoe UI,sans-serif" font-size="25" font-weight="700">afoil_gen · NACA 0012 LES structured C-grid</text>',
  `<text x="48" y="72" fill="#8fa9b8" font-family="Inter,Segoe UI,sans-serif" font-size="14">${preview.quality.blockCount} conformal blocks · Gordon–Hall interpolation · coarse visualization of the full validated mesh</text>`,
  `<rect x="${mainRect.x}" y="${mainRect.y}" width="${mainRect.width}" height="${mainRect.height}" rx="6" fill="#0b1c29" stroke="#1c3d4d"/>`,
);

svg.push('<g clip-path="url(#mainClip)">');
drawAxes(svg, mainTransform, mainBounds, mainRect);
drawPreview(svg, preview, layers, mainTransform, 1);
svg.push('</g>');

for (const label of preview.boundaryLabels) {
  const point = mainTransform(label.position);
  svg.push(
    `<g transform="translate(${round(point.x)} ${round(point.y)})">`,
    '<circle r="3.5" fill="#8de1ef" stroke="#07131e" stroke-width="1.5"/>',
    `<text x="8" y="-7" fill="#d8edf2" font-family="Inter,Segoe UI,sans-serif" font-size="12" paint-order="stroke" stroke="#07131e" stroke-width="3">${escapeXml(label.name)}</text>`,
    '</g>',
  );
}

svg.push(
  `<g filter="url(#shadow)">`,
  `<rect x="${insetRect.x - 12}" y="${insetRect.y - 38}" width="${insetRect.width + 24}" height="${insetRect.height + 50}" rx="8" fill="#091824" stroke="#315467"/>`,
  `<text x="${insetRect.x}" y="${insetRect.y - 14}" fill="#f5f8fa" font-family="Inter,Segoe UI,sans-serif" font-size="14" font-weight="700">Near-airfoil and trailing-edge detail</text>`,
  `<rect x="${insetRect.x}" y="${insetRect.y}" width="${insetRect.width}" height="${insetRect.height}" rx="5" fill="#0c202e"/>`,
  '</g>',
  '<g clip-path="url(#insetClip)">',
);
drawAxes(svg, insetTransform, insetBounds, insetRect, false);
drawPreview(svg, preview, layers, insetTransform, 1.15);
svg.push('</g>');

const q2 = preview.quality;
const q3 = preview.quality3d;
svg.push(
  '<g transform="translate(900 20)">',
  '<rect width="452" height="64" rx="7" fill="#0e2635" stroke="#285369"/>',
  `<text x="16" y="22" fill="#78e0a5" font-family="Inter,Segoe UI,sans-serif" font-size="13" font-weight="700">VALID 2D + 3D</text>`,
  `<text x="16" y="44" fill="#b8ccd7" font-family="ui-monospace,SFMono-Regular,monospace" font-size="11">${formatInt(q2.pointCount)} points · ${formatInt(q2.cellCount)} quads · min A ${formatScientific(q2.minimumCellArea)}</text>`,
  `<text x="16" y="59" fill="#7896a6" font-family="ui-monospace,SFMono-Regular,monospace" font-size="10">${q3 ? `${formatInt(q3.cellCount)} hexes · min V ${formatScientific(q3.minimumCellVolume)}` : '2D mesh only'}</text>`,
  '</g>',
  '<g transform="translate(48 842)" font-family="Inter,Segoe UI,sans-serif" font-size="12">',
  '<line x1="0" y1="0" x2="34" y2="0" stroke="#2c7187" stroke-width="2"/><text x="43" y="4" fill="#9bb2bf">mesh lines</text>',
  '<line x1="150" y1="0" x2="184" y2="0" stroke="#f0b64b" stroke-width="2.5"/><text x="193" y="4" fill="#9bb2bf">block boundaries</text>',
  '<line x1="335" y1="0" x2="369" y2="0" stroke="#ff786f" stroke-width="2.5"/><text x="378" y="4" fill="#9bb2bf">wake refinement interfaces</text>',
  '<line x1="595" y1="0" x2="629" y2="0" stroke="#f7fbfc" stroke-width="3.2"/><text x="638" y="4" fill="#9bb2bf">airfoil wall</text>',
  '</g>',
  `<text x="1352" y="872" text-anchor="end" fill="#617d8c" font-family="Inter,Segoe UI,sans-serif" font-size="10">Source: ${escapeXml(inputPath)}</text>`,
  '</svg>',
);

await writeFile(outputPath, `${svg.join('\n')}\n`);

function drawPreview(target, data, lineLayers, transform, widthScale) {
  for (const layer of lineLayers) {
    for (const line of layer.values) {
      target.push(
        polyline(
          line.points,
          transform,
          layer.stroke,
          layer.width * widthScale,
          layer.opacity,
        ),
      );
    }
  }
  target.push(
    polyline(data.outerBoundary, transform, '#7ddbea', 2.1 * widthScale, 0.95),
    polyline(data.wakeUpper, transform, '#ff786f', 2.2 * widthScale, 1),
    polyline(data.wakeLower, transform, '#ff786f', 2.2 * widthScale, 1),
  );

  const airfoilPolygon = [...data.airfoilUpper, ...data.airfoilLower.toReversed()];
  const path = airfoilPolygon
    .map((point, index) => {
      const mapped = transform(point);
      return `${index === 0 ? 'M' : 'L'}${round(mapped.x)},${round(mapped.y)}`;
    })
    .join(' ');
  target.push(
    `<path d="${path} Z" fill="#dceef3" fill-opacity="0.18" stroke="#f7fbfc" stroke-width="${3.0 * widthScale}" vector-effect="non-scaling-stroke"/>`,
  );
}

function drawAxes(target, transform, bounds, rect, labels = true) {
  if (bounds.minY <= 0 && bounds.maxY >= 0) {
    const left = transform({ x: bounds.minX, y: 0 });
    const right = transform({ x: bounds.maxX, y: 0 });
    target.push(
      `<line x1="${round(left.x)}" y1="${round(left.y)}" x2="${round(right.x)}" y2="${round(right.y)}" stroke="#244252" stroke-width="1"/>`,
    );
  }
  if (bounds.minX <= 0 && bounds.maxX >= 0) {
    const bottom = transform({ x: 0, y: bounds.minY });
    const top = transform({ x: 0, y: bounds.maxY });
    target.push(
      `<line x1="${round(bottom.x)}" y1="${round(bottom.y)}" x2="${round(top.x)}" y2="${round(top.y)}" stroke="#244252" stroke-width="1"/>`,
    );
  }
  if (labels) {
    target.push(
      `<text x="${rect.x + 10}" y="${rect.y + rect.height - 12}" fill="#5f8293" font-family="ui-monospace,SFMono-Regular,monospace" font-size="10">x/c →</text>`,
      `<text x="${rect.x + 10}" y="${rect.y + 18}" fill="#5f8293" font-family="ui-monospace,SFMono-Regular,monospace" font-size="10">y/c ↑</text>`,
    );
  }
}

function polyline(points, transform, stroke, strokeWidth, opacity) {
  const mapped = points
    .map((point) => {
      const value = transform(point);
      return `${round(value.x)},${round(value.y)}`;
    })
    .join(' ');
  return `<polyline points="${mapped}" fill="none" stroke="${stroke}" stroke-width="${strokeWidth}" stroke-opacity="${opacity}" stroke-linejoin="round" stroke-linecap="round" vector-effect="non-scaling-stroke"/>`;
}

function boundsFor(points, paddingFraction) {
  const xs = points.map((point) => point.x);
  const ys = points.map((point) => point.y);
  const minX = Math.min(...xs);
  const maxX = Math.max(...xs);
  const minY = Math.min(...ys);
  const maxY = Math.max(...ys);
  const paddingX = (maxX - minX) * paddingFraction;
  const paddingY = (maxY - minY) * paddingFraction;
  return {
    minX: minX - paddingX,
    maxX: maxX + paddingX,
    minY: minY - paddingY,
    maxY: maxY + paddingY,
  };
}

function makeTransform(bounds, rect) {
  const scale = Math.min(
    rect.width / (bounds.maxX - bounds.minX),
    rect.height / (bounds.maxY - bounds.minY),
  );
  const contentWidth = (bounds.maxX - bounds.minX) * scale;
  const contentHeight = (bounds.maxY - bounds.minY) * scale;
  const offsetX = rect.x + (rect.width - contentWidth) / 2;
  const offsetY = rect.y + (rect.height - contentHeight) / 2;
  return (point) => ({
    x: offsetX + (point.x - bounds.minX) * scale,
    y: offsetY + (bounds.maxY - point.y) * scale,
  });
}

function round(value) {
  return Number(value.toFixed(3));
}

function formatInt(value) {
  return new Intl.NumberFormat('en-US').format(value);
}

function formatScientific(value) {
  return value == null ? 'n/a' : Number(value).toExponential(3);
}

function escapeXml(value) {
  return String(value)
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&apos;');
}
