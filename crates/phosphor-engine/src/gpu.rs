//! Low-level wgpu plumbing: the shared shader module, pipelines, bind groups, textures.

use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};

use crate::shaders;

/// Format for every intermediate and output texture.
pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// The graphics card can draw into 16-bit textures (deep colour); the app says so when it makes the device (see
/// `deep_colour_usable`).
static DEEP_COLOUR: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// What deep colour draws into: 16 bits a channel, still 0..1 like 8-bit (looks stay the same, only finer).
pub const DEEP_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Unorm;

/// What point pictures are kept in (points.wgsl): 16-bit float, so places aren't rounded to colour steps.
pub const DATA_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// Can this adapter draw into `DEEP_FORMAT`? Then the device must also be made with `deep_colour_features`.
pub fn deep_colour_usable(adapter: &wgpu::Adapter) -> bool {
    let needed = wgpu::Features::TEXTURE_FORMAT_16BIT_NORM | wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES;
    let usages = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
    adapter.features().contains(needed) && adapter.get_texture_format_features(DEEP_FORMAT).allowed_usages.contains(usages)
}

/// The device features deep colour needs (when `deep_colour_usable`).
pub fn deep_colour_features() -> wgpu::Features {
    wgpu::Features::TEXTURE_FORMAT_16BIT_NORM | wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES
}

/// Tells the engine the device was made with deep colour's features.
pub fn set_deep_colour(on: bool) {
    DEEP_COLOUR.store(on, std::sync::atomic::Ordering::Relaxed);
}

pub fn deep_colour() -> bool {
    DEEP_COLOUR.load(std::sync::atomic::Ordering::Relaxed)
}

/// Uniform block shared by all passes; mirrors `struct U` in common.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub(crate) struct Uniforms {
    /// Shader parameters (up to `shaders::MAX_PARAMS` floats; the last vec4 is reserved for
    /// ISF pass/frame info). Mapping passes use the first 20 floats for corners and homography.
    pub p: [[f32; 4]; 16],
    pub resolution: [f32; 2],
    pub time: f32,
    pub opacity: f32,
    pub uv_xform: [f32; 4],
    pub mode: u32,
    pub yuv: u32,
    /// Keeps the block 16-byte aligned (the per-pass stride rounds up to the GPU's offset alignment).
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
    /// Keyed by entry and the format of the target it draws into (float ISF buffers need their own).
    pipelines: HashMap<(&'static str, wgpu::TextureFormat), wgpu::RenderPipeline>,
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
                tex_entry(14),
                tex_entry(15),
                tex_entry(16),
                tex_entry(17),
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
        let fixed = ["fs_composite", "fs_to_rgb", "fs_mix", "fs_crossfade", "fs_finalize", "fs_surface", "fs_mesh", "fs_mask", "fs_sample", "fs_surface_add", "fs_surface_screen", "fs_surface_mul", "fs_mesh_add", "fs_mesh_screen", "fs_mesh_mul"];
        let all = fixed
            .into_iter()
            .chain(shaders::all_effect_entries())
            .chain(shaders::GENERATORS.iter().map(|d| d.entry));
        for entry in all {
            self.pipeline(entry);
        }
    }

    pub fn pipeline(&mut self, entry: &'static str) -> &wgpu::RenderPipeline {
        self.pipeline_for(entry, FORMAT)
    }

    /// The pipeline drawing `entry` into a target of `format` (built on first use for formats other than the default).
    pub fn pipeline_for(&mut self, entry: &'static str, format: wgpu::TextureFormat) -> &wgpu::RenderPipeline {
        let Self { device, module, pipeline_layout, pipelines, custom_modules, .. } = self;
        let module = custom_modules.get(entry).unwrap_or(module);
        pipelines.entry((entry, format)).or_insert_with(|| build_pipeline(device, pipeline_layout, module, entry, format))
    }

    /// What a background compile needs: the device and the shared pipeline layout (both are handles).
    pub fn compile_parts(&self) -> (wgpu::Device, wgpu::PipelineLayout) {
        (self.device.clone(), self.pipeline_layout.clone())
    }

    /// Drops a shader's module and pipelines (it's being replaced: an edited file).
    pub fn forget_custom(&mut self, entry: &str) {
        self.custom_modules.remove(entry);
        self.pipelines.retain(|(e, _), _| *e != entry);
    }

    /// Installs a module + pipeline built elsewhere (background shader compilation).
    pub fn install_custom(&mut self, entry: &'static str, module: wgpu::ShaderModule, pipeline: wgpu::RenderPipeline) {
        self.custom_modules.insert(entry, module);
        self.pipelines.insert((entry, FORMAT), pipeline);
    }
}

