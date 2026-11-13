use eframe::egui;
use ornatus_core::Mode;

use crate::theme;

/// Top-level application state.
pub struct OrnatusApp {
    mode: Mode,
}

impl OrnatusApp {
    /// Create the app and apply the theme.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);
        Self {
            mode: Mode::default(),
        }
    }
}

impl eframe::App for OrnatusApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::left("tools")
            .resizable(false)
            .exact_size(180.0)
            .show(ui, |ui| {
                ui.heading("ornatus");
                ui.separator();
                ui.label("Mode");
                for mode in Mode::ALL {
                    ui.selectable_value(&mut self.mode, mode, mode.to_string());
                }
            });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.centered_and_justified(|ui| {
                ui.label(format!("Viewport (Phase 2). Mode: {}", self.mode));
            });
        });
    }
}
