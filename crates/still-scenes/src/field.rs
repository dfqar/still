use crate::targets::{ColorTarget, capped_size, output_format};
use bytemuck::{Pod, Zeroable};
use still_core::{FramePlan, PointerInput, Quality};
use still_renderer::wgpu::{self, util::DeviceExt};

const MAX_LINES: u32 = 360;
const POINTS_PER_LINE: u32 = 57;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FieldQualityBudget {
    pub megapixels: f32,
    pub lines: u32,
    pub steps: u32,
}
pub fn field_quality_budget(quality: Quality) -> FieldQualityBudget {
    match quality {
        Quality::Eco => FieldQualityBudget {
            megapixels: 0.75,
            lines: 128,
            steps: 24,
        },
        Quality::Auto | Quality::Balanced => FieldQualityBudget {
            megapixels: 1.4,
            lines: 240,
            steps: 40,
        },
        Quality::High => FieldQualityBudget {
            megapixels: 2.2,
            lines: MAX_LINES,
            steps: 56,
        },
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct FieldUniforms {
    time: f32,
    seed: f32,
    density: f32,
    motion: f32,
    bend: f32,
    light: f32,
    line_count: u32,
    steps: u32,
    pointer: [f32; 2],
    viewport: [f32; 2],
}

pub struct Field {
    uniform_buffer: wgpu::Buffer,
    _points_buffer: wgpu::Buffer,
    compute_group: wgpu::BindGroup,
    draw_group: wgpu::BindGroup,
    compute: wgpu::ComputePipeline,
    draw: wgpu::RenderPipeline,
    target: Option<ColorTarget>,
    format: wgpu::TextureFormat,
    quality: Quality,
    seed: u64,
    time: f32,
    pointer: [f32; 2],
    generation: u64,
}

impl Field {
    pub fn new(
        device: &wgpu::Device,
        adapter: &wgpu::Adapter,
        quality: Quality,
        seed: u64,
    ) -> Self {
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Field uniforms"),
            contents: bytemuck::bytes_of(&FieldUniforms::zeroed()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let points = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Field integrated streamline geometry"),
            size: u64::from(MAX_LINES * POINTS_PER_LINE * 16),
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let layout = |read_only, visibility| {
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Field geometry layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            })
        };
        let compute_layout = layout(false, wgpu::ShaderStages::COMPUTE);
        let draw_layout = layout(true, wgpu::ShaderStages::VERTEX_FRAGMENT);
        let group = |layout: &wgpu::BindGroupLayout| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Field geometry"),
                layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: uniform_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: points.as_entire_binding(),
                    },
                ],
            })
        };
        let compute_group = group(&compute_layout);
        let draw_group = group(&draw_layout);
        let compute_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Field streamline integration"),
            source: wgpu::ShaderSource::Wgsl(include_str!("field.wgsl").into()),
        });
        let draw_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Field line rendering"),
            source: wgpu::ShaderSource::Wgsl(include_str!("field_draw.wgsl").into()),
        });
        let compute_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Field compute layout"),
                bind_group_layouts: &[&compute_layout],
                push_constant_ranges: &[],
            });
        let compute = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Field integrate paths"),
            layout: Some(&compute_pipeline_layout),
            module: &compute_shader,
            entry_point: "integrate_lines",
            compilation_options: Default::default(),
        });
        let draw_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Field draw layout"),
            bind_group_layouts: &[&draw_layout],
            push_constant_ranges: &[],
        });
        let format = output_format(adapter);
        let additive = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        };
        let draw = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Field ribbons"),
            layout: Some(&draw_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &draw_shader,
                entry_point: "line_vertex",
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &draw_shader,
                entry_point: "line_fragment",
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState {
                        color: additive,
                        alpha: wgpu::BlendComponent::REPLACE,
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview: None,
        });
        Self {
            uniform_buffer,
            _points_buffer: points,
            compute_group,
            draw_group,
            compute,
            draw,
            target: None,
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
        let Some(size) = capped_size(viewport, field_quality_budget(self.quality).megapixels)
        else {
            self.release_gpu_resources();
            return;
        };
        if self.target.as_ref().is_some_and(|t| t.size == size) {
            return;
        }
        self.target = Some(ColorTarget::new(device, size, self.format, "Field output"));
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
        let target = self.target.as_ref()?;
        let dt = frame.display_dt.clamp(0.0, 0.1);
        self.time += dt * (0.15 + values[1] * 0.85);
        let target_pointer = if pointer.active {
            pointer.position
        } else {
            [0.0; 2]
        };
        for (current, target) in self.pointer.iter_mut().zip(target_pointer) {
            *current += (target - *current) * (1.0 - (-dt / 0.85).exp());
        }
        let budget = field_quality_budget(self.quality);
        let line_count = (budget.lines as f32 * (0.18 + values[0] * 0.82)).round() as u32;
        queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::bytes_of(&FieldUniforms {
                time: self.time,
                seed: crate::seed::seed_phase(self.seed),
                density: values[0],
                motion: values[1],
                bend: values[2],
                light: values[3],
                line_count,
                steps: budget.steps,
                pointer: self.pointer,
                viewport: target.size.map(|v| v as f32),
            }),
        );
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Field integrate streamlines"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.compute);
            pass.set_bind_group(0, &self.compute_group, &[]);
            pass.dispatch_workgroups(line_count.div_ceil(64), 1, 1);
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Field draw streamlines"),
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
            pass.set_pipeline(&self.draw);
            pass.set_bind_group(0, &self.draw_group, &[]);
            pass.draw(0..6, 0..line_count * budget.steps);
        }
        Some((self.generation, &target.view))
    }
    pub fn release_gpu_resources(&mut self) {
        if self.target.take().is_some() {
            self.generation = self.generation.wrapping_add(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn field_shaders_and_uniform_layout_validate() {
        for source in [include_str!("field.wgsl"), include_str!("field_draw.wgsl")] {
            let module = naga::front::wgsl::parse_str(source).expect("Field WGSL parses");
            naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::all(),
            )
            .validate(&module)
            .expect("Field WGSL validates");
        }
        assert_eq!(std::mem::size_of::<FieldUniforms>(), 48);
        assert_eq!(MAX_LINES * POINTS_PER_LINE * 16, 328_320);
    }
}
