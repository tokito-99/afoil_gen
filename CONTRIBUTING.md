# Contributing

Thank you for helping improve `afoil_gen`. Changes should preserve the project's core guarantees: deterministic output, explicit failure, stable boundary semantics, and honest separation between mesh diagnostics and CFD validation.

## Development setup

Install Rust 1.88 or newer, Node.js 22, npm, and `wasm-pack`. The checked-in toolchain file installs the Rust formatting, linting, and WebAssembly components.

```bash
npm ci
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
npm run test
npm run build
```

## Pull requests

Keep changes focused and include tests at the lowest useful layer. Numerical changes need deterministic regression coverage and an explanation of the governing assumptions. Export changes need parser or target-tool evidence when that tool is available. UI changes should keep advanced controls behind progressive disclosure and remain usable without a network service.

Do not silently repair invalid CFD inputs, weaken a target to fit a cell budget, infer transition or separation without solver data, or claim LES readiness from geometry checks alone. Update the public documentation when behavior, commands, schema fields, or caveats change.

Generated meshes and solver logs can be very large. Use minimal cases in tests and do not commit proprietary solver files, confidential geometries, or third-party data without permission.

By contributing, you agree that your work is licensed under the repository's MIT OR Apache-2.0 terms.
