//! Screenshot export: a transparent PNG at 1x, 2x or 4x of the viewport
//! (decision 0005).

use std::io::Cursor;

use eframe::egui;
use ornatus_render::{CapturedImage, ImageCapture};

use super::OrnatusApp;

/// Output scales the user can pick.
const SCALES: [u32; 3] = [1, 2, 4];
/// Most pixels one screenshot may have, to stay inside browser memory.
const MAX_PIXELS: f32 = 25_000_000.0;

/// Export settings and the screenshot in flight.
pub(super) struct ExportTool {
    scale: u32,
    /// Set by the button, acted on in the viewport where the size is known.
    requested: bool,
    pending: Option<ImageCapture>,
    /// Viewport size in physical pixels, as of the last frame.
    viewport_px: Option<(u32, u32)>,
}

impl Default for ExportTool {
    fn default() -> Self {
        Self {
            scale: 2,
            requested: false,
            pending: None,
            viewport_px: None,
        }
    }
}

/// The output size for a viewport of `view` pixels at the requested scale,
/// shrunk if it would not fit the GPU or memory. Returns the size and the
/// scale actually used.
fn fit_size(view: (u32, u32), requested: u32, max_dimension: u32) -> ((u32, u32), f32) {
    let (w, h) = (view.0.max(1) as f32, view.1.max(1) as f32);
    let by_dimension = max_dimension as f32 / w.max(h);
    let by_memory = (MAX_PIXELS / (w * h)).sqrt();
    let scale = (requested as f32).min(by_dimension).min(by_memory);
    (
        (
            (w * scale).floor().max(1.0) as u32,
            (h * scale).floor().max(1.0) as u32,
        ),
        scale,
    )
}

fn encode_png(image: CapturedImage) -> Result<Vec<u8>, String> {
    let buffer = image::RgbaImage::from_raw(image.width, image.height, image.rgba)
        .ok_or("the image data has the wrong size")?;
    let mut bytes = Vec::new();
    buffer
        .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(bytes)
}

impl OrnatusApp {
    /// Called every frame from the viewport with its screen rectangle.
    pub(super) fn export_frame(&mut self, ctx: &egui::Context, rect: egui::Rect, aspect: f32) {
        let ppp = ctx.pixels_per_point();
        self.export.viewport_px = Some((
            (rect.width() * ppp).round().max(1.0) as u32,
            (rect.height() * ppp).round().max(1.0) as u32,
        ));
        if std::mem::take(&mut self.export.requested) {
            self.start_capture(aspect);
        }
        if let Some(capture) = &self.export.pending {
            ctx.request_repaint();
            if let Some(result) = capture.poll(&self.render_state.device) {
                self.export.pending = None;
                match result.and_then(|image| {
                    let size = (image.width, image.height);
                    encode_png(image).map(|png| (png, size))
                }) {
                    Ok((png, size)) => self.deliver_png(&png, size),
                    Err(error) => self.status = Some(format!("Export failed: {error}")),
                }
            }
        }
    }

    fn start_capture(&mut self, aspect: f32) {
        let Some(view) = self.export.viewport_px else {
            return;
        };
        let camera = self.controller.camera;
        let max_dimension = self.render_state.device.limits().max_texture_dimension_2d;
        let ((width, height), _) = fit_size(view, self.export.scale, max_dimension);
        let mut result = Err("the renderer is not ready".to_owned());
        self.with_renderer(|renderer, device, queue| {
            result = renderer.begin_capture(
                device,
                queue,
                width,
                height,
                camera.view_projection(aspect),
                camera.eye(),
            );
        });
        match result {
            Ok(capture) => {
                self.export.pending = Some(capture);
                self.status = Some(format!("Rendering {width} by {height} pixels…"));
            }
            Err(error) => self.status = Some(format!("Export failed: {error}")),
        }
    }

    fn deliver_png(&mut self, png: &[u8], size: (u32, u32)) {
        #[cfg(target_arch = "wasm32")]
        {
            self.status = Some(
                match crate::web::download("ornatus.png", "image/png", png) {
                    Ok(()) => format!("Saved ornatus.png ({} by {} pixels).", size.0, size.1),
                    Err(error) => format!("Could not start the download: {error:?}"),
                },
            );
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let seconds = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            let name = format!("ornatus-{seconds}.png");
            self.status = Some(match std::fs::write(&name, png) {
                Ok(()) => format!("Saved {name} ({} by {} pixels).", size.0, size.1),
                Err(error) => format!("Could not save {name}: {error}"),
            });
        }
    }

    /// The export section of the tool panel.
    pub(super) fn export_panel(&mut self, ui: &mut egui::Ui) {
        ui.label("Screenshot (transparent PNG)");
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("export_scale")
                .selected_text(format!("{}x", self.export.scale))
                .width(60.0)
                .show_ui(ui, |ui| {
                    for scale in SCALES {
                        ui.selectable_value(&mut self.export.scale, scale, format!("{scale}x"));
                    }
                });
            let idle = self.export.pending.is_none();
            if ui
                .add_enabled(idle, egui::Button::new("Save PNG"))
                .clicked()
            {
                self.export.requested = true;
            }
        });
        if let Some(view) = self.export.viewport_px {
            let max_dimension = self.render_state.device.limits().max_texture_dimension_2d;
            let ((w, h), used) = fit_size(view, self.export.scale, max_dimension);
            ui.label(format!("{w} by {h} pixels"));
            if used + 0.01 < self.export.scale as f32 {
                ui.colored_label(
                    egui::Color32::from_rgb(240, 190, 90),
                    "Smaller than chosen: this GPU or browser cannot go larger.",
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_requested_scale_is_used_when_it_fits() {
        let ((w, h), scale) = fit_size((800, 600), 2, 8192);
        assert_eq!((w, h), (1600, 1200));
        assert!((scale - 2.0).abs() < 1e-6);
    }

    #[test]
    fn the_scale_shrinks_to_the_gpu_limit() {
        let ((w, h), scale) = fit_size((1000, 500), 4, 2048);
        assert_eq!((w, h), (2048, 1024));
        assert!(scale < 2.1);
    }

    #[test]
    fn the_scale_shrinks_to_the_memory_cap() {
        let ((w, h), _) = fit_size((3000, 2000), 4, 16384);
        assert!(
            f64::from(w) * f64::from(h) <= f64::from(MAX_PIXELS) + 1.0,
            "{w} by {h}"
        );
    }

    #[test]
    fn a_tiny_viewport_never_gives_a_zero_size() {
        let ((w, h), _) = fit_size((0, 0), 1, 2048);
        assert!(w >= 1 && h >= 1);
    }

    #[test]
    fn encoded_images_are_valid_pngs_with_alpha() {
        let png = encode_png(CapturedImage {
            width: 2,
            height: 1,
            rgba: vec![255, 0, 0, 255, 0, 0, 0, 0],
        })
        .expect("encodes");
        let decoded = image::load_from_memory(&png).expect("decodes").to_rgba8();
        assert_eq!(decoded.get_pixel(0, 0).0, [255, 0, 0, 255]);
        assert_eq!(decoded.get_pixel(1, 0).0[3], 0);
    }
}
