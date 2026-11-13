//! The paint layer: a texture in UV space that freehand strokes are drawn
//! into (decision 0003).
//!
//! Three textures are involved:
//! - `committed`: everything painted so far. The model shader samples it.
//! - `base`: strokes too old to undo, baked in. Undo restores `committed`
//!   from `base` and replays the strokes that are still undoable.
//! - `scratch`: coverage of the stroke being drawn. Dabs write the maximum of
//!   their coverage, so a stroke never gets darker where it overlaps itself,
//!   then the whole stroke is composited at the stroke's opacity.
//!
//! Only render passes are used (no compute), so this works on WebGL2.

use bytemuck::{Pod, Zeroable};
use ornatus_core::paint::{Brush, Stroke};
use wgpu::util::DeviceExt as _;

use crate::sticker::{Mipper, StickerDraw, StickerImages, StickerPass};

pub(crate) const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const COVERAGE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;
/// Dabs per draw call.
const DAB_CAPACITY: usize = 16_384;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct DabInstance {
    center: [f32; 2],
    radius: f32,
    hardness: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct StrokeUniform {
    /// rgb: sRGB color, a: opacity.
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct PreviewUniform {
    color: [f32; 4],
    /// x: erase, y: active.
    flags: [f32; 4],
}

fn texture(
    device: &wgpu::Device,
    size: u32,
    format: wgpu::TextureFormat,
    label: &str,
) -> wgpu::Texture {
    texture_with_mips(device, size, format, label, 1)
}

pub(crate) fn mip_count(size: u32) -> u32 {
    size.max(1).ilog2() + 1
}

/// A view of one mip level, which is what a render pass can draw into.
pub(crate) fn level_view(texture: &wgpu::Texture, level: u32) -> wgpu::TextureView {
    texture.create_view(&wgpu::TextureViewDescriptor {
        base_mip_level: level,
        mip_level_count: Some(1),
        ..Default::default()
    })
}

fn texture_with_mips(
    device: &wgpu::Device,
    size: u32,
    format: wgpu::TextureFormat,
    label: &str,
    mips: u32,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        mip_level_count: mips,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

pub(crate) fn texture_entry(binding: u32, filterable: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn uniform_entry(binding: u32, visibility: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

/// Layout of the bind group the model shader uses to read the paint layer
/// (group 2 in `model.wgsl`).
pub fn model_paint_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("ornatus model paint layout"),
        entries: &[
            texture_entry(0, true),
            texture_entry(1, true),
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            uniform_entry(3, wgpu::ShaderStages::FRAGMENT),
            texture_entry(4, true),
        ],
    })
}

/// A bind group that reads as "no paint", for meshes that are not painted.
pub fn empty_model_paint_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
) -> wgpu::BindGroup {
    let color = texture(device, 1, COLOR_FORMAT, "ornatus empty paint");
    let coverage = texture(device, 1, COVERAGE_FORMAT, "ornatus empty coverage");
    let stickers = texture(device, 1, COLOR_FORMAT, "ornatus empty stickers");
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor::default());
    let preview = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("ornatus empty preview"),
        contents: bytemuck::bytes_of(&PreviewUniform {
            color: [0.0; 4],
            flags: [0.0; 4],
        }),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("ornatus empty paint group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(
                    &color.create_view(&Default::default()),
                ),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(
                    &coverage.create_view(&Default::default()),
                ),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: preview.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(
                    &stickers.create_view(&Default::default()),
                ),
            },
        ],
    })
}

struct Pipelines {
    dab: wgpu::RenderPipeline,
    paint: wgpu::RenderPipeline,
    erase: wgpu::RenderPipeline,
    blit: wgpu::RenderPipeline,
    dab_layout: wgpu::BindGroupLayout,
    composite_layout: wgpu::BindGroupLayout,
    blit_layout: wgpu::BindGroupLayout,
    model_layout: wgpu::BindGroupLayout,
}

fn color_target(
    format: wgpu::TextureFormat,
    blend: Option<wgpu::BlendState>,
) -> Option<wgpu::ColorTargetState> {
    Some(wgpu::ColorTargetState {
        format,
        blend,
        write_mask: wgpu::ColorWrites::ALL,
    })
}

