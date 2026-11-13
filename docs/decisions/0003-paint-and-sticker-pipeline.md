# 0003: Paint and sticker pipeline

Status: accepted, 2026-11-12

## Context

Stickers and brush strokes need to land on a chosen surface of a user-supplied model, stay sharp, and be undoable. Two approaches exist: a paint texture in UV space, or projected decals.

## Decision

- **UV paint layer** for both brush and stickers in v0.1. Projected decals are a possible later fallback for models without usable UVs.
- **Layers.** Freehand strokes are baked into a raster stroke layer. Stickers are kept as data (image, position, scale, rotation, order, opacity, flip) and re-composited in order over the stroke layer. This makes stickers editable at any time and makes project save in v0.2 simple.
- **Texture size.** 2K by default, user-selectable 4K. RGBA8.
- **Brush.** Color, size, opacity, hardness and an eraser. No pen pressure in v0.1.
- **UV audit.** Missing UVs on the selected surface block painting with a clear error. Overlapping or mirrored UV islands show a warning with the affected area highlighted, and the user may proceed.
- **Audit method (Phase 3).** Mirroring is judged from each triangle's UV handedness against its surface normal (u running right and v running down seen from outside). Overlap is found by rasterizing every triangle into a 1024 by 1024 grid and flagging triangles that share a cell, so triangles that only touch along an edge are not flagged. UVs outside the 0 to 1 square, triangles with no UV area, and surfaces using under 10 percent of the UV square also warn. Only missing UVs block.
- **Surface isolation.** Only the surface the user selects receives hits. Rays never paint on back-facing triangles.
- **Finish.** Keep the base material metalness and roughness. Sticker areas render with slightly lower roughness (about 0.25 to 0.35) so vinyl stands out.
- **Undo.** A command stack where one stroke, sticker move or sticker deletion is one step. Redo supported. Target depth is 100 steps with periodic texture checkpoints. If checkpoint memory proves too heavy, the depth drops to 50. Measure in Phase 5.
- **Leaving the page.** A warning appears when there are unsaved edits. No autosave in v0.1.

## Consequences

- Automatic UV unwrapping and mesh splitting are not in v0.1.
- Seams between UV islands need handling in the brush (Phase 5).
- The undo history for stickers works on data, not pixels, so it stays small.
