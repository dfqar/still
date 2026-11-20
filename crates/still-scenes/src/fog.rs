use bytemuck::{Pod, Zeroable};
use still_core::{FramePlan, PointerInput, Quality};
use still_renderer::wgpu::{self, util::DeviceExt};

pub const FOG_SCENE_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FogParameters {
    pub density: f32,
    pub drift: f32,
    pub depth: f32,
    pub light: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct FogPreset {
    pub id: &'static str,
    pub name: &'static str,
    pub seed: u64,
    pub parameters: FogParameters,
}

pub const FOG_PRESETS: [FogPreset; 4] = [
    FogPreset {
        id: "builtin.morning-haze",
        name: "Morning Haze",
        seed: 0xF0_61_10_01,
        parameters: FogParameters {
            density: 0.48,
            drift: 0.46,
            depth: 0.72,
            light: 0.62,
        },
    },
    FogPreset {
        id: "builtin.dense-silence",
        name: "Dense Silence",
        seed: 0xF0_61_10_02,
        parameters: FogParameters {
            density: 0.82,
            drift: 0.24,
            depth: 0.88,
            light: 0.38,
        },
    },
    FogPreset {
        id: "builtin.cold-layer",
        name: "Cold Layer",
        seed: 0xF0_61_10_03,
        parameters: FogParameters {
            density: 0.62,
            drift: 0.68,
            depth: 0.56,
            light: 0.48,
        },
    },
    FogPreset {
        id: "builtin.after-rain",
        name: "After Rain",
        seed: 0xF0_61_10_04,
        parameters: FogParameters {
            density: 0.38,
            drift: 0.34,
            depth: 0.94,
            light: 0.76,
        },
    },
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FogQualityBudget {
    pub megapixels: f32,
    pub ray_steps: u32,
    pub noise_octaves: u32,
}

pub fn fog_quality_budget(quality: Quality) -> FogQualityBudget {
    match quality {
        Quality::Eco => FogQualityBudget {
            megapixels: 0.45,
            ray_steps: 10,
            noise_octaves: 2,
        },
        Quality::Auto | Quality::Balanced => FogQualityBudget {
            megapixels: 0.8,
            ray_steps: 14,
            noise_octaves: 3,
        },
        Quality::High => FogQualityBudget {
            megapixels: 1.35,
            ray_steps: 20,
            noise_octaves: 4,
        },
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct FogUniforms {
    time: f32,
    seed: f32,
    density: f32,
    drift: f32,
    depth: f32,
    light: f32,
    pointer: [f32; 2],
    viewport: [f32; 2],
    ray_steps: u32,
    noise_octaves: u32,
    _padding: [f32; 4],
}

struct FogTarget {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    size: [u32; 2],
}

pub struct Fog {
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    target: Option<FogTarget>,
    output_generation: u64,
    output_format: wgpu::TextureFormat,
    quality: Quality,
    seed: u64,
    time: f32,
    pointer: [f32; 2],
}

impl Fog {
    pub fn new(
        device: &wgpu::Device,
        adapter: &wgpu::Adapter,
        quality: Quality,
        seed: u64,
    ) -> Self {
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Fog uniforms"),
            contents: bytemuck::bytes_of(&FogUniforms::zeroed()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Fog bind group layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Fog bind group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Fog shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("fog.wgsl").into()),
        });
        let output_format = preferred_output_format(adapter);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Fog pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Fog pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: "fullscreen_vertex",
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: "fog_fragment",
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: output_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });
        Self {
            uniform_buffer,
            bind_group,
            pipeline,
            target: None,
            output_generation: 0,
            output_format,
            quality,
            seed,
            time: 0.0,
            pointer: [0.0; 2],
        }
    }

    pub fn set_quality(&mut self, quality: Quality) {
        if self.quality != quality {
            self.quality = quality;
            self.release_gpu_resources();
        }
    }

    pub fn set_seed(&mut self, seed: u64) {
        self.seed = seed;
        self.time = 0.0;
    }

    pub fn prepare(&mut self, device: &wgpu::Device, viewport: [u32; 2]) {
        if viewport.contains(&0) {
            self.release_gpu_resources();
            return;
        }
        let pixels = f64::from(viewport[0]) * f64::from(viewport[1]);
        let budget = fog_quality_budget(self.quality);
        let scale = ((f64::from(budget.megapixels) * 1_000_000.0 / pixels).sqrt()).min(1.0);
        let size = [
            ((f64::from(viewport[0]) * scale).round() as u32).max(64),
            ((f64::from(viewport[1]) * scale).round() as u32).max(64),
        ];
        if self
            .target
            .as_ref()
            .is_some_and(|target| target.size == size)
        {
            return;
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Fog output"),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.output_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.target = Some(FogTarget {
            _texture: texture,
            view,
            size,
        });
        self.output_generation = self.output_generation.wrapping_add(1);
    }

    pub fn encode(
        &mut self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        frame: FramePlan,
        parameters: FogParameters,
        pointer: PointerInput,
    ) -> Option<(u64, &wgpu::TextureView)> {
        let target = self.target.as_ref()?;
        let dt = frame.display_dt.clamp(0.0, 0.1);
        self.time += dt;
        let target_pointer = if pointer.active {
            pointer.position
        } else {
            [0.0; 2]
        };
        let blend = 1.0 - (-dt / 0.85).exp();
        for (current, target) in self.pointer.iter_mut().zip(target_pointer) {
            *current += (target - *current) * blend;
        }
        let budget = fog_quality_budget(self.quality);
        let uniforms = FogUniforms {
            time: self.time,
            seed: crate::seed::seed_phase(self.seed),
            density: parameters.density,
            drift: parameters.drift,
            depth: parameters.depth,
            light: parameters.light,
            pointer: self.pointer,
            viewport: [target.size[0] as f32, target.size[1] as f32],
            ray_steps: budget.ray_steps,
            noise_octaves: budget.noise_octaves,
            _padding: [0.0; 4],
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Fog render"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target.view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.draw(0..3, 0..1);
        drop(pass);
        Some((self.output_generation, &target.view))
    }

    pub fn release_gpu_resources(&mut self) {
        if self.target.take().is_some() {
            self.output_generation = self.output_generation.wrapping_add(1);
        }
    }
}

fn preferred_output_format(adapter: &wgpu::Adapter) -> wgpu::TextureFormat {
    let format = wgpu::TextureFormat::Rgba16Float;
    let features = adapter.get_texture_format_features(format);
    if features
        .allowed_usages
        .contains(wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING)
        && features
            .flags
            .contains(wgpu::TextureFormatFeatureFlags::FILTERABLE)
    {
        format
    } else {
        wgpu::TextureFormat::Rgba8Unorm
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quality_budgets_are_scene_specific_and_monotonic() {
        let eco = fog_quality_budget(Quality::Eco);
        let balanced = fog_quality_budget(Quality::Balanced);
        let high = fog_quality_budget(Quality::High);
        assert!(eco.megapixels < balanced.megapixels);
        assert!(balanced.megapixels < high.megapixels);
        assert!(eco.ray_steps < balanced.ray_steps);
        assert!(balanced.ray_steps < high.ray_steps);
    }

    #[test]
    fn fog_shader_validates() {
        let source = include_str!("fog.wgsl");
        let module = naga::front::wgsl::parse_str(source).expect("Fog WGSL parses");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .expect("Fog WGSL validates");
        assert_eq!(std::mem::size_of::<FogUniforms>(), 64);
    }
}
