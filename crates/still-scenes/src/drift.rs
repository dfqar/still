use bytemuck::{Pod, Zeroable};
use still_core::{FramePlan, PointerInput, Quality};
use still_renderer::wgpu::{self, util::DeviceExt};

use crate::DriftParameters;

pub const MAX_PARTICLES: u32 = 30_000;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct Particle {
    position: [f32; 2],
    previous_position: [f32; 2],
    velocity: [f32; 2],
    normalized_age: f32,
    seed: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct Uniforms {
    dt: f32,
    time: f32,
    active_count: u32,
    seed: u32,
    speed: f32,
    flow: f32,
    light: f32,
    interpolation: f32,
    pointer_position: [f32; 2],
    pointer_velocity: [f32; 2],
    viewport: [f32; 2],
    trail_decay: f32,
    noise_octaves: u32,
}

struct History {
    _textures: [wgpu::Texture; 2],
    views: [wgpu::TextureView; 2],
    decay_groups: [wgpu::BindGroup; 2],
    size: [u32; 2],
    index: usize,
}

pub struct Drift {
    _particle_buffer: wgpu::Buffer,
    uniform_buffer: wgpu::Buffer,
    compute_bind_group: wgpu::BindGroup,
    particle_bind_group: wgpu::BindGroup,
    texture_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    compute_pipeline: wgpu::ComputePipeline,
    particle_pipeline: wgpu::RenderPipeline,
    decay_pipeline: wgpu::RenderPipeline,
    history: Option<History>,
    history_generation: u64,
    history_format: wgpu::TextureFormat,
    quality: Quality,
    seed: u64,
    time: f32,
    initialized: bool,
}

impl Drift {
    pub fn new(
        device: &wgpu::Device,
        adapter: &wgpu::Adapter,
        quality: Quality,
        seed: u64,
    ) -> Self {
        let particle_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Drift particles"),
            size: MAX_PARTICLES as u64 * std::mem::size_of::<Particle>() as u64,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Drift uniforms"),
            contents: bytemuck::bytes_of(&Uniforms::zeroed()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let compute_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Drift compute layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let render_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Drift render storage layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let compute_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Drift compute bind group"),
            layout: &compute_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: particle_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: uniform_buffer.as_entire_binding(),
                },
            ],
        });
        let particle_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Drift particle bind group"),
            layout: &render_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: particle_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: uniform_buffer.as_entire_binding(),
                },
            ],
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Drift history texture layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Drift linear clamp sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let sim = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Drift simulation"),
            source: wgpu::ShaderSource::Wgsl(include_str!("drift_sim.wgsl").into()),
        });
        let draw = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Drift drawing"),
            source: wgpu::ShaderSource::Wgsl(include_str!("drift_draw.wgsl").into()),
        });
        let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Drift compute pipeline"),
            layout: Some(
                &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: None,
                    bind_group_layouts: &[&compute_layout],
                    push_constant_ranges: &[],
                }),
            ),
            module: &sim,
            entry_point: "simulate",
            compilation_options: Default::default(),
        });
        let history_format = preferred_history_format(adapter);
        let particle_pipeline = render_pipeline(
            device,
            "Drift particles",
            &draw,
            "particle_vertex",
            "particle_fragment",
            &[&render_layout],
            history_format,
            Some(wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
            }),
        );
        let decay_pipeline = render_pipeline(
            device,
            "Drift trail decay",
            &draw,
            "fullscreen_vertex",
            "decay_fragment",
            &[&texture_layout],
            history_format,
            None,
        );
        Self {
            _particle_buffer: particle_buffer,
            uniform_buffer,
            compute_bind_group,
            particle_bind_group,
            texture_layout,
            sampler,
            compute_pipeline,
            particle_pipeline,
            decay_pipeline,
            history: None,
            history_generation: 0,
            history_format,
            quality,
            seed,
            time: 0.0,
            initialized: false,
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
        self.initialized = false;
        self.time = 0.0;
    }
    pub fn release_gpu_resources(&mut self) {
        self.history = None;
        self.history_generation = self.history_generation.wrapping_add(1);
    }

    pub fn prepare(&mut self, device: &wgpu::Device, size: [u32; 2]) {
        self.ensure_history(device, size);
    }

    pub fn history_sources(&self) -> Option<(u64, [&wgpu::TextureView; 2])> {
        self.history.as_ref().map(|history| {
            (
                self.history_generation,
                [&history.views[0], &history.views[1]],
            )
        })
    }

    pub fn output(&self, index: usize) -> Option<(u64, &wgpu::TextureView)> {
        let history = self.history.as_ref()?;
        Some((self.history_generation, &history.views[index.min(1)]))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn encode(
        &mut self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        plan: FramePlan,
        parameters: &DriftParameters,
        pointer: PointerInput,
    ) -> Option<usize> {
        let Some(history) = &mut self.history else {
            return None;
        };
        self.time += plan.display_dt.min(0.1);
        let active_count = active_particle_count(self.quality, parameters.density);
        // Continuous-time exponential decay yields the same mean trail brightness at 60/120 Hz.
        let decay = (-plan.display_dt.max(0.0) * 2.65).exp();
        let uniforms = Uniforms {
            dt: plan.fixed_dt,
            time: self.time,
            active_count,
            seed: (self.seed ^ (self.seed >> 32)) as u32,
            speed: parameters.speed,
            flow: parameters.flow,
            light: parameters.light,
            interpolation: plan.interpolation,
            pointer_position: pointer.position,
            pointer_velocity: if pointer.active {
                pointer.velocity
            } else {
                [0.0; 2]
            },
            viewport: [history.size[0] as f32, history.size[1] as f32],
            trail_decay: decay,
            noise_octaves: drift_quality_budget(self.quality).noise_octaves,
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
        let tick_count = if self.initialized {
            plan.ticks
        } else {
            plan.ticks.max(1)
        };
        for _ in 0..tick_count {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Drift simulation tick"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.compute_pipeline);
            pass.set_bind_group(0, &self.compute_bind_group, &[]);
            pass.dispatch_workgroups(active_count.div_ceil(256), 1, 1);
        }
        self.initialized = true;
        let src = history.index;
        let dst = 1 - src;
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Drift history"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &history.views[dst],
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
            pass.set_pipeline(&self.decay_pipeline);
            pass.set_bind_group(0, &history.decay_groups[src], &[]);
            pass.draw(0..3, 0..1);
            pass.set_pipeline(&self.particle_pipeline);
            pass.set_bind_group(0, &self.particle_bind_group, &[]);
            pass.draw(0..6, 0..active_count);
        }
        history.index = dst;
        Some(dst)
    }

    fn ensure_history(&mut self, device: &wgpu::Device, surface: [u32; 2]) {
        if surface.contains(&0) {
            self.history = None;
            return;
        }
        let pixels = surface[0] as f64 * surface[1] as f64;
        let scale = ((drift_quality_budget(self.quality).megapixels as f64 * 1_000_000.0 / pixels)
            .sqrt())
        .min(1.0);
        let size = [
            ((surface[0] as f64 * scale).round() as u32).max(64),
            ((surface[1] as f64 * scale).round() as u32).max(64),
        ];
        if self.history.as_ref().is_some_and(|h| h.size == size) {
            return;
        }
        let format = self.history_format;
        let make = |label| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        };
        let textures = [make("Drift history A"), make("Drift history B")];
        let views = [
            textures[0].create_view(&Default::default()),
            textures[1].create_view(&Default::default()),
        ];
        let decay_groups = [
            texture_bind_group(
                device,
                &self.texture_layout,
                &views[0],
                &self.sampler,
                &self.uniform_buffer,
            ),
            texture_bind_group(
                device,
                &self.texture_layout,
                &views[1],
                &self.sampler,
                &self.uniform_buffer,
            ),
        ];
        self.history_generation = self.history_generation.wrapping_add(1);
        self.history = Some(History {
            _textures: textures,
            views,
            decay_groups,
            size,
            index: 0,
        });
    }
}

