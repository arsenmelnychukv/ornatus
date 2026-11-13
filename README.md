# ornatus

A 3D laptop customizer for the browser. Open a laptop `.glb` model (or use the built-in one), draw freehand on the lid, place stickers, and save a transparent PNG. Everything runs on your machine: nothing is uploaded. Written in Rust, compiled to WebAssembly.

Live: <https://arsenmelnychukv.github.io/ornatus/>

Status: v0.1 release candidate. See [ROADMAP.md](ROADMAP.md) for what is done and what comes next, and [docs/decisions](docs/decisions/) for why things are the way they are.

## Using it

1. Pick the surface to paint in the list. The outer lid is suggested for most models. Surfaces without UVs cannot be painted, and UV problems are listed and shown in red on the model.
2. **Draw** (B brush, E eraser): left button, with color, size, opacity and hardness.
3. **Place** (P): add a PNG, WebP or JPEG (button or drop it on the window), then click the surface.
4. **Select** (V): click a sticker to move it, scale it from a corner, or turn it with the round handle. Fields in the panel give exact values.
5. **Screenshot**: choose 1x, 2x or 4x and save a PNG with a transparent background.

| Action | Control |
|---|---|
| Orbit | right or middle button, or Space + left button |
| Pan | Shift + orbit |
| Zoom | scroll |
| Undo / redo | Ctrl+Z / Ctrl+Shift+Z or Ctrl+Y |
| Sticker | Delete removes, Ctrl+D copies, H flips, `[` and `]` change the order |
| While dragging a sticker | Shift frees the ratio when scaling and snaps to 15 degrees when turning |

## Limits

- Models must be plain `.glb` files up to about 100 MB. Draco, meshopt and external files are not supported yet.
- Paint and stickers belong to one surface. Choosing another surface, or another model, discards them after asking.
- Project save and load, `.glb` export and the lid hinge slider come in v0.2. Until then, closing the page loses your work, and the browser asks first.
- Desktop browsers only for now. WebGPU is used when available, with a WebGL2 fallback.
- The project bundles no third-party 3D models. You supply your own and are responsible for its license.

## Development

Requires Rust (see `rust-toolchain.toml`) and [`trunk`](https://trunkrs.dev).

```bash
cargo run                # native dev window
cargo run -- model.glb   # open a model on start
trunk serve              # web build at http://127.0.0.1:8080
cargo xtask help         # dev tasks: default model, test models, 100 MB stress test
cargo test --workspace
```

The workspace has `ornatus-core` (scene, paint, picking, undo, UV audit: no GPU code), `ornatus-render` (wgpu), `ornatus-app` (egui, native and web entry points), `ornatus-fixtures` (generated models for tests) and `xtask`. Before a release, run through [docs/release-checklist.md](docs/release-checklist.md).

## License

Dual-licensed under MIT or Apache-2.0, at your option. You may read, fork and modify the code under those terms.

## Contributions

This is a solo project. Pull requests, issues and patches from others are not accepted.
