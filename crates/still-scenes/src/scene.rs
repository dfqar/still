use still_core::{
    FramePlan, PointerInput, Quality, SceneDescriptor, SceneParameterDescriptor, SceneParameters,
    SmoothedSceneParameters,
};
use still_renderer::wgpu;

use crate::{
    BUILTIN_PRESETS, DRIFT_SCENE_SCHEMA_VERSION, Drift, FOG_PRESETS, FOG_SCENE_SCHEMA_VERSION, Fog,
    FogParameters, OCEAN_PRESETS, OCEAN_SCENE_SCHEMA_VERSION, Ocean, OceanParameters,
};

pub const DRIFT_PARAMETERS: [SceneParameterDescriptor; 4] = [
    SceneParameterDescriptor {
        stable_id: "drift.density",
        display_name: "Density",
        minimum: 0.25,
        maximum: 1.0,
        default_value: 0.66,
        smoothing_seconds: 0.28,
    },
    SceneParameterDescriptor {
        stable_id: "drift.speed",
        display_name: "Speed",
        minimum: 0.2,
        maximum: 1.8,
        default_value: 0.82,
        smoothing_seconds: 0.42,
    },
    SceneParameterDescriptor {
        stable_id: "drift.flow",
        display_name: "Flow",
        minimum: 0.15,
        maximum: 1.5,
        default_value: 0.72,
        smoothing_seconds: 0.48,
    },
    SceneParameterDescriptor {
        stable_id: "drift.light",
        display_name: "Light",
        minimum: 0.15,
        maximum: 1.0,
        default_value: 0.58,
        smoothing_seconds: 0.32,
    },
];

pub const DRIFT_DESCRIPTOR: SceneDescriptor = SceneDescriptor {
    stable_id: "drift",
    display_name: "Drift",
    schema_version: DRIFT_SCENE_SCHEMA_VERSION,
    supports_modulation: true,
    parameters: &DRIFT_PARAMETERS,
};

pub const FOG_PARAMETERS: [SceneParameterDescriptor; 4] = [
    SceneParameterDescriptor {
        stable_id: "fog.density",
        display_name: "Density",
        minimum: 0.15,
        maximum: 1.0,
        default_value: 0.48,
        smoothing_seconds: 0.5,
    },
    SceneParameterDescriptor {
        stable_id: "fog.drift",
        display_name: "Drift",
        minimum: 0.05,
        maximum: 1.0,
        default_value: 0.46,
        smoothing_seconds: 0.65,
    },
    SceneParameterDescriptor {
        stable_id: "fog.depth",
        display_name: "Depth",
        minimum: 0.2,
        maximum: 1.0,
        default_value: 0.72,
        smoothing_seconds: 0.58,
    },
    SceneParameterDescriptor {
        stable_id: "fog.light",
        display_name: "Light",
        minimum: 0.15,
        maximum: 1.0,
        default_value: 0.62,
        smoothing_seconds: 0.44,
    },
];

pub const FOG_DESCRIPTOR: SceneDescriptor = SceneDescriptor {
    stable_id: "fog",
    display_name: "Fog",
    schema_version: FOG_SCENE_SCHEMA_VERSION,
    supports_modulation: false,
    parameters: &FOG_PARAMETERS,
};

pub const OCEAN_PARAMETERS: [SceneParameterDescriptor; 4] = [
    SceneParameterDescriptor {
        stable_id: "ocean.calm",
        display_name: "Calm",
        minimum: 0.0,
        maximum: 1.0,
        default_value: 0.72,
        smoothing_seconds: 0.62,
    },
    SceneParameterDescriptor {
        stable_id: "ocean.scale",
        display_name: "Scale",
        minimum: 0.15,
        maximum: 1.0,
        default_value: 0.58,
        smoothing_seconds: 0.72,
    },
    SceneParameterDescriptor {
        stable_id: "ocean.flow",
        display_name: "Flow",
        minimum: 0.05,
        maximum: 1.0,
        default_value: 0.38,
        smoothing_seconds: 0.68,
    },
    SceneParameterDescriptor {
        stable_id: "ocean.light",
        display_name: "Light",
        minimum: 0.15,
        maximum: 1.0,
        default_value: 0.42,
        smoothing_seconds: 0.46,
    },
];