fn preferred_history_format(adapter: &wgpu::Adapter) -> wgpu::TextureFormat {
    let f = adapter.get_texture_format_features(wgpu::TextureFormat::Rgba16Float);
    if f.allowed_usages
        .contains(wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING)
        && f.flags
            .contains(wgpu::TextureFormatFeatureFlags::FILTERABLE)
    {
        wgpu::TextureFormat::Rgba16Float
    } else {
        wgpu::TextureFormat::Rgba8Unorm
    }
}

fn active_particle_count(quality: Quality, density: f32) -> u32 {
    let density_scale = 0.75 + density.clamp(0.25, 1.0) * 0.25;
    ((drift_quality_budget(quality).particles as f32 * density_scale).round() as u32)
        .clamp(1, MAX_PARTICLES)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DriftQualityBudget {
    pub particles: u32,
    pub megapixels: f32,
    pub noise_octaves: u32,
}

pub fn drift_quality_budget(quality: Quality) -> DriftQualityBudget {
    match quality {
        Quality::Eco => DriftQualityBudget {
            particles: 10_000,
            megapixels: 1.0,
            noise_octaves: 2,
        },
        Quality::Auto | Quality::Balanced => DriftQualityBudget {
            particles: 20_000,
            megapixels: 2.0,
            noise_octaves: 3,
        },
        Quality::High => DriftQualityBudget {
            particles: 30_000,
            megapixels: 4.0,
            noise_octaves: 3,
        },
    }
}

#[allow(clippy::too_many_arguments)]
fn render_pipeline(
    device: &wgpu::Device,
    label: &str,
    module: &wgpu::ShaderModule,
    vs: &str,
    fs: &str,
    layouts: &[&wgpu::BindGroupLayout],
    format: wgpu::TextureFormat,
    blend: Option<wgpu::BlendState>,
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts: layouts,
        push_constant_ranges: &[],
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: vs,
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: fs,
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
    })
}

