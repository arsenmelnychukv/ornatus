//! Freehand painting: the brush panel, stroke input, undo and redo (Phase 5).

use eframe::egui;
use ornatus_core::paint::{Brush, HitSample, Stroke, StrokeBuilder};
use ornatus_core::pick::Pick;
use ornatus_core::undo::UndoStack;
use ornatus_render::StrokeStyle;

use super::OrnatusApp;

/// Paint texture sizes the user can pick.
const SIZES: [(u32, &str); 2] = [(2048, "2K"), (4096, "4K")];
/// Pointer path samples are at most this many screen pixels apart.
const SAMPLE_SPACING_PX: f32 = 3.0;

/// Brush settings, stroke in progress and undo history.
pub(super) struct PaintTool {
    pub(super) color: [u8; 3],
    /// Brush radius in texels of the paint texture.
    pub(super) radius_texels: f32,
    pub(super) opacity: f32,
    pub(super) hardness: f32,
    pub(super) erase: bool,
    pub(super) texture_size: u32,
    history: UndoStack<Stroke>,
    /// True once a stroke was too old to undo and got baked in.
    baked: bool,
    current: Option<StrokeBuilder>,
    last_pointer: Option<egui::Pos2>,
    /// Whether the page currently asks before it is closed.
    warning_on: bool,
}

impl Default for PaintTool {
    fn default() -> Self {
        let brush = Brush::default();
        Self {
            color: brush.color,
            radius_texels: 12.0,
            opacity: brush.opacity,
            hardness: brush.hardness,
            erase: false,
            texture_size: SIZES[0].0,
            history: UndoStack::default(),
            baked: false,
            current: None,
            last_pointer: None,
            warning_on: false,
        }
    }
}

impl PaintTool {
    fn brush(&self) -> Brush {
        Brush {
            color: self.color,
            radius_uv: self.radius_texels / self.texture_size as f32,
            opacity: self.opacity,
            hardness: self.hardness,
            erase: self.erase,
        }
    }

    /// True when there is paint the user would lose by leaving.
    pub(super) fn has_unsaved_paint(&self) -> bool {
        self.baked || self.history.can_undo()
    }

    /// Forget all strokes (a new surface or model).
    pub(super) fn reset(&mut self) {
        self.history.clear();
        self.baked = false;
        self.current = None;
        self.last_pointer = None;
    }
}

impl OrnatusApp {
    /// Give the chosen surface a fresh paint layer, or remove it.
    pub(super) fn reset_paint_surface(&mut self) {
        let paintable = self
            .audit
            .as_ref()
            .is_some_and(|audit| audit.is_paintable());
        let mesh = self.selected.filter(|_| paintable);
        let size = self.paint.texture_size;
        self.paint.reset();
        self.with_renderer(|renderer, device, _| renderer.set_paint_surface(device, mesh, size));
        self.update_leave_warning();
    }

    pub(super) fn can_paint(&self) -> bool {
        self.selected.is_some()
            && self
                .audit
                .as_ref()
                .is_some_and(|audit| audit.is_paintable())
    }

    fn update_leave_warning(&mut self) {
        let wanted = self.paint.has_unsaved_paint();
        if wanted != self.paint.warning_on {
            self.paint.warning_on = wanted;
            #[cfg(target_arch = "wasm32")]
            crate::web::set_leave_warning(wanted);
        }
    }

    /// The hit under a screen position, if it is on the paint surface.
    fn paint_sample(
        &self,
        pointer: egui::Pos2,
        rect: egui::Rect,
        aspect: f32,
    ) -> Option<HitSample> {
        let target = self.selected?;
        let ndc = glam::Vec2::new(
            (pointer.x - rect.left()) / rect.width().max(1.0) * 2.0 - 1.0,
            1.0 - (pointer.y - rect.top()) / rect.height().max(1.0) * 2.0,
        );
        let ray = self.controller.camera.ray(ndc, aspect);
        match self.pick_scene.pick(&self.model, &ray, target) {
            Pick::Hit(hit) => Some(HitSample {
                uv: hit.uv?,
                point: hit.point,
                uv_per_world: hit.uv_per_world.unwrap_or(1.0),
            }),
            _ => None,
        }
    }