pub const OCEAN_DESCRIPTOR: SceneDescriptor = SceneDescriptor {
    stable_id: "ocean",
    display_name: "Ocean",
    schema_version: OCEAN_SCENE_SCHEMA_VERSION,
    supports_modulation: false,
    parameters: &OCEAN_PARAMETERS,
};

pub const SCENE_DESCRIPTORS: [SceneDescriptor; 6] = [
    DRIFT_DESCRIPTOR,
    FOG_DESCRIPTOR,
    OCEAN_DESCRIPTOR,
    crate::GLASS_DESCRIPTOR,
    crate::FIELD_DESCRIPTOR,
    crate::FROST_DESCRIPTOR,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScenePresetDescriptor {
    pub stable_id: &'static str,
    pub display_name: &'static str,
}

pub const DRIFT_PRESET_DESCRIPTORS: [ScenePresetDescriptor; 4] = [
    ScenePresetDescriptor {
        stable_id: "builtin.still-water",
        display_name: "Still Water",
    },
    ScenePresetDescriptor {
        stable_id: "builtin.crosswind",
        display_name: "Crosswind",
    },
    ScenePresetDescriptor {
        stable_id: "builtin.night-current",
        display_name: "Night Current",
    },
    ScenePresetDescriptor {
        stable_id: "builtin.quiet-vortex",
        display_name: "Quiet Vortex",
    },
];

pub const FOG_PRESET_DESCRIPTORS: [ScenePresetDescriptor; 4] = [
    ScenePresetDescriptor {
        stable_id: "builtin.morning-haze",
        display_name: "Morning Haze",
    },
    ScenePresetDescriptor {
        stable_id: "builtin.dense-silence",
        display_name: "Dense Silence",
    },
    ScenePresetDescriptor {
        stable_id: "builtin.cold-layer",
        display_name: "Cold Layer",
    },
    ScenePresetDescriptor {
        stable_id: "builtin.after-rain",
        display_name: "After Rain",
    },
];

pub const OCEAN_PRESET_DESCRIPTORS: [ScenePresetDescriptor; 4] = [
    ScenePresetDescriptor {
        stable_id: "builtin.black-tide",
        display_name: "Black Tide",
    },
    ScenePresetDescriptor {
        stable_id: "builtin.distant-swell",
        display_name: "Distant Swell",
    },
    ScenePresetDescriptor {
        stable_id: "builtin.moon-channel",
        display_name: "Moon Channel",
    },
    ScenePresetDescriptor {
        stable_id: "builtin.deep-weather",
        display_name: "Deep Weather",
    },
];

pub struct SceneOutput<'a> {
    pub source_key: u64,
    pub view: &'a wgpu::TextureView,
    pub exposure: f32,
}

/// GPU-facing scene lifecycle. Scene implementations own all pipelines,
/// buffers, textures, simulation state, parameters, and preset definitions.
/// The runtime only schedules them and composites their linear output.
pub trait Scene {
    fn descriptor(&self) -> &'static SceneDescriptor;
    fn parameters(&self) -> &SceneParameters;
    fn target_parameters(&self) -> &SceneParameters;
    fn set_parameter(&mut self, stable_id: &str, value: f32) -> Option<f32>;
    fn presets(&self) -> &'static [ScenePresetDescriptor];
    fn selected_preset(&self) -> Option<&str>;
    fn select_builtin_preset(&mut self, stable_id: &str) -> bool;
    fn restore_content(
        &mut self,
        seed: u64,
        parameters: SceneParameters,
        selected_preset: Option<String>,
    );
    fn seed(&self) -> u64;
    fn set_seed(&mut self, seed: u64);
    fn set_quality(&mut self, quality: Quality);
    /// Optional transient media palette/envelope. Saved manual content is untouched.
    fn set_ambient_input(&mut self, _input: Option<still_core::AmbientFrame>) {}
    fn set_ambient_controls(&mut self, _input: Option<still_core::AmbientControlFrame>) {}
    fn set_ambient_smoothing(&mut self, _settings: still_core::AmbientSmoothing) {}
    fn set_modulation(&mut self, _input: Option<still_core::SceneModulation>) {}
    fn prepare(&mut self, device: &wgpu::Device, viewport: [u32; 2]);
    fn encode<'a>(
        &'a mut self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        frame: FramePlan,
        pointer: PointerInput,
    ) -> Option<SceneOutput<'a>>;
    fn suspend(&mut self) {}
    fn resume(&mut self) {}
    fn release_gpu(&mut self);
}