fn fullscreen_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::BindGroupLayout,
    vertex: &str,
    fragment: &str,
    target: Option<wgpu::ColorTargetState>,
) -> wgpu::RenderPipeline {
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(fragment),
        bind_group_layouts: &[Some(layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(fragment),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(vertex),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fragment),
            compilation_options: Default::default(),
            targets: &[target],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

impl Pipelines {
    fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ornatus paint shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("paint.wgsl").into()),
        });
        let dab_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ornatus dab layout"),
            entries: &[uniform_entry(0, wgpu::ShaderStages::VERTEX)],
        });
        let composite_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ornatus composite layout"),
            entries: &[
                texture_entry(0, false),
                uniform_entry(1, wgpu::ShaderStages::FRAGMENT),
            ],
        });
        let blit_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ornatus blit layout"),
            entries: &[
                texture_entry(0, true),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        // Dabs: instanced quads that keep the larger coverage where they overlap.
        let max_blend = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Max,
        };
        let dab_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ornatus dab pipeline layout"),
            bind_group_layouts: &[Some(&dab_layout)],
            immediate_size: 0,
        });
        let dab = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ornatus dab pipeline"),
            layout: Some(&dab_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_dab"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<DabInstance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32, 2 => Float32],
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_dab"),
                compilation_options: Default::default(),
                targets: &[color_target(
                    COVERAGE_FORMAT,
                    Some(wgpu::BlendState { color: max_blend, alpha: max_blend }),
                )],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let over = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        };
        let remove = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::Zero,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        };
        let paint = fullscreen_pipeline(
            device,
            &shader,
            &composite_layout,
            "vs_full",
            "fs_paint",
            color_target(
                COLOR_FORMAT,
                Some(wgpu::BlendState {
                    color: over,
                    alpha: over,
                }),
            ),
        );
        let erase = fullscreen_pipeline(
            device,
            &shader,
            &composite_layout,
            "vs_full",
            "fs_erase",
            color_target(
                COLOR_FORMAT,
                Some(wgpu::BlendState {
                    color: remove,
                    alpha: remove,
                }),
            ),
        );
        let blit = fullscreen_pipeline(
            device,
            &shader,
            &blit_layout,
            "vs_blit",
            "fs_blit",
            color_target(COLOR_FORMAT, None),
        );
        Self {
            dab,
            paint,
            erase,
            blit,
            dab_layout,
            composite_layout,
            blit_layout,
            model_layout: model_paint_layout(device),
        }
    }
}

/// A stroke's look, as the GPU needs it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrokeStyle {
    /// sRGB color, 0 to 1.
    pub color: [f32; 3],
    /// Stroke opacity, 0 to 1.
    pub opacity: f32,
    /// Remove paint instead of adding it.
    pub erase: bool,
    /// Dab radius in UV units.
    pub radius_uv: f32,
    /// 0 soft, 1 hard.
    pub hardness: f32,
}

impl From<&Brush> for StrokeStyle {
    fn from(brush: &Brush) -> Self {
        Self {
            color: brush.color.map(|c| f32::from(c) / 255.0),
            opacity: brush.opacity,
            erase: brush.erase,
            radius_uv: brush.radius_uv,
            hardness: brush.hardness,
        }
    }
}

/// Which texture a stroke is composited into.
#[derive(Clone, Copy)]
enum Target {
    Committed,
    Base,
}

/// The paint layer for one surface.
pub struct PaintLayer {
    size: u32,
    committed: wgpu::Texture,
    base: wgpu::Texture,
    scratch: wgpu::Texture,
    /// Every sticker drawn in order, in UV space.
    stickers: wgpu::Texture,
    sticker_pass: StickerPass,
    mipper: Mipper,
    pipelines: Pipelines,
    dab_buffer: wgpu::Buffer,
    stroke_buffer: wgpu::Buffer,
    preview_buffer: wgpu::Buffer,
    sampler: wgpu::Sampler,
    dab_group: wgpu::BindGroup,
    composite_group: wgpu::BindGroup,
    model_group: wgpu::BindGroup,
}

