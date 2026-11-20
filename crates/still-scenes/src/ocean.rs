use bytemuck::{Pod, Zeroable};
use still_core::{FramePlan, PointerInput, Quality};
use still_renderer::wgpu::{self, util::DeviceExt};

pub const OCEAN_SCENE_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OceanParameters {
    pub calm: f32,
    pub scale: f32,
    pub flow: f32,
    pub light: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct OceanPreset {
    pub id: &'static str,
    pub name: &'static str,
    pub seed: u64,
    pub parameters: OceanParameters,
}

pub const OCEAN_PRESETS: [OceanPreset; 4] = [
    OceanPreset {
        id: "builtin.black-tide",
        name: "Black Tide",
        seed: 0x0C_EA_00_01,
        parameters: OceanParameters {
            calm: 0.72,
            scale: 0.58,
            flow: 0.38,
            light: 0.42,
        },
    },
    OceanPreset {
        id: "builtin.distant-swell",
        name: "Distant Swell",
        seed: 0x0C_EA_00_02,
        parameters: OceanParameters {
            calm: 0.48,
            scale: 0.82,
            flow: 0.32,
            light: 0.56,
        },
    },
    OceanPreset {
        id: "builtin.moon-channel",
        name: "Moon Channel",
        seed: 0x0C_EA_00_03,
        parameters: OceanParameters {
            calm: 0.62,
            scale: 0.44,
            flow: 0.58,
            light: 0.82,
        },
    },
    OceanPreset {
        id: "builtin.deep-weather",
        name: "Deep Weather",
        seed: 0x0C_EA_00_04,
        parameters: OceanParameters {
            calm: 0.22,
            scale: 0.72,
            flow: 0.74,
            light: 0.48,
        },
    },
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OceanQualityBudget {
    pub megapixels: f32,
    pub mesh_segments: u32,
    pub wave_count: u32,
}

pub fn ocean_quality_budget(quality: Quality) -> OceanQualityBudget {
    match quality {
        Quality::Eco => OceanQualityBudget {
            megapixels: 0.55,
            mesh_segments: 72,
            wave_count: 3,
        },
        Quality::Auto | Quality::Balanced => OceanQualityBudget {
            megapixels: 0.95,
            mesh_segments: 128,
            wave_count: 5,
        },
        Quality::High => OceanQualityBudget {
            megapixels: 1.55,
            mesh_segments: 192,
            wave_count: 6,
        },
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct OceanUniforms {
    time: f32,
    seed: f32,
    calm: f32,
    scale: f32,
    flow: f32,
    light: f32,
    grid_size: f32,
    wave_count: f32,
    pointer: [f32; 2],
    viewport: [f32; 2],
    _padding: [f32; 4],
}

struct OceanTarget {
    _color: wgpu::Texture,
    color_view: wgpu::TextureView,
    _depth: wgpu::Texture,
    depth_view: wgpu::TextureView,
    size: [u32; 2],
}

struct OceanMesh {
    index_buffer: wgpu::Buffer,
    index_count: u32,
    grid_size: u32,
}

pub struct Ocean {
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    sky_pipeline: wgpu::RenderPipeline,
    target: Option<OceanTarget>,
    mesh: Option<OceanMesh>,
    output_generation: u64,
    output_format: wgpu::TextureFormat,
    quality: Quality,
    seed: u64,
    time: f32,
    pointer: [f32; 2],
}

impl Ocean {
    pub fn new(
        device: &wgpu::Device,
        adapter: &wgpu::Adapter,
        quality: Quality,
        seed: u64,
    ) -> Self {
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Ocean uniforms"),
            contents: bytemuck::bytes_of(&OceanUniforms::zeroed()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Ocean bind group layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Ocean bind group"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Ocean shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("ocean.wgsl").into()),
        });
        let output_format = preferred_output_format(adapter);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Ocean pipeline layout"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let create_pipeline = |label, vertex_entry, fragment_entry, depth_write| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: vertex_entry,
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: fragment_entry,
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: output_format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: depth_write,
                    depth_compare: wgpu::CompareFunction::LessEqual,
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                multiview: None,
            })
        };
        let pipeline = create_pipeline(
            "Ocean surface pipeline",
            "ocean_vertex",
            "ocean_fragment",
            true,
        );
        let sky_pipeline = create_pipeline(
            "Ocean sky pipeline",
            "ocean_sky_vertex",
            "ocean_sky_fragment",
            false,
        );
        Self {
            uniform_buffer,
            bind_group,
            pipeline,
            sky_pipeline,
            target: None,
            mesh: None,
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
        let budget = ocean_quality_budget(self.quality);
        if self
            .mesh
            .as_ref()
            .is_none_or(|mesh| mesh.grid_size != budget.mesh_segments + 1)
        {
            self.mesh = Some(create_mesh(device, budget.mesh_segments));
        }
        let pixels = f64::from(viewport[0]) * f64::from(viewport[1]);
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
        self.target = Some(create_target(device, size, self.output_format));
        self.output_generation = self.output_generation.wrapping_add(1);
    }

    pub fn encode(
        &mut self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        frame: FramePlan,
        parameters: OceanParameters,
        pointer: PointerInput,
    ) -> Option<(u64, &wgpu::TextureView)> {
        let target = self.target.as_ref()?;
        let mesh = self.mesh.as_ref()?;
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
        let budget = ocean_quality_budget(self.quality);
        let uniforms = OceanUniforms {
            time: self.time,
            seed: crate::seed::seed_phase(self.seed),
            calm: parameters.calm,
            scale: parameters.scale,
            flow: parameters.flow,
            light: parameters.light,
            grid_size: mesh.grid_size as f32,
            wave_count: budget.wave_count as f32,
            pointer: self.pointer,
            viewport: [target.size[0] as f32, target.size[1] as f32],
            _padding: [0.0; 4],
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Ocean render"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target.color_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.000_02,
                        g: 0.000_035,
                        b: 0.000_055,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &target.depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&self.sky_pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.draw(0..3, 0..1);
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..mesh.index_count, 0, 0..1);
        drop(pass);
        Some((self.output_generation, &target.color_view))
    }

    pub fn release_gpu_resources(&mut self) {
        self.target = None;
        self.mesh = None;
        self.output_generation = self.output_generation.wrapping_add(1);
    }
}

fn create_mesh(device: &wgpu::Device, segments: u32) -> OceanMesh {
    let grid_size = segments + 1;
    let mut indices = Vec::with_capacity((segments * segments * 6) as usize);
    for y in 0..segments {
        for x in 0..segments {
            let a = y * grid_size + x;
            let b = a + 1;
            let c = a + grid_size;
            let d = c + 1;
            indices.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }
    let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Ocean grid indices"),
        contents: bytemuck::cast_slice(&indices),
        usage: wgpu::BufferUsages::INDEX,
    });
    OceanMesh {
        index_buffer: buffer,
        index_count: indices.len() as u32,
        grid_size,
    }
}

