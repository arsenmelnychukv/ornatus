//! wgpu rendering for ornatus.
//!
//! Render passes only, no compute shaders, so the WebGL2 fallback works
//! (decision 0002).

mod paint;
mod sticker;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use ornatus_core::model::{MeshData, Model};
use wgpu::util::DeviceExt as _;

pub use paint::{PaintLayer, StrokeStyle};
pub use sticker::{StickerDraw, StickerImages};

/// Depth format the viewport expects from its render pass.
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 3],
    normal: [f32; 3],
    uv: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CameraUniform {
    view_proj: [[f32; 4]; 4],
    eye: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct MaterialUniform {
    base_color: [f32; 4],
    params: [f32; 4],
}

struct GpuMesh {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
    material_buffer: wgpu::Buffer,
    material_values: MaterialUniform,
    material: wgpu::BindGroup,
}

/// Triangles flagged by the UV audit, drawn over one mesh.
struct Overlay {
    mesh: usize,
    indices: wgpu::Buffer,
    index_count: u32,
}

/// Draws a [`Model`] into an existing render pass.
pub struct ModelRenderer {
    pipeline: wgpu::RenderPipeline,
    overlay_pipeline: wgpu::RenderPipeline,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    material_layout: wgpu::BindGroupLayout,
    /// Bound for meshes that have no paint.
    empty_paint: wgpu::BindGroup,
    paint: Option<(usize, PaintLayer)>,
    sticker_images: StickerImages,
    manual_srgb: bool,
    /// One entry per mesh in the model, `None` for meshes with no triangles.
    meshes: Vec<Option<GpuMesh>>,
    selected: Option<usize>,
    overlay: Option<Overlay>,
}

struct PipelineSettings<'a> {
    fragment_entry: &'a str,
    depth_write: bool,
    depth_compare: wgpu::CompareFunction,
    bias: wgpu::DepthBiasState,
}

fn build_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    color_format: wgpu::TextureFormat,
    settings: &PipelineSettings<'_>,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("ornatus model pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Vertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2],
            })],
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(settings.fragment_entry),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: color_format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(settings.depth_write),
            depth_compare: Some(settings.depth_compare),
            stencil: wgpu::StencilState::default(),
            bias: settings.bias,
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

