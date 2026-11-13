# 0004: Models are user input

Status: accepted, 2026-11-12

## Context

The project must not redistribute third-party 3D models, and must not hard-code any laptop. Users bring their own.

## Decision

- **Accepted format.** Plain `.glb` only in v0.1. `.gltf` with external files, Draco, meshopt and KTX2 compression, and other formats (FBX, OBJ) show a clear error and are later work.
- **Size.** Soft limit of 100 MB, with a warning above it.
- **Loading.** File picker and drag and drop. The path or file is a runtime value, never a constant.
- **Discovery.** All mesh and material names are read at load time. The user picks the paintable surface from a list, with a highlight in the 3D view. No model-specific code.
- **Default model.** The app ships our own low-poly laptop (lid, base, hinge, screen) so a first-time visitor sees something to paint immediately. A small Rust tool in the repo (`cargo xtask model`, backed by the `ornatus-fixtures` crate) generates it from simple shapes with clean UVs. The same tool produces fixtures for the UV audit tests.
- **Git hygiene.** `*.glb`, `*.gltf` and `assets/models/` are git-ignored, except the generated default model once it exists and its generator is in the repo (add an explicit exception then).
- **Licensing.** Docs tell users to respect the license of any model they import.

## Consequences

- Models with bad or no UVs get an error or a warning, not a silent failure.
- The default model doubles as a test fixture.
- Parsing a 100 MB model in wasm needs a memory check in Phase 2.
