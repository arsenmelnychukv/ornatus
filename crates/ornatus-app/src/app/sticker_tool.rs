//! Stickers: importing images, placing them, and editing them with handles
//! (Phase 6, decisions 0003 and 0005).

use std::f32::consts::PI;

use eframe::egui::{self, PointerButton};
use glam::{Vec2, Vec3};
use ornatus_core::Mode;
use ornatus_core::document::Edit;
use ornatus_core::locate::UvLocator;
use ornatus_core::sticker::{Sticker, surface_uv_aspect};
use ornatus_render::StickerDraw;

use super::OrnatusApp;

/// Images larger than this on their longest side are shrunk on import.
const MAX_IMAGE_SIDE: u32 = 2048;
/// Width of a freshly placed sticker, in units of the UV width.
const DEFAULT_WIDTH: f32 = 0.25;
const MIN_HALF: f32 = 0.01;
const MAX_HALF: f32 = 1.0;
/// How far above the top edge the rotation handle sits, in UV widths.
const ROTATE_HANDLE_DISTANCE: f32 = 0.07;
const HANDLE_RADIUS: f32 = 10.0;
const SNAP_DEGREES: f32 = 15.0;
/// File types the sticker picker offers.
#[cfg(target_arch = "wasm32")]
pub(super) const IMAGE_ACCEPT: &str = ".png,.jpg,.jpeg,.webp";

/// An imported image.
pub(super) struct ImageEntry {
    pub(super) id: u32,
    pub(super) name: String,
    pub(super) width: u32,
    pub(super) height: u32,
}

#[derive(Clone, Copy)]
enum DragKind {
    Move { grab: Vec2 },
    Scale,
    Rotate { start_angle: f32 },
}

#[derive(Clone, Copy)]
struct Drag {
    kind: DragKind,
    start: Sticker,
}

/// Sticker state: imported images, the selection and any edit in progress.
pub(super) struct StickerTool {
    pub(super) images: Vec<ImageEntry>,
    next_image_id: u32,
    /// The image the next click in Place mode puts down.
    armed: Option<u32>,
    /// The selected sticker, by id.
    selected: Option<u32>,
    drag: Option<Drag>,
    /// The selected sticker while it is being dragged or edited, before it
    /// becomes one undo step.
    working: Option<Sticker>,
    uv_aspect: f32,
    locator: UvLocator,
    ghost: Option<StickerDraw>,
    last_draws: Option<Vec<StickerDraw>>,
}

impl Default for StickerTool {
    fn default() -> Self {
        Self {
            images: Vec::new(),
            next_image_id: 1,
            armed: None,
            selected: None,
            drag: None,
            working: None,
            uv_aspect: 1.0,
            locator: UvLocator::default(),
            ghost: None,
            last_draws: None,
        }
    }
}

fn draw_for(sticker: &Sticker, k: f32, opacity_scale: f32) -> StickerDraw {
    let (axis_u, axis_v) = sticker.axes(k);
    StickerDraw {
        image: sticker.image,
        center: sticker.center.to_array(),
        axis_u: axis_u.to_array(),
        axis_v: axis_v.to_array(),
        opacity: sticker.opacity * opacity_scale,
        flip: sticker.flip_x,
    }
}

fn decode_image(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    let image = image::load_from_memory(bytes).map_err(|e| e.to_string())?;
    let image = if image.width().max(image.height()) > MAX_IMAGE_SIDE {
        image.resize(
            MAX_IMAGE_SIDE,
            MAX_IMAGE_SIDE,
            image::imageops::FilterType::Triangle,
        )
    } else {
        image
    };
    let rgba = image.to_rgba8();
    Ok((rgba.width(), rgba.height(), rgba.into_raw()))
}

impl OrnatusApp {
    /// Set up for a new paint surface: its UV stretch and lookup grid.
    pub(super) fn reset_stickers(&mut self) {
        let paintable = self.can_paint();
        let (aspect, locator) = match self.selected.filter(|_| paintable) {
            Some(i) => {
                let mesh = &self.model.meshes[i];
                (surface_uv_aspect(mesh), UvLocator::new(mesh))
            }
            None => (1.0, UvLocator::default()),
        };
        let tool = &mut self.stickers;
        tool.selected = None;
        tool.drag = None;
        tool.working = None;
        tool.ghost = None;
        tool.last_draws = None;
        tool.uv_aspect = aspect;
        tool.locator = locator;
    }

