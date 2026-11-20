use crate::Quality;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PowerState {
    pub low_power: bool,
    /// 0 nominal, 1 fair, 2 serious, 3 critical.
    pub thermal_level: u8,
}

#[derive(Clone, Debug)]
pub struct AutoQualityPolicy {
    selected: Quality,
    resolved: Quality,
    power: PowerState,
    refresh_hz: f32,
    cpu_ema_seconds: f32,
    frame_ema_seconds: f32,
    pressure_seconds: f32,
    headroom_seconds: f32,
}

impl AutoQualityPolicy {
    pub fn new(selected: Quality) -> Self {
        let resolved = if selected == Quality::Auto {
            Quality::Balanced
        } else {
            selected
        };
        Self {
            selected,
            resolved,
            power: PowerState::default(),
            refresh_hz: 60.0,
            cpu_ema_seconds: 0.0,
            frame_ema_seconds: 0.0,
            pressure_seconds: 0.0,
            headroom_seconds: 0.0,
        }
    }
    pub fn selected(&self) -> Quality {
        self.selected
    }
    pub fn resolved(&self) -> Quality {
        self.resolved
    }
    pub fn set_selected(&mut self, selected: Quality) {
        self.selected = selected;
        self.resolved = if selected == Quality::Auto {
            Quality::Balanced
        } else {
            selected
        };
        self.pressure_seconds = 0.0;
        self.headroom_seconds = 0.0;
    }
    pub fn set_power(&mut self, power: PowerState) {
        let was_constrained = self.power.low_power || self.power.thermal_level >= 2;
        self.power = power;
        self.reconsider();
        let constrained = self.power.low_power || self.power.thermal_level >= 2;
        if constrained != was_constrained {
            self.pressure_seconds = 0.0;
            self.headroom_seconds = 0.0;
        }
        if self.selected == Quality::Auto
            && was_constrained
            && !constrained
            && self.resolved == Quality::Eco
        {
            self.resolved = Quality::Balanced;
        }
    }
    pub fn set_refresh_hz(&mut self, refresh_hz: f32) {
        if refresh_hz.is_finite() {
            self.refresh_hz = refresh_hz.clamp(30.0, 120.0);
        }
    }

    /// Observe Release CPU frame time. Hysteresis prevents quality pumping.
    pub fn observe(&mut self, cpu_seconds: f32, elapsed_seconds: f32) -> bool {
        if self.selected != Quality::Auto {
            return false;
        }
        let dt = elapsed_seconds.clamp(0.0, 0.1);
        let sample = cpu_seconds.clamp(0.0, 0.1);
        self.cpu_ema_seconds += (sample - self.cpu_ema_seconds) * (1.0 - (-dt / 0.75).exp());
        self.frame_ema_seconds += (dt - self.frame_ema_seconds) * (1.0 - (-dt / 0.75).exp());
        let budget = 1.0 / self.refresh_hz;
        if self.cpu_ema_seconds > budget * 0.72 || self.frame_ema_seconds > budget * 1.25 {
            self.pressure_seconds += dt;
            self.headroom_seconds = 0.0;
        } else if self.cpu_ema_seconds < budget * 0.38 {
            self.headroom_seconds += dt;
            self.pressure_seconds = 0.0;
        } else {
            self.pressure_seconds = (self.pressure_seconds - dt).max(0.0);
            self.headroom_seconds = (self.headroom_seconds - dt).max(0.0);
        }
        let before = self.resolved;
        self.reconsider();
        if self.pressure_seconds > 2.0 {
            self.resolved = Quality::Eco;
            self.pressure_seconds = 0.0;
        }
        if self.headroom_seconds > 10.0 && !self.power.low_power && self.power.thermal_level == 0 {
            self.resolved = match self.resolved {
                Quality::Eco => Quality::Balanced,
                Quality::Balanced if self.refresh_hz <= 75.0 => Quality::High,
                tier => tier,
            };
            self.headroom_seconds = 0.0;
        }
        before != self.resolved
    }

    fn reconsider(&mut self) {
        if self.selected != Quality::Auto {
            self.resolved = self.selected;
        } else if self.power.low_power || self.power.thermal_level >= 2 {
            self.resolved = Quality::Eco;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn auto_reacts_to_power_and_recovers() {
        let mut p = AutoQualityPolicy::new(Quality::Auto);
        p.set_power(PowerState {
            low_power: true,
            thermal_level: 0,
        });
        assert_eq!(p.resolved(), Quality::Eco);
        p.set_power(PowerState::default());
        assert_eq!(p.resolved(), Quality::Balanced);
        for _ in 0..599 {
            p.observe(0.0001, 1.0 / 60.0);
        }
        assert_eq!(p.resolved(), Quality::Balanced);
        p.observe(0.0001, 1.0 / 60.0);
        assert_eq!(p.resolved(), Quality::High);
    }
    #[test]
    fn explicit_quality_ignores_auto_pressure() {
        let mut p = AutoQualityPolicy::new(Quality::High);
        p.set_power(PowerState {
            low_power: true,
            thermal_level: 3,
        });
        assert_eq!(p.resolved(), Quality::High);
    }

    #[test]
    fn auto_reacts_to_sustained_missed_frame_budget() {
        let mut p = AutoQualityPolicy::new(Quality::Auto);
        p.set_refresh_hz(120.0);
        for _ in 0..180 {
            p.observe(0.001, 1.0 / 60.0);
        }
        assert_eq!(p.resolved(), Quality::Eco);
    }
}