pub struct DriftScene {
    renderer: Drift,
    parameters: SmoothedSceneParameters,
    seed: u64,
    selected_preset: Option<String>,
    modulation: Option<still_core::SceneModulation>,
}

impl DriftScene {
    pub fn new(
        device: &wgpu::Device,
        adapter: &wgpu::Adapter,
        quality: Quality,
        seed: u64,
    ) -> Self {
        Self {
            renderer: Drift::new(device, adapter, quality, seed),
            parameters: SmoothedSceneParameters::new(SceneParameters::defaults(&DRIFT_DESCRIPTOR)),
            seed,
            selected_preset: Some("builtin.still-water".to_owned()),
            modulation: None,
        }
    }

    pub fn restore_parameters(&mut self, values: SceneParameters) {
        self.parameters.snap(values);
        self.selected_preset = None;
    }
}

impl Scene for DriftScene {
    fn set_modulation(&mut self, input: Option<still_core::SceneModulation>) {
        self.modulation = input;
    }
    fn descriptor(&self) -> &'static SceneDescriptor {
        &DRIFT_DESCRIPTOR
    }

    fn parameters(&self) -> &SceneParameters {
        self.parameters.current()
    }

    fn target_parameters(&self) -> &SceneParameters {
        self.parameters.target()
    }

    fn set_parameter(&mut self, stable_id: &str, value: f32) -> Option<f32> {
        let value = self
            .parameters
            .set_target(&DRIFT_DESCRIPTOR, stable_id, value)?;
        self.selected_preset = None;
        Some(value)
    }

    fn presets(&self) -> &'static [ScenePresetDescriptor] {
        &DRIFT_PRESET_DESCRIPTORS
    }

    fn selected_preset(&self) -> Option<&str> {
        self.selected_preset.as_deref()
    }

    fn select_builtin_preset(&mut self, stable_id: &str) -> bool {
        let Some(preset) = BUILTIN_PRESETS.iter().find(|preset| preset.id == stable_id) else {
            return false;
        };
        let mut values = SceneParameters::defaults(&DRIFT_DESCRIPTOR);
        let _ = values.set(
            &DRIFT_DESCRIPTOR,
            "drift.density",
            preset.parameters.density,
        );
        let _ = values.set(&DRIFT_DESCRIPTOR, "drift.speed", preset.parameters.speed);
        let _ = values.set(&DRIFT_DESCRIPTOR, "drift.flow", preset.parameters.flow);
        let _ = values.set(&DRIFT_DESCRIPTOR, "drift.light", preset.parameters.light);
        self.parameters.snap(values);
        self.set_seed(preset.seed);
        self.selected_preset = Some(preset.id.to_owned());
        true
    }

    fn restore_content(
        &mut self,
        seed: u64,
        parameters: SceneParameters,
        selected_preset: Option<String>,
    ) {
        self.parameters.snap(parameters);
        self.seed = seed;
        self.renderer.set_seed(seed);
        self.selected_preset = selected_preset;
    }

    fn seed(&self) -> u64 {
        self.seed
    }

    fn set_seed(&mut self, seed: u64) {
        self.seed = seed;
        self.renderer.set_seed(seed);
        self.selected_preset = None;
    }

    fn set_quality(&mut self, quality: Quality) {
        self.renderer.set_quality(quality);
    }

    fn prepare(&mut self, device: &wgpu::Device, viewport: [u32; 2]) {
        self.renderer.prepare(device, viewport);
    }

    fn encode<'a>(
        &'a mut self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        frame: FramePlan,
        pointer: PointerInput,
    ) -> Option<SceneOutput<'a>> {
        self.parameters.advance(&DRIFT_DESCRIPTOR, frame.display_dt);
        let values = self.parameters.current();
        let parameters = crate::DriftParameters {
            density: values.get_at(0)?,
            speed: values.get_at(1)?,
            flow: values.get_at(2)?,
            light: values.get_at(3)?,
        }
        .modulated(self.modulation);
        let output_index = self
            .renderer
            .encode(queue, encoder, frame, &parameters, pointer)?;
        let (generation, view) = self.renderer.output(output_index)?;
        Some(SceneOutput {
            source_key: 0xD100_0000_0000_0000 | generation.wrapping_mul(2) | output_index as u64,
            view,
            exposure: parameters.light,
        })
    }

    fn release_gpu(&mut self) {
        self.renderer.release_gpu_resources();
    }
}