fn texture_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
    uniforms: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Drift texture bind group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: uniforms.as_entire_binding(),
            },
        ],
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReferenceParticle {
    pub position: [f32; 2],
    pub velocity: [f32; 2],
    pub age: f32,
    pub seed: u32,
}

fn hash(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846ca68b);
    x ^ (x >> 16)
}
fn unit(x: u32) -> f32 {
    (hash(x) as f64 / u32::MAX as f64) as f32
}

pub fn deterministic_particle(index: u32, seed: u32) -> ReferenceParticle {
    ReferenceParticle {
        position: [
            unit(index ^ seed) * 2.0 - 1.0,
            unit(index.wrapping_add(0x9e3779b9) ^ seed) * 2.0 - 1.0,
        ],
        velocity: [0.0; 2],
        age: unit(index.wrapping_mul(17) ^ seed),
        seed: hash(index ^ seed),
    }
}

/// Small deterministic oracle for tests only; the product path never calls it.
pub fn cpu_reference(mut p: ReferenceParticle, ticks: u32, dt: f32) -> ReferenceParticle {
    for _ in 0..ticks {
        let a = (p.position[0] * 2.3 + p.position[1] * 1.7 + p.seed as f32 * 1e-6).sin();
        p.velocity[0] = p.velocity[0] * 0.985 + a.cos() * dt * 0.4;
        p.velocity[1] = p.velocity[1] * 0.985 - a.sin() * dt * 0.4;
        p.position[0] += p.velocity[0] * dt;
        p.position[1] += p.velocity[1] * dt;
        p.age += dt * 0.08;
        if p.position.iter().any(|v| v.abs() > 1.2) || p.age >= 1.0 {
            p = deterministic_particle(p.seed, p.seed.rotate_left(13));
        }
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn initialization_is_deterministic() {
        assert_eq!(deterministic_particle(42, 7), deterministic_particle(42, 7));
    }
    #[test]
    fn gpu_particle_layout_is_compact() {
        assert_eq!(std::mem::size_of::<Particle>(), 32);
    }
    #[test]
    fn density_stays_within_quality_particle_budgets() {
        assert_eq!(active_particle_count(Quality::Eco, 0.25), 8_125);
        assert_eq!(active_particle_count(Quality::Eco, 1.0), 10_000);
        assert!((18_000..=20_000).contains(&active_particle_count(Quality::Balanced, 0.66)));
        assert_eq!(active_particle_count(Quality::High, 1.0), 30_000);
    }
    #[test]
    fn reference_is_repeatable_and_finite() {
        let a = cpu_reference(deterministic_particle(4, 9), 240, 1.0 / 120.0);
        let b = cpu_reference(deterministic_particle(4, 9), 240, 1.0 / 120.0);
        assert_eq!(a, b);
        assert!(a.position.into_iter().chain(a.velocity).all(f32::is_finite));
    }
    #[test]
    fn trail_decay_is_cadence_independent() {
        let a = (-2.65_f32 / 60.0).exp().powi(60);
        let b = (-2.65_f32 / 120.0).exp().powi(120);
        assert!((a - b).abs() < 1e-5);
    }
    #[test]
    fn wgsl_and_gpu_pipelines_validate_on_metal() {
        for source in [
            include_str!("drift_sim.wgsl"),
            include_str!("drift_draw.wgsl"),
        ] {
            let module = naga::front::wgsl::parse_str(source).expect("Drift WGSL parses");
            naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::all(),
            )
            .validate(&module)
            .expect("Drift WGSL validates");
        }
        if let Ok(renderer) = still_renderer::Renderer::new() {
            let _scene = Drift::new(&renderer.device, &renderer.adapter, Quality::Balanced, 7);
            renderer.device.poll(wgpu::Maintain::Wait);
        }
    }
}
