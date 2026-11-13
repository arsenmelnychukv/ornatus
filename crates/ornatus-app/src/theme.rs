//! Small dark theme, written in Rust (decision 0007).

use eframe::egui::{self, Color32, CornerRadius, Visuals};

const PANEL: Color32 = Color32::from_rgb(24, 24, 28);
const WINDOW: Color32 = Color32::from_rgb(30, 30, 36);
const ACCENT: Color32 = Color32::from_rgb(120, 140, 255);

/// Apply the ornatus theme to the egui context.
pub fn apply(ctx: &egui::Context) {
    let mut visuals = Visuals::dark();
    visuals.panel_fill = PANEL;
    visuals.window_fill = WINDOW;
    visuals.selection.bg_fill = ACCENT.gamma_multiply(0.55);
    visuals.hyperlink_color = ACCENT;
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(6);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(6);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(6);
    visuals.widgets.active.corner_radius = CornerRadius::same(6);
    ctx.set_visuals(visuals);
    ctx.global_style_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(10.0, 5.0);
    });
}