pub struct FogScene {
    renderer: Fog,
    parameters: SmoothedSceneParameters,
    seed: u64,
    selected_preset: Option<String>,
}

impl FogScene {
    pub fn new(
        device: &wgpu::Device,
        adapter: &wgpu::Adapter,
        quality: Quality,
        seed: u64,
    ) -> Self {
        Self {
            renderer: Fog::new(device, adapter, quality, seed),
            parameters: SmoothedSceneParameters::new(SceneParameters::defaults(&FOG_DESCRIPTOR)),
            seed,
            selected_preset: Some("builtin.morning-haze".to_owned()),
        }
    }
}

impl Scene for FogScene {
    fn descriptor(&self) -> &'static SceneDescriptor {
        &FOG_DESCRIPTOR
    }

    fn parameters(&self) -> &SceneParameters {
        self.parameters.current()
    }

    fn target_parameters(&self) -> &SceneParameters {
        self.parameters.target()
    }

    fn set_parameter(&mut self, stable_id: &str, value: f32) -> Option<f32> {
        let value = self
            .parameters
            .set_target(&FOG_DESCRIPTOR, stable_id, value)?;
        self.selected_preset = None;
        Some(value)
    }

    fn presets(&self) -> &'static [ScenePresetDescriptor] {
        &FOG_PRESET_DESCRIPTORS
    }

    fn selected_preset(&self) -> Option<&str> {
        self.selected_preset.as_deref()
    }

    fn select_builtin_preset(&mut self, stable_id: &str) -> bool {
        let Some(preset) = FOG_PRESETS.iter().find(|preset| preset.id == stable_id) else {
            return false;
        };
        let mut values = SceneParameters::defaults(&FOG_DESCRIPTOR);
        let _ = values.set(&FOG_DESCRIPTOR, "fog.density", preset.parameters.density);
        let _ = values.set(&FOG_DESCRIPTOR, "fog.drift", preset.parameters.drift);
        let _ = values.set(&FOG_DESCRIPTOR, "fog.depth", preset.parameters.depth);
        let _ = values.set(&FOG_DESCRIPTOR, "fog.light", preset.parameters.light);
        self.parameters.snap(values);
        self.seed = preset.seed;
        self.renderer.set_seed(preset.seed);
        self.selected_preset = Some(preset.id.to_owned());
        true
    }

    fn restore_content(
        &mut self,
        seed: u64,
        parameters: SceneParameters,
        selected_preset: Option<String>,
    ) {
        self.parameters.snap(parameters);
        self.seed = seed;
        self.renderer.set_seed(seed);
        self.selected_preset = selected_preset;
    }

    fn seed(&self) -> u64 {
        self.seed
    }

    fn set_seed(&mut self, seed: u64) {
        self.seed = seed;
        self.renderer.set_seed(seed);
        self.selected_preset = None;
    }

    fn set_quality(&mut self, quality: Quality) {
        self.renderer.set_quality(quality);
    }

    fn prepare(&mut self, device: &wgpu::Device, viewport: [u32; 2]) {
        self.renderer.prepare(device, viewport);
    }

    fn encode<'a>(
        &'a mut self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        frame: FramePlan,
        pointer: PointerInput,
    ) -> Option<SceneOutput<'a>> {
        self.parameters.advance(&FOG_DESCRIPTOR, frame.display_dt);
        let values = self.parameters.current();
        let parameters = FogParameters {
            density: values.get_at(0)?,
            drift: values.get_at(1)?,
            depth: values.get_at(2)?,
            light: values.get_at(3)?,
        };
        let (generation, view) = self
            .renderer
            .encode(queue, encoder, frame, parameters, pointer)?;
        Some(SceneOutput {
            source_key: 0xF060_0000_0000_0000 | generation,
            view,
            exposure: parameters.light,
        })
    }

    fn release_gpu(&mut self) {
        self.renderer.release_gpu_resources();
    }
}

