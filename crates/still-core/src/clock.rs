#[derive(Clone, Copy, Debug, Default)]
pub struct FramePlan {
    /// Continuous presentation may interpolate the first palette after resume.
    pub animated: bool,
    pub ticks: u32,
    pub fixed_dt: f32,
    pub interpolation: f32,
    pub display_dt: f32,
}

#[derive(Debug)]
pub struct FrameClock {
    fixed: f64,
    accumulator: f64,
    last: Option<f64>,
    max_ticks: u32,
}

impl Default for FrameClock {
    fn default() -> Self {
        Self::new(1.0 / 120.0, 4)
    }
}

impl FrameClock {
    pub fn new(fixed: f64, max_ticks: u32) -> Self {
        Self {
            fixed,
            accumulator: 0.0,
            last: None,
            max_ticks,
        }
    }

    pub fn reset(&mut self) {
        self.accumulator = 0.0;
        self.last = None;
    }

    pub fn advance(&mut self, timestamp: f64) -> FramePlan {
        let Some(last) = self.last.replace(timestamp) else {
            return FramePlan {
                fixed_dt: self.fixed as f32,
                ..Default::default()
            };
        };
        let raw = (timestamp - last).max(0.0);
        // A stall never causes a long catch-up. Four fixed ticks is the hard cap.
        let display_dt = raw.min(self.fixed * self.max_ticks as f64);
        if raw > 0.25 {
            self.accumulator = 0.0;
        }
        self.accumulator += display_dt;
        let ticks = ((self.accumulator / self.fixed) as u32).min(self.max_ticks);
        self.accumulator -= ticks as f64 * self.fixed;
        FramePlan {
            animated: false,
            ticks,
            fixed_dt: self.fixed as f32,
            interpolation: (self.accumulator / self.fixed) as f32,
            display_dt: display_dt as f32,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simulation_speed_matches_60_and_120_hz() {
        fn ticks(hz: u32) -> u32 {
            let mut c = FrameClock::default();
            let mut total = 0;
            c.advance(0.0);
            for i in 1..=hz {
                total += c.advance(i as f64 / hz as f64).ticks;
            }
            total
        }
        assert!((ticks(60) as i32 - 120).abs() <= 1);
        assert!((ticks(120) as i32 - 120).abs() <= 1);
    }

    #[test]
    fn stall_is_bounded() {
        let mut c = FrameClock::default();
        c.advance(0.0);
        assert!(c.advance(10.0).ticks <= 4);
    }

    #[test]
    fn resume_after_sleep_discards_wall_clock_gap() {
        let mut c = FrameClock::default();
        c.advance(10.0);
        assert!(c.advance(10.0 + 1.0 / 60.0).ticks <= 2);

        // suspend/resume resets the clock before the first post-wake callback.
        c.reset();
        let first = c.advance(3_610.0);
        assert_eq!(first.ticks, 0);
        assert_eq!(first.display_dt, 0.0);
        assert!(c.advance(3_610.0 + 1.0 / 60.0).ticks <= 2);
    }
}
