import { readFile, writeFile } from 'node:fs/promises';
import process from 'node:process';

const [, , inputPath, outputPath, ...rawOptions] = process.argv;
if (!inputPath || !outputPath) {
  console.error(
    'Usage: node render-preview.mjs <preview.json> <mesh.svg> [--title text] [--subtitle text] [--view domain|near|composite] [--labels]',
  );
  process.exit(2);
}

const options = parseOptions(rawOptions);
const preview = JSON.parse(await readFile(inputPath, 'utf8'));
const width = 1400;
const height = 860;
const canvasRect = { x: 48, y: 104, width: 1304, height: 668 };
const title = options.title ?? 'afoil_gen structured C-grid';
const subtitle =
  options.subtitle ??
  `${preview.quality.blockCount} conformal blocks · ${preview.previewLevel.toLowerCase().replaceAll('_', ' ')}`;
const view = options.view ?? 'composite';

if (!['domain', 'near', 'composite'].includes(view)) {
  console.error(`Unsupported view "${view}". Use domain, near, or composite.`);
  process.exit(2);
}

const geometryPoints = [
  ...preview.outerBoundary,
  ...preview.airfoilUpper,
  ...preview.airfoilLower,
  ...preview.wakeUpper,
  ...preview.wakeLower,
];
const domainBounds = boundsFor(geometryPoints, 0.035);
const nearBounds = nearfieldBounds(preview);
const mainBounds = view === 'near' ? nearBounds : domainBounds;
const mainTransform = makeTransform(mainBounds, canvasRect);
const insetRect = { x: 788, y: 490, width: 536, height: 246 };
const insetTransform = makeTransform(nearBounds, insetRect);

const svg = [];
svg.push(
  `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}" role="img" aria-labelledby="title description">`,
  `<title id="title">${escapeXml(title)}</title>`,
  `<desc id="description">${escapeXml(subtitle)}</desc>`,
  '<defs>',
  `<clipPath id="mainClip"><rect x="${canvasRect.x}" y="${canvasRect.y}" width="${canvasRect.width}" height="${canvasRect.height}" rx="8"/></clipPath>`,
  `<clipPath id="insetClip"><rect x="${insetRect.x}" y="${insetRect.y}" width="${insetRect.width}" height="${insetRect.height}" rx="6"/></clipPath>`,
  '<filter id="shadow" x="-20%" y="-20%" width="140%" height="140%"><feDropShadow dx="0" dy="5" stdDeviation="8" flood-opacity="0.35"/></filter>',
  '</defs>',
  '<rect width="1400" height="860" fill="#07131e"/>',
  `<text x="48" y="43" fill="#f5f8fa" font-family="Inter,Segoe UI,sans-serif" font-size="24" font-weight="700">${escapeXml(title)}</text>`,
  `<text x="48" y="70" fill="#8fa9b8" font-family="Inter,Segoe UI,sans-serif" font-size="13">${escapeXml(subtitle)}</text>`,
  `<rect x="${canvasRect.x}" y="${canvasRect.y}" width="${canvasRect.width}" height="${canvasRect.height}" rx="8" fill="#0b1c29" stroke="#1c3d4d"/>`,
  '<g clip-path="url(#mainClip)">',
);

drawAxes(svg, mainTransform, mainBounds, canvasRect);
drawPreview(svg, preview, mainTransform, 1);
svg.push('</g>');

if (options.labels) {
  drawBoundaryLabels(svg, preview, mainTransform, canvasRect);
}

if (view === 'composite') {
  svg.push(
    '<g filter="url(#shadow)">',
    `<rect x="${insetRect.x - 12}" y="${insetRect.y - 36}" width="${insetRect.width + 24}" height="${insetRect.height + 48}" rx="9" fill="#091824" stroke="#315467"/>`,
    `<text x="${insetRect.x}" y="${insetRect.y - 13}" fill="#f5f8fa" font-family="Inter,Segoe UI,sans-serif" font-size="13" font-weight="700">Near-airfoil detail</text>`,
    `<rect x="${insetRect.x}" y="${insetRect.y}" width="${insetRect.width}" height="${insetRect.height}" rx="6" fill="#0c202e"/>`,
    '</g>',
    '<g clip-path="url(#insetClip)">',
  );
  drawAxes(svg, insetTransform, nearBounds, insetRect, false);
  drawPreview(svg, preview, insetTransform, 1.15);
  svg.push('</g>');
}