pub struct OceanScene {
    renderer: Ocean,
    parameters: SmoothedSceneParameters,
    seed: u64,
    selected_preset: Option<String>,
}

impl OceanScene {
    pub fn new(
        device: &wgpu::Device,
        adapter: &wgpu::Adapter,
        quality: Quality,
        seed: u64,
    ) -> Self {
        Self {
            renderer: Ocean::new(device, adapter, quality, seed),
            parameters: SmoothedSceneParameters::new(SceneParameters::defaults(&OCEAN_DESCRIPTOR)),
            seed,
            selected_preset: Some("builtin.black-tide".to_owned()),
        }
    }
}

impl Scene for OceanScene {
    fn descriptor(&self) -> &'static SceneDescriptor {
        &OCEAN_DESCRIPTOR
    }
    fn parameters(&self) -> &SceneParameters {
        self.parameters.current()
    }
    fn target_parameters(&self) -> &SceneParameters {
        self.parameters.target()
    }
    fn set_parameter(&mut self, stable_id: &str, value: f32) -> Option<f32> {
        let value = self
            .parameters
            .set_target(&OCEAN_DESCRIPTOR, stable_id, value)?;
        self.selected_preset = None;
        Some(value)
    }
    fn presets(&self) -> &'static [ScenePresetDescriptor] {
        &OCEAN_PRESET_DESCRIPTORS
    }
    fn selected_preset(&self) -> Option<&str> {
        self.selected_preset.as_deref()
    }
    fn select_builtin_preset(&mut self, stable_id: &str) -> bool {
        let Some(preset) = OCEAN_PRESETS.iter().find(|preset| preset.id == stable_id) else {
            return false;
        };
        let mut values = SceneParameters::defaults(&OCEAN_DESCRIPTOR);
        let _ = values.set(&OCEAN_DESCRIPTOR, "ocean.calm", preset.parameters.calm);
        let _ = values.set(&OCEAN_DESCRIPTOR, "ocean.scale", preset.parameters.scale);
        let _ = values.set(&OCEAN_DESCRIPTOR, "ocean.flow", preset.parameters.flow);
        let _ = values.set(&OCEAN_DESCRIPTOR, "ocean.light", preset.parameters.light);
        self.parameters.snap(values);
        self.seed = preset.seed;
        self.renderer.set_seed(preset.seed);
        self.selected_preset = Some(preset.id.to_owned());
        true
    }
    fn restore_content(
        &mut self,
        seed: u64,
        parameters: SceneParameters,
        selected_preset: Option<String>,
    ) {
        self.parameters.snap(parameters);
        self.seed = seed;
        self.renderer.set_seed(seed);
        self.selected_preset = selected_preset;
    }
    fn seed(&self) -> u64 {
        self.seed
    }
    fn set_seed(&mut self, seed: u64) {
        self.seed = seed;
        self.renderer.set_seed(seed);
        self.selected_preset = None;
    }
    fn set_quality(&mut self, quality: Quality) {
        self.renderer.set_quality(quality);
    }
    fn prepare(&mut self, device: &wgpu::Device, viewport: [u32; 2]) {
        self.renderer.prepare(device, viewport);
    }
    fn encode<'a>(
        &'a mut self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        frame: FramePlan,
        pointer: PointerInput,
    ) -> Option<SceneOutput<'a>> {
        self.parameters.advance(&OCEAN_DESCRIPTOR, frame.display_dt);
        let values = self.parameters.current();
        let parameters = OceanParameters {
            calm: values.get_at(0)?,
            scale: values.get_at(1)?,
            flow: values.get_at(2)?,
            light: values.get_at(3)?,
        };
        let (generation, view) = self
            .renderer
            .encode(queue, encoder, frame, parameters, pointer)?;
        Some(SceneOutput {
            source_key: 0x0CEA_0000_0000_0000 | generation,
            view,
            exposure: parameters.light,
        })
    }
    fn release_gpu(&mut self) {
        self.renderer.release_gpu_resources();
    }
}

