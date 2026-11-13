# 0002: Web-first architecture

Status: accepted, 2026-11-12

## Context

A small, nuanced tool is better when nobody has to download it. Rust with `wgpu` can target both the browser and the desktop from one codebase.

## Decision

- **The browser is the product.** The shipped app is a WebAssembly build served as a static site.
- **Graphics.** `wgpu` with WebGPU as the primary backend and the WebGL2 fallback enabled. The paint and sticker pipeline uses only render passes, no compute shaders, so both backends work.
- **Native build.** `cargo run` opens a desktop window through `winit`. It is a development harness only and is never released.
- **Fully client-side.** No backend, no accounts, no uploads. Models and stickers are read in the browser and stay on the user's machine.
- **Hosting.** GitHub Pages, deployed from GitHub Actions on every merge to `main`. The same workflow runs format, lint and tests on every push.
- **Supported browsers.** The latest two stable versions of Chrome, Edge, Firefox and Safari on desktop. If wasm loads but neither WebGPU nor WebGL2 is available, the Rust code shows a clear "cannot run this" message (written from Rust through `web-sys`). A browser too old to load wasm at all shows a blank page; that is accepted.
- **No authored HTML, CSS or JavaScript.** Confirmed 2026-11-13. The only HTML is the minimal loader page that `trunk` generates, which hosts the canvas and starts the wasm. Nothing in it is hand-designed.
- **Input.** Desktop first (mouse, trackpad, pen). Input goes through a pointer-event abstraction so touch support is a small addition later.
- **UI toolkit.** `egui`. The core stays UI-agnostic so a different UI could replace it.

## Consequences

- `ornatus-core` and `ornatus-render` must not assume a desktop: no blocking file I/O, no threads they cannot get on wasm, no native dialogs.
- Features that need a server (cloud save, share links) are out until a decision says otherwise.
- Memory limits in the browser apply to the model size limit (see 0004).
