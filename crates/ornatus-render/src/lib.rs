//! wgpu rendering for ornatus.
//!
//! Render passes only, no compute shaders, so the WebGL2 fallback works
//! (decision 0002).

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use ornatus_core::model::{MeshData, Model};
use wgpu::util::DeviceExt as _;

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
    material: wgpu::BindGroup,
}

/// Draws a [`Model`] into an existing render pass.
pub struct ModelRenderer {
    pipeline: wgpu::RenderPipeline,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    material_layout: wgpu::BindGroupLayout,
    manual_srgb: bool,
    meshes: Vec<GpuMesh>,
}

impl ModelRenderer {
    /// Build the pipeline for a render pass with these attachment formats.
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
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ornatus model layout"),
            bind_group_layouts: &[Some(&camera_layout), Some(&material_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ornatus model pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2],
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
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
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
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
            camera_buffer,
            camera_bind_group,
            material_layout,
            manual_srgb: !color_format.is_srgb(),
            meshes: Vec::new(),
        }
    }

    /// Replace the model that gets drawn.
    pub fn set_model(&mut self, device: &wgpu::Device, model: &Model) {
        self.meshes = model
            .meshes
            .iter()
            .filter(|mesh| !mesh.indices.is_empty())
            .map(|mesh| self.upload(device, mesh))
            .collect();
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
        let material = MaterialUniform {
            base_color: mesh.base_color,
            params: [mesh.metallic, mesh.roughness, 0.0, 0.0],
        };
        let material_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ornatus material"),
            contents: bytemuck::bytes_of(&material),
            usage: wgpu::BufferUsages::UNIFORM,
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
        }
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
        for mesh in &self.meshes {
            pass.set_bind_group(1, &mesh.material, &[]);
            pass.set_vertex_buffer(0, mesh.vertices.slice(..));
            pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..mesh.index_count, 0, 0..1);
        }
    }
}