pub fn descriptor(stable_id: &str) -> Option<&'static SceneDescriptor> {
    SCENE_DESCRIPTORS
        .iter()
        .find(|scene| scene.stable_id == stable_id)
}

pub fn preset_descriptors(stable_id: &str) -> Option<&'static [ScenePresetDescriptor]> {
    match stable_id {
        "drift" => Some(&DRIFT_PRESET_DESCRIPTORS),
        "fog" => Some(&FOG_PRESET_DESCRIPTORS),
        "ocean" => Some(&OCEAN_PRESET_DESCRIPTORS),
        "glass" => Some(&crate::GLASS_PRESET_DESCRIPTORS),
        "field" => Some(&crate::FIELD_PRESET_DESCRIPTORS),
        "frost" => Some(&crate::FROST_PRESET_DESCRIPTORS),
        _ => None,
    }
}

pub fn builtin_preset_content(scene_id: &str, preset_id: &str) -> Option<(u64, SceneParameters)> {
    match scene_id {
        "frost" => crate::frost::preset_content(preset_id),
        "glass" | "field" => crate::new_spaces::content(scene_id, preset_id),
        "drift" => {
            let preset = BUILTIN_PRESETS
                .iter()
                .find(|preset| preset.id == preset_id)?;
            let mut values = SceneParameters::defaults(&DRIFT_DESCRIPTOR);
            let _ = values.set(
                &DRIFT_DESCRIPTOR,
                "drift.density",
                preset.parameters.density,
            );
            let _ = values.set(&DRIFT_DESCRIPTOR, "drift.speed", preset.parameters.speed);
            let _ = values.set(&DRIFT_DESCRIPTOR, "drift.flow", preset.parameters.flow);
            let _ = values.set(&DRIFT_DESCRIPTOR, "drift.light", preset.parameters.light);
            Some((preset.seed, values))
        }
        "fog" => {
            let preset = FOG_PRESETS.iter().find(|preset| preset.id == preset_id)?;
            let mut values = SceneParameters::defaults(&FOG_DESCRIPTOR);
            let _ = values.set(&FOG_DESCRIPTOR, "fog.density", preset.parameters.density);
            let _ = values.set(&FOG_DESCRIPTOR, "fog.drift", preset.parameters.drift);
            let _ = values.set(&FOG_DESCRIPTOR, "fog.depth", preset.parameters.depth);
            let _ = values.set(&FOG_DESCRIPTOR, "fog.light", preset.parameters.light);
            Some((preset.seed, values))
        }
        "ocean" => {
            let preset = OCEAN_PRESETS.iter().find(|preset| preset.id == preset_id)?;
            let mut values = SceneParameters::defaults(&OCEAN_DESCRIPTOR);
            let _ = values.set(&OCEAN_DESCRIPTOR, "ocean.calm", preset.parameters.calm);
            let _ = values.set(&OCEAN_DESCRIPTOR, "ocean.scale", preset.parameters.scale);
            let _ = values.set(&OCEAN_DESCRIPTOR, "ocean.flow", preset.parameters.flow);
            let _ = values.set(&OCEAN_DESCRIPTOR, "ocean.light", preset.parameters.light);
            Some((preset.seed, values))
        }
        _ => None,
    }
}