drawQualityBadge(svg, preview);
drawLegend(svg);
svg.push('</svg>');

await writeFile(outputPath, `${svg.join('\n')}\n`);

function parseOptions(args) {
  const parsed = { labels: false };
  for (let index = 0; index < args.length; index += 1) {
    const argument = args[index];
    if (argument === '--labels') {
      parsed.labels = true;
      continue;
    }
    if (['--title', '--subtitle', '--view'].includes(argument)) {
      const value = args[index + 1];
      if (value === undefined) {
        throw new Error(`${argument} requires a value`);
      }
      parsed[argument.slice(2)] = value;
      index += 1;
      continue;
    }
    throw new Error(`Unknown option: ${argument}`);
  }
  return parsed;
}

function drawPreview(target, data, transform, widthScale) {
  for (const line of data.meshLines) {
    target.push(polyline(line.points, transform, '#2c7187', 0.62 * widthScale, 0.72));
  }
  for (const line of data.blockBoundaries) {
    target.push(polyline(line.points, transform, '#f0b64b', 1.5 * widthScale, 0.92));
  }

  target.push(
    polyline(data.outerBoundary, transform, '#8da6b8', 1.3 * widthScale, 0.9),
    polyline(data.wakeUpper, transform, '#ff786f', 1.9 * widthScale, 1),
    polyline(data.wakeLower, transform, '#ff786f', 1.9 * widthScale, 1),
  );

  for (const patch of data.boundaryPatches) {
    const color = boundaryColor(patch.role);
    for (const segment of patch.segments) {
      target.push(polyline(segment, transform, color, 2.6 * widthScale, 0.98));
    }
  }

  const airfoilPolygon = [...data.airfoilUpper, ...data.airfoilLower.toReversed()];
  const path = airfoilPolygon
    .map((point, index) => {
      const mapped = transform(point);
      return `${index === 0 ? 'M' : 'L'}${round(mapped.x)},${round(mapped.y)}`;
    })
    .join(' ');
  target.push(
    `<path d="${path} Z" fill="#dceef3" fill-opacity="0.2" stroke="#f7fbfc" stroke-width="${2.8 * widthScale}" vector-effect="non-scaling-stroke"/>`,
  );
}

function drawBoundaryLabels(target, data, transform, rect) {
  for (const label of data.boundaryLabels) {
    const point = transform(label.position);
    if (!insideRect(point, rect)) continue;
    target.push(
      `<g transform="translate(${round(point.x)} ${round(point.y)})">`,
      `<circle r="3.5" fill="${boundaryColor(label.role)}" stroke="#07131e" stroke-width="1.5"/>`,
      `<text x="8" y="-7" fill="#d8edf2" font-family="Inter,Segoe UI,sans-serif" font-size="11" paint-order="stroke" stroke="#07131e" stroke-width="3">${escapeXml(label.name)}</text>`,
      '</g>',
    );
  }
}

function drawQualityBadge(target, data) {
  const q2 = data.quality;
  const q3 = data.quality3d;
  target.push(
    '<g transform="translate(914 18)">',
    '<rect width="438" height="65" rx="8" fill="#0e2635" stroke="#285369"/>',
    `<text x="15" y="23" fill="#78e0a5" font-family="Inter,Segoe UI,sans-serif" font-size="12" font-weight="700">${q2.valid && (q3?.valid ?? true) ? 'VALIDATED MESH' : 'REVIEW MESH'}</text>`,
    `<text x="15" y="44" fill="#b8ccd7" font-family="ui-monospace,SFMono-Regular,monospace" font-size="11">${formatInt(q2.pointCount)} points · ${formatInt(q2.cellCount)} quads · ${q2.blockCount} blocks</text>`,
    `<text x="15" y="59" fill="#7896a6" font-family="ui-monospace,SFMono-Regular,monospace" font-size="10">${q3 ? `${formatInt(q3.cellCount)} hexes · min V ${formatScientific(q3.minimumCellVolume)}` : `2D mesh · min A ${formatScientific(q2.minimumCellArea)}`}</text>`,
    '</g>',
  );
}

