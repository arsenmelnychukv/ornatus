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

### Phase 3: Surface selection and UV audit (done)

- [x] Surface list with a highlight on the selected surface, and a suggested default (the outer lid, by name hints).
- [x] Missing UVs block painting with a clear message.
- [x] Mirrored, overlapping, out-of-range and degenerate UVs, and low UV coverage, show warnings. Affected triangles are drawn in red on the model, with a toggle.
- [x] `cargo xtask flawed` writes a test model with one quad per problem.
- **Done when:** the app knows which surface is paintable and tells the user what is wrong with it. Met. The audit takes about 0.5 s on a 3.7 million triangle surface natively.

### Phase 4: Picking (done)

- [x] CPU ray against every mesh through a BVH, returning mesh, triangle, barycentric weights, UV, world point, normal and which side was hit.
- [x] Only the selected surface can be hit from its outer side. Another surface in front reports "blocked", and the back side reports "back facing".
- [x] Camera rays from the pointer. Unit tests cover known meshes, corner UVs, occlusion by the screen, back faces and a comparison with brute force.
- [x] A readout in the viewport corner (status, triangle, point, UV) with a UV square marking where the cursor is.
- **Done when:** clicking the surface shows the correct UV coordinate in a debug overlay. Met. On a 3.7 million triangle surface the BVH builds in about 1.3 s natively and a ray costs about one microsecond.

### Phase 5: Freehand paint (built, two known gaps)

- [x] Paint layer texture on the GPU, 2K by default and 4K selectable. Strokes are drawn into a coverage texture and composited at the stroke's opacity, so overlapping dabs do not darken a stroke.
- [x] Brush with color, size, opacity and hardness, and an eraser.
- [x] Undo and redo (Ctrl+Z, Ctrl+Shift+Z, Ctrl+Y and buttons) with 100 steps. Strokes that fall off the end are baked into a base texture.
- [x] Draw mode uses the left button. Orbit uses the right or middle button, or Space, and never starts a stroke.
- [x] Pointer paths are sampled every 3 pixels. Samples that jump across UV islands are not joined by a line.
- [x] Unsaved-changes warning when leaving the page (browser build).
- [x] A brush outline under the cursor, sized to the paint on the surface.
- [ ] **Gap: seam padding.** Paint at the edge of a UV island is not extended past the edge, so a thin seam can show when the texture is filtered.
- [ ] **Gap: mipmaps.** The paint texture has no mipmaps, so strokes can shimmer when the model is far away.
- Not done by design for v0.1: choosing another surface discards the paint on the old one.
- **Done when:** the user draws on the surface and undoes mistakes cleanly. Met in the browser build.

### Phase 6: Stickers (built, partly tested)

- [x] Import PNG, WebP and JPEG through the file picker (web) or drag and drop. Files are told apart by their first bytes, so a model and an image can be dropped the same way. Images over 2048 pixels are shrunk.
- [x] Stickers are data (image, center, size, turn, opacity, flip) drawn in order over the stroke layer into a UV-space texture, with the model shader mixing them in. Size is measured on the surface, so a square image stays square even on a stretched UV map.
- [x] Place mode with a ghost preview, normal-aligned by construction (UV space), and click to place. Select mode with a outline, four corner scale handles (aspect locked, Shift unlocks), a rotation handle (Shift snaps to 15 degrees), and drag to move.
- [x] Panel fields for position, width, turn and opacity, plus Flip, Duplicate, Delete, Send back and Bring forward. Shortcuts: Delete, Ctrl+D, H, [ and ].
- [x] One undo history for strokes and sticker edits (Ctrl+Z, Ctrl+Shift+Z).
- [x] Sticker areas render glossier (roughness about 0.3).
- Seen working in the browser: import, ghost, place, select, scale handle, undo.
- Not yet looked at by eye: the rotation handle, flip, reorder, duplicate, delete and the panel fields. They have unit tests for their data logic only.
- Known limits: the rotation handle and corners are only drawn where the sticker sits on the surface, so a sticker hanging over the edge hides them. Importing more than one image is supported, but there is no delete for an imported image.
- **Done when:** the v0.1 acceptance criteria below pass, except export.

### Phase 7: Screenshot export (done)

- [x] The model is drawn into an offscreen texture from the current camera at 1x, 2x or 4x of the viewport, on a transparent background, then read back without blocking and saved as a PNG. Paint and stickers are included. The selection tint and the red problem overlay are left out.
- [x] The browser build downloads `ornatus.png`. The desktop dev build writes `ornatus-<seconds>.png` in the current folder.
- [x] The size shrinks to fit the GPU's texture limit and a 25 million pixel memory cap, and the panel says when it did.
- [x] Checked in the browser: a 1364 by 1586 PNG with a transparent corner, opaque model pixels and matching colors.
- Not checked: Safari and Firefox downloads, and the 4x size on a real high-resolution display.
- **Done when:** the exported image matches the viewport with a transparent background. Met.

### Phase 8: Polish and v0.1 release (in progress)

- [x] Lighting presets (Studio, Bright, Dramatic) and viewport backgrounds (Dark, Gray, Light). Screenshots stay transparent.
- [x] Shortcuts for the tools (B, E, V, P) and a controls list in the panel.
- [x] Cursors that match the tool, and a status bar with the model's triangle count, the graphics backend, the frame cost and the model name.
- [x] README rewritten for users, with controls, limits and the no-contributions policy.
- [x] A manual release checklist: [docs/release-checklist.md](docs/release-checklist.md).
- [ ] **Performance pass.** The status bar shows the CPU cost of each frame, so it can be checked on real hardware. The browser I could test in throttles animation, so no 60 fps number has been measured yet.
- [ ] **Browser matrix.** Nothing has been run in Safari, Firefox or Edge. Use the checklist.
- [ ] **GitHub settings** (decision 0006). Issues and Discussions are off. The wiki and projects are still on, `main` is not protected, and there is no limit on who can open pull requests.
- [ ] Tag v0.1.
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

- **Transparent materials.** Meshes with partly transparent materials are drawn in file order with depth writes on, so overlapping transparent surfaces can look wrong. Fixing it needs sorting or a separate pass.
- **Large surfaces.** A surface whose vertex or index data exceeds the GPU's buffer limit is skipped with a warning. Surfaces under that limit but with millions of triangles make the UV audit and the BVH build freeze the page for a few seconds.

- **User models vary.** UVs may be packed, mirrored or missing, and the lid may not be a separate mesh. Phase 3 handles detection and warnings.
- **Model size in the browser.** A 100 MB `.glb` may exceed memory on some machines. Measure in Phase 2 and adjust the limit.
- **WebGL2 fallback differences.** Test the fallback in CI or the manual checklist.
- **Third-party model licenses.** The project does not redistribute models. Docs tell users to respect the license of any model they import.
- **Trademarks.** Do not use laptop brand names in the project name, assets or docs.
- **`egui` and `wgpu` version drift.** Pin versions in `Cargo.lock`, upgrade deliberately.
