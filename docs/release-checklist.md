# Release checklist

v0.1.0 status (2026-11-16): the feature items below were checked by hand in a browser and worked (browser not recorded), CI was green. Not run: the clean-checkout cargo checks, the wasm size line and the no-WebGPU fallback.

Run this by hand before tagging a release. Automated tests cover the logic in `ornatus-core` and shader validity, but nothing automated covers the GPU, the browsers or the interface.

## Before testing

- [ ] `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo clippy -p ornatus-app --target wasm32-unknown-unknown -- -D warnings`, `cargo test --workspace`
- [x] CI is green on `main` and the Pages deploy finished.
- [ ] `trunk build --release` works and the wasm is about 8 MB or less (about 3 MB gzipped).

## In each supported browser

Latest two stable versions of Chrome, Edge, Firefox and Safari on desktop. Note the backend the status bar reports (WebGPU or the GL fallback).

- [x] The page loads, shows the default laptop, and the status bar reports a backend.
- [x] Orbit (right button, Space + drag), pan (Shift + orbit), zoom (scroll).
- [x] Open a model with the button, and by dropping a file. Try a model with no UVs, a model with problems (`cargo xtask flawed`), and a very large file (`cargo xtask stress`).
- [x] Choose a surface. Paint with the brush, then the eraser. Try each size, opacity and hardness. Switch to 4K.
- [x] Undo and redo, by shortcut and by button, across several strokes.
- [x] Add a PNG, a JPEG and a WebP sticker. Place, move, scale from each corner, turn (with and without Shift), flip, copy, reorder, delete, change opacity.
- [x] Undo and redo sticker edits mixed with strokes.
- [x] Switch surface after painting: the discard prompt appears. Close or reload the page after painting: the browser asks first.
- [x] Save a PNG at 1x, 2x and 4x. Open it: transparent background, same framing, paint and stickers present.
- [x] Each lighting preset and background changes the view. The screenshot keeps a transparent background.
- [ ] In a browser without WebGPU or WebGL2 (or with both disabled), the page shows the "cannot run" message instead of a blank page.

## After testing

- [x] Update the "not checked" lines in `ROADMAP.md`.
- [x] Tag the release and publish notes.
