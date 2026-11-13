# 0007: Code layout, testing and theme

Status: accepted, 2026-11-12

## Decision

- **Workspace.** A Cargo workspace with three crates:
  - `ornatus-core`: scene data, paint and sticker data, picking, undo, UV audit. No GPU and no window code, so it is fully unit-testable.
  - `ornatus-render`: `wgpu` rendering.
  - `ornatus-app`: `winit`, `egui`, and the wasm entry point.
- **Web tooling.** `trunk` builds and serves the wasm app.
- **Testing.** Unit tests in `ornatus-core` for picking, UV audit and undo, run in CI on every push. No automated GPU or screenshot tests in v0.1. A manual checklist runs before each release. Golden-image tests may come once the renderer is stable.
- **Rust only.** Updated 2026-11-13: the project stays lightweight. No landing page, no authored HTML, CSS or JavaScript UI, and no separate design-system step. The Impeccable design step was dropped.
- **App UI.** `egui` with a small dark theme written in Rust (colors, spacing, rounding set in code). The default `egui` fonts are used.

## Consequences

- If `egui` blocks a needed UI behavior in practice, the UI toolkit decision (0002) is the one to revisit.
