use crate::targets::{ColorTarget, capped_size, output_format};
use bytemuck::{Pod, Zeroable};
use still_core::{FramePlan, PointerInput, Quality};
use still_renderer::wgpu::{self, util::DeviceExt};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlassQualityBudget {
    pub megapixels: f32,
    pub ray_steps: u32,
}

pub fn glass_quality_budget(quality: Quality) -> GlassQualityBudget {
    match quality {
        Quality::Eco => GlassQualityBudget {
            megapixels: 0.55,
            ray_steps: 28,
        },
        Quality::Auto | Quality::Balanced => GlassQualityBudget {
            megapixels: 0.95,
            ray_steps: 44,
        },
        Quality::High => GlassQualityBudget {
            megapixels: 1.5,
            ray_steps: 64,
        },
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GlassUniforms {
    time: f32,
    seed: f32,
    form: f32,
    motion: f32,
    refraction: f32,
    light: f32,
    steps: u32,
    padding: u32,
    pointer: [f32; 2],
    viewport: [f32; 2],
}

struct GlassTargets {
    background: ColorTarget,
    output: ColorTarget,
    background_group: wgpu::BindGroup,
}

pub struct Glass {
    uniform_buffer: wgpu::Buffer,
    uniforms_group: wgpu::BindGroup,
    background_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    background_pipeline: wgpu::RenderPipeline,
    glass_pipeline: wgpu::RenderPipeline,
    targets: Option<GlassTargets>,
    format: wgpu::TextureFormat,
    quality: Quality,
    seed: u64,
    time: f32,
    pointer: [f32; 2],
    generation: u64,
}

impl Glass {
    pub fn new(
        device: &wgpu::Device,
        adapter: &wgpu::Adapter,
        quality: Quality,
        seed: u64,
    ) -> Self {
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Glass uniforms"),
            contents: bytemuck::bytes_of(&GlassUniforms::zeroed()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Glass uniform layout"),
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
        let uniforms_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Glass uniforms"),
            layout: &uniform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });
        let background_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Glass optical backdrop layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Glass backdrop sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Glass optical shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("glass.wgsl").into()),
        });
        let format = output_format(adapter);
        let make_pipeline = |entry, layouts: &[&wgpu::BindGroupLayout]| {
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(entry),
                bind_group_layouts: layouts,
                push_constant_ranges: &[],
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: "fullscreen_vertex",
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: entry,
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview: None,
            })
        };
        let background_pipeline = make_pipeline("background_fragment", &[&uniform_layout]);
        let glass_pipeline =
            make_pipeline("glass_fragment", &[&uniform_layout, &background_layout]);
        Self {
            uniform_buffer,
            uniforms_group,
            background_layout,
            sampler,
            background_pipeline,
            glass_pipeline,
            targets: None,
            format,
            quality,
            seed,
            time: 0.0,
            pointer: [0.0; 2],
            generation: 0,
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
        let Some(size) = capped_size(viewport, glass_quality_budget(self.quality).megapixels)
        else {
            self.release_gpu_resources();
            return;
        };
        if self.targets.as_ref().is_some_and(|t| t.output.size == size) {
            return;
        }
        let background = ColorTarget::new(device, size, self.format, "Glass backdrop");
        let output = ColorTarget::new(device, size, self.format, "Glass output");
        let background_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Glass optical backdrop"),
            layout: &self.background_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&background.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        self.targets = Some(GlassTargets {
            background,
            output,
            background_group,
        });
        self.generation = self.generation.wrapping_add(1);
    }
    pub fn encode(
        &mut self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        frame: FramePlan,
        values: [f32; 4],
        pointer: PointerInput,
    ) -> Option<(u64, &wgpu::TextureView)> {
        let targets = self.targets.as_ref()?;
        let dt = frame.display_dt.clamp(0.0, 0.1);
        self.time += dt * (0.2 + values[1] * 0.8);
        let target_pointer = if pointer.active {
            pointer.position
        } else {
            [0.0; 2]
        };
        for (current, target) in self.pointer.iter_mut().zip(target_pointer) {
            *current += (target - *current) * (1.0 - (-dt / 0.85).exp());
        }
        queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::bytes_of(&GlassUniforms {
                time: self.time,
                seed: crate::seed::seed_phase(self.seed),
                form: values[0],
                motion: values[1],
                refraction: values[2],
                light: values[3],
                steps: glass_quality_budget(self.quality).ray_steps,
                padding: 0,
                pointer: self.pointer,
                viewport: targets.output.size.map(|v| v as f32),
            }),
        );
        for (view, pipeline, background) in [
            (&targets.background.view, &self.background_pipeline, None),
            (
                &targets.output.view,
                &self.glass_pipeline,
                Some(&targets.background_group),
            ),
        ] {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Glass optical pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
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
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &self.uniforms_group, &[]);
            if let Some(group) = background {
                pass.set_bind_group(1, group, &[]);
            }
            pass.draw(0..3, 0..1);
        }
        Some((self.generation, &targets.output.view))
    }
    pub fn release_gpu_resources(&mut self) {
        if self.targets.take().is_some() {
            self.generation = self.generation.wrapping_add(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn glass_shader_and_uniform_layout_validate() {
        let module =
            naga::front::wgsl::parse_str(include_str!("glass.wgsl")).expect("Glass WGSL parses");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .expect("Glass WGSL validates");
        assert_eq!(std::mem::size_of::<GlassUniforms>(), 48);
    }
}
