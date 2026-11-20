/// Mix the entire persisted seed before converting to a GPU float. Converting
/// a u32 directly rounded neighboring preset seeds to the same float phase.
pub(crate) fn seed_phase(seed: u64) -> f32 {
    let mut x = seed.wrapping_add(0x9e3779b97f4a7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    x ^= x >> 31;
    (x >> 40) as f32 / 16_777_216.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neighboring_preset_seeds_have_distinct_repeatable_phases() {
        for start in [0xF0_61_10_01, 0x0C_EA_10_01] {
            let phases: Vec<_> = (start..start + 4).map(seed_phase).collect();
            for (i, phase) in phases.iter().enumerate() {
                assert!((0.0..1.0).contains(phase));
                assert_eq!(*phase, seed_phase(start + i as u64));
                assert!(!phases[..i].contains(phase));
            }
        }
    }
}
