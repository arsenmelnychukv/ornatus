# ornatus roadmap

> Name: `ornatus` (Latin, "adorned"). Decisions behind this roadmap are in [docs/decisions/](docs/decisions/).

A 3D laptop customizer that runs in the browser. The user opens the site, drops in any laptop `.glb` model (or uses the built-in one), places PNG stickers and draws freehand artwork on the lid, and sees the result in real time. Written in Rust, compiled to WebAssembly.

All code is written from scratch. Libraries (wgpu, egui, etc.) are fine. No existing customizer is used as a base.

## Goals

- Real-time 3D preview in the browser, with nothing to install.
- Fully client-side: models and stickers never leave the user's machine.
- Stickers: import PNG/WebP/JPEG, move, scale, rotate, flip, reorder, delete. Never mirrored by accident.
- Freehand drawing: color, size, opacity, hardness, eraser, undo and redo.
- Paint and stickers land only on the surface the user selects (default suggestion: the outer lid), never on the screen, keys or hinges.
- Export a transparent screenshot in v0.1. Project save and `.glb` export follow in v0.2.
- Architecture that can grow: more tools, more model types, touch input.

## Non-goals

- Backend, accounts, cloud sync, multiplayer.
- Full 3D modeling or sculpting.
- Outside contributions. The repo is public to read and fork (MIT or Apache-2.0), but pull requests and issues are not accepted.
- A landing page, website design work, or any hand-written HTML, CSS or JavaScript UI. The app is the whole product.
- Shipping any third-party 3D model.
- Native releases. The desktop build exists only for development.
- Phones and touch in v0.1. Pen pressure in v0.1.

## Decisions at a glance

| Area | Decision | Record |
|---|---|---|
| Audience | People previewing sticker layouts on their own laptop | [0001](docs/decisions/0001-product-scope-and-name.md) |
| Platform | Browser first, WebGPU with WebGL2 fallback, static hosting on GitHub Pages | [0002](docs/decisions/0002-web-first-architecture.md) |
| Browsers | Latest two stable Chrome, Edge, Firefox, Safari on desktop | [0002](docs/decisions/0002-web-first-architecture.md) |
| Paint | UV paint layer, stickers kept as data over a baked stroke layer | [0003](docs/decisions/0003-paint-and-sticker-pipeline.md) |
| Models | Plain `.glb`, 100 MB soft limit, user-supplied, plus our own default | [0004](docs/decisions/0004-models.md) |
| UX | Place, Select and Draw modes, standard handles, 15 degree snap | [0005](docs/decisions/0005-interaction-design.md) |
| License | MIT or Apache-2.0, no outside contributions | [0006](docs/decisions/0006-license-and-contributions.md) |
| Code | Cargo workspace of three crates, `trunk`, Rust only, small hand-written `egui` theme | [0007](docs/decisions/0007-code-layout-testing-and-theme.md) |

## Tech stack

| Concern | Choice | Why |
|---|---|---|
| Language | Rust (stable) | Project requirement |
| Window / input | `eframe` (wraps `winit`) | Works on the web and desktop |
| GPU | `wgpu` | WebGPU first, WebGL2 fallback, desktop backends for development |
| UI | `egui` via `eframe` | All Rust, fast to iterate, small theme written in Rust |
| Math | `glam` | Small, fast, wgpu-friendly |
| glTF loading | `gltf` crate | Meshes, materials, node hierarchy |
| Images | `image` crate | PNG/WebP/JPEG decode and encode |
| Picking | Own ray vs triangle code (BVH later) | Needed for UV hit lookup |
| Web build | `trunk` | Builds and serves the wasm app |
| Errors / logging | `thiserror`, `anyhow`, `tracing` | Standard |

## Code layout

- `ornatus-core`: scene data, paint and sticker data, picking, undo, UV audit. No GPU, no window, no browser APIs. Fully unit-testable.
- `ornatus-render`: `wgpu` rendering. Render passes only, no compute shaders.
- `ornatus-app`: `eframe`, `egui`, wasm entry point.
- `ornatus-fixtures`: writes `.glb` bytes for the default laptop and test fixtures. Used by `xtask` and by core's tests.
- `xtask`: `cargo xtask model` regenerates the default laptop file.

## Phases

### Phase 0: Discovery (done)

- [x] Grill session: scope, platform, paint approach, model rules, UX, license, layout, testing. Recorded in `docs/decisions/`.
- [x] Name: `ornatus`.
- [x] Scope reduced on 2026-11-13: lightweight, Rust only. No landing page, no HTML or CSS design work, no Impeccable design step.
- **Done when:** decisions are recorded and the roadmap matches them.

### Phase 1: Foundation (waiting on first push)

- Install the Rust toolchain via `rustup`.
- Cargo workspace with the three crates and `xtask`. `trunk` setup.
- `LICENSE-MIT` and `LICENSE-APACHE`.
- CI: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, wasm build.
- GitHub Pages deploy on merge to `main`.
- Window, clear color, `egui` panel running natively and in the browser.
- Small dark `egui` theme written in Rust.
- "Your browser cannot run this" message shown from Rust when neither WebGPU nor WebGL2 is available. The only HTML is a ten-line `index.html` that `trunk` needs as its entry point.
- **Done when:** the live Pages site opens the themed app shell in the latest Chrome, Firefox and Safari.

