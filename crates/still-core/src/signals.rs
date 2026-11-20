use crate::PointerInput;

/// Compact source registry for present and future modulation. Only time and
/// pointer are populated in 1.0; no platform or audio types enter core.
#[derive(Clone, Copy, Debug, Default)]
pub struct SignalBus {
    time_seconds: f64,
    pointer: PointerInput,
}

impl SignalBus {
    pub fn time_seconds(&self) -> f64 {
        self.time_seconds
    }
    pub fn pointer(&self) -> PointerInput {
        self.pointer
    }

    pub fn advance(&mut self, dt: f32, latest_pointer: Option<PointerInput>) {
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.1)
        } else {
            0.0
        };
        self.time_seconds += dt as f64;
        if let Some(input) = latest_pointer {
            self.pointer.position = input.position;
            self.pointer.velocity[0] +=
                (input.velocity[0].clamp(-3.0, 3.0) - self.pointer.velocity[0]) * 0.22;
            self.pointer.velocity[1] +=
                (input.velocity[1].clamp(-3.0, 3.0) - self.pointer.velocity[1]) * 0.22;
            self.pointer.active = input.active;
        } else {
            let decay = (-dt * 8.0).exp();
            self.pointer.velocity[0] *= decay;
            self.pointer.velocity[1] *= decay;
        }
        if !self.pointer.active
            || self.pointer.velocity[0].mul_add(
                self.pointer.velocity[0],
                self.pointer.velocity[1] * self.pointer.velocity[1],
            ) < 1e-6
        {
            self.pointer.velocity = [0.0; 2];
        }
    }

    pub fn reset_transients(&mut self) {
        self.pointer.velocity = [0.0; 2];
        self.pointer.active = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stationary_pointer_wake_fades() {
        let mut bus = SignalBus::default();
        bus.advance(
            1.0 / 60.0,
            Some(PointerInput {
                position: [0.2, -0.1],
                velocity: [2.0, 0.0],
                active: true,
            }),
        );
        let initial = bus.pointer().velocity[0];
        for _ in 0..120 {
            bus.advance(1.0 / 120.0, None);
        }
        assert!(bus.pointer().velocity[0] < initial * 0.001);
    }
}
