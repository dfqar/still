use serde::{Deserialize, Serialize};

/// Generic, transient visual controls. Their source and clock belong to the caller.
/// They never change manual scene content and contain no audio-specific data.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SceneModulation {
    pub time_seconds: f64,
    pub motion: f32,
    pub turbulence: f32,
    pub light: f32,
}

impl SceneModulation {
    pub fn validated(mut self) -> Option<Self> {
        if !self.time_seconds.is_finite()
            || self.time_seconds < 0.0
            || [self.motion, self.turbulence, self.light]
                .iter()
                .any(|v| !v.is_finite())
        {
            return None;
        }
        self.motion = self.motion.clamp(0.0, 1.0);
        self.turbulence = self.turbulence.clamp(0.0, 1.0);
        self.light = self.light.clamp(0.0, 1.0);
        Some(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transient_controls_validate_atomically() {
        let input = SceneModulation {
            motion: 4.0,
            turbulence: -1.0,
            ..Default::default()
        };
        assert_eq!(input.validated().unwrap().motion, 1.0);
        assert_eq!(input.validated().unwrap().turbulence, 0.0);
        assert!(
            SceneModulation {
                light: f32::NAN,
                ..input
            }
            .validated()
            .is_none()
        );
        assert!(
            SceneModulation {
                time_seconds: -1.0,
                ..input
            }
            .validated()
            .is_none()
        );
    }
}