    /// Handle painting input for this frame. Returns true while a stroke is
    /// being drawn, so the caller can keep the camera still.
    pub(super) fn paint_input(
        &mut self,
        response: &egui::Response,
        rect: egui::Rect,
        aspect: f32,
        navigating: bool,
    ) -> bool {
        use ornatus_core::Mode;
        let drawing_allowed = self.mode == Mode::Draw && self.can_paint() && !navigating;
        if response.drag_started_by(egui::PointerButton::Primary) && drawing_allowed {
            self.paint.current = Some(StrokeBuilder::new(self.paint.brush()));
            // Start the path where the button went down, so even a very fast
            // drag that finishes within one frame leaves a stroke.
            self.paint.last_pointer = response.ctx.input(|input| input.pointer.press_origin());
            self.with_renderer(|renderer, device, queue| {
                if let Some((_, layer)) = renderer.paint() {
                    layer.begin_stroke(device, queue);
                }
            });
        }
        if self.paint.current.is_none() {
            return false;
        }
        let stroking = response.dragged_by(egui::PointerButton::Primary)
            || response.drag_stopped_by(egui::PointerButton::Primary);
        if stroking && !navigating {
            let pointer = response
                .interact_pointer_pos()
                .or_else(|| response.ctx.input(|input| input.pointer.latest_pos()));
            if let Some(pointer) = pointer {
                let from = self.paint.last_pointer.unwrap_or(pointer);
                let steps = ((pointer - from).length() / SAMPLE_SPACING_PX)
                    .ceil()
                    .max(1.0) as u32;
                for step in 1..=steps {
                    let at = from.lerp(pointer, step as f32 / steps as f32);
                    let sample = self.paint_sample(at, rect, aspect);
                    if let Some(builder) = &mut self.paint.current {
                        builder.add(sample);
                    }
                }
                self.paint.last_pointer = Some(pointer);
            }
            self.stamp_new_dabs();
        }
        if response.drag_stopped() || navigating {
            self.finish_stroke();
            return false;
        }
        true
    }

    fn stamp_new_dabs(&mut self) {
        let Some(builder) = &mut self.paint.current else {
            return;
        };
        let dabs = builder.take_new_dabs().to_vec();
        let style = StrokeStyle::from(&self.paint.brush());
        self.with_renderer(|renderer, device, queue| {
            if let Some((_, layer)) = renderer.paint() {
                layer.stamp(device, queue, &style, &dabs);
                layer.set_preview(queue, Some(&style));
            }
        });
    }

    fn finish_stroke(&mut self) {
        let Some(builder) = self.paint.current.take() else {
            return;
        };
        self.paint.last_pointer = None;
        let stroke = builder.finish();
        let style = StrokeStyle::from(&stroke.brush);
        let evicted = if stroke.dabs.is_empty() {
            None
        } else {
            self.paint.history.push(stroke)
        };
        if evicted.is_some() {
            self.paint.baked = true;
        }
        self.with_renderer(|renderer, device, queue| {
            if let Some((_, layer)) = renderer.paint() {
                layer.end_stroke(device, queue, &style);
                if let Some(old) = &evicted {
                    layer.bake(device, queue, old);
                }
            }
        });
        self.update_leave_warning();
    }

    pub(super) fn undo_paint(&mut self) {
        if self.paint.history.undo() {
            self.replay_paint();
        }
    }

    pub(super) fn redo_paint(&mut self) {
        if self.paint.history.redo() {
            self.replay_paint();
        }
    }

    fn replay_paint(&mut self) {
        let strokes = self.paint.history.done().to_vec();
        self.with_renderer(|renderer, device, queue| {
            if let Some((_, layer)) = renderer.paint() {
                layer.replay(device, queue, &strokes);
            }
        });
        self.update_leave_warning();
    }

