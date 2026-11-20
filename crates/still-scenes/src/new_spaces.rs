use crate::scene::{Scene, SceneOutput, ScenePresetDescriptor};
use still_core::{
    FramePlan, PointerInput, Quality, SceneDescriptor, SceneParameterDescriptor, SceneParameters,
    SmoothedSceneParameters,
};
use still_renderer::wgpu;

const fn parameter(
    id: &'static str,
    name: &'static str,
    min: f32,
    default: f32,
) -> SceneParameterDescriptor {
    SceneParameterDescriptor {
        stable_id: id,
        display_name: name,
        minimum: min,
        maximum: 1.0,
        default_value: default,
        smoothing_seconds: 0.65,
    }
}

pub const GLASS_PARAMETERS: [SceneParameterDescriptor; 4] = [
    parameter("glass.form", "Form", 0.0, 0.18),
    parameter("glass.motion", "Motion", 0.05, 0.28),
    parameter("glass.refraction", "Refraction", 0.0, 0.52),
    parameter("glass.light", "Light", 0.15, 0.66),
];
pub const FIELD_PARAMETERS: [SceneParameterDescriptor; 4] = [
    parameter("field.density", "Density", 0.0, 0.52),
    parameter("field.motion", "Motion", 0.05, 0.35),
    parameter("field.bend", "Bend", 0.0, 0.58),
    parameter("field.light", "Light", 0.15, 0.54),
];
pub const GLASS_DESCRIPTOR: SceneDescriptor = SceneDescriptor {
    stable_id: "glass",
    display_name: "Glass",
    schema_version: 1,
    supports_modulation: false,
    parameters: &GLASS_PARAMETERS,
};
pub const FIELD_DESCRIPTOR: SceneDescriptor = SceneDescriptor {
    stable_id: "field",
    display_name: "Field",
    schema_version: 1,
    supports_modulation: false,
    parameters: &FIELD_PARAMETERS,
};

#[derive(Clone, Copy, Debug)]
pub struct SpacePreset {
    pub descriptor: ScenePresetDescriptor,
    pub seed: u64,
    pub values: [f32; 4],
}
const fn preset(id: &'static str, name: &'static str, seed: u64, values: [f32; 4]) -> SpacePreset {
    SpacePreset {
        descriptor: ScenePresetDescriptor {
            stable_id: id,
            display_name: name,
        },
        seed,
        values,
    }
}
pub const GLASS_PRESETS: [SpacePreset; 4] = [
    preset(
        "builtin.clear-form",
        "Clear Form",
        0x61_A5_50_01,
        [0.18, 0.28, 0.52, 0.66],
    ),
    preset(
        "builtin.smoked-lens",
        "Smoked Lens",
        0x61_A5_50_02,
        [0.32, 0.16, 0.92, 0.48],
    ),
    preset(
        "builtin.liquid-fold",
        "Liquid Fold",
        0x61_A5_50_03,
        [0.90, 0.64, 0.70, 0.72],
    ),
    preset(
        "builtin.quiet-prism",
        "Quiet Prism",
        0x61_A5_50_04,
        [0.62, 0.22, 0.38, 0.88],
    ),
];
pub const FIELD_PRESETS: [SpacePreset; 4] = [
    preset(
        "builtin.soft-current",
        "Soft Current",
        0xF1_E1_D0_01,
        [0.52, 0.35, 0.58, 0.54],
    ),
    preset(
        "builtin.contour",
        "Contour",
        0xF1_E1_D0_02,
        [0.80, 0.18, 0.92, 0.44],
    ),
    preset(
        "builtin.long-form",
        "Long Form",
        0xF1_E1_D0_03,
        [0.30, 0.24, 0.12, 0.72],
    ),
    preset(
        "builtin.woven-silence",
        "Woven Silence",
        0xF1_E1_D0_04,
        [0.92, 0.46, 0.78, 0.60],
    ),
];
pub const GLASS_PRESET_DESCRIPTORS: [ScenePresetDescriptor; 4] = [
    GLASS_PRESETS[0].descriptor,
    GLASS_PRESETS[1].descriptor,
    GLASS_PRESETS[2].descriptor,
    GLASS_PRESETS[3].descriptor,
];
pub const FIELD_PRESET_DESCRIPTORS: [ScenePresetDescriptor; 4] = [
    FIELD_PRESETS[0].descriptor,
    FIELD_PRESETS[1].descriptor,
    FIELD_PRESETS[2].descriptor,
    FIELD_PRESETS[3].descriptor,
];