    /// Decode an image and make it available as a sticker.
    pub(super) fn import_image(&mut self, name: &str, bytes: &[u8]) {
        let (width, height, rgba) = match decode_image(bytes) {
            Ok(decoded) => decoded,
            Err(error) => {
                self.status = Some(format!("Could not read the image {name}: {error}"));
                return;
            }
        };
        let id = self.stickers.next_image_id;
        let mut uploaded = Ok(());
        self.with_renderer(|renderer, device, queue| {
            uploaded = renderer.add_sticker_image(device, queue, id, width, height, &rgba);
        });
        if let Err(error) = uploaded {
            self.status = Some(format!("Could not use the image {name}: {error}"));
            return;
        }
        self.stickers.next_image_id += 1;
        self.stickers.images.push(ImageEntry {
            id,
            name: name.to_owned(),
            width,
            height,
        });
        self.stickers.armed = Some(id);
        self.mode = Mode::Place;
        self.status = None;
    }

    fn image_entry(&self, id: u32) -> Option<&ImageEntry> {
        self.stickers.images.iter().find(|image| image.id == id)
    }

    /// The sticker with this id as it is right now, including a live edit.
    fn sticker_now(&self, id: u32) -> Option<Sticker> {
        match self.stickers.working {
            Some(working) if working.id == id => Some(working),
            _ => self
                .paint
                .document
                .stickers()
                .into_iter()
                .find(|s| s.id == id),
        }
    }

    fn selected_sticker(&self) -> Option<Sticker> {
        self.stickers.selected.and_then(|id| self.sticker_now(id))
    }

    fn all_stickers_now(&self) -> Vec<Sticker> {
        let mut stickers = self.paint.document.stickers();
        if let Some(working) = self.stickers.working
            && let Some(slot) = stickers.iter_mut().find(|s| s.id == working.id)
        {
            *slot = working;
        }
        stickers
    }

    /// Redraw the sticker texture if anything changed since the last draw.
    pub(super) fn update_sticker_preview(&mut self) {
        let k = self.stickers.uv_aspect;
        let mut draws: Vec<StickerDraw> = self
            .all_stickers_now()
            .iter()
            .map(|s| draw_for(s, k, 1.0))
            .collect();
        draws.extend(self.stickers.ghost);
        if self.stickers.last_draws.as_ref() == Some(&draws) {
            return;
        }
        self.with_renderer(|renderer, device, queue| {
            renderer.render_stickers(device, queue, &draws);
        });
        self.stickers.last_draws = Some(draws);
    }

    /// Force a redraw of the sticker texture.
    pub(super) fn refresh_stickers(&mut self) {
        self.stickers.last_draws = None;
        self.update_sticker_preview();
    }

    /// The history changed (undo, redo): redraw and drop a selection that
    /// no longer exists.
    pub(super) fn stickers_changed(&mut self) {
        self.stickers.working = None;
        self.stickers.drag = None;
        if let Some(id) = self.stickers.selected
            && self.sticker_now(id).is_none()
        {
            self.stickers.selected = None;
        }
        self.refresh_stickers();
    }

    fn push_edit(&mut self, edit: Edit) {
        let evicted = self.paint.document.push(edit);
        if let Some(old) = evicted {
            self.with_renderer(|renderer, device, queue| {
                if let Some((_, layer)) = renderer.paint() {
                    layer.bake(device, queue, &old);
                }
            });
        }
        self.refresh_stickers();
        self.update_leave_warning();
    }

    fn sticker_index(&self, id: u32) -> Option<usize> {
        self.paint
            .document
            .stickers()
            .iter()
            .position(|s| s.id == id)
    }

