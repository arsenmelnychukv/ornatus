# 0005: Interaction design

Status: accepted, 2026-11-12

## Context

Research on existing tools (3D painting and decal tools, product customizers, 2D editors) found common patterns and common complaints. The biggest usability trap is orbit controls conflicting with placing and dragging.

## Decision

- **Modes.** Place (ghost preview, click to place), Select (click a sticker to edit it) and Draw (brush and eraser).
- **Orbit.** Orbit never fires while a sticker or stroke is being dragged. Orbit works with right-drag, middle-drag or holding Space in any mode.
- **Ghost preview.** The sticker follows the cursor over the surface, aligned to the surface, and a click commits it.
- **Handles.** Corner handles scale with the aspect ratio locked by default (Shift unlocks). A rotation handle snaps to 15 degree steps while Shift is held. A typed angle and size field gives precision. Handles stay a fixed size on screen at any camera distance.
- **Shortcuts.** Delete removes, Ctrl+D duplicates, `[` and `]` change order, H flips on purpose. Each sticker has an opacity setting.
- **Never mirrored by accident.** A sticker is only mirrored when the user flips it.
- **Screenshot export.** Transparent PNG at 1x, 2x or 4x of the viewport size from the current camera angle. Nothing else in v0.1. The image is rendered offscreen with the same pipeline as the viewport, so it matches what is on screen. It leaves out the selection tint and the problem overlay. Color is divided by alpha after the read back, because blending stores it multiplied.
- **Sizes on the surface.** A sticker's size and turn are measured in an undistorted frame on the surface (units of the UV width, with the vertical direction scaled by the surface's UV aspect), so images are not stretched by an uneven UV map.

## Consequences

- Stickers need stable ids and a screen-space hit test or pick buffer.
- Ordering stickers means re-compositing them in order into the UV layer.
