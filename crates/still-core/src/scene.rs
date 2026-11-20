use std::collections::BTreeMap;

/// Stable metadata shared by the native shell and every scene implementation.
/// The core deliberately does not attach a simulation or rendering model to a
/// scene: those details belong to `still-scenes`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneDescriptor {
    pub stable_id: &'static str,
    pub display_name: &'static str,
    pub schema_version: u32,
    pub supports_modulation: bool,
    pub parameters: &'static [SceneParameterDescriptor],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneParameterDescriptor {
    pub stable_id: &'static str,
    pub display_name: &'static str,
    pub minimum: f32,
    pub maximum: f32,
    pub default_value: f32,
    pub smoothing_seconds: f32,
}

impl SceneParameterDescriptor {
    pub fn validate(self, value: f32) -> f32 {
        if value.is_finite() {
            value.clamp(self.minimum, self.maximum)
        } else {
            self.default_value
        }
    }
}

/// Runtime parameter storage indexed by a scene's descriptor order. It is
/// allocated only when a scene is created or restored, never once per frame.
#[derive(Clone, Debug, PartialEq)]
pub struct SceneParameters {
    values: Vec<f32>,
}

impl SceneParameters {
    pub fn defaults(descriptor: &SceneDescriptor) -> Self {
        Self {
            values: descriptor
                .parameters
                .iter()
                .map(|parameter| parameter.default_value)
                .collect(),
        }
    }

    pub fn from_persisted(descriptor: &SceneDescriptor, persisted: &BTreeMap<String, f32>) -> Self {
        let mut values = Self::defaults(descriptor);
        for (id, value) in persisted {
            let _ = values.set(descriptor, id, *value);
        }
        values
    }

    pub fn get(&self, descriptor: &SceneDescriptor, stable_id: &str) -> Option<f32> {
        let index = parameter_index(descriptor, stable_id)?;
        self.values.get(index).copied()
    }

    pub fn get_at(&self, index: usize) -> Option<f32> {
        self.values.get(index).copied()
    }

    pub fn set(
        &mut self,
        descriptor: &SceneDescriptor,
        stable_id: &str,
        value: f32,
    ) -> Option<f32> {
        let index = parameter_index(descriptor, stable_id)?;
        let value = descriptor.parameters[index].validate(value);
        *self.values.get_mut(index)? = value;
        Some(value)
    }

    pub fn set_at(
        &mut self,
        descriptor: &SceneDescriptor,
        index: usize,
        value: f32,
    ) -> Option<f32> {
        let parameter = descriptor.parameters.get(index)?;
        let value = parameter.validate(value);
        *self.values.get_mut(index)? = value;
        Some(value)
    }

    pub fn as_slice(&self) -> &[f32] {
        &self.values
    }

    pub fn to_persisted(&self, descriptor: &SceneDescriptor) -> BTreeMap<String, f32> {
        descriptor
            .parameters
            .iter()
            .zip(self.values.iter().copied())
            .map(|(parameter, value)| (parameter.stable_id.to_owned(), value))
            .collect()
    }
}

/// Cadence-independent smoothing shared by scenes without imposing any
/// interpretation on their parameter values.
#[derive(Clone, Debug)]
pub struct SmoothedSceneParameters {
    current: SceneParameters,
    target: SceneParameters,
}

impl SmoothedSceneParameters {
    pub fn new(values: SceneParameters) -> Self {
        Self {
            current: values.clone(),
            target: values,
        }
    }

    pub fn current(&self) -> &SceneParameters {
        &self.current
    }

    pub fn target(&self) -> &SceneParameters {
        &self.target
    }

    pub fn snap(&mut self, values: SceneParameters) {
        self.current = values.clone();
        self.target = values;
    }

    pub fn set_target(
        &mut self,
        descriptor: &SceneDescriptor,
        stable_id: &str,
        value: f32,
    ) -> Option<f32> {
        self.target.set(descriptor, stable_id, value)
    }

    pub fn advance(&mut self, descriptor: &SceneDescriptor, dt: f32) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        for (index, parameter) in descriptor.parameters.iter().enumerate() {
            let smoothing = parameter.smoothing_seconds.max(0.000_1);
            let alpha = 1.0 - (-dt / smoothing).exp();
            let current = self.current.values[index];
            let target = self.target.values[index];
            self.current.values[index] = current + (target - current) * alpha;
        }
    }
}

fn parameter_index(descriptor: &SceneDescriptor, stable_id: &str) -> Option<usize> {
    descriptor
        .parameters
        .iter()
        .position(|parameter| parameter.stable_id == stable_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PARAMETERS: [SceneParameterDescriptor; 2] = [
        SceneParameterDescriptor {
            stable_id: "test.amount",
            display_name: "Amount",
            minimum: 0.0,
            maximum: 1.0,
            default_value: 0.5,
            smoothing_seconds: 0.25,
        },
        SceneParameterDescriptor {
            stable_id: "test.motion",
            display_name: "Motion",
            minimum: 0.2,
            maximum: 2.0,
            default_value: 0.8,
            smoothing_seconds: 0.5,
        },
    ];
    const SCENE: SceneDescriptor = SceneDescriptor {
        stable_id: "test",
        display_name: "Test",
        schema_version: 1,
        supports_modulation: false,
        parameters: &PARAMETERS,
    };

    #[test]
    fn scene_parameters_validate_unknown_and_non_finite_values() {
        let mut values = SceneParameters::defaults(&SCENE);
        assert_eq!(values.set(&SCENE, "test.amount", 9.0), Some(1.0));
        assert_eq!(values.set(&SCENE, "test.motion", f32::NAN), Some(0.8));
        assert_eq!(values.set(&SCENE, "other.amount", 0.2), None);
    }

    #[test]
    fn persisted_values_are_namespaced_and_forward_compatible() {
        let persisted = BTreeMap::from([
            ("test.amount".to_owned(), 0.7),
            ("future.value".to_owned(), 99.0),
        ]);
        let values = SceneParameters::from_persisted(&SCENE, &persisted);
        assert_eq!(values.get(&SCENE, "test.amount"), Some(0.7));
        assert_eq!(values.get(&SCENE, "test.motion"), Some(0.8));
        assert!(!values.to_persisted(&SCENE).contains_key("future.value"));
    }

    #[test]
    fn generic_smoothing_is_cadence_independent() {
        fn run(hz: u32) -> f32 {
            let mut values = SmoothedSceneParameters::new(SceneParameters::defaults(&SCENE));
            values.set_target(&SCENE, "test.motion", 2.0);
            for _ in 0..hz {
                values.advance(&SCENE, 1.0 / hz as f32);
            }
            values.current().get(&SCENE, "test.motion").unwrap()
        }
        assert!((run(60) - run(120)).abs() < 1e-5);
    }
}