impl PaintLayer {
    /// A transparent layer, `size` texels on each side.
    pub fn new(device: &wgpu::Device, size: u32) -> Self {
        let pipelines = Pipelines::new(device);
        let committed = texture_with_mips(
            device,
            size,
            COLOR_FORMAT,
            "ornatus paint committed",
            mip_count(size),
        );
        let base = texture(device, size, COLOR_FORMAT, "ornatus paint base");
        let scratch = texture(device, size, COVERAGE_FORMAT, "ornatus paint scratch");
        let stickers = texture_with_mips(
            device,
            size,
            COLOR_FORMAT,
            "ornatus paint stickers",
            mip_count(size),
        );
        let dab_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ornatus dabs"),
            size: (DAB_CAPACITY * std::mem::size_of::<DabInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let dab_globals = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ornatus dab globals"),
            contents: bytemuck::bytes_of(&[size as f32, size as f32, 0.0, 0.0]),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let stroke_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ornatus stroke"),
            size: std::mem::size_of::<StrokeUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let preview_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ornatus stroke preview"),
            size: std::mem::size_of::<PreviewUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ornatus paint sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        let dab_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ornatus dab group"),
            layout: &pipelines.dab_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: dab_globals.as_entire_binding(),
            }],
        });
        let scratch_view = scratch.create_view(&Default::default());
        let composite_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ornatus composite group"),
            layout: &pipelines.composite_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&scratch_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: stroke_buffer.as_entire_binding(),
                },
            ],
        });
        let model_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ornatus model paint group"),
            layout: &pipelines.model_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &committed.create_view(&Default::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&scratch_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: preview_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(
                        &stickers.create_view(&Default::default()),
                    ),
                },
            ],
        });
        Self {
            size,
            committed,
            base,
            scratch,
            stickers,
            sticker_pass: StickerPass::new(device),
            mipper: Mipper::new(device),
            pipelines,
            dab_buffer,
            stroke_buffer,
            preview_buffer,
            sampler,
            dab_group,
            composite_group,
            model_group,
        }
    }

    /// Texture width and height in texels.
    pub fn size(&self) -> u32 {
        self.size
    }

    /// The bind group the model shader reads this layer through.
    pub fn model_group(&self) -> &wgpu::BindGroup {
        &self.model_group
    }

    fn submit(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        record: impl FnOnce(&mut wgpu::CommandEncoder),
    ) {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        record(&mut encoder);
        queue.submit([encoder.finish()]);
    }

    fn clear_texture(&self, device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) {
        let view = level_view(texture, 0);
        self.submit(device, queue, |encoder| {
            encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ornatus clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        });
    }

    /// Start a stroke: empty the coverage texture.
    pub fn begin_stroke(&self, device: &wgpu::Device, queue: &wgpu::Queue) {
        self.clear_texture(device, queue, &self.scratch);
    }

    /// Stamp dabs into the coverage of the stroke being drawn.
    pub fn stamp(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        style: &StrokeStyle,
        dabs: &[[f32; 2]],
    ) {
        let view = self.scratch.create_view(&Default::default());
        for chunk in dabs.chunks(DAB_CAPACITY) {
            let instances: Vec<DabInstance> = chunk
                .iter()
                .map(|&center| DabInstance {
                    center,
                    radius: style.radius_uv,
                    hardness: style.hardness,
                })
                .collect();
            queue.write_buffer(&self.dab_buffer, 0, bytemuck::cast_slice(&instances));
            self.submit(device, queue, |encoder| {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("ornatus stamp dabs"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(&self.pipelines.dab);
                pass.set_bind_group(0, &self.dab_group, &[]);
                pass.set_vertex_buffer(0, self.dab_buffer.slice(..));
                pass.draw(0..6, 0..instances.len() as u32);
            });
        }
    }

    /// Show the stroke being drawn on the surface before it is committed.
    pub fn set_preview(&self, queue: &wgpu::Queue, style: Option<&StrokeStyle>) {
        let uniform = match style {
            Some(style) => PreviewUniform {
                color: [
                    style.color[0],
                    style.color[1],
                    style.color[2],
                    style.opacity,
                ],
                flags: [f32::from(u8::from(style.erase)), 1.0, 0.0, 0.0],
            },
            None => PreviewUniform {
                color: [0.0; 4],
                flags: [0.0; 4],
            },
        };
        queue.write_buffer(&self.preview_buffer, 0, bytemuck::bytes_of(&uniform));
    }

    fn composite(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        style: &StrokeStyle,
        target: Target,
    ) {
        let uniform = StrokeUniform {
            color: [
                style.color[0],
                style.color[1],
                style.color[2],
                style.opacity,
            ],
        };
        queue.write_buffer(&self.stroke_buffer, 0, bytemuck::bytes_of(&uniform));
        let texture = match target {
            Target::Committed => &self.committed,
            Target::Base => &self.base,
        };
        let view = level_view(texture, 0);
        self.submit(device, queue, |encoder| {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ornatus composite stroke"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(if style.erase {
                &self.pipelines.erase
            } else {
                &self.pipelines.paint
            });
            pass.set_bind_group(0, &self.composite_group, &[]);
            pass.draw(0..3, 0..1);
        });
    }

    /// Finish the stroke being drawn: composite it into the layer and clear
    /// the preview.
    pub fn end_stroke(&self, device: &wgpu::Device, queue: &wgpu::Queue, style: &StrokeStyle) {
        self.composite(device, queue, style, Target::Committed);
        self.generate_mips(device, queue);
        self.clear_texture(device, queue, &self.scratch);
        self.set_preview(queue, None);
    }

    fn draw_stroke(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        stroke: &Stroke,
        target: Target,
    ) {
        let style = StrokeStyle::from(&stroke.brush);
        self.clear_texture(device, queue, &self.scratch);
        self.stamp(device, queue, &style, &stroke.dabs);
        self.composite(device, queue, &style, target);
        self.clear_texture(device, queue, &self.scratch);
    }

    /// Fill the smaller mip levels of the committed paint, so the surface
    /// does not shimmer when it is far away.
    fn generate_mips(&self, device: &wgpu::Device, queue: &wgpu::Queue) {
        self.mipper
            .generate(device, queue, &self.committed, self.size, self.size);
    }

    /// Redraw the stickers, first at the bottom, over a clear texture.
    pub fn render_stickers(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        images: &StickerImages,
        draws: &[StickerDraw],
    ) {
        self.sticker_pass
            .render(device, queue, &self.stickers, images, draws);
        self.mipper
            .generate(device, queue, &self.stickers, self.size, self.size);
    }

    fn copy_base_to_committed(&self, device: &wgpu::Device, queue: &wgpu::Queue) {
        let extent = wgpu::Extent3d {
            width: self.size,
            height: self.size,
            depth_or_array_layers: 1,
        };
        self.submit(device, queue, |encoder| {
            encoder.copy_texture_to_texture(
                self.base.as_image_copy(),
                self.committed.as_image_copy(),
                extent,
            );
        });
    }

    /// Rebuild the layer from the baked base plus these strokes (undo, redo).
    pub fn replay(&self, device: &wgpu::Device, queue: &wgpu::Queue, strokes: &[Stroke]) {
        self.copy_base_to_committed(device, queue);
        for stroke in strokes {
            self.draw_stroke(device, queue, stroke, Target::Committed);
        }
        self.generate_mips(device, queue);
    }

    /// Bake a stroke that can no longer be undone into the base. The layer
    /// itself already shows it.
    pub fn bake(&self, device: &wgpu::Device, queue: &wgpu::Queue, stroke: &Stroke) {
        self.draw_stroke(device, queue, stroke, Target::Base);
    }

    /// Remove all paint.
    pub fn clear(&self, device: &wgpu::Device, queue: &wgpu::Queue) {
        self.clear_texture(device, queue, &self.committed);
        self.generate_mips(device, queue);
        self.clear_texture(device, queue, &self.base);
        self.clear_texture(device, queue, &self.scratch);
        self.set_preview(queue, None);
    }

    /// A new layer at a different size with the same paint. The baked base is
    /// resampled, and `strokes` are replayed at the new size.
    pub fn resized(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        size: u32,
        strokes: &[Stroke],
    ) -> Self {
        let new = Self::new(device, size);
        let source = self.base.create_view(&Default::default());
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ornatus resize group"),
            layout: &new.pipelines.blit_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&source),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        let target = new.base.create_view(&Default::default());
        new.submit(device, queue, |encoder| {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ornatus resize"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&new.pipelines.blit);
            pass.set_bind_group(0, &group, &[]);
            pass.draw(0..3, 0..1);
        });
        new.replay(device, queue, strokes);
        new
    }
}
