//! Stickers on the GPU: imported images, and the pass that draws every
//! sticker, in order, into a texture in UV space (decision 0003).

use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};

use crate::paint::{COLOR_FORMAT, level_view, mip_count, texture_entry};

/// Stickers drawn per frame at most.
const MAX_STICKERS: usize = 512;

/// One sticker to draw, already in UV space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StickerDraw {
    /// The image to show.
    pub image: u32,
    /// Center in UV space.
    pub center: [f32; 2],
    /// Half of the sticker's width, as a UV vector.
    pub axis_u: [f32; 2],
    /// Half of the sticker's height, as a UV vector.
    pub axis_v: [f32; 2],
    /// 0 to 1.
    pub opacity: f32,
    /// Mirror the image.
    pub flip: bool,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct StickerInstance {
    center: [f32; 2],
    axis_u: [f32; 2],
    axis_v: [f32; 2],
    opacity: f32,
    flip: f32,
}

/// Builds smaller mip levels by repeatedly halving with a linear filter.
pub(crate) struct Mipper {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
}

/// Layout shared by everything that binds "an image and a sampler".
pub(crate) fn image_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("ornatus image layout"),
        entries: &[
            texture_entry(0, true),
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

fn shader(device: &wgpu::Device) -> wgpu::ShaderModule {
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("ornatus paint shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("paint.wgsl").into()),
    })
}

impl Mipper {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let layout = image_layout(device);
        let module = shader(device);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ornatus mip layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ornatus mip pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_blit"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_blit"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: COLOR_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ornatus mip sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            pipeline,
            layout,
            sampler,
        }
    }

    /// Fill every mip level below the first from the one above it.
    pub(crate) fn generate(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
        width: u32,
        height: u32,
    ) {
        let levels = mip_count(width.max(height));
        for level in 1..levels {
            let source = level_view(texture, level - 1);
            let target = level_view(texture, level);
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("ornatus mip group"),
                layout: &self.layout,
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
            let mut encoder =
                device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("ornatus mip"),
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
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &group, &[]);
                pass.draw(0..3, 0..1);
            }
            queue.submit([encoder.finish()]);
        }
    }
}

/// Multiply color by alpha, so linear filtering does not fringe at soft edges.
fn premultiply(rgba: &[u8]) -> Vec<u8> {
    rgba.as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| {
            let a = u16::from(p[3]);
            let scale = |c: u8| ((u16::from(c) * a + 127) / 255) as u8;
            [scale(p[0]), scale(p[1]), scale(p[2]), p[3]]
        })
        .collect()
}

struct GpuImage {
    group: wgpu::BindGroup,
    _texture: wgpu::Texture,
}

/// Imported images, ready to be drawn as stickers.
pub struct StickerImages {
    mipper: Mipper,
    images: HashMap<u32, GpuImage>,
}

impl StickerImages {
    /// An empty set.
    pub fn new(device: &wgpu::Device) -> Self {
        Self {
            mipper: Mipper::new(device),
            images: HashMap::new(),
        }
    }

    /// Upload an image under `id`. `rgba` is straight (not premultiplied) 8-bit
    /// RGBA, `width * height * 4` bytes.
    pub fn add(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        id: u32,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> Result<(), String> {
        let expected = width as usize * height as usize * 4;
        if width == 0 || height == 0 || rgba.len() != expected {
            return Err(format!(
                "image data is {} bytes, expected {expected}",
                rgba.len()
            ));
        }
        let premultiplied = premultiply(rgba);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ornatus sticker image"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: mip_count(width.max(height)),
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: COLOR_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            texture.as_image_copy(),
            &premultiplied,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * width),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.mipper.generate(device, queue, &texture, width, height);
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ornatus sticker image group"),
            layout: &self.mipper.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &texture.create_view(&Default::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.mipper.sampler),
                },
            ],
        });
        self.images.insert(
            id,
            GpuImage {
                group,
                _texture: texture,
            },
        );
        Ok(())
    }

    /// Forget an image.
    pub fn remove(&mut self, id: u32) {
        self.images.remove(&id);
    }

    fn group(&self, id: u32) -> Option<&wgpu::BindGroup> {
        self.images.get(&id).map(|image| &image.group)
    }
}

/// Draws stickers into a texture.
pub(crate) struct StickerPass {
    pipeline: wgpu::RenderPipeline,
    buffer: wgpu::Buffer,
}

impl StickerPass {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let layout = image_layout(device);
        let module = shader(device);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ornatus sticker layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let over = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ornatus sticker pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_sticker"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<StickerInstance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x2, 1 => Float32x2, 2 => Float32x2, 3 => Float32, 4 => Float32
                    ],
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_sticker"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: COLOR_FORMAT,
                    blend: Some(wgpu::BlendState {
                        color: over,
                        alpha: over,
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ornatus stickers"),
            size: (MAX_STICKERS * std::mem::size_of::<StickerInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self { pipeline, buffer }
    }

    /// Clear `target` and draw the stickers into it, first one at the bottom.
    pub(crate) fn render(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::Texture,
        images: &StickerImages,
        draws: &[StickerDraw],
    ) {
        let draws = &draws[..draws.len().min(MAX_STICKERS)];
        let instances: Vec<StickerInstance> = draws
            .iter()
            .map(|d| StickerInstance {
                center: d.center,
                axis_u: d.axis_u,
                axis_v: d.axis_v,
                opacity: d.opacity,
                flip: f32::from(u8::from(d.flip)),
            })
            .collect();
        if !instances.is_empty() {
            queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&instances));
        }
        let view = level_view(target, 0);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ornatus stickers"),
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
            pass.set_pipeline(&self.pipeline);
            pass.set_vertex_buffer(0, self.buffer.slice(..));
            for (i, draw) in draws.iter().enumerate() {
                if let Some(group) = images.group(draw.image) {
                    pass.set_bind_group(0, group, &[]);
                    pass.draw(0..6, i as u32..i as u32 + 1);
                }
            }
        }
        queue.submit([encoder.finish()]);
    }
}

#[cfg(test)]
mod tests {
    use super::premultiply;

    #[test]
    fn color_is_scaled_by_alpha() {
        let out = premultiply(&[200, 100, 50, 255, 200, 100, 50, 128, 200, 100, 50, 0]);
        assert_eq!(&out[0..4], [200, 100, 50, 255]);
        assert_eq!(&out[4..8], [100, 50, 25, 128]);
        assert_eq!(&out[8..12], [0, 0, 0, 0]);
    }
}
