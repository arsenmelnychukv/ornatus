//! Draws the 3D model inside an egui panel through a wgpu paint callback.

use eframe::egui_wgpu::{self, CallbackTrait};
use eframe::wgpu;
use glam::{Mat4, Vec3};
use ornatus_render::ModelRenderer;

/// What the viewport needs to draw one frame.
pub struct ViewportCallback {
    pub view_projection: Mat4,
    pub eye: Vec3,
}

impl CallbackTrait for ViewportCallback {
    fn prepare(
        &self,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen: &egui_wgpu::ScreenDescriptor,
        _encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if let Some(renderer) = resources.get::<ModelRenderer>() {
            renderer.set_camera(queue, self.view_projection, self.eye);
        }
        Vec::new()
    }

    fn paint(
        &self,
        _info: eframe::egui::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        if let Some(renderer) = resources.get::<ModelRenderer>() {
            renderer.draw(pass);
        }
    }
}
