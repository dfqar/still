/// Transient input from a future media adapter. Colors are normalized sRGB;
/// energy is a normalized, already analyzed envelope, never raw audio samples.
/// This input is deliberately separate from saved manual scene parameters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AmbientFrame {
    pub colors: [[f32; 3]; 3],
    pub energy: f32,
}

impl AmbientFrame {
    /// Reject non-finite input atomically; clamp finite out-of-range values.
    pub fn validated(colors: [[f32; 3]; 3], energy: f32) -> Option<Self> {
        if !energy.is_finite() || colors.iter().flatten().any(|v| !v.is_finite()) {
            return None;
        }
        Some(Self {
            colors: colors.map(|rgb| rgb.map(|v| v.clamp(0.0, 1.0))),
            energy: energy.clamp(0.0, 1.0),
        })
    }
}

/// Generic transient light controls. No audio bands, beat detector or routing
/// policy lives here. An adapter supplies whatever visual state it needs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AmbientLightControl {
    pub color: [f32; 3],
    pub gain: f32,
    pub offset: [f32; 2],
    pub scale: [f32; 2],
    pub rotation_degrees: f32,
}

impl Default for AmbientLightControl {
    fn default() -> Self {
        Self {
            color: [1.0; 3],
            gain: 1.0,
            offset: [0.0; 2],
            scale: [1.0; 2],
            rotation_degrees: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AmbientControlFrame {
    pub lights: [AmbientLightControl; 3],
    pub brightness: f32,
}

impl AmbientControlFrame {
    pub fn validated(mut self) -> Option<Self> {
        if !self.brightness.is_finite() {
            return None;
        }
        for light in &mut self.lights {
            if light
                .color
                .iter()
                .chain(light.offset.iter())
                .chain(light.scale.iter())
                .chain([&light.gain, &light.rotation_degrees])
                .any(|v| !v.is_finite())
            {
                return None;
            }
            light.color = light.color.map(|v| v.clamp(0.0, 1.0));
            light.gain = light.gain.clamp(0.0, 4.0);
            light.offset = light.offset.map(|v| v.clamp(-2.0, 2.0));
            light.scale = light.scale.map(|v| v.clamp(0.05, 4.0));
            light.rotation_degrees = light.rotation_degrees.clamp(-360.0, 360.0);
        }
        self.brightness = self.brightness.clamp(0.0, 4.0);
        Some(self)
    }
}

/// Code-only response settings for external control, independent of the
/// user-facing scene sliders. Zero seconds means an immediate response.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AmbientSmoothing {
    pub color_seconds: f32,
    pub attack_seconds: f32,
    pub release_seconds: f32,
    pub transform_seconds: f32,
}

impl Default for AmbientSmoothing {
    fn default() -> Self {
        Self {
            color_seconds: 0.85,
            attack_seconds: 0.85,
            release_seconds: 0.85,
            transform_seconds: 0.85,
        }
    }
}

impl AmbientSmoothing {
    pub fn approach_frame(current: f32, target: f32, dt: f32, seconds: f32, animated: bool) -> f32 {
        if dt == 0.0 && !animated {
            target
        } else {
            Self::approach(current, target, dt, seconds)
        }
    }
    pub fn validated(self) -> Option<Self> {
        [
            self.color_seconds,
            self.attack_seconds,
            self.release_seconds,
            self.transform_seconds,
        ]
        .iter()
        .all(|v| v.is_finite() && (0.0..=60.0).contains(v))
        .then_some(self)
    }

    pub fn approach(current: f32, target: f32, dt: f32, seconds: f32) -> f32 {
        if !dt.is_finite() || dt <= 0.0 {
            return current;
        }
        if seconds <= 0.0 {
            return target;
        }
        current + (target - current) * (1.0 - (-dt / seconds).exp())
    }

    pub fn gain(self, current: f32, target: f32, dt: f32) -> f32 {
        Self::approach(
            current,
            target,
            dt,
            if target > current {
                self.attack_seconds
            } else {
                self.release_seconds
            },
        )
    }
}

pub fn srgb_to_linear(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn animated_first_frame_preserves_color_while_single_paint_adopts_latest_palette() {
        assert_eq!(
            AmbientSmoothing::approach_frame(0.2, 0.9, 0.0, 0.85, true),
            0.2
        );
        assert_eq!(
            AmbientSmoothing::approach_frame(0.2, 0.9, 0.0, 0.85, false),
            0.9
        );
        let first = AmbientSmoothing::approach_frame(0.2, 0.9, 1.0 / 60.0, 0.85, true);
        assert!(first > 0.2 && first < 0.22);
        let later = (0..120).fold(first, |v, _| {
            AmbientSmoothing::approach_frame(v, 0.9, 1.0 / 60.0, 0.85, true)
        });
        assert!(later > 0.8 && later < 0.9);
    }

    #[test]
    fn media_input_is_atomic_finite_and_color_correct() {
        assert!(AmbientFrame::validated([[f32::NAN; 3]; 3], 0.0).is_none());
        assert!(AmbientFrame::validated([[0.5; 3]; 3], f32::INFINITY).is_none());
        let frame = AmbientFrame::validated([[2.0, -1.0, 0.5]; 3], 3.0).unwrap();
        assert_eq!(frame.colors[0], [1.0, 0.0, 0.5]);
        assert_eq!(frame.energy, 1.0);
        assert!((srgb_to_linear(0.5) - 0.214041).abs() < 1e-5);
        assert_eq!(srgb_to_linear(0.0), 0.0);
        assert_eq!(srgb_to_linear(1.0), 1.0);
    }

    #[test]
    fn external_response_supports_fast_attack_slow_release_and_no_smoothing() {
        let response = AmbientSmoothing {
            attack_seconds: 0.008,
            release_seconds: 0.2,
            ..Default::default()
        };
        let peak = response.gain(0.0, 1.0, 1.0 / 60.0);
        assert!(peak > 0.85);
        assert!(response.gain(peak, 0.0, 1.0 / 60.0) > peak * 0.9);
        assert_eq!(AmbientSmoothing::approach(0.0, 1.0, 1.0 / 120.0, 0.0), 1.0);
        fn simulate(hz: u32) -> f32 {
            (0..hz).fold(0.0, |v, _| {
                AmbientSmoothing::default().gain(v, 1.0, 1.0 / hz as f32)
            })
        }
        assert!((simulate(60) - simulate(120)).abs() < 1e-5);
        assert!(
            AmbientSmoothing {
                attack_seconds: f32::NAN,
                ..response
            }
            .validated()
            .is_none()
        );
        assert!(
            AmbientSmoothing {
                color_seconds: -1.0,
                ..response
            }
            .validated()
            .is_none()
        );
    }

    #[test]
    fn broad_controls_are_finite_bounded_and_neutral_by_default() {
        let mut frame = AmbientControlFrame {
            lights: [AmbientLightControl::default(); 3],
            brightness: 1.0,
        };
        assert_eq!(frame.validated(), Some(frame));
        frame.lights[1].gain = 100.0;
        frame.lights[2].scale = [-1.0, 100.0];
        let valid = frame.validated().unwrap();
        assert_eq!(valid.lights[1].gain, 4.0);
        assert_eq!(valid.lights[2].scale, [0.05, 4.0]);
        frame.lights[0].offset[0] = f32::INFINITY;
        assert!(frame.validated().is_none());
    }
}
