# Release checklist

Run this by hand before tagging a release. Automated tests cover the logic in `ornatus-core` and shader validity, but nothing automated covers the GPU, the browsers or the interface.

## Before testing

- [ ] `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo clippy -p ornatus-app --target wasm32-unknown-unknown -- -D warnings`, `cargo test --workspace`
- [ ] CI is green on `main` and the Pages deploy finished.
- [ ] `trunk build --release` works and the wasm is about 8 MB or less (about 3 MB gzipped).

## In each supported browser

Latest two stable versions of Chrome, Edge, Firefox and Safari on desktop. Note the backend the status bar reports (WebGPU or the GL fallback).

- [ ] The page loads, shows the default laptop, and the status bar reports a backend.
- [ ] Orbit (right button, Space + drag), pan (Shift + orbit), zoom (scroll).
- [ ] Open a model with the button, and by dropping a file. Try a model with no UVs, a model with problems (`cargo xtask flawed`), and a very large file (`cargo xtask stress`).
- [ ] Choose a surface. Paint with the brush, then the eraser. Try each size, opacity and hardness. Switch to 4K.
- [ ] Undo and redo, by shortcut and by button, across several strokes.
- [ ] Add a PNG, a JPEG and a WebP sticker. Place, move, scale from each corner, turn (with and without Shift), flip, copy, reorder, delete, change opacity.
- [ ] Undo and redo sticker edits mixed with strokes.
- [ ] Switch surface after painting: the discard prompt appears. Close or reload the page after painting: the browser asks first.
- [ ] Save a PNG at 1x, 2x and 4x. Open it: transparent background, same framing, paint and stickers present.
- [ ] Each lighting preset and background changes the view. The screenshot keeps a transparent background.
- [ ] In a browser without WebGPU or WebGL2 (or with both disabled), the page shows the "cannot run" message instead of a blank page.

## After testing

- [ ] Update the "not checked" lines in `ROADMAP.md`.
- [ ] Tag the release and publish notes.