    fn new_sticker(&self, image: u32, center: Vec2) -> Option<Sticker> {
        let entry = self.image_entry(image)?;
        let half_x = DEFAULT_WIDTH / 2.0;
        let ratio = entry.height as f32 / entry.width.max(1) as f32;
        let half_y = half_x * ratio;
        // Keep very tall images on the surface.
        let shrink = (0.5 / half_y.max(half_x)).min(1.0);
        Some(Sticker {
            id: 0,
            image,
            center,
            half: Vec2::new(half_x, half_y) * shrink,
            rotation: 0.0,
            opacity: 1.0,
            flip_x: false,
        })
    }

    fn add_sticker(&mut self, mut sticker: Sticker) {
        sticker.id = self.paint.document.next_sticker_id();
        self.stickers.selected = Some(sticker.id);
        self.push_edit(Edit::Add(sticker));
    }

    /// Turn the live edit into one undo step.
    fn commit_working(&mut self) {
        let Some(after) = self.stickers.working.take() else {
            return;
        };
        let Some(index) = self.sticker_index(after.id) else {
            return;
        };
        let before = self.paint.document.stickers()[index];
        if before != after {
            self.push_edit(Edit::Replace {
                index,
                before,
                after,
            });
        } else {
            self.refresh_stickers();
        }
    }

    fn delete_selected(&mut self) {
        let Some(id) = self.stickers.selected else {
            return;
        };
        if let Some(index) = self.sticker_index(id) {
            let sticker = self.paint.document.stickers()[index];
            self.stickers.selected = None;
            self.stickers.working = None;
            self.push_edit(Edit::Remove { index, sticker });
        }
    }

    fn duplicate_selected(&mut self) {
        self.commit_working();
        if let Some(mut copy) = self.selected_sticker() {
            copy.center += Vec2::splat(0.03);
            self.add_sticker(copy);
        }
    }

    fn flip_selected(&mut self) {
        if let Some(mut sticker) = self.selected_sticker() {
            sticker.flip_x = !sticker.flip_x;
            self.stickers.working = Some(sticker);
            self.commit_working();
        }
    }

    /// Move the selected sticker up (`delta` > 0) or down in the stacking order.
    fn reorder_selected(&mut self, delta: isize) {
        let Some(id) = self.stickers.selected else {
            return;
        };
        let count = self.paint.document.stickers().len();
        if let Some(from) = self.sticker_index(id) {
            let to = from
                .saturating_add_signed(delta)
                .min(count.saturating_sub(1));
            if to != from {
                self.push_edit(Edit::Reorder { from, to });
            }
        }
    }

    /// Delete, Ctrl+D, [ and ] and H.
    pub(super) fn sticker_shortcuts(&mut self, ctx: &egui::Context) {
        use egui::{Key, Modifiers};
        if ctx.egui_wants_keyboard_input() || self.stickers.selected.is_none() {
            return;
        }
        let (delete, duplicate, back, forward, flip) = ctx.input_mut(|input| {
            (
                input.consume_key(Modifiers::NONE, Key::Delete)
                    || input.consume_key(Modifiers::NONE, Key::Backspace),
                input.consume_key(Modifiers::COMMAND, Key::D),
                input.consume_key(Modifiers::NONE, Key::OpenBracket),
                input.consume_key(Modifiers::NONE, Key::CloseBracket),
                input.consume_key(Modifiers::NONE, Key::H),
            )
        });
        if delete {
            self.delete_selected();
        } else if duplicate {
            self.duplicate_selected();
        } else if back {
            self.reorder_selected(-1);
        } else if forward {
            self.reorder_selected(1);
        } else if flip {
            self.flip_selected();
        }
    }

    fn project(&self, world: Vec3, rect: egui::Rect, aspect: f32) -> Option<egui::Pos2> {
        let clip = self.controller.camera.view_projection(aspect) * world.extend(1.0);
        if clip.w <= 1e-6 {
            return None;
        }
        let ndc = clip.truncate() / clip.w;
        Some(egui::pos2(
            rect.left() + (ndc.x * 0.5 + 0.5) * rect.width(),
            rect.top() + (0.5 - ndc.y * 0.5) * rect.height(),
        ))
    }