impl ModelRenderer {
    /// Build the pipelines for a render pass with this color format.
    pub fn new(device: &wgpu::Device, color_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ornatus model shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("model.wgsl").into()),
        });
        let uniform_entry = |visibility| wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ornatus camera layout"),
            entries: &[uniform_entry(wgpu::ShaderStages::VERTEX_FRAGMENT)],
        });
        let material_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ornatus material layout"),
            entries: &[uniform_entry(wgpu::ShaderStages::FRAGMENT)],
        });
        let paint_layout = paint::model_paint_layout(device);
        let empty_paint = paint::empty_model_paint_group(device, &paint_layout);
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ornatus model layout"),
            bind_group_layouts: &[
                Some(&camera_layout),
                Some(&material_layout),
                Some(&paint_layout),
            ],
            immediate_size: 0,
        });
        let pipeline = build_pipeline(
            device,
            &layout,
            &shader,
            color_format,
            &PipelineSettings {
                fragment_entry: "fs_main",
                depth_write: true,
                depth_compare: wgpu::CompareFunction::Less,
                bias: wgpu::DepthBiasState::default(),
            },
        );
        // Pulled slightly towards the camera so it sits on top of its own surface.
        let overlay_pipeline = build_pipeline(
            device,
            &layout,
            &shader,
            color_format,
            &PipelineSettings {
                fragment_entry: "fs_flag",
                depth_write: false,
                depth_compare: wgpu::CompareFunction::LessEqual,
                bias: wgpu::DepthBiasState {
                    constant: -4,
                    slope_scale: -2.0,
                    clamp: 0.0,
                },
            },
        );
        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ornatus camera"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ornatus camera"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });
        Self {
            pipeline,
            overlay_pipeline,
            camera_buffer,
            camera_bind_group,
            material_layout,
            empty_paint,
            paint: None,
            sticker_images: StickerImages::new(device),
            manual_srgb: !color_format.is_srgb(),
            meshes: Vec::new(),
            selected: None,
            overlay: None,
        }
    }

    /// Replace the model that gets drawn. Mesh numbers used by
    /// [`set_selected`](Self::set_selected) and [`set_overlay`](Self::set_overlay)
    /// follow the order of `model.meshes`.
    pub fn set_model(&mut self, device: &wgpu::Device, model: &Model) {
        self.meshes = model
            .meshes
            .iter()
            .map(|mesh| (!mesh.indices.is_empty()).then(|| self.upload(device, mesh)))
            .collect();
        self.selected = None;
        self.overlay = None;
        self.paint = None;
    }

    fn upload(&self, device: &wgpu::Device, mesh: &MeshData) -> GpuMesh {
        let vertices: Vec<Vertex> = mesh
            .positions
            .iter()
            .enumerate()
            .map(|(i, p)| Vertex {
                position: p.to_array(),
                normal: mesh.normals.get(i).copied().unwrap_or(Vec3::Y).to_array(),
                uv: mesh
                    .uvs
                    .as_ref()
                    .and_then(|uvs| uvs.get(i))
                    .copied()
                    .unwrap_or([0.0, 0.0]),
            })
            .collect();
        let material_values = MaterialUniform {
            base_color: mesh.base_color,
            params: [mesh.metallic, mesh.roughness, 0.0, 0.0],
        };
        let material_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ornatus material"),
            contents: bytemuck::bytes_of(&material_values),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        GpuMesh {
            vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&mesh.name),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            }),
            indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&mesh.name),
                contents: bytemuck::cast_slice(&mesh.indices),
                usage: wgpu::BufferUsages::INDEX,
            }),
            index_count: u32::try_from(mesh.indices.len()).unwrap_or(u32::MAX),
            material: device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("ornatus material"),
                layout: &self.material_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: material_buffer.as_entire_binding(),
                }],
            }),
            material_buffer,
            material_values,
        }
    }

    /// Paint on this mesh from now on, with a fresh transparent layer, or on
    /// nothing with `None`. Any previous paint is dropped.
    pub fn set_paint_surface(&mut self, device: &wgpu::Device, mesh: Option<usize>, size: u32) {
        self.paint = mesh.map(|mesh| (mesh, PaintLayer::new(device, size)));
    }

    /// The paint layer and the mesh it belongs to, if a surface is chosen.
    pub fn paint(&self) -> Option<(usize, &PaintLayer)> {
        self.paint.as_ref().map(|(mesh, layer)| (*mesh, layer))
    }

    /// Upload an image that stickers can show. `rgba` is straight 8-bit RGBA.
    pub fn add_sticker_image(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        id: u32,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> Result<(), String> {
        self.sticker_images
            .add(device, queue, id, width, height, rgba)
    }

    /// Redraw all stickers on the paint surface. Does nothing without one.
    pub fn render_stickers(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        draws: &[StickerDraw],
    ) {
        if let Some((_, layer)) = &self.paint {
            layer.render_stickers(device, queue, &self.sticker_images, draws);
        }
    }

    /// Swap in a replacement layer for the current paint surface (for example
    /// after a resize).
    pub fn replace_paint(&mut self, layer: PaintLayer) {
        if let Some((_, current)) = &mut self.paint {
            *current = layer;
        }
    }

    /// Tint one mesh as the selected surface, or clear the tint with `None`.
    pub fn set_selected(&mut self, queue: &wgpu::Queue, mesh: Option<usize>) {
        for (index, strength) in [(self.selected, 0.0), (mesh, 1.0)] {
            if let Some(Some(gpu)) = index.and_then(|i| self.meshes.get(i)) {
                let mut values = gpu.material_values;
                values.params[2] = strength;
                queue.write_buffer(&gpu.material_buffer, 0, bytemuck::bytes_of(&values));
            }
        }
        self.selected = mesh;
    }

    /// Draw the given triangles of a mesh in red on top of it. `triangles`
    /// are triangle numbers (index list position divided by three).
    pub fn set_overlay(
        &mut self,
        device: &wgpu::Device,
        mesh_index: usize,
        mesh: &MeshData,
        triangles: &[u32],
    ) {
        let indices: Vec<u32> = triangles
            .iter()
            .filter_map(|&t| mesh.indices.get(t as usize * 3..t as usize * 3 + 3))
            .flatten()
            .copied()
            .collect();
        self.overlay = (!indices.is_empty()).then(|| Overlay {
            mesh: mesh_index,
            index_count: u32::try_from(indices.len()).unwrap_or(u32::MAX),
            indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("ornatus overlay"),
                contents: bytemuck::cast_slice(&indices),
                usage: wgpu::BufferUsages::INDEX,
            }),
        });
    }

    /// Remove the problem-area overlay.
    pub fn clear_overlay(&mut self) {
        self.overlay = None;
    }

    /// Upload the camera for the next draw.
    pub fn set_camera(&self, queue: &wgpu::Queue, view_projection: Mat4, eye: Vec3) {
        let uniform = CameraUniform {
            view_proj: view_projection.to_cols_array_2d(),
            eye: [
                eye.x,
                eye.y,
                eye.z,
                if self.manual_srgb { 1.0 } else { 0.0 },
            ],
        };
        queue.write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&uniform));
    }

    /// Draw the model into a render pass that has a [`DEPTH_FORMAT`] depth attachment.
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.camera_bind_group, &[]);
        let painted = self
            .paint
            .as_ref()
            .map(|(mesh, layer)| (*mesh, layer.model_group()));
        for (index, mesh) in self.meshes.iter().enumerate() {
            let Some(mesh) = mesh else { continue };
            let paint_group = match painted {
                Some((painted_mesh, group)) if painted_mesh == index => group,
                _ => &self.empty_paint,
            };
            pass.set_bind_group(1, &mesh.material, &[]);
            pass.set_bind_group(2, paint_group, &[]);
            pass.set_vertex_buffer(0, mesh.vertices.slice(..));
            pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..mesh.index_count, 0, 0..1);
        }
        if let Some(overlay) = &self.overlay
            && let Some(Some(mesh)) = self.meshes.get(overlay.mesh)
        {
            pass.set_pipeline(&self.overlay_pipeline);
            pass.set_bind_group(1, &mesh.material, &[]);
            pass.set_bind_group(2, &self.empty_paint, &[]);
            pass.set_vertex_buffer(0, mesh.vertices.slice(..));
            pass.set_index_buffer(overlay.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..overlay.index_count, 0, 0..1);
        }
    }
}

#[cfg(test)]
mod tests {
    fn validate(source: &str) {
        let module = naga::front::wgsl::parse_str(source).expect("parses");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .expect("validates");
    }

    /// The shaders must parse and validate without a GPU.
    #[test]
    fn model_shader_is_valid_wgsl() {
        validate(include_str!("model.wgsl"));
    }

    #[test]
    fn paint_shader_is_valid_wgsl() {
        validate(include_str!("paint.wgsl"));
    }
}