fn create_target(
    device: &wgpu::Device,
    size: [u32; 2],
    format: wgpu::TextureFormat,
) -> OceanTarget {
    let extent = wgpu::Extent3d {
        width: size[0],
        height: size[1],
        depth_or_array_layers: 1,
    };
    let color = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Ocean output"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let depth = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Ocean depth"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let color_view = color.create_view(&Default::default());
    let depth_view = depth.create_view(&Default::default());
    OceanTarget {
        _color: color,
        color_view,
        _depth: depth,
        depth_view,
        size,
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
        let eco = ocean_quality_budget(Quality::Eco);
        let balanced = ocean_quality_budget(Quality::Balanced);
        let high = ocean_quality_budget(Quality::High);
        assert!(eco.megapixels < balanced.megapixels && balanced.megapixels < high.megapixels);
        assert!(
            eco.mesh_segments < balanced.mesh_segments
                && balanced.mesh_segments < high.mesh_segments
        );
        assert!(eco.wave_count < balanced.wave_count && balanced.wave_count < high.wave_count);
    }

    #[test]
    fn ocean_shader_validates() {
        let module =
            naga::front::wgsl::parse_str(include_str!("ocean.wgsl")).expect("Ocean WGSL parses");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .expect("Ocean WGSL validates");
        assert_eq!(std::mem::size_of::<OceanUniforms>(), 64);
        if let Ok(renderer) = still_renderer::Renderer::new() {
            let _scene = Ocean::new(&renderer.device, &renderer.adapter, Quality::Balanced, 7);
            renderer.device.poll(wgpu::Maintain::Wait);
        }
    }
}
