#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DriftParameters {
    pub density: f32,
    pub speed: f32,
    pub flow: f32,
    pub light: f32,
}

impl Default for DriftParameters {
    fn default() -> Self {
        Self {
            density: 0.66,
            speed: 0.82,
            flow: 0.72,
            light: 0.58,
        }
    }
}

impl DriftParameters {
    /// A transient overlay, independent of saved/manual targets and their smoothing.
    pub fn modulated(mut self, input: Option<still_core::SceneModulation>) -> Self {
        if let Some(input) = input.and_then(still_core::SceneModulation::validated) {
            self.speed = (self.speed + input.motion * 0.4).clamp(0.2, 1.8);
            self.flow = (self.flow + input.turbulence * 0.25).clamp(0.15, 1.5);
            self.light = (self.light + input.light * 0.7).clamp(0.15, 1.0);
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overlay_changes_render_values_without_changing_manual_content() {
        let manual = DriftParameters::default();
        let effective = manual.modulated(Some(still_core::SceneModulation {
            motion: 1.0,
            turbulence: 0.5,
            light: 1.0,
            time_seconds: 8.0,
        }));
        assert_eq!(effective.speed, manual.speed + 0.4);
        assert_eq!(effective.flow, manual.flow + 0.125);
        assert_eq!(effective.light, 1.0);
        assert_eq!(manual.modulated(None), manual);
        assert_eq!(manual, DriftParameters::default());
    }
}
