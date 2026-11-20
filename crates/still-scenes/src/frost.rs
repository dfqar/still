use crate::scene::{Scene, SceneOutput, ScenePresetDescriptor};
use crate::targets::{ColorTarget, capped_size, output_format};
use bytemuck::{Pod, Zeroable};
use still_core::{
    AmbientControlFrame, AmbientFrame, AmbientLightControl, AmbientSmoothing, FramePlan,
    PointerInput, Quality, SceneDescriptor, SceneModulation, SceneParameterDescriptor,
    SceneParameters, SmoothedSceneParameters, srgb_to_linear,
};
use still_renderer::wgpu::{self, util::DeviceExt};

const fn parameter(
    id: &'static str,
    name: &'static str,
    min: f32,
    max: f32,
    default: f32,
) -> SceneParameterDescriptor {
    SceneParameterDescriptor {
        stable_id: id,
        display_name: name,
        minimum: min,
        maximum: max,
        default_value: default,
        smoothing_seconds: 0.65,
    }
}

pub const FROST_PARAMETERS: [SceneParameterDescriptor; 33] = [
    parameter("frost.brightness", "Brightness", 0.0, 2.5, 0.8),
    parameter("frost.diffusion", "Diffusion", 0.0, 1.0, 0.65),
    parameter("frost.frost", "Frost", 0.0, 1.0, 0.88),
    parameter("frost.grain", "Grain", 0.0, 1.0, 0.42),
    parameter("frost.ambient", "Ambient floor", 0.0, 2.0, 0.40),
    parameter("frost.motion", "Motion", 0.0, 1.0, 0.18),
    parameter("frost.light1.red", "Red", 0.0, 1.0, 0.16),
    parameter("frost.light1.green", "Green", 0.0, 1.0, 0.72),
    parameter("frost.light1.blue", "Blue", 0.0, 1.0, 1.0),
    parameter("frost.light1.power", "Intensity", 0.0, 3.0, 0.85),
    parameter("frost.light1.x", "Horizontal", 0.0, 1.0, 0.26),
    parameter("frost.light1.y", "Vertical", 0.0, 1.0, 0.58),
    parameter("frost.light1.width", "Width", 0.05, 1.2, 0.55),
    parameter("frost.light1.height", "Height", 0.05, 1.2, 0.42),
    parameter("frost.light1.rotation", "Rotation", 0.0, 360.0, 325.0),
    parameter("frost.light2.red", "Red", 0.0, 1.0, 1.0),
    parameter("frost.light2.green", "Green", 0.0, 1.0, 0.32),
    parameter("frost.light2.blue", "Blue", 0.0, 1.0, 0.12),
    parameter("frost.light2.power", "Intensity", 0.0, 3.0, 0.72),
    parameter("frost.light2.x", "Horizontal", 0.0, 1.0, 0.72),
    parameter("frost.light2.y", "Vertical", 0.0, 1.0, 0.40),
    parameter("frost.light2.width", "Width", 0.05, 1.2, 0.40),
    parameter("frost.light2.height", "Height", 0.05, 1.2, 0.60),
    parameter("frost.light2.rotation", "Rotation", 0.0, 360.0, 25.0),
    parameter("frost.light3.red", "Red", 0.0, 1.0, 0.58),
    parameter("frost.light3.green", "Green", 0.0, 1.0, 0.22),
    parameter("frost.light3.blue", "Blue", 0.0, 1.0, 1.0),
    parameter("frost.light3.power", "Intensity", 0.0, 3.0, 0.48),
    parameter("frost.light3.x", "Horizontal", 0.0, 1.0, 0.53),
    parameter("frost.light3.y", "Vertical", 0.0, 1.0, 0.82),
    parameter("frost.light3.width", "Width", 0.05, 1.2, 0.65),
    parameter("frost.light3.height", "Height", 0.05, 1.2, 0.34),
    parameter("frost.light3.rotation", "Rotation", 0.0, 360.0, 0.0),
];
pub const FROST_DESCRIPTOR: SceneDescriptor = SceneDescriptor {
    stable_id: "frost",
    display_name: "Frost",
    schema_version: 1,
    supports_modulation: true,
    parameters: &FROST_PARAMETERS,
};
pub const FROST_PRESET_DESCRIPTORS: [ScenePresetDescriptor; 4] = [
    ScenePresetDescriptor {
        stable_id: "builtin.opal",
        display_name: "Opal",
    },
    ScenePresetDescriptor {
        stable_id: "builtin.afterglow",
        display_name: "Afterglow",
    },
    ScenePresetDescriptor {
        stable_id: "builtin.blue-hour",
        display_name: "Blue Hour",
    },
    ScenePresetDescriptor {
        stable_id: "builtin.prism",
        display_name: "Prism",
    },
];

