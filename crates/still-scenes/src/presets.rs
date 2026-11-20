use crate::DriftParameters;

pub const DRIFT_SCENE_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug)]
pub struct BuiltinPreset {
    pub id: &'static str,
    pub name: &'static str,
    pub seed: u64,
    pub parameters: DriftParameters,
}

pub const BUILTIN_PRESETS: [BuiltinPreset; 4] = [
    BuiltinPreset {
        id: "builtin.still-water",
        name: "Still Water",
        seed: 0x51_71_11,
        parameters: DriftParameters {
            density: 0.66,
            speed: 0.82,
            flow: 0.72,
            light: 0.58,
        },
    },
    BuiltinPreset {
        id: "builtin.crosswind",
        name: "Crosswind",
        seed: 0xC4_05_5A_1D,
        parameters: DriftParameters {
            density: 0.72,
            speed: 1.08,
            flow: 0.58,
            light: 0.52,
        },
    },
    BuiltinPreset {
        id: "builtin.night-current",
        name: "Night Current",
        seed: 0x91_7A_2C_03,
        parameters: DriftParameters {
            density: 0.84,
            speed: 0.56,
            flow: 1.08,
            light: 0.42,
        },
    },
    BuiltinPreset {
        id: "builtin.quiet-vortex",
        name: "Quiet Vortex",
        seed: 0x70_12_BC_44,
        parameters: DriftParameters {
            density: 0.58,
            speed: 0.74,
            flow: 1.34,
            light: 0.64,
        },
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn builtin_ids_and_names_are_unique() {
        for (i, a) in BUILTIN_PRESETS.iter().enumerate() {
            assert!(a.id.starts_with("builtin."));
            for b in BUILTIN_PRESETS.iter().skip(i + 1) {
                assert_ne!(a.id, b.id);
                assert_ne!(a.name, b.name);
            }
        }
    }
}
