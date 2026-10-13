import type { MeshPreview } from '../types/mesh.js';

const MAX_BROWSER_GMSH_BYTES = 192 * 1024 * 1024;

export interface BrowserGmshAssessment {
  estimatedBytes: number;
  browserSafe: boolean;
  crossSectionEstimatedBytes: number;
  crossSectionBrowserSafe: boolean;
}

export interface BrowserFluentAssessment {
  estimatedBytes: number;
  browserSafe: boolean;
  dimension: 2 | 3;
}

/**
 * Conservatively estimates Gmsh ASCII size before Rust and JavaScript create
 * simultaneous mesh, string, and Blob allocations.
 */
export function assessBrowserGmsh(
  preview: MeshPreview,
): BrowserGmshAssessment {
  const crossSectionEstimatedBytes =
    preview.quality.pointCount * 58 +
    preview.quality.cellCount * 58 +
    preview.quality.blockCount * 160;
  const quality = preview.quality3d ?? preview.quality;
  const estimatedBytes =
    preview.quality3d === undefined
      ? crossSectionEstimatedBytes
      : quality.pointCount * 64 +
        quality.cellCount * 105 +
        2 * preview.quality.cellCount * 54 +
        preview.quality.blockCount * 200;

  return {
    estimatedBytes,
    browserSafe: estimatedBytes <= MAX_BROWSER_GMSH_BYTES,
    crossSectionEstimatedBytes,
    crossSectionBrowserSafe:
      crossSectionEstimatedBytes <= MAX_BROWSER_GMSH_BYTES,
  };
}

/**
 * Estimates Fluent legacy ASCII size. Native CLI export streams the same
 * connectivity and is preferred once the browser would duplicate a large
 * Rust string into JavaScript and a Blob.
 */
export function assessBrowserFluent(
  preview: MeshPreview,
  threeDimensional: boolean,
): BrowserFluentAssessment {
  const isThreeDimensional =
    threeDimensional && preview.quality3d !== undefined;
  const quality = preview.quality3d ?? preview.quality;
  const estimatedFaceCount = isThreeDimensional
    ? quality.cellCount * 3 + preview.quality.cellCount * 2
    : preview.quality.cellCount * 2 + preview.quality.pointCount;
  const estimatedBytes =
    quality.pointCount * (isThreeDimensional ? 63 : 43) +
    quality.cellCount * 12 +
    estimatedFaceCount * (isThreeDimensional ? 78 : 42) +
    preview.quality.blockCount * 220;

  return {
    estimatedBytes,
    browserSafe: estimatedBytes <= MAX_BROWSER_GMSH_BYTES,
    dimension: isThreeDimensional ? 3 : 2,
  };
}

export function formatFileSize(bytes: number): string {
  if (bytes < 1024 * 1024) {
    return `${Math.max(1, Math.round(bytes / 1024))} KB`;
  }
  if (bytes < 1024 * 1024 * 1024) {
    return `${(bytes / (1024 * 1024)).toFixed(0)} MB`;
  }
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}