pub(crate) fn preset_content(id: &str) -> Option<(u64, SceneParameters)> {
    let index = FROST_PRESET_DESCRIPTORS
        .iter()
        .position(|p| p.stable_id == id)?;
    let mut values = SceneParameters::defaults(&FROST_DESCRIPTOR);
    let (material, colors, lights) = match index {
        0 => return Some((0xF205_7001, values)),
        1 => (
            [0.65, 0.86, 0.98, 0.62, 0.30, 0.08],
            [[1.0, 0.28, 0.07], [1.0, 0.63, 0.23], [0.70, 0.09, 0.28]],
            [
                [0.95, 0.30, 0.68, 0.65, 0.40, 335.0],
                [0.62, 0.72, 0.34, 0.32, 0.63, 32.0],
                [0.55, 0.82, 0.82, 0.58, 0.35, 0.0],
            ],
        ),
        2 => (
            [0.62, 0.76, 0.94, 0.38, 0.22, 0.12],
            [[0.12, 0.33, 1.0], [0.04, 0.85, 0.82], [0.54, 0.18, 0.93]],
            [
                [0.90, 0.26, 0.52, 0.44, 0.65, 30.0],
                [0.52, 0.73, 0.70, 0.57, 0.27, 340.0],
                [0.44, 0.70, 0.18, 0.55, 0.40, 0.0],
            ],
        ),
        _ => (
            [0.85, 0.32, 0.78, 0.52, 0.25, 0.26],
            [[1.0, 0.10, 0.35], [0.12, 0.93, 0.64], [0.36, 0.24, 1.0]],
            [
                [0.75, 0.27, 0.40, 0.25, 0.67, 35.0],
                [0.64, 0.52, 0.64, 0.28, 0.61, 35.0],
                [0.78, 0.78, 0.36, 0.24, 0.64, 35.0],
            ],
        ),
    };
    for (i, value) in material.into_iter().enumerate() {
        values.set_at(&FROST_DESCRIPTOR, i, value)?;
    }
    for i in 0..3 {
        for (j, value) in colors[i].into_iter().chain(lights[i]).enumerate() {
            values.set_at(&FROST_DESCRIPTOR, 6 + i * 9 + j, value)?;
        }
    }
    Some((0xF205_7001 + index as u64, values))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrostQualityBudget {
    pub megapixels: f32,
    pub samples: u32,
}
pub fn frost_quality_budget(quality: Quality) -> FrostQualityBudget {
    match quality {
        Quality::Eco => FrostQualityBudget {
            megapixels: 0.65,
            samples: 9,
        },
        Quality::Auto | Quality::Balanced => FrostQualityBudget {
            megapixels: 1.2,
            samples: 17,
        },
        Quality::High => FrostQualityBudget {
            megapixels: 2.0,
            samples: 29,
        },
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct LightUniform {
    color_power: [f32; 4],
    position_size: [f32; 4],
    rotation: [f32; 4],
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct FrostUniforms {
    lights: [LightUniform; 3],
    material: [f32; 4],
    animation: [f32; 4],
    viewport: [f32; 4],
}
struct FrostTargets {
    lights: ColorTarget,
    output: ColorTarget,
    group: wgpu::BindGroup,
}

pub struct FrostScene {
    parameters: SmoothedSceneParameters,
    seed: u64,
    selected_preset: Option<String>,
    quality: Quality,
    time: f64,
    ambient: Option<AmbientFrame>,
    controls: Option<AmbientControlFrame>,
    control_lights: [AmbientLightControl; 3],
    control_brightness: f32,
    smoothing: AmbientSmoothing,
    colors: [[f32; 3]; 3],
    palette_initialized: bool,
    energy: f32,
    modulation: Option<SceneModulation>,
    presented_reaction: [f32; 3],
    uniform: wgpu::Buffer,
    uniform_group: wgpu::BindGroup,
    texture_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    light_pipeline: wgpu::RenderPipeline,
    frost_pipeline: wgpu::RenderPipeline,
    format: wgpu::TextureFormat,
    targets: Option<FrostTargets>,
    generation: u64,
}
impl FrostScene {
    pub fn new(
        device: &wgpu::Device,
        adapter: &wgpu::Adapter,
        quality: Quality,
        seed: u64,
    ) -> Self {
        let parameters = SceneParameters::defaults(&FROST_DESCRIPTOR);
        let colors = manual_colors(&parameters);
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Frost light rig"),
            contents: bytemuck::bytes_of(&FrostUniforms::zeroed()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Frost uniforms"),
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
        let uniform_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Frost uniforms"),
            layout: &uniform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Frost irradiance"),
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
            label: Some("Frost diffusion sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Frost matte sheet"),
            source: wgpu::ShaderSource::Wgsl(include_str!("frost.wgsl").into()),
        });
        let format = output_format(adapter);
        let pipeline = |entry, layouts: &[&wgpu::BindGroupLayout]| {
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
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview: None,
            })
        };
        let light_pipeline = pipeline("light_fragment", &[&uniform_layout]);
        let frost_pipeline = pipeline("frost_fragment", &[&uniform_layout, &texture_layout]);
        Self {
            parameters: SmoothedSceneParameters::new(parameters),
            seed,
            selected_preset: Some(FROST_PRESET_DESCRIPTORS[0].stable_id.to_owned()),
            quality,
            time: 0.0,
            ambient: None,
            controls: None,
            control_lights: [AmbientLightControl::default(); 3],
            control_brightness: 1.0,
            smoothing: AmbientSmoothing::default(),
            colors,
            palette_initialized: false,
            energy: 0.0,
            modulation: None,
            presented_reaction: [0.0; 3],
            uniform,
            uniform_group,
            texture_layout,
            sampler,
            light_pipeline,
            frost_pipeline,
            format,
            targets: None,
            generation: 0,
        }
    }
}
fn manual_colors(parameters: &SceneParameters) -> [[f32; 3]; 3] {
    std::array::from_fn(|i| {
        std::array::from_fn(|j| parameters.get_at(6 + i * 9 + j).unwrap_or(0.0))
    })
}

fn light_drift(time: f64, seed: f32, light: usize, motion: f32) -> [f32; 2] {
    const PERIODS: [f64; 3] = [48.0, 61.0, 73.0];
    let period = PERIODS[light];
    let turn = (time % period) / period * std::f64::consts::TAU;
    let phase = f64::from(seed) * std::f64::consts::TAU + light as f64 * 2.13;
    let strength = motion / (0.45 + 0.55 * motion);

    // Each broad spot follows its own path. Subtract the initial phase so the
    // first paint and restored presets start at their supplied light centers.
    [
        ((turn + phase).sin() - phase.sin()) as f32 * 0.16 * strength,
        ((turn + phase * 1.7).cos() - (phase * 1.7).cos()) as f32 * 0.11 * strength,
    ]
}

impl Scene for FrostScene {
    fn descriptor(&self) -> &'static SceneDescriptor {
        &FROST_DESCRIPTOR
    }
    fn parameters(&self) -> &SceneParameters {
        self.parameters.current()
    }
    fn target_parameters(&self) -> &SceneParameters {
        self.parameters.target()
    }
    fn set_parameter(&mut self, id: &str, value: f32) -> Option<f32> {
        let value = self.parameters.set_target(&FROST_DESCRIPTOR, id, value)?;
        self.selected_preset = None;
        Some(value)
    }
    fn presets(&self) -> &'static [ScenePresetDescriptor] {
        &FROST_PRESET_DESCRIPTORS
    }
    fn selected_preset(&self) -> Option<&str> {
        self.selected_preset.as_deref()
    }
    fn select_builtin_preset(&mut self, id: &str) -> bool {
        let Some((seed, values)) = preset_content(id) else {
            return false;
        };
        self.restore_content(seed, values, Some(id.to_owned()));
        true
    }
    fn restore_content(&mut self, seed: u64, values: SceneParameters, preset: Option<String>) {
        // A preset may change the manual rig while a media palette is live.
        // Keep that palette continuous; clearing media later restores the new rig.
        if self.ambient.is_none() && self.controls.is_none() {
            self.colors = manual_colors(&values);
        }
        self.parameters.snap(values);
        self.seed = seed;
        self.time = 0.0;
        self.selected_preset = preset;
    }
    fn seed(&self) -> u64 {
        self.seed
    }
    fn set_seed(&mut self, seed: u64) {
        self.seed = seed;
        self.time = 0.0;
        self.selected_preset = None;
    }
    fn set_ambient_input(&mut self, input: Option<AmbientFrame>) {
        self.ambient = input;
        self.controls = None;
    }
    fn set_modulation(&mut self, input: Option<SceneModulation>) {
        self.modulation = input.and_then(SceneModulation::validated);
    }
    fn set_ambient_controls(&mut self, input: Option<AmbientControlFrame>) {
        if input.is_some_and(|v| v.validated().is_none()) {
            return;
        }
        self.controls = input.and_then(AmbientControlFrame::validated);
        self.ambient = None;
    }
    fn set_ambient_smoothing(&mut self, settings: AmbientSmoothing) {
        if let Some(settings) = settings.validated() {
            self.smoothing = settings;
        }
    }
    fn set_quality(&mut self, quality: Quality) {
        if self.quality != quality {
            self.quality = quality;
            self.release_gpu();
        }
    }
    fn prepare(&mut self, device: &wgpu::Device, viewport: [u32; 2]) {
        let Some(size) = capped_size(viewport, frost_quality_budget(self.quality).megapixels)
        else {
            self.release_gpu();
            return;
        };
        if self.targets.as_ref().is_some_and(|t| t.output.size == size) {
            return;
        }
        let lights = ColorTarget::new(device, size, self.format, "Frost light map");
        let output = ColorTarget::new(device, size, self.format, "Frost glass output");
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Frost irradiance"),
            layout: &self.texture_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&lights.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        self.targets = Some(FrostTargets {
            lights,
            output,
            group,
        });
        self.generation = self.generation.wrapping_add(1);
    }
    fn encode<'a>(
        &'a mut self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        frame: FramePlan,
        _pointer: PointerInput,
    ) -> Option<SceneOutput<'a>> {
        let targets = self.targets.as_ref()?;
        let dt = frame.display_dt.clamp(0.0, 0.1);
        self.parameters.advance(&FROST_DESCRIPTOR, dt);
        self.time += f64::from(dt);
        let p = self.parameters.current().as_slice();
        let manual = manual_colors(self.parameters.current());
        let desired = self.controls.map_or_else(
            || self.ambient.map_or(manual, |input| input.colors),
            |input| input.lights.map(|light| light.color),
        );
        // The first encoded frame starts from its supplied palette. There is
        // no previously presented color to interpolate from; a host media input
        // must not reveal the scene's unrelated manual preset for one frame.
        if !self.palette_initialized {
            self.colors = desired;
            self.palette_initialized = true;
        }
        for (current, target) in self
            .colors
            .iter_mut()
            .flatten()
            .zip(desired.iter().flatten())
        {
            *current = AmbientSmoothing::approach_frame(
                *current,
                *target,
                dt,
                self.smoothing.color_seconds,
                frame.animated,
            );
        }
        self.energy = self.smoothing.gain(
            self.energy,
            self.ambient.map_or(0.0, |input| input.energy),
            dt,
        );
        self.control_brightness = self.smoothing.gain(
            self.control_brightness,
            self.controls.map_or(1.0, |input| input.brightness),
            dt,
        );
        for (i, light) in self.control_lights.iter_mut().enumerate() {
            let target = self
                .controls
                .map_or_else(AmbientLightControl::default, |input| input.lights[i]);
            light.gain = self.smoothing.gain(light.gain, target.gain, dt);
            for j in 0..2 {
                light.offset[j] = AmbientSmoothing::approach(
                    light.offset[j],
                    target.offset[j],
                    dt,
                    self.smoothing.transform_seconds,
                );
                light.scale[j] = AmbientSmoothing::approach(
                    light.scale[j],
                    target.scale[j],
                    dt,
                    self.smoothing.transform_seconds,
                );
            }
            light.rotation_degrees = AmbientSmoothing::approach(
                light.rotation_degrees,
                target.rotation_degrees,
                dt,
                self.smoothing.transform_seconds,
            );
        }
        // The overlay modulates the rendered rig independently of the palette,
        // manual parameters and saved presets.
        self.presented_reaction = presented_reaction(
            self.modulation,
            self.presented_reaction,
            frame,
            self.smoothing.release_seconds,
        );
        let [motion, turbulence, light] = self.presented_reaction;
        let gains = [
            1.0 + motion * 0.18,
            1.0 + turbulence * 0.14,
            1.0 + light * 0.22,
        ];
        let seed = (self.seed % 65_521) as f32 / 65_521.0;
        let lights = std::array::from_fn(|i| {
            let o = 6 + i * 9;
            let [dx, dy] = light_drift(self.time, seed, i, p[5]);
            LightUniform {
                color_power: [
                    srgb_to_linear(self.colors[i][0]),
                    srgb_to_linear(self.colors[i][1]),
                    srgb_to_linear(self.colors[i][2]),
                    p[o + 3] * self.control_lights[i].gain * gains[i],
                ],
                position_size: [
                    p[o + 4] + self.control_lights[i].offset[0] + dx,
                    p[o + 5] + self.control_lights[i].offset[1] + dy,
                    p[o + 6] * self.control_lights[i].scale[0],
                    p[o + 7] * self.control_lights[i].scale[1],
                ],
                rotation: [
                    (p[o + 8] + self.control_lights[i].rotation_degrees).to_radians(),
                    0.0,
                    0.0,
                    0.0,
                ],
            }
        });
        let uniforms = FrostUniforms {
            lights,
            material: [p[1], p[2], p[3], p[4]],
            animation: [
                self.time as f32,
                p[5],
                seed,
                (self.energy + light * 0.3).min(1.0),
            ],
            viewport: [
                targets.output.size[0] as f32,
                targets.output.size[1] as f32,
                frost_quality_budget(self.quality).samples as f32,
                p[0] * self.control_brightness,
            ],
        };
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&uniforms));
        for (view, pipeline, group) in [
            (&targets.lights.view, &self.light_pipeline, None),
            (
                &targets.output.view,
                &self.frost_pipeline,
                Some(&targets.group),
            ),
        ] {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Frost light diffusion"),
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
            pass.set_bind_group(0, &self.uniform_group, &[]);
            if let Some(group) = group {
                pass.set_bind_group(1, group, &[]);
            }
            pass.draw(0..3, 0..1);
        }
        Some(SceneOutput {
            source_key: 0xF205_0000_0000_0000 | self.generation,
            view: &targets.output.view,
            exposure: 0.0,
        })
    }
    fn release_gpu(&mut self) {
        if self.targets.take().is_some() {
            self.generation = self.generation.wrapping_add(1);
        }
    }
}