pub(crate) fn content(scene: &str, preset_id: &str) -> Option<(u64, SceneParameters)> {
    let (descriptor, presets) = match scene {
        "glass" => (&GLASS_DESCRIPTOR, &GLASS_PRESETS),
        "field" => (&FIELD_DESCRIPTOR, &FIELD_PRESETS),
        _ => return None,
    };
    let preset = presets
        .iter()
        .find(|p| p.descriptor.stable_id == preset_id)?;
    let mut parameters = SceneParameters::defaults(descriptor);
    for (parameter, value) in descriptor.parameters.iter().zip(preset.values) {
        parameters.set(descriptor, parameter.stable_id, value)?;
    }
    Some((preset.seed, parameters))
}

// These two scene adapters share state/parameter plumbing; their GPU renderers
// remain independent. Existing Drift/Fog/Ocean code is left intact.
macro_rules! space_scene {
    ($name:ident, $gpu:ty, $descriptor:ident, $presets:ident, $key:expr) => {
        pub struct $name {
            renderer: $gpu,
            parameters: SmoothedSceneParameters,
            seed: u64,
            selected_preset: Option<String>,
        }
        impl $name {
            pub fn new(
                device: &wgpu::Device,
                adapter: &wgpu::Adapter,
                quality: Quality,
                seed: u64,
            ) -> Self {
                Self {
                    renderer: <$gpu>::new(device, adapter, quality, seed),
                    parameters: SmoothedSceneParameters::new(SceneParameters::defaults(
                        &$descriptor,
                    )),
                    seed,
                    selected_preset: Some($presets[0].stable_id.to_owned()),
                }
            }
        }
        impl Scene for $name {
            fn descriptor(&self) -> &'static SceneDescriptor {
                &$descriptor
            }
            fn parameters(&self) -> &SceneParameters {
                self.parameters.current()
            }
            fn target_parameters(&self) -> &SceneParameters {
                self.parameters.target()
            }
            fn set_parameter(&mut self, id: &str, value: f32) -> Option<f32> {
                let value = self.parameters.set_target(&$descriptor, id, value)?;
                self.selected_preset = None;
                Some(value)
            }
            fn presets(&self) -> &'static [ScenePresetDescriptor] {
                &$presets
            }
            fn selected_preset(&self) -> Option<&str> {
                self.selected_preset.as_deref()
            }
            fn select_builtin_preset(&mut self, id: &str) -> bool {
                let Some((seed, parameters)) = content($descriptor.stable_id, id) else {
                    return false;
                };
                self.restore_content(seed, parameters, Some(id.to_owned()));
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
                self.parameters.advance(&$descriptor, frame.display_dt);
                let parameters = self.parameters.current();
                let values = [
                    parameters.get_at(0)?,
                    parameters.get_at(1)?,
                    parameters.get_at(2)?,
                    parameters.get_at(3)?,
                ];
                let (generation, view) = self
                    .renderer
                    .encode(queue, encoder, frame, values, pointer)?;
                Some(SceneOutput {
                    source_key: $key | generation,
                    view,
                    exposure: values[3],
                })
            }
            fn release_gpu(&mut self) {
                self.renderer.release_gpu_resources();
            }
        }
    };
}
space_scene!(
    GlassScene,
    crate::glass::Glass,
    GLASS_DESCRIPTOR,
    GLASS_PRESET_DESCRIPTORS,
    0x61A5_0000_0000_0000
);
space_scene!(
    FieldScene,
    crate::field::Field,
    FIELD_DESCRIPTOR,
    FIELD_PRESET_DESCRIPTORS,
    0xF1E1_0000_0000_0000
);

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authored_presets_are_valid_distinct_and_deterministic() {
        for (descriptor, presets) in [
            (&GLASS_DESCRIPTOR, &GLASS_PRESETS),
            (&FIELD_DESCRIPTOR, &FIELD_PRESETS),
        ] {
            let mut seeds = std::collections::BTreeSet::new();
            for preset in presets {
                assert!(seeds.insert(preset.seed));
                let (seed, values) =
                    content(descriptor.stable_id, preset.descriptor.stable_id).unwrap();
                assert_eq!(seed, preset.seed);
                for (i, parameter) in descriptor.parameters.iter().enumerate() {
                    assert!((parameter.minimum..=parameter.maximum).contains(&preset.values[i]));
                    assert_eq!(values.get_at(i), Some(preset.values[i]));
                }
            }
        }
        assert!(content("glass", "builtin.soft-current").is_none());
        assert!(content("field", "builtin.clear-form").is_none());
    }
}
