# Changelog

All notable changes to this project are documented here. The format follows Keep a Changelog, and releases use semantic versioning.

## [Unreleased]

### Added

- README mesh gallery with validated sharp and finite trailing-edge examples across RANS, WMLES, and WRLES configurations.

### Fixed

- Rust 1.99 Clippy compatibility for overflow-safe floating-point midpoints and collection assertions.

### Changed

- Pinned the development and CI toolchain while retaining an explicit Rust 1.88 minimum-version check.

## [0.1.0] - 2026-10-13

### Added

- Deterministic NACA 4-digit C-grid generation, 2D and periodic 3D extrusion, quality audits, RANS and LES presets, React/WebAssembly workspace, and OpenFOAM, Gmsh, and Fluent exporters.
- Safeguarded wall-normal coordinate adjustment and quality-monotone algebraic smoothing.
- Side-resolved wall-shear profile import with flow and airfoil provenance.
- Local wall-unit audit signals for streamwise spacing, measured layer coverage, transition, and reverse shear.
- Bounded cell and point budget planner that does not relax physical targets.
- Grid, time-step, and span study preparation plus uncertainty-aware analysis.
- Isolated Gmsh, OpenFOAM, and Fluent round-trip verification workflows.
- Compact frontend controls for optimization, spatial pilot data, and cell-budget planning.
- GitHub CI, tagged release packaging, issue templates, licensing, and contributor guidance.

### Changed

- Cell-center y+ can be selected explicitly, making first-layer thickness and centroid distance unambiguous.
- Optimized or spatially adapted meshes direct users to actual-coordinate Gmsh or Fluent export instead of compact `blockMeshDict`.
- Minimum supported Rust version is 1.88.

[Unreleased]: https://github.com/tokito-99/afoil_gen/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/tokito-99/afoil_gen/releases/tag/v0.1.0
