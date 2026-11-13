//! The parts of the interface around the work itself (Phase 8): lighting
//! and background choices, tool shortcuts, mouse cursors and the status bar.

use eframe::egui;
use ornatus_core::Mode;
use ornatus_core::environment::{BACKGROUNDS, ENVIRONMENTS};
use ornatus_core::pick::Pick;

use super::OrnatusApp;

/// Lighting and background choices.
#[derive(Default)]
pub(super) struct Look {
    environment: usize,
    background: usize,
}

impl OrnatusApp {
    /// The viewport background color.
    pub(super) fn background_color(&self) -> egui::Color32 {
        let [r, g, b] = BACKGROUNDS[self.look.background.min(BACKGROUNDS.len() - 1)].color;
        egui::Color32::from_rgb(r, g, b)
    }

    fn apply_environment(&self) {
        let environment = ENVIRONMENTS[self.look.environment.min(ENVIRONMENTS.len() - 1)];
        self.with_renderer(|renderer, _, _| renderer.set_environment(environment));
    }

    /// Lighting and background pickers.
    pub(super) fn look_panel(&mut self, ui: &mut egui::Ui) {
        ui.label("Look");
        let mut environment = self.look.environment;
        egui::ComboBox::from_id_salt("environment")
            .selected_text(format!("Lighting: {}", ENVIRONMENTS[environment].name))
            .show_ui(ui, |ui| {
                for (i, preset) in ENVIRONMENTS.iter().enumerate() {
                    ui.selectable_value(&mut environment, i, preset.name);
                }
            });
        if environment != self.look.environment {
            self.look.environment = environment;
            self.apply_environment();
        }
        egui::ComboBox::from_id_salt("background")
            .selected_text(format!(
                "Background: {}",
                BACKGROUNDS[self.look.background].name
            ))
            .show_ui(ui, |ui| {
                for (i, background) in BACKGROUNDS.iter().enumerate() {
                    ui.selectable_value(&mut self.look.background, i, background.name);
                }
            });
    }

    /// B and E pick the brush or eraser, V selects, P places.
    pub(super) fn tool_shortcuts(&mut self, ctx: &egui::Context) {
        use egui::{Key, Modifiers};
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let (brush, eraser, select, place) = ctx.input_mut(|input| {
            (
                input.consume_key(Modifiers::NONE, Key::B),
                input.consume_key(Modifiers::NONE, Key::E),
                input.consume_key(Modifiers::NONE, Key::V),
                input.consume_key(Modifiers::NONE, Key::P),
            )
        });
        if brush || eraser {
            self.mode = Mode::Draw;
            self.paint.erase = eraser;
        } else if select {
            self.mode = Mode::Select;
        } else if place {
            self.mode = Mode::Place;
        }
    }

    /// A short list of the controls.
    pub(super) fn shortcut_help(&self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("Controls").show(ui, |ui| {
            for line in [
                "Orbit: right or middle button, or Space + drag",
                "Pan: Shift + orbit",
                "Zoom: scroll",
                "B brush, E eraser, V select, P place",
                "Ctrl+Z undo, Ctrl+Shift+Z or Ctrl+Y redo",
                "Sticker: Delete, Ctrl+D copy, H flip, [ ] order",
                "Shift while scaling: free ratio; while turning: 15° steps",
            ] {
                ui.label(line);
            }
        });
    }

    /// The mouse cursor for what a click would do here.
    pub(super) fn cursor_icon(&self, navigating: bool) -> egui::CursorIcon {
        if navigating || self.sticker_drag_active() {
            return egui::CursorIcon::Grabbing;
        }
        if !self.can_paint() {
            return egui::CursorIcon::Default;
        }
        match self.mode {
            Mode::Draw => egui::CursorIcon::Crosshair,
            Mode::Place => egui::CursorIcon::Crosshair,
            Mode::Select => {
                if matches!(self.hover, Some(Pick::Hit(_))) && self.cursor_over_sticker() {
                    egui::CursorIcon::Grab
                } else {
                    egui::CursorIcon::Default
                }
            }
        }
    }

    /// Graphics backend, frame cost and model size.
    pub(super) fn status_bar(&self, ui: &mut egui::Ui, frame: &eframe::Frame) {
        ui.horizontal(|ui| {
            ui.weak(format!("{} triangles", self.triangle_count));
            ui.weak("·");
            ui.weak(&self.backend);
            if let Some(cpu) = frame.info().cpu_usage {
                ui.weak("·");
                ui.weak(format!("frame {:.1} ms", cpu * 1000.0));
            }
            ui.weak("·");
            ui.weak(&self.model_name);
        });
    }
}