    fn change_texture_size(&mut self, size: u32) {
        if size == self.paint.texture_size {
            return;
        }
        self.paint.texture_size = size;
        let strokes = self.paint.history.done().to_vec();
        self.with_renderer(|renderer, device, queue| {
            let resized = renderer
                .paint()
                .map(|(_, layer)| layer.resized(device, queue, size, &strokes));
            if let Some(layer) = resized {
                renderer.replace_paint(layer);
            }
        });
    }

    /// Ctrl+Z, Ctrl+Shift+Z and Ctrl+Y.
    pub(super) fn paint_shortcuts(&mut self, ctx: &egui::Context) {
        use egui::{Key, Modifiers};
        let (redo, undo) = ctx.input_mut(|input| {
            let redo = input.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z)
                || input.consume_key(Modifiers::COMMAND, Key::Y);
            (redo, !redo && input.consume_key(Modifiers::COMMAND, Key::Z))
        });
        if redo {
            self.redo_paint();
        } else if undo {
            self.undo_paint();
        }
    }

    /// The brush section of the tool panel.
    pub(super) fn brush_panel(&mut self, ui: &mut egui::Ui) {
        ui.label("Brush");
        if !self.can_paint() {
            ui.label("Choose a surface that can be painted.");
            return;
        }
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.paint.erase, false, "Brush");
            ui.selectable_value(&mut self.paint.erase, true, "Eraser");
        });
        if !self.paint.erase {
            ui.horizontal(|ui| {
                ui.label("Color");
                ui.color_edit_button_srgb(&mut self.paint.color);
            });
        }
        ui.add(egui::Slider::new(&mut self.paint.radius_texels, 1.0..=200.0).text("Size"));
        ui.add(egui::Slider::new(&mut self.paint.opacity, 0.05..=1.0).text("Opacity"));
        ui.add(egui::Slider::new(&mut self.paint.hardness, 0.0..=1.0).text("Hardness"));
        let mut size = self.paint.texture_size;
        egui::ComboBox::from_label("Paint resolution")
            .selected_text(
                SIZES
                    .iter()
                    .find(|(s, _)| *s == size)
                    .map_or("", |(_, l)| *l),
            )
            .show_ui(ui, |ui| {
                for (value, label) in SIZES {
                    ui.selectable_value(&mut size, value, label);
                }
            });
        if size != self.paint.texture_size {
            self.change_texture_size(size);
        }
        ui.horizontal(|ui| {
            if ui
                .add_enabled(self.paint.history.can_undo(), egui::Button::new("Undo"))
                .clicked()
            {
                self.undo_paint();
            }
            if ui
                .add_enabled(self.paint.history.can_redo(), egui::Button::new("Redo"))
                .clicked()
            {
                self.redo_paint();
            }
        });
        ui.label("Draw with the left button. Orbit with the right or middle button, or Space.");
    }

    /// The brush outline under the cursor, sized to the paint on the surface.
    pub(super) fn draw_brush_ring(
        &self,
        painter: &egui::Painter,
        pointer: egui::Pos2,
        rect: egui::Rect,
        aspect: f32,
    ) {
        let (Some(here), Some(next)) = (
            self.paint_sample(pointer, rect, aspect),
            self.paint_sample(pointer + egui::vec2(1.0, 0.0), rect, aspect),
        ) else {
            return;
        };
        let uv_per_pixel = glam::Vec2::from(here.uv).distance(glam::Vec2::from(next.uv));
        if uv_per_pixel <= f32::EPSILON {
            return;
        }
        let radius = (self.paint.brush().radius_uv / uv_per_pixel).clamp(2.0, 600.0);
        painter.circle_stroke(
            pointer,
            radius,
            egui::Stroke::new(1.0, egui::Color32::from_white_alpha(200)),
        );
    }
}