/// The render pipeline for one fragment entry: which vertex stage, topology and blending it needs.
/// Free function so a worker thread can build it (every wgpu object here is a thread-safe handle).
pub(crate) fn build_pipeline(device: &wgpu::Device, pipeline_layout: &wgpu::PipelineLayout, module: &wgpu::ShaderModule, entry: &'static str, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
    // Mapping surfaces are real quads blended onto the output; everything else is a
    // fullscreen triangle that overwrites its target.
    let surface = entry.starts_with("fs_surface");
    // Mesh-warped surfaces and masks draw triangles from a vertex buffer (x, y, s, t).
    let mesh = entry.starts_with("fs_mesh") || entry == "fs_mask";
    // Slice blend modes (`SliceBlend::entry`), all on premultiplied colour.
    let slice_blend = |src_factor, dst_factor| {
        let c = wgpu::BlendComponent { src_factor, dst_factor, operation: wgpu::BlendOperation::Add };
        let a = wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add };
        Some(wgpu::BlendState { color: c, alpha: a })
    };
    // Rutt-Etra draws additive line lists generated in the vertex shader.
    let rutt = entry == "fs_rutt" || entry == "fs_rutt_points";
    let points = entry == "fs_rutt_points";
    // Points renders (points.wgsl) make their vertices from the point picture: dots and a mesh as triangles, lines.
    let pts = shaders::points_render(entry);
    let additive = wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(entry),
        layout: Some(pipeline_layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some(if let Some(v) = shaders::vertex_entry(entry) {
                v
            } else if surface {
                "vs_surface"
            } else if mesh {
                "vs_mesh"
            } else if points {
                "vs_rutt_points"
            } else if entry == "fs_pts_dots" {
                "vs_pts_dots"
            } else if entry == "fs_pts_lines" {
                "vs_pts_lines"
            } else if entry == "fs_pts_mesh" {
                "vs_pts_mesh"
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
        // A points mesh keeps the nearest face (a depth buffer the encoder attaches); nothing else tests depth.
        depth_stencil: (entry == "fs_pts_mesh").then_some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some(entry),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: if entry.ends_with("_add") && (surface || mesh) {
                    slice_blend(wgpu::BlendFactor::One, wgpu::BlendFactor::One)
                } else if entry.ends_with("_screen") {
                    slice_blend(wgpu::BlendFactor::One, wgpu::BlendFactor::OneMinusSrc)
                } else if entry.ends_with("_mul") {
                    slice_blend(wgpu::BlendFactor::Dst, wgpu::BlendFactor::OneMinusSrcAlpha)
                } else if surface || mesh || entry == "fs_grid" {
                    Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING)
                } else if rutt || (pts && entry != "fs_pts_mesh") {
                    Some(wgpu::BlendState { color: additive, alpha: additive })
                } else if pts {
                    Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING)
                } else if shaders::instances(entry).is_some() {
                    // Instanced copies (particles) land over each other, straight alpha like every source.
                    Some(wgpu::BlendState::ALPHA_BLENDING)
                } else {
                    None
                },
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
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
        Self::ensure_format(slot, device, label, w, h, FORMAT);
    }

    /// Reallocates only when the size or format changes.
    pub(crate) fn ensure_format(slot: &mut Option<Target>, device: &wgpu::Device, label: &'static str, w: u32, h: u32, format: wgpu::TextureFormat) {
        if !matches!(slot, Some(t) if t.width == w.max(1) && t.height == h.max(1) && t.texture.format() == format) {
            *slot = Some(Target::with_format(device, label, w, h, format));
        }
    }
}