    fn screen_point(&self, uv: Vec2, rect: egui::Rect, aspect: f32) -> Option<egui::Pos2> {
        let mesh = &self.model.meshes[self.selected?];
        let world = self.stickers.locator.world_point(mesh, uv)?;
        self.project(world, rect, aspect)
    }

    /// Screen positions of a sticker's corners and its rotation handle.
    fn handle_points(
        &self,
        sticker: &Sticker,
        rect: egui::Rect,
        aspect: f32,
    ) -> Option<([egui::Pos2; 4], egui::Pos2)> {
        let k = self.stickers.uv_aspect;
        let corners = sticker.corners_uv(k);
        let corners = [
            self.screen_point(corners[0], rect, aspect)?,
            self.screen_point(corners[1], rect, aspect)?,
            self.screen_point(corners[2], rect, aspect)?,
            self.screen_point(corners[3], rect, aspect)?,
        ];
        let up = -(1.0 + ROTATE_HANDLE_DISTANCE / sticker.half.y.max(MIN_HALF));
        let handle = self.screen_point(sticker.uv_at(Vec2::new(0.0, up), k), rect, aspect)?;
        Some((corners, handle))
    }

    /// The outline and handles of the selected sticker (Select mode).
    pub(super) fn draw_sticker_handles(
        &self,
        painter: &egui::Painter,
        rect: egui::Rect,
        aspect: f32,
    ) {
        if self.mode != Mode::Select || !self.can_paint() {
            return;
        }
        let Some(sticker) = self.selected_sticker() else {
            return;
        };
        let Some((corners, rotate)) = self.handle_points(&sticker, rect, aspect) else {
            return;
        };
        let accent = egui::Color32::from_rgb(120, 140, 255);
        let outline = egui::Stroke::new(1.5, accent);
        let mut ring = corners.to_vec();
        ring.push(corners[0]);
        painter.add(egui::Shape::line(ring, outline));
        let top = corners[0].lerp(corners[1], 0.5);
        painter.line_segment([top, rotate], outline);
        painter.circle(rotate, 6.0, egui::Color32::WHITE, outline);
        for corner in corners {
            let square = egui::Rect::from_center_size(corner, egui::vec2(10.0, 10.0));
            painter.rect(
                square,
                egui::CornerRadius::same(2),
                egui::Color32::WHITE,
                outline,
                egui::StrokeKind::Inside,
            );
        }
    }

    /// Placing, selecting and dragging stickers. Called once per frame from
    /// the viewport.
    pub(super) fn sticker_input(
        &mut self,
        response: &egui::Response,
        rect: egui::Rect,
        aspect: f32,
        navigating: bool,
    ) {
        self.stickers.ghost = None;
        if self.can_paint() && !navigating {
            match self.mode {
                Mode::Place => self.place_input(response, rect, aspect),
                Mode::Select => self.select_input(response, rect, aspect),
                Mode::Draw => {}
            }
        }
        self.update_sticker_preview();
    }

    fn place_input(&mut self, response: &egui::Response, rect: egui::Rect, aspect: f32) {
        let (Some(pointer), Some(image)) = (response.hover_pos(), self.stickers.armed) else {
            return;
        };
        let Some(sample) = self.paint_sample(pointer, rect, aspect) else {
            return;
        };
        let Some(sticker) = self.new_sticker(image, Vec2::from(sample.uv)) else {
            return;
        };
        self.stickers.ghost = Some(draw_for(&sticker, self.stickers.uv_aspect, 0.6));
        if response.clicked_by(PointerButton::Primary) {
            self.add_sticker(sticker);
        }
    }

    fn topmost_sticker_at(&self, uv: Vec2) -> Option<Sticker> {
        let k = self.stickers.uv_aspect;
        self.all_stickers_now()
            .into_iter()
            .rev()
            .find(|s| s.contains(uv, k))
    }

