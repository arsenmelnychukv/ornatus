use std::sync::{Arc, Mutex};

use eframe::egui::{self, PointerButton};
use eframe::egui_wgpu::{self, RenderState};
use ornatus_core::Mode;
use ornatus_core::camera::{CameraController, OrbitCamera};
use ornatus_core::model::{self, LoadWarning, Model};
use ornatus_core::pick::{Pick, PickScene};
use ornatus_core::surface::suggest_surface;
use ornatus_core::uv::{self, UvAudit};
use ornatus_render::ModelRenderer;

use crate::theme;

mod paint_tool;
use crate::viewport::ViewportCallback;
use paint_tool::PaintTool;

const DEFAULT_MODEL: &[u8] = include_bytes!("../assets/default-laptop.glb");
/// The longest side of any loaded model is scaled to this many units.
const MODEL_SIZE: f32 = 2.0;
const ORBIT_SPEED: f32 = 0.01;
const ZOOM_SPEED: f32 = 0.01;

/// A dropped file's name and its bytes, or why they could not be read.
type DroppedBytes = (String, Result<Vec<u8>, String>);

/// Files that finished reading, shared with async readers on the web.
pub(crate) type Inbox = Arc<Mutex<Vec<DroppedBytes>>>;

/// Top-level application state.
pub struct OrnatusApp {
    mode: Mode,
    render_state: RenderState,
    model: Model,
    model_name: String,
    controller: CameraController,
    status: Option<String>,
    /// The surface chosen as the paint target, as an index into `model.meshes`.
    selected: Option<usize>,
    audit: Option<UvAudit>,
    show_problems: bool,
    paint: PaintTool,
    pick_scene: PickScene,
    /// What the cursor is over in the viewport, if it is over it.
    hover: Option<Pick>,
    /// Dropped files that finished reading, waiting to be loaded.
    inbox: Inbox,
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
            // Drawing is the only tool so far, so start there.
            mode: Mode::Draw,
            render_state,
            model: Model::default(),
            model_name: String::new(),
            controller: CameraController::new(OrbitCamera::default()),
            status: None,
            selected: None,
            audit: None,
            show_problems: true,
            paint: PaintTool::default(),
            pick_scene: PickScene::default(),
            hover: None,
            inbox: Arc::default(),
        };
        app.load_bytes("Default laptop", DEFAULT_MODEL);
        app
    }

    fn with_renderer(
        &self,
        f: impl FnOnce(&mut ModelRenderer, &eframe::wgpu::Device, &eframe::wgpu::Queue),
    ) {
        if let Some(renderer) = self
            .render_state
            .renderer
            .write()
            .callback_resources
            .get_mut::<ModelRenderer>()
        {
            f(
                renderer,
                &self.render_state.device,
                &self.render_state.queue,
            );
        }
    }

    /// Choose the paint surface (or none), audit its UVs and update the viewport.
    fn select_surface(&mut self, index: Option<usize>) {
        self.selected = index.filter(|&i| i < self.model.meshes.len());
        self.audit = self.selected.map(|i| uv::audit(&self.model.meshes[i]));
        self.refresh_highlights();
        self.reset_paint_surface();
    }

    fn refresh_highlights(&self) {
        let selected = self.selected;
        let flagged = self
            .audit
            .as_ref()
            .filter(|_| self.show_problems)
            .map(|audit| audit.flagged_triangles.as_slice());
        self.with_renderer(|renderer, device, queue| {
            renderer.set_selected(queue, selected);
            match (selected, flagged) {
                (Some(i), Some(triangles)) => {
                    renderer.set_overlay(device, i, &self.model.meshes[i], triangles);
                }
                _ => renderer.clear_overlay(),
            }
        });
    }

    /// Open a `.glb` from disk (desktop dev harness only).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn open_path(&mut self, path: &std::path::Path) {
        let name = path
            .file_name()
            .map_or_else(|| "file".to_owned(), |n| n.to_string_lossy().into_owned());
        match std::fs::read(path) {
            Ok(bytes) => self.load_bytes(&name, &bytes),
            Err(error) => self.status = Some(format!("Could not read {name}: {error}")),
        }
    }

    fn load_bytes(&mut self, name: &str, bytes: &[u8]) {
        match model::load_glb(bytes) {
            Ok(loaded) => {
                let mut model = loaded.model;
                model.center_and_fit(MODEL_SIZE);
                self.with_renderer(|renderer, device, _| renderer.set_model(device, &model));
                self.pick_scene = PickScene::new(&model);
                self.hover = None;
                self.model = model;
                self.model_name = name.to_owned();
                self.controller = CameraController::new(OrbitCamera::default());
                self.select_surface(suggest_surface(&self.model));
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
        #[cfg(target_arch = "wasm32")]
        if ui.button("Open .glb…").clicked() {
            crate::web::pick_file(Arc::clone(&self.inbox), ui.ctx().clone());
        }
        #[cfg(not(target_arch = "wasm32"))]
        ui.label("Pass a .glb path as an argument.");
        ui.label("Or drop a .glb file on the window.");
        if let Some(status) = &self.status {
            ui.colored_label(egui::Color32::from_rgb(240, 190, 90), status);
        }
        ui.separator();
        self.surface_list(ui);
        self.audit_report(ui);
        ui.separator();
        self.brush_panel(ui);
    }

    fn surface_list(&mut self, ui: &mut egui::Ui) {
        ui.label(format!(
            "Paint surface ({} to choose from)",
            self.model.meshes.len()
        ));
        let mut clicked = None;
        let row_height =
            ui.text_style_height(&egui::TextStyle::Button) + ui.spacing().item_spacing.y;
        egui::ScrollArea::vertical()
            .id_salt("surfaces")
            .max_height(220.0)
            .auto_shrink([false, true])
            .show_rows(ui, row_height, self.model.meshes.len(), |ui, rows| {
                for i in rows {
                    let mesh = &self.model.meshes[i];
                    let material = mesh.material.as_deref().unwrap_or("no material");
                    let label = format!("{} ({material})", mesh.name);
                    if ui
                        .selectable_label(self.selected == Some(i), label)
                        .clicked()
                    {
                        clicked = Some(i);
                    }
                }
            });
        if clicked.is_some() {
            self.select_surface(clicked);
        }
    }

    /// Debug readout of what the cursor is over (Phase 4), drawn in the
    /// bottom left corner of the viewport.
    fn draw_cursor_overlay(&self, painter: &egui::Painter, viewport: egui::Rect) {
        let Some(pick) = &self.hover else { return };
        let (status, hit) = match pick {
            Pick::Miss => ("Nothing here".to_owned(), None),
            Pick::Hit(hit) => ("On the paint surface".to_owned(), Some(hit)),
            Pick::BackFacing(hit) => ("Back of the paint surface".to_owned(), Some(hit)),
            Pick::Occluded(hit) => (
                format!("Blocked by {}", self.model.meshes[hit.mesh].name),
                Some(hit),
            ),
        };
        let mut lines = vec![status];
        if let Some(hit) = hit {
            lines.push(format!("Triangle {}", hit.triangle));
            lines.push(format!(
                "Point {:.3}, {:.3}, {:.3}",
                hit.point.x, hit.point.y, hit.point.z
            ));
            lines.push(match hit.uv {
                Some([u, v]) => format!("UV {u:.3}, {v:.3}"),
                None => "No UV here".to_owned(),
            });
        }
        let font = egui::FontId::proportional(13.0);
        let line_height = 17.0;
        let size = egui::vec2(230.0, lines.len() as f32 * line_height + 120.0 + 18.0);
        let panel = egui::Rect::from_min_size(
            viewport.left_bottom() + egui::vec2(10.0, -size.y - 10.0),
            size,
        );
        painter.rect_filled(
            panel,
            egui::CornerRadius::same(6),
            egui::Color32::from_black_alpha(190),
        );
        for (i, line) in lines.iter().enumerate() {
            painter.text(
                panel.min + egui::vec2(8.0, 6.0 + i as f32 * line_height),
                egui::Align2::LEFT_TOP,
                line,
                font.clone(),
                egui::Color32::from_gray(225),
            );
        }
        let map = egui::Rect::from_min_size(
            panel.min + egui::vec2(8.0, 10.0 + lines.len() as f32 * line_height),
            egui::vec2(100.0, 100.0),
        );
        painter.rect_filled(
            map,
            egui::CornerRadius::same(3),
            egui::Color32::from_rgb(18, 18, 22),
        );
        painter.rect_stroke(
            map,
            egui::CornerRadius::same(3),
            egui::Stroke::new(1.0, egui::Color32::from_gray(90)),
            egui::StrokeKind::Inside,
        );
        // Where in UV space the cursor is. The v axis runs down, like an image.
        if let (Pick::Hit(_), Some([u, v])) = (pick, hit.and_then(|h| h.uv)) {
            let at = map.min + egui::vec2(u * map.width(), v * map.height());
            painter.circle_filled(at, 4.0, egui::Color32::from_rgb(120, 140, 255));
        }
    }

    fn audit_report(&mut self, ui: &mut egui::Ui) {
        let Some(audit) = &self.audit else {
            ui.label("Pick the surface to paint on.");
            return;
        };
        ui.separator();
        if audit.is_paintable() && audit.findings.is_empty() {
            ui.colored_label(egui::Color32::from_rgb(120, 220, 150), "Ready to paint");
        } else if audit.is_paintable() {
            ui.colored_label(
                egui::Color32::from_rgb(240, 190, 90),
                "Can paint here, but check the warnings",
            );
        } else {
            ui.colored_label(
                egui::Color32::from_rgb(255, 110, 110),
                "Cannot paint on this surface",
            );
        }
        for finding in &audit.findings {
            let color = if finding.is_blocking() {
                egui::Color32::from_rgb(255, 110, 110)
            } else {
                egui::Color32::from_rgb(240, 190, 90)
            };
            ui.colored_label(color, finding.to_string());
        }
        if audit.is_paintable() {
            ui.label(format!("UV coverage: {:.0}%", audit.coverage * 100.0));
        }
        if !audit.flagged_triangles.is_empty() {
            let mut show = self.show_problems;
            if ui
                .checkbox(&mut show, "Show problem areas in red")
                .changed()
            {
                self.show_problems = show;
                self.refresh_highlights();
            }
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
        let drawing = self.paint_input(&response, rect, aspect, navigating);
        self.hover = None;
        if let (Some(pointer), Some(target)) = (response.hover_pos(), self.selected)
            && !navigating
        {
            let ndc = glam::Vec2::new(
                (pointer.x - rect.left()) / rect.width().max(1.0) * 2.0 - 1.0,
                1.0 - (pointer.y - rect.top()) / rect.height().max(1.0) * 2.0,
            );
            let pick = self
                .pick_scene
                .pick(&self.model, &camera.ray(ndc, aspect), target);
            self.hover = Some(pick);
            let color = match pick {
                Pick::Hit(_) => egui::Color32::from_rgb(120, 220, 150),
                Pick::BackFacing(_) => egui::Color32::from_rgb(240, 190, 90),
                Pick::Occluded(_) | Pick::Miss => egui::Color32::from_gray(150),
            };
            if self.mode == Mode::Draw && self.can_paint() && matches!(pick, Pick::Hit(_)) {
                self.draw_brush_ring(&ui.painter_at(rect), pointer, rect, aspect);
            } else {
                ui.painter()
                    .circle_stroke(pointer, 7.0, egui::Stroke::new(1.5, color));
            }
        }
        if drawing {
            ui.request_repaint();
        }
        ui.painter().add(egui_wgpu::Callback::new_paint_callback(
            rect,
            ViewportCallback {
                view_projection: camera.view_projection(aspect),
                eye: camera.eye(),
            },
        ));
        self.draw_cursor_overlay(&ui.painter_at(rect), rect);
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
        self.paint_shortcuts(ui.ctx());
        egui::Panel::left("tools")
            .resizable(false)
            .exact_size(220.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.tool_panel(ui));
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| self.viewport(ui));
    }
}