### Phase 2: Model viewer (done)

- [x] `xtask` generates the default low-poly laptop with clean UVs.
- [x] Load a `.glb` from the file picker, drag and drop, or (desktop) a path argument. Clear errors for broken, compressed or external-data files, warning above 100 MB.
- [x] List all mesh and material names.
- [x] Center at the origin using the bounding box, normalize scale.
- [x] Orbit camera with damping, zoom, pan.
- [x] PBR shading lit by a procedural studio environment (no HDR file shipped).
- [x] Memory check with a 100 MB model: it loads in about 0.2 s natively with a 323 MB peak, and loads and renders in the browser (1.9 million vertices) without errors.
- **Done when:** the default model and any reasonable user `.glb` load centered and well lit. Met.

### Phase 3: Surface selection and UV audit

- Mesh and material list with a highlight on the selected surface.
- Missing UVs block painting with a clear error.
- Overlapping or mirrored UV islands show a warning with the affected area highlighted.
- **Done when:** the app knows which surface is paintable and tells the user what is wrong with it.

### Phase 4: Picking

- CPU ray vs triangle on the selected surface, returning triangle, barycentrics, UV, world point and normal. Back-facing triangles are ignored.
- Unit tests with known meshes.
- **Done when:** clicking the surface shows the correct UV coordinate in a debug overlay.

### Phase 5: Freehand paint

- Stroke layer texture on the GPU, 2K by default, 4K selectable.
- Brush: color, size, opacity, hardness. Smooth interpolation between pointer samples. Eraser.
- Modes (Place, Select, Draw) and the orbit rules from decision 0005.
- Undo and redo (Ctrl+Z, Ctrl+Shift+Z): 100 steps, or 50 if checkpoint memory is too heavy. Measure here.
- Seam handling between UV islands.
- Unsaved-changes warning when leaving the page.
- **Done when:** the user draws on the surface and undoes mistakes cleanly.

### Phase 6: Stickers

- Import PNG/WebP/JPEG through a file picker and drag and drop.
- Stickers as data objects, composited in order over the stroke layer.
- Place mode with a ghost preview, Select mode with handles (aspect locked, Shift unlocks, 15 degree snap, typed fields).
- Delete, duplicate, flip, reorder, per-sticker opacity.
- Sticker finish (glossier roughness).
- Undo and redo for every sticker operation.
- **Done when:** the v0.1 acceptance criteria below pass, except export.

### Phase 7: Screenshot export

- Transparent PNG at 1x, 2x or 4x of the viewport from the current camera angle.
- **Done when:** the exported image matches the viewport with a transparent background.

### Phase 8: Polish and v0.1 release

- Environment presets, shortcuts, tool cursors, status bar.
- Performance pass: 60 fps at 1080p on integrated graphics.
- Manual release checklist across the supported browsers.
- README with the no-contributions policy. GitHub settings per decision 0006.
- **Done when:** v0.1 is tagged and live.

### Phase 9: v0.2

- Project save and load (stickers and strokes as data), with autosave in browser storage.
- Export the paint layer as PNG and a customized `.glb`.
- Lid hinge angle slider, with the user choosing the lid node.

### Later ideas (unscheduled)

- Touch and phone support, pen pressure.
- Projected decals for models without usable UVs.
- Automatic UV unwrap and mesh splitting.
- Draco, meshopt and `.gltf` support.
- Text tool, shapes, layers panel, blend modes.
- Print-ready skin export and share-image generator.
- Native releases if there is demand.

## Acceptance criteria (v0.1, end of Phase 8)

- The live site loads in the latest two stable versions of Chrome, Edge, Firefox and Safari on desktop, and unsupported browsers see a clear message.
- The default laptop and a user-supplied `.glb` load centered, with damped orbit controls and studio lighting.
- The user imports any local PNG sticker, adjusts size and rotation, and places it on the chosen surface with no z-fighting and no bleed onto other parts.
- The user draws freehand strokes next to stickers and undoes mistakes cleanly.
- The user exports a transparent PNG screenshot.
- Nothing leaves the user's machine.

## Risks and open questions

- **User models vary.** UVs may be packed, mirrored or missing, and the lid may not be a separate mesh. Phase 3 handles detection and warnings.
- **Model size in the browser.** A 100 MB `.glb` may exceed memory on some machines. Measure in Phase 2 and adjust the limit.
- **WebGL2 fallback differences.** Test the fallback in CI or the manual checklist.
- **Third-party model licenses.** The project does not redistribute models. Docs tell users to respect the license of any model they import.
- **Trademarks.** Do not use laptop brand names in the project name, assets or docs.
- **`egui` and `wgpu` version drift.** Pin versions in `Cargo.lock`, upgrade deliberately.