function drawLegend(target) {
  target.push(
    '<g transform="translate(48 818)" font-family="Inter,Segoe UI,sans-serif" font-size="11">',
    '<line x1="0" y1="0" x2="30" y2="0" stroke="#2c7187" stroke-width="2"/><text x="39" y="4" fill="#9bb2bf">mesh</text>',
    '<line x1="96" y1="0" x2="126" y2="0" stroke="#f0b64b" stroke-width="2.5"/><text x="135" y="4" fill="#9bb2bf">blocks</text>',
    '<line x1="210" y1="0" x2="240" y2="0" stroke="#79b473" stroke-width="2.5"/><text x="249" y="4" fill="#9bb2bf">inlet</text>',
    '<line x1="310" y1="0" x2="340" y2="0" stroke="#d96c6c" stroke-width="2.5"/><text x="349" y="4" fill="#9bb2bf">outlet</text>',
    '<line x1="420" y1="0" x2="450" y2="0" stroke="#ff786f" stroke-width="2.5"/><text x="459" y="4" fill="#9bb2bf">wake interfaces</text>',
    '<line x1="590" y1="0" x2="620" y2="0" stroke="#f7fbfc" stroke-width="3"/><text x="629" y="4" fill="#9bb2bf">airfoil wall</text>',
    '</g>',
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
      `<text x="${rect.x + 10}" y="${rect.y + rect.height - 12}" fill="#5f8293" font-family="ui-monospace,SFMono-Regular,monospace" font-size="10">x →</text>`,
      `<text x="${rect.x + 10}" y="${rect.y + 18}" fill="#5f8293" font-family="ui-monospace,SFMono-Regular,monospace" font-size="10">y ↑</text>`,
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

function nearfieldBounds(data) {
  const airfoilPoints = [...data.airfoilUpper, ...data.airfoilLower];
  const airfoilBounds = boundsFor(airfoilPoints, 0);
  const chord = Math.max(
    airfoilBounds.maxX - airfoilBounds.minX,
    airfoilBounds.maxY - airfoilBounds.minY,
  );
  const provisional = {
    minX: airfoilBounds.minX - 0.18 * chord,
    maxX: airfoilBounds.maxX + 1.35 * chord,
    minY: airfoilBounds.minY - 0.42 * chord,
    maxY: airfoilBounds.maxY + 0.42 * chord,
  };
  const localWake = [...data.wakeUpper, ...data.wakeLower].filter(
    (point) => point.x >= provisional.minX && point.x <= provisional.maxX,
  );
  if (localWake.length === 0) return provisional;
  const wakeBounds = boundsFor(localWake, 0);
  return {
    ...provisional,
    minY: Math.min(provisional.minY, wakeBounds.minY - 0.1 * chord),
    maxY: Math.max(provisional.maxY, wakeBounds.maxY + 0.1 * chord),
  };
}

function boundsFor(points, paddingFraction) {
  const xs = points.map((point) => point.x);
  const ys = points.map((point) => point.y);
  const minX = Math.min(...xs);
  const maxX = Math.max(...xs);
  const minY = Math.min(...ys);
  const maxY = Math.max(...ys);
  const paddingX = Math.max(maxX - minX, 1e-12) * paddingFraction;
  const paddingY = Math.max(maxY - minY, 1e-12) * paddingFraction;
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

function boundaryColor(role) {
  return {
    INLET: '#79b473',
    OUTLET: '#d96c6c',
    AIRFOIL: '#f7fbfc',
    UPPER_FARFIELD: '#8da6b8',
    LOWER_FARFIELD: '#8da6b8',
  }[role];
}

function insideRect(point, rect) {
  return (
    point.x >= rect.x &&
    point.x <= rect.x + rect.width &&
    point.y >= rect.y &&
    point.y <= rect.y + rect.height
  );
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
