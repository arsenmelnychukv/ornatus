# 0001: Product scope and name

Status: accepted, 2026-11-12

## Context

ornatus lets people preview a laptop with stickers and freehand artwork on the lid. It started as a plan to adapt an existing Three.js project. The decision is to write our own code in Rust instead.

## Decision

- **Audience.** People previewing a sticker layout on their own laptop. Print-ready skin export and social mockups are later possibilities, not v0.1 goals.
- **Own code.** No code is taken from existing customizer projects. Libraries are fine.
- **v0.1 cut line.** Model viewer, surface selection, freehand paint, stickers, undo, and screenshot export (transparent PNG at 1x, 2x or 4x). Project save and load, `.glb` export and the lid hinge slider move to v0.2.
- **Name.** `ornatus`, Latin for "adorned". It is free on crates.io and npm, and `.app` and `.dev` showed no DNS records at check time (unverified at a registrar). No conflicting product was found. Runner-up: `pysanka`.
- **No laptop brand names** in the project name, assets or docs.

## Consequences

- Later features (print-ready export, social mockups, text and shapes) must not force changes to the v0.1 data model.
- A custom domain is optional now that there is no landing page. If wanted, check `ornatus.app` and `ornatus.dev` at a registrar first (DNS lookups only suggested they were free).
