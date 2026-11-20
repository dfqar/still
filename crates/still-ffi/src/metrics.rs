use std::collections::BTreeMap;

const CAPACITY: usize = 4096;

pub struct FrameMetrics {
    cpu: Box<[f32; CAPACITY]>,
    display: Box<[f32; CAPACITY]>,
    index: usize,
    count: usize,
    frames: u64,
    seconds: f64,
    max_gap: f32,
    gaps: u64,
}

impl Default for FrameMetrics {
    fn default() -> Self {
        Self {
            cpu: Box::new([0.0; CAPACITY]),
            display: Box::new([0.0; CAPACITY]),
            index: 0,
            count: 0,
            frames: 0,
            seconds: 0.0,
            max_gap: 0.0,
            gaps: 0,
        }
    }
}

impl FrameMetrics {
    fn record(&mut self, cpu: f32, display: f32) {
        self.cpu[self.index] = cpu;
        self.display[self.index] = display;
        self.index = (self.index + 1) % CAPACITY;
        self.count = (self.count + 1).min(CAPACITY);
        self.frames += 1;
        self.seconds += f64::from(display);
        self.max_gap = self.max_gap.max(display);
        self.gaps += u64::from(display > 0.1);
    }

    fn export(&self) -> serde_json::Value {
        let mut cpu = self.cpu[..self.count].to_vec();
        let mut display: Vec<_> = self.display[..self.count]
            .iter()
            .copied()
            .filter(|v| *v > 0.0)
            .collect();
        cpu.sort_by(f32::total_cmp);
        display.sort_by(f32::total_cmp);
        let percentile = |values: &[f32], fraction: f32| {
            if values.is_empty() {
                0.0
            } else {
                values[((values.len() - 1) as f32 * fraction).round() as usize] * 1000.0
            }
        };
        let sum: f64 = display.iter().map(|v| f64::from(*v)).sum();
        serde_json::json!({
            "sample_count": self.count,
            "presented_frames": self.frames,
            "observed_seconds": self.seconds,
            "lifetime_effective_fps": if self.seconds > 0.0 { self.frames as f64 / self.seconds } else { 0.0 },
            "max_present_gap_ms": self.max_gap * 1000.0,
            "present_gaps_over_100ms": self.gaps,
            "cpu_update_encode_ms": { "p50": percentile(&cpu, 0.5), "p95": percentile(&cpu, 0.95), "p99": percentile(&cpu, 0.99), "max": percentile(&cpu, 1.0) },
            "display_interval_ms": { "p50": percentile(&display, 0.5), "p95": percentile(&display, 0.95), "max": percentile(&display, 1.0) },
            "effective_fps": if sum > 0.0 { display.len() as f64 / sum } else { 0.0 }
        })
    }
}

pub struct Metrics {
    total: FrameMetrics,
    scenes: BTreeMap<&'static str, FrameMetrics>,
    transition_frames: u64,
}

impl Default for Metrics {
    fn default() -> Self {
        Self {
            total: FrameMetrics::default(),
            scenes: still_scenes::SCENE_DESCRIPTORS
                .iter()
                .map(|d| (d.stable_id, FrameMetrics::default()))
                .collect(),
            transition_frames: 0,
        }
    }
}

impl Metrics {
    pub fn record(&mut self, scene: &'static str, cpu: f32, display: f32, transition: bool) {
        self.total.record(cpu, display);
        if transition {
            self.transition_frames += 1;
        } else if let Some(metrics) = self.scenes.get_mut(scene) {
            metrics.record(cpu, display);
        }
    }

    pub fn export(&self) -> serde_json::Value {
        let mut result = self.total.export();
        result["metric_schema_version"] = 2.into();
        result["transition_frames"] = self.transition_frames.into();
        result["scenes"] = self
            .scenes
            .iter()
            .map(|(id, metrics)| ((*id).to_owned(), metrics.export()))
            .collect::<serde_json::Map<_, _>>()
            .into();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_ring_keeps_lifetime_counters_and_uncapped_gaps() {
        let mut m = FrameMetrics::default();
        m.record(0.001, 4.0);
        for _ in 0..5000 {
            m.record(0.0005, 1.0 / 60.0);
        }
        let json = m.export();
        assert_eq!(json["sample_count"], CAPACITY);
        assert_eq!(json["presented_frames"], 5001);
        assert_eq!(json["present_gaps_over_100ms"], 1);
        assert_eq!(json["max_present_gap_ms"], 4000.0);
        assert!(json["observed_seconds"].as_f64().unwrap() > 87.0);
    }

    #[test]
    fn transitions_do_not_pollute_scene_cpu_measurements() {
        let mut m = Metrics::default();
        m.record("drift", 0.001, 1.0 / 60.0, false);
        m.record("fog", 0.050, 1.0 / 30.0, true);
        m.record("fog", 0.002, 1.0 / 60.0, false);
        let json = m.export();
        assert_eq!(json["presented_frames"], 3);
        assert_eq!(json["transition_frames"], 1);
        assert_eq!(json["scenes"]["fog"]["presented_frames"], 1);
        assert_eq!(json["scenes"]["ocean"]["presented_frames"], 0);
        assert!(
            (json["scenes"]["fog"]["cpu_update_encode_ms"]["p95"]
                .as_f64()
                .unwrap()
                - 2.0)
                .abs()
                < 0.001
        );
    }
}