    fn select_input(&mut self, response: &egui::Response, rect: egui::Rect, aspect: f32) {
        let ctx = response.ctx.clone();
        if response.drag_started_by(PointerButton::Primary)
            && let Some(origin) = ctx.input(|input| input.pointer.press_origin())
        {
            self.begin_drag(origin, rect, aspect);
        }
        if let Some(drag) = self.stickers.drag {
            let moving = response.dragged_by(PointerButton::Primary)
                || response.drag_stopped_by(PointerButton::Primary);
            let pointer = response
                .interact_pointer_pos()
                .or_else(|| ctx.input(|input| input.pointer.latest_pos()));
            if moving
                && let Some(pointer) = pointer
                && let Some(sample) = self.paint_sample(pointer, rect, aspect)
            {
                let shift = ctx.input(|input| input.modifiers.shift);
                self.continue_drag(drag, Vec2::from(sample.uv), shift);
            }
            if response.drag_stopped() {
                self.stickers.drag = None;
                self.commit_working();
            }
        } else if response.clicked_by(PointerButton::Primary)
            && let Some(pointer) = response.interact_pointer_pos()
        {
            let hit = self
                .paint_sample(pointer, rect, aspect)
                .and_then(|sample| self.topmost_sticker_at(Vec2::from(sample.uv)));
            self.stickers.selected = hit.map(|s| s.id);
        }
    }

    fn begin_drag(&mut self, origin: egui::Pos2, rect: egui::Rect, aspect: f32) {
        let k = self.stickers.uv_aspect;
        let uv = self
            .paint_sample(origin, rect, aspect)
            .map(|sample| Vec2::from(sample.uv));
        if let Some(sticker) = self.selected_sticker()
            && let Some((corners, rotate)) = self.handle_points(&sticker, rect, aspect)
        {
            let near = |p: egui::Pos2| p.distance(origin) <= HANDLE_RADIUS;
            if near(rotate) {
                let offset = rotate_dir(&sticker, uv, k);
                let start_angle = offset.y.atan2(offset.x);
                self.stickers.drag = Some(Drag {
                    kind: DragKind::Rotate { start_angle },
                    start: sticker,
                });
                return;
            }
            // On a small sticker the corner handles sit on top of the body, so a
            // press inside it moves the sticker instead of scaling it.
            let shortest_side = (0..4)
                .map(|i| corners[i].distance(corners[(i + 1) % 4]))
                .fold(f32::MAX, f32::min);
            let pressed_inside = uv.is_some_and(|uv| sticker.contains(uv, k));
            let crowded = shortest_side < 4.0 * HANDLE_RADIUS && pressed_inside;
            if !crowded && corners.iter().any(|&c| near(c)) {
                self.stickers.drag = Some(Drag {
                    kind: DragKind::Scale,
                    start: sticker,
                });
                return;
            }
        }
        let Some(uv) = uv else { return };
        if let Some(sticker) = self.topmost_sticker_at(uv) {
            self.stickers.selected = Some(sticker.id);
            self.stickers.drag = Some(Drag {
                kind: DragKind::Move {
                    grab: uv - sticker.center,
                },
                start: sticker,
            });
        } else {
            self.stickers.selected = None;
        }
    }

    fn continue_drag(&mut self, drag: Drag, uv: Vec2, shift: bool) {
        let k = self.stickers.uv_aspect;
        let start = drag.start;
        let mut sticker = start;
        match drag.kind {
            DragKind::Move { grab } => sticker.center = uv - grab,
            DragKind::Scale => {
                let offset = start.frame_offset(uv, k);
                let half = if shift {
                    offset.abs()
                } else {
                    // Aspect stays locked unless Shift is held.
                    let factor = (offset.x.abs() / start.half.x).max(offset.y.abs() / start.half.y);
                    start.half * factor
                };
                sticker.half = half.clamp(Vec2::splat(MIN_HALF), Vec2::splat(MAX_HALF));
            }
            DragKind::Rotate { start_angle } => {
                let direction = Vec2::new(uv.x - start.center.x, (uv.y - start.center.y) / k);
                let angle = direction.y.atan2(direction.x);
                let mut rotation = start.rotation + (angle - start_angle);
                if shift {
                    let step = SNAP_DEGREES.to_radians();
                    rotation = (rotation / step).round() * step;
                }
                sticker.rotation = rotation.rem_euclid(2.0 * PI);
            }
        }
        self.stickers.working = Some(sticker);
    }

