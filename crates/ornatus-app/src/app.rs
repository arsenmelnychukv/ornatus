use std::sync::{Arc, Mutex};

use eframe::egui::{self, PointerButton};
use eframe::egui_wgpu::{self, RenderState};
use ornatus_core::Mode;
use ornatus_core::camera::{CameraController, OrbitCamera};
use ornatus_core::model::{self, LoadWarning, Model};
use ornatus_render::ModelRenderer;

use crate::theme;
use crate::viewport::ViewportCallback;

const DEFAULT_MODEL: &[u8] = include_bytes!("../assets/default-laptop.glb");
/// The longest side of any loaded model is scaled to this many units.
const MODEL_SIZE: f32 = 2.0;
const ORBIT_SPEED: f32 = 0.01;
const ZOOM_SPEED: f32 = 0.01;

/// A dropped file's name and its bytes, or why they could not be read.
type DroppedBytes = (String, Result<Vec<u8>, String>);

/// Top-level application state.
pub struct OrnatusApp {
    mode: Mode,
    render_state: RenderState,
    model: Model,
    model_name: String,
    controller: CameraController,
    status: Option<String>,
    /// Dropped files that finished reading, waiting to be loaded.
    inbox: Arc<Mutex<Vec<DroppedBytes>>>,
}

impl OrnatusApp {
    /// Create the app, apply the theme and load the default model.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);
        let render_state = cc
            .wgpu_render_state
            .clone()
            .expect("ornatus needs the wgpu renderer");
        let renderer = ModelRenderer::new(&render_state.device, render_state.target_format);
        render_state
            .renderer
            .write()
            .callback_resources
            .insert(renderer);
        let mut app = Self {
            mode: Mode::default(),
            render_state,
            model: Model::default(),
            model_name: String::new(),
            controller: CameraController::new(OrbitCamera::default()),
            status: None,
            inbox: Arc::default(),
        };
        app.load_bytes("Default laptop", DEFAULT_MODEL);
        app
    }

    fn load_bytes(&mut self, name: &str, bytes: &[u8]) {
        match model::load_glb(bytes) {
            Ok(loaded) => {
                let mut model = loaded.model;
                model.center_and_fit(MODEL_SIZE);
                if let Some(renderer) = self
                    .render_state
                    .renderer
                    .write()
                    .callback_resources
                    .get_mut::<ModelRenderer>()
                {
                    renderer.set_model(&self.render_state.device, &model);
                }
                self.model = model;
                self.model_name = name.to_owned();
                self.controller = CameraController::new(OrbitCamera::default());
                self.status = (!loaded.warnings.is_empty()).then(|| {
                    loaded
                        .warnings
                        .iter()
                        .map(describe_warning)
                        .collect::<Vec<_>>()
                        .join(" ")
                });
            }
            Err(error) => self.status = Some(format!("Could not open {name}: {error}")),
        }
    }

    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let files = ctx.input(|input| input.raw.dropped_files.clone());
        for file in files {
            let name = file.path().file_name().map_or_else(
                || "dropped file".to_owned(),
                |n| n.to_string_lossy().into_owned(),
            );
            let inbox = Arc::clone(&self.inbox);
            #[cfg(not(target_arch = "wasm32"))]
            {
                let bytes = file.bytes();
                inbox.lock().expect("inbox lock").push((name, bytes));
            }
            #[cfg(target_arch = "wasm32")]
            {
                // Browsers can only read files asynchronously.
                let ctx = ctx.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let bytes = file.bytes_async().await;
                    inbox.lock().expect("inbox lock").push((name, bytes));
                    ctx.request_repaint();
                });
            }
        }
        let ready = std::mem::take(&mut *self.inbox.lock().expect("inbox lock"));
        for (name, bytes) in ready {
            match bytes {
                Ok(bytes) => self.load_bytes(&name, &bytes),
                Err(error) => self.status = Some(format!("Could not read {name}: {error}")),
            }
        }
    }

    fn tool_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("ornatus");
        ui.separator();
        ui.label("Mode");
        for mode in Mode::ALL {
            ui.selectable_value(&mut self.mode, mode, mode.to_string());
        }
        ui.separator();
        ui.label(format!("Model: {}", self.model_name));
        ui.label("Drop a .glb file here to open it.");
        if let Some(status) = &self.status {
            ui.colored_label(egui::Color32::from_rgb(240, 190, 90), status);
        }
        ui.separator();
        ui.label(format!("Surfaces ({})", self.model.meshes.len()));
        for mesh in &self.model.meshes {
            let material = mesh.material.as_deref().unwrap_or("no material");
            ui.label(format!("{} ({material})", mesh.name));
        }
    }

    fn viewport(&mut self, ui: &mut egui::Ui) {
        let (rect, response) =
            ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
        let (dt, space, shift, scroll) = ui.input(|input| {
            (
                input.stable_dt.min(0.1),
                input.key_down(egui::Key::Space),
                input.modifiers.shift,
                input.smooth_scroll_delta.y,
            )
        });
        // Orbit never fights placement (decision 0005): right or middle drag,
        // or Space plus a primary drag. Shift turns the orbit into a pan.
        let navigating = response.dragged_by(PointerButton::Secondary)
            || response.dragged_by(PointerButton::Middle)
            || (space && response.dragged_by(PointerButton::Primary));
        if navigating {
            let delta = response.drag_delta();
            if shift {
                let height = rect.height().max(1.0);
                self.controller
                    .camera
                    .pan(delta.x / height, delta.y / height);
            } else {
                self.controller
                    .orbit_input(-delta.x * ORBIT_SPEED, delta.y * ORBIT_SPEED, dt);
            }
        }
        if response.hovered() {
            self.controller.camera.zoom(scroll * ZOOM_SPEED);
        }
        self.controller.update(dt, navigating);
        if self.controller.is_moving() {
            ui.request_repaint();
        }

        let camera = self.controller.camera;
        let aspect = rect.width() / rect.height().max(1.0);
        ui.painter().add(egui_wgpu::Callback::new_paint_callback(
            rect,
            ViewportCallback {
                view_projection: camera.view_projection(aspect),
                eye: camera.eye(),
            },
        ));
    }
}

fn describe_warning(warning: &LoadWarning) -> String {
    match warning {
        LoadWarning::LargeFile { bytes } => {
            format!(
                "Large file ({} MB). It may be slow or run out of memory.",
                bytes / (1024 * 1024)
            )
        }
        LoadWarning::SkippedPrimitive { mesh } => {
            format!("Skipped non-triangle geometry in {mesh}.")
        }
    }
}

impl eframe::App for OrnatusApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.handle_dropped_files(ui.ctx());
        egui::Panel::left("tools")
            .resizable(false)
            .exact_size(220.0)
            .show(ui, |ui| self.tool_panel(ui));
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| self.viewport(ui));
    }
}