fn presented_reaction(
    input: Option<SceneModulation>,
    previous: [f32; 3],
    frame: FramePlan,
    release_seconds: f32,
) -> [f32; 3] {
    if let Some(input) = input {
        [input.motion, input.turbulence, input.light]
    } else if frame.animated && frame.display_dt > 0.0 {
        // Clear the input immediately, while releasing only the displayed light.
        previous.map(|value| {
            AmbientSmoothing::approach(value, 0.0, frame.display_dt.min(0.1), release_seconds)
        })
    } else {
        // An inactive paint or the first frame after a clock reset has no tail.
        [0.0; 3]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frost_light_centers_drift_independently_and_preserve_fixed_rigs() {
        let default_motion = FROST_PARAMETERS[5].default_value;
        for seed in [0.0, 0.43, 0.99] {
            let end: [[f32; 2]; 3] =
                std::array::from_fn(|i| light_drift(14.0, seed, i, default_motion));
            for (i, position) in end.iter().enumerate() {
                assert_eq!(light_drift(0.0, seed, i, default_motion), [0.0; 2]);
                assert_eq!(light_drift(14.0, seed, i, 0.0), [0.0; 2]);
                assert!(position[0].abs() < 0.11 && position[1].abs() < 0.08);
                let next = light_drift(14.0 + 1.0 / 60.0, seed, i, default_motion);
                assert!((next[0] - position[0]).abs() < 0.0002);
                assert!((next[1] - position[1]).abs() < 0.0002);
            }
            assert!(end.iter().any(|[x, y]| x.abs().max(y.abs()) > 0.04));
            assert!(end.windows(2).all(|lights| lights[0] != lights[1]));
        }
    }

    #[test]
    fn frost_releases_visible_light_after_clear_without_replaying_it_on_resume() {
        let frame = FramePlan {
            animated: true,
            display_dt: 1.0 / 60.0,
            ..Default::default()
        };
        let input = SceneModulation {
            motion: 0.8,
            turbulence: 0.4,
            light: 0.9,
            ..Default::default()
        };
        let before = presented_reaction(Some(input), [0.0; 3], frame, 0.45);
        let after = presented_reaction(None, before, frame, 0.45);
        for (old, new) in before.into_iter().zip(after) {
            assert!(new > 0.9 * old && new < old);
        }
        let settled = (0..120).fold(after, |light, _| {
            presented_reaction(None, light, frame, 0.45)
        });
        assert!(settled.into_iter().all(|gain| gain < 0.02));
        let first = FramePlan {
            display_dt: 0.0,
            ..frame
        };
        assert_eq!(presented_reaction(None, before, first, 0.45), [0.0; 3]);
        let inactive = FramePlan {
            animated: false,
            ..frame
        };
        assert_eq!(presented_reaction(None, before, inactive, 0.45), [0.0; 3]);
    }
    #[test]
    fn frosted_material_shader_and_uniforms_validate() {
        let module =
            naga::front::wgsl::parse_str(include_str!("frost.wgsl")).expect("Frost WGSL parses");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .expect("Frost WGSL validates");
        assert_eq!(std::mem::size_of::<FrostUniforms>(), 192);
    }
    #[test]
    fn presets_keep_full_independent_light_rigs() {
        for preset in FROST_PRESET_DESCRIPTORS {
            let (_, values) = preset_content(preset.stable_id).unwrap();
            assert_eq!(values.as_slice().len(), 33);
            for (descriptor, value) in FROST_PARAMETERS.iter().zip(values.as_slice()) {
                assert!(
                    value.is_finite() && (descriptor.minimum..=descriptor.maximum).contains(value)
                );
            }
        }
        assert!(preset_content("builtin.liquid-fold").is_none());
    }

    #[test]
    fn first_animated_paint_seeds_media_then_resumed_animation_smooths_without_preset_flash() {
        let renderer = match still_renderer::Renderer::new() {
            Ok(renderer) => renderer,
            Err(error) => {
                assert!(
                    std::env::var_os("STILL_REQUIRE_METAL_TESTS").is_none(),
                    "Metal required: {error}"
                );
                return;
            }
        };
        let mut scene = FrostScene::new(&renderer.device, &renderer.adapter, Quality::Eco, 7);
        scene.prepare(&renderer.device, [32, 32]);
        let saved = scene.target_parameters().as_slice().to_vec();
        let gray = AmbientFrame::validated([[0.2; 3], [0.5; 3], [0.8; 3]], 0.0).unwrap();
        scene.set_ambient_input(Some(gray));
        let mut encoder = renderer
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        assert!(
            scene
                .encode(
                    &renderer.queue,
                    &mut encoder,
                    FramePlan {
                        animated: true,
                        ..FramePlan::default()
                    },
                    PointerInput::default()
                )
                .is_some()
        );
        assert_eq!(
            scene.colors, gray.colors,
            "First active paint exposed the unrelated manual preset palette"
        );
        let red = AmbientFrame::validated([[0.9, 0.1, 0.1]; 3], 0.0).unwrap();
        scene.set_ambient_input(Some(red));
        assert!(
            scene
                .encode(
                    &renderer.queue,
                    &mut encoder,
                    FramePlan {
                        animated: true,
                        ..FramePlan::default()
                    },
                    PointerInput::default()
                )
                .is_some()
        );
        assert_eq!(
            scene.colors, gray.colors,
            "Opening an animated surface snapped its palette"
        );
        assert!(
            scene
                .encode(
                    &renderer.queue,
                    &mut encoder,
                    FramePlan {
                        display_dt: 1.0 / 60.0,
                        ..FramePlan::default()
                    },
                    PointerInput::default()
                )
                .is_some()
        );
        assert_ne!(
            scene.colors, red.colors,
            "Active playback must keep gentle color smoothing"
        );
        assert!(
            scene
                .encode(
                    &renderer.queue,
                    &mut encoder,
                    FramePlan::default(),
                    PointerInput::default()
                )
                .is_some()
        );
        assert_eq!(
            scene.colors, red.colors,
            "Resumed still paint did not adopt the new palette"
        );
        assert_eq!(scene.target_parameters().as_slice(), saved.as_slice());
        renderer.queue.submit([encoder.finish()]);
    }

    #[test]
    fn changing_manual_presets_does_not_flash_the_live_media_palette() {
        let renderer = match still_renderer::Renderer::new() {
            Ok(renderer) => renderer,
            Err(error) => {
                assert!(
                    std::env::var_os("STILL_REQUIRE_METAL_TESTS").is_none(),
                    "Metal required: {error}"
                );
                return;
            }
        };
        let mut scene = FrostScene::new(&renderer.device, &renderer.adapter, Quality::Eco, 7);
        let media = AmbientFrame::validated([[0.05, 0.1, 1.0]; 3], 0.5).unwrap();
        scene.set_ambient_input(Some(media));
        scene.colors = media.colors;
        assert!(scene.select_builtin_preset("builtin.afterglow"));
        assert_eq!(scene.colors, media.colors);
        assert_ne!(manual_colors(scene.target_parameters()), media.colors);
        scene.set_ambient_input(None);
        assert_eq!(scene.colors, media.colors); // release also starts without a jump
        assert_eq!(scene.ambient, None);
    }
}