    /// The sticker panel: image list and editing fields.
    pub(super) fn sticker_panel(&mut self, ui: &mut egui::Ui) {
        ui.label("Stickers");
        if !self.can_paint() {
            ui.label("Choose a surface that can be painted.");
            return;
        }
        #[cfg(target_arch = "wasm32")]
        if ui.button("Add sticker…").clicked() {
            crate::web::pick_file(
                std::sync::Arc::clone(&self.inbox),
                ui.ctx().clone(),
                IMAGE_ACCEPT,
            );
        }
        ui.label("PNG, WebP or JPEG. Drop one on the window to add it.");
        let mut armed = self.stickers.armed;
        for image in &self.stickers.images {
            if ui
                .selectable_label(armed == Some(image.id), &image.name)
                .clicked()
            {
                armed = Some(image.id);
            }
        }
        if armed != self.stickers.armed {
            self.stickers.armed = armed;
            self.mode = Mode::Place;
        }
        match self.mode {
            Mode::Place => {
                ui.label("Place: click the surface to put the sticker down.");
            }
            Mode::Select => {
                ui.label("Select: click a sticker, drag it, or use its handles.");
            }
            Mode::Draw => {}
        }
        self.sticker_fields(ui);
    }

    fn sticker_fields(&mut self, ui: &mut egui::Ui) {
        let Some(current) = self.selected_sticker() else {
            return;
        };
        ui.separator();
        ui.label("Selected sticker");
        let mut edit = current;
        let (mut changed, mut released) = (false, false);
        let mut track = |r: &egui::Response| {
            changed |= r.changed();
            released |= r.drag_stopped() || r.lost_focus();
        };
        ui.horizontal(|ui| {
            track(
                &ui.add(
                    egui::DragValue::new(&mut edit.center.x)
                        .speed(0.002)
                        .range(0.0..=1.0)
                        .prefix("X "),
                ),
            );
            track(
                &ui.add(
                    egui::DragValue::new(&mut edit.center.y)
                        .speed(0.002)
                        .range(0.0..=1.0)
                        .prefix("Y "),
                ),
            );
        });
        let mut width = edit.half.x * 2.0;
        let response = ui.add(
            egui::DragValue::new(&mut width)
                .speed(0.002)
                .range(MIN_HALF * 2.0..=MAX_HALF * 2.0)
                .prefix("Width "),
        );
        if response.changed() {
            edit.half *= width / (current.half.x * 2.0);
        }
        track(&response);
        let mut degrees = edit.rotation.to_degrees();
        let response = ui.add(
            egui::DragValue::new(&mut degrees)
                .speed(0.5)
                .range(0.0..=360.0)
                .suffix("°")
                .prefix("Turn "),
        );
        if response.changed() {
            edit.rotation = degrees.to_radians();
        }
        track(&response);
        track(&ui.add(egui::Slider::new(&mut edit.opacity, 0.05..=1.0).text("Opacity")));
        if changed {
            self.stickers.working = Some(edit);
        }
        if released {
            self.commit_working();
        }
        ui.horizontal(|ui| {
            if ui.button("Flip").clicked() {
                self.flip_selected();
            }
            if ui.button("Duplicate").clicked() {
                self.duplicate_selected();
            }
            if ui.button("Delete").clicked() {
                self.delete_selected();
            }
        });
        ui.horizontal(|ui| {
            if ui.button("Send back").clicked() {
                self.reorder_selected(-1);
            }
            if ui.button("Bring forward").clicked() {
                self.reorder_selected(1);
            }
        });
        ui.label("Delete, Ctrl+D, H flips, [ and ] change the order.");
    }
}

/// The vector from the sticker's center to a UV point, in the undistorted
/// frame, used to measure the angle for rotation.
fn rotate_dir(sticker: &Sticker, uv: Option<Vec2>, k: f32) -> Vec2 {
    uv.map_or(Vec2::X, |uv| {
        Vec2::new(uv.x - sticker.center.x, (uv.y - sticker.center.y) / k)
    })
}