pub fn create_scene(
    stable_id: &str,
    device: &wgpu::Device,
    adapter: &wgpu::Adapter,
    quality: Quality,
    seed: u64,
) -> Option<Box<dyn Scene>> {
    match stable_id {
        "drift" => Some(Box::new(DriftScene::new(device, adapter, quality, seed))),
        "fog" => Some(Box::new(FogScene::new(device, adapter, quality, seed))),
        "ocean" => Some(Box::new(OceanScene::new(device, adapter, quality, seed))),
        "glass" => Some(Box::new(crate::GlassScene::new(
            device, adapter, quality, seed,
        ))),
        "field" => Some(Box::new(crate::FieldScene::new(
            device, adapter, quality, seed,
        ))),
        "frost" => Some(Box::new(crate::FrostScene::new(
            device, adapter, quality, seed,
        ))),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_ids_and_parameter_ids_are_unique_and_namespaced() {
        for (scene_index, scene) in SCENE_DESCRIPTORS.iter().enumerate() {
            assert!(!scene.stable_id.is_empty());
            assert!(!scene.display_name.is_empty());
            assert!(scene.schema_version > 0);
            for other in SCENE_DESCRIPTORS.iter().skip(scene_index + 1) {
                assert_ne!(scene.stable_id, other.stable_id);
            }
            for (parameter_index, parameter) in scene.parameters.iter().enumerate() {
                assert!(
                    parameter
                        .stable_id
                        .starts_with(&format!("{}.", scene.stable_id))
                );
                for other in scene.parameters.iter().skip(parameter_index + 1) {
                    assert_ne!(parameter.stable_id, other.stable_id);
                }
            }
        }
    }

    #[test]
    fn scene_and_preset_identifiers_are_distinct_namespaces() {
        assert_eq!(DRIFT_DESCRIPTOR.stable_id, "drift");
        assert!(
            DRIFT_PRESET_DESCRIPTORS
                .iter()
                .all(|preset| preset.stable_id.starts_with("builtin."))
        );
        assert!(
            DRIFT_PRESET_DESCRIPTORS
                .iter()
                .all(|preset| preset.stable_id != DRIFT_DESCRIPTOR.stable_id)
        );
    }

    #[test]
    fn all_spaces_encode_presets_resize_and_recreate_on_metal() {
        let renderer = match still_renderer::Renderer::new() {
            Ok(renderer) => renderer,
            Err(error) => {
                assert!(
                    std::env::var_os("STILL_REQUIRE_METAL_TESTS").is_none(),
                    "Metal required for this gate: {error}"
                );
                return;
            }
        };
        renderer
            .device
            .push_error_scope(wgpu::ErrorFilter::Validation);
        for descriptor in SCENE_DESCRIPTORS {
            for quality in [Quality::Eco, Quality::Balanced, Quality::High] {
                let mut scene = create_scene(
                    descriptor.stable_id,
                    &renderer.device,
                    &renderer.adapter,
                    quality,
                    7,
                )
                .unwrap();
                for preset in scene.presets() {
                    assert!(scene.select_builtin_preset(preset.stable_id));
                    assert_eq!(scene.selected_preset(), Some(preset.stable_id));
                    for viewport in [[640, 420], [1280, 840], [0, 0], [900, 560]] {
                        scene.prepare(&renderer.device, viewport);
                        let mut encoder = renderer.create_encoder();
                        let output = scene.encode(
                            &renderer.queue,
                            &mut encoder,
                            FramePlan {
                                animated: false,
                                ticks: 2,
                                fixed_dt: 1.0 / 120.0,
                                display_dt: 1.0 / 60.0,
                                interpolation: 0.25,
                            },
                            PointerInput::default(),
                        );
                        assert_eq!(output.is_some(), !viewport.contains(&0));
                        renderer.queue.submit(Some(encoder.finish()));
                        renderer.device.poll(wgpu::Maintain::Wait);
                    }
                }
                scene.suspend();
                scene.release_gpu();
                scene.resume();
                scene.prepare(&renderer.device, [800, 600]);
                let mut encoder = renderer.create_encoder();
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
                renderer.queue.submit(Some(encoder.finish()));
                renderer.device.poll(wgpu::Maintain::Wait);
            }
        }
        assert!(pollster::block_on(renderer.device.pop_error_scope()).is_none());
    }
}
