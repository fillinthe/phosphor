//! Low-level wgpu plumbing: the shared shader module, pipelines, bind groups, textures.

use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};

use crate::shaders;

/// Format for every intermediate and output texture.
pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// Uniform block shared by all passes; mirrors `struct U` in common.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub(crate) struct Uniforms {
    /// Shader parameters (up to `shaders::MAX_PARAMS` floats; the last vec4 is reserved for
    /// ISF pass/frame info). Mapping passes use the first 20 floats for corners and homography.
    pub p: [[f32; 4]; 12],
    pub resolution: [f32; 2],
    pub time: f32,
    pub opacity: f32,
    pub uv_xform: [f32; 4],
    pub mode: u32,
    pub yuv: u32,
    /// Pads to 256 bytes, the GPU's uniform offset alignment anyway (so the larger block is free).
    pub _pad: [u32; 6],
}

impl Uniforms {
    pub fn with_params(values: &[f32]) -> Self {
        let mut u = Self { uv_xform: [1.0, 1.0, 0.0, 0.0], opacity: 1.0, ..Default::default() };
        for (i, v) in values.iter().take(shaders::MAX_PARAMS).enumerate() {
            u.p[i / 4][i % 4] = *v;
        }
        u
    }
}

pub(crate) struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    module: wgpu::ShaderModule,
    pub layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    pipelines: HashMap<&'static str, wgpu::RenderPipeline>,
    /// Modules for user shaders, keyed by their (unique) entry point.
    pub custom_modules: HashMap<&'static str, wgpu::ShaderModule>,
    pub sampler: wgpu::Sampler,
    pub sampler_mirror: wgpu::Sampler,
    /// 1x1 transparent black, bound to unused texture slots.
    pub blank: wgpu::TextureView,
    pub uniform_stride: u64,
}

impl Gpu {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("phosphor"),
            source: wgpu::ShaderSource::Wgsl(shaders::wgsl_source().into()),
        });

        let tex_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let sampler_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("phosphor"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<Uniforms>() as u64),
                    },
                    count: None,
                },
                sampler_entry(1),
                sampler_entry(2),
                tex_entry(3),
                tex_entry(4),
                tex_entry(5),
                // Multi-pass buffers (see `shaders::PassPlan`).
                tex_entry(6),
                tex_entry(7),
                tex_entry(8),
                tex_entry(9),
                tex_entry(10),
                tex_entry(11),
                tex_entry(12),
                tex_entry(13),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("phosphor"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });

        let make_sampler = |mode| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                address_mode_u: mode,
                address_mode_v: mode,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            })
        };
        let sampler = make_sampler(wgpu::AddressMode::ClampToEdge);
        let sampler_mirror = make_sampler(wgpu::AddressMode::MirrorRepeat);

        let blank_tex = device.create_texture(&texture_desc("blank", 1, 1, FORMAT));
        queue.write_texture(
            blank_tex.as_image_copy(),
            &[0, 0, 0, 0],
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4), rows_per_image: None },
            wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        );
        let blank = blank_tex.create_view(&Default::default());

        let align = device.limits().min_uniform_buffer_offset_alignment as u64;
        let size = std::mem::size_of::<Uniforms>() as u64;
        let uniform_stride = size.div_ceil(align) * align;

        Self {
            device,
            queue,
            module,
            layout,
            pipeline_layout,
            pipelines: HashMap::new(),
            custom_modules: HashMap::new(),
            sampler,
            sampler_mirror,
            blank,
            uniform_stride,
        }
    }

    /// Compiles every pipeline up front so the first use of an effect never hitches a show.
    pub fn prewarm(&mut self) {
        let fixed = ["fs_composite", "fs_to_rgb", "fs_mix", "fs_crossfade", "fs_finalize", "fs_surface", "fs_mesh", "fs_mask"];
        let all = fixed
            .into_iter()
            .chain(shaders::all_effect_entries())
            .chain(shaders::GENERATORS.iter().map(|d| d.entry));
        for entry in all {
            self.pipeline(entry);
        }
    }

    pub fn pipeline(&mut self, entry: &'static str) -> &wgpu::RenderPipeline {
        let Self { device, module, pipeline_layout, pipelines, custom_modules, .. } = self;
        let module = custom_modules.get(entry).unwrap_or(module);
        // Mapping surfaces are real quads blended onto the output; everything else is a
        // fullscreen triangle that overwrites its target.
        let surface = entry == "fs_surface";
        // Mesh-warped surfaces and masks draw triangles from a vertex buffer (x, y, s, t).
        let mesh = entry == "fs_mesh" || entry == "fs_mask";
        // Rutt-Etra draws additive line lists generated in the vertex shader.
        let rutt = entry == "fs_rutt" || entry == "fs_rutt_points";
        let points = entry == "fs_rutt_points";
        let additive = wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add };
        pipelines.entry(entry).or_insert_with(|| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(pipeline_layout),
                vertex: wgpu::VertexState {
                    module,
                    entry_point: Some(if surface {
                        "vs_surface"
                    } else if mesh {
                        "vs_mesh"
                    } else if points {
                        "vs_rutt_points"
                    } else if rutt {
                        "vs_rutt"
                    } else {
                        "vs_fullscreen"
                    }),
                    compilation_options: Default::default(),
                    buffers: if mesh { MESH_VERTEX } else { &[] },
                },
                primitive: wgpu::PrimitiveState {
                    topology: if points {
                        wgpu::PrimitiveTopology::PointList
                    } else if rutt {
                        wgpu::PrimitiveTopology::LineList
                    } else {
                        wgpu::PrimitiveTopology::TriangleList
                    },
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: FORMAT,
                        blend: if surface || entry == "fs_mesh" || entry == "fs_mask" {
                            Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING)
                        } else if rutt {
                            Some(wgpu::BlendState { color: additive, alpha: additive })
                        } else {
                            None
                        },
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        })
    }
}

const MESH_ATTRIBUTES: &[wgpu::VertexAttribute] = &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2];
const MESH_VERTEX: &[Option<wgpu::VertexBufferLayout<'static>>] =
    &[Some(wgpu::VertexBufferLayout { array_stride: 16, step_mode: wgpu::VertexStepMode::Vertex, attributes: MESH_ATTRIBUTES })];

pub(crate) fn texture_desc(
    label: &'static str,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
) -> wgpu::TextureDescriptor<'static> {
    wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width: width.max(1), height: height.max(1), depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    }
}

/// A GPU texture plus its default view and size.
pub struct Target {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub width: u32,
    pub height: u32,
}

impl Target {
    pub(crate) fn new(device: &wgpu::Device, label: &'static str, width: u32, height: u32) -> Self {
        Self::with_format(device, label, width, height, FORMAT)
    }

    pub(crate) fn with_format(
        device: &wgpu::Device,
        label: &'static str,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> Self {
        let texture = device.create_texture(&texture_desc(label, width, height, format));
        let view = texture.create_view(&Default::default());
        Self { texture, view, width: width.max(1), height: height.max(1) }
    }

    /// Reallocates only when the size changes.
    pub(crate) fn ensure(slot: &mut Option<Target>, device: &wgpu::Device, label: &'static str, w: u32, h: u32) {
        if !matches!(slot, Some(t) if t.width == w.max(1) && t.height == h.max(1)) {
            *slot = Some(Target::new(device, label, w, h));
        }
    }
}
