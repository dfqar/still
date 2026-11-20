use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const STATE_SCHEMA_VERSION: u32 = 3;
const LEGACY_SCENE_ID: &str = "drift";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct UserPreset {
    pub name: String,
    pub seed: u64,
    #[serde(default)]
    pub parameters: BTreeMap<String, f32>,
    pub scene_schema_version: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SceneContentState {
    pub seed: u64,
    #[serde(default)]
    pub parameters: BTreeMap<String, f32>,
    #[serde(default)]
    pub selected_preset: Option<String>,
    #[serde(default)]
    pub user_presets: Vec<UserPreset>,
}

impl SceneContentState {
    pub fn new(
        seed: u64,
        parameters: BTreeMap<String, f32>,
        selected_preset: Option<String>,
    ) -> Self {
        Self {
            seed,
            parameters,
            selected_preset,
            user_presets: vec![],
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ContentState {
    pub schema_version: u32,
    pub selected_scene: String,
    #[serde(default)]
    pub scenes: BTreeMap<String, SceneContentState>,
    #[serde(default)]
    pub modulation_bindings: Vec<serde_json::Value>,
}

impl ContentState {
    pub fn new(
        default_scene_id: &str,
        default_scene: SceneContentState,
    ) -> Result<Self, StateError> {
        if !valid_stable_id(default_scene_id) {
            return Err(StateError::Scene);
        }
        Ok(Self {
            schema_version: STATE_SCHEMA_VERSION,
            selected_scene: default_scene_id.to_owned(),
            scenes: BTreeMap::from([(default_scene_id.to_owned(), default_scene)]),
            modulation_bindings: vec![],
        })
    }

    pub fn encode(&self) -> Result<Vec<u8>, StateError> {
        Ok(serde_json::to_vec_pretty(self)?)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, StateError> {
        let value: serde_json::Value = serde_json::from_slice(bytes)?;
        let version = value
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        if version > u64::from(STATE_SCHEMA_VERSION) {
            return Err(StateError::FutureVersion(version));
        }
        let mut state = if version >= 3 {
            serde_json::from_value(value)?
        } else {
            migrate_legacy(serde_json::from_value(value)?)
        };
        state.validate()?;
        Ok(state)
    }

    pub fn scene(&self, stable_id: &str) -> Option<&SceneContentState> {
        self.scenes.get(stable_id)
    }

    pub fn scene_mut(&mut self, stable_id: &str) -> Option<&mut SceneContentState> {
        self.scenes.get_mut(stable_id)
    }

    pub fn active_scene(&self) -> Option<&SceneContentState> {
        self.scene(&self.selected_scene)
    }

    pub fn active_scene_mut(&mut self) -> Option<&mut SceneContentState> {
        let selected = self.selected_scene.clone();
        self.scene_mut(&selected)
    }

    pub fn ensure_scene(
        &mut self,
        stable_id: &str,
        default: impl FnOnce() -> SceneContentState,
    ) -> Result<&mut SceneContentState, StateError> {
        if !valid_stable_id(stable_id) {
            return Err(StateError::Scene);
        }
        Ok(self
            .scenes
            .entry(stable_id.to_owned())
            .or_insert_with(default))
    }

    pub fn save_user_preset(
        &mut self,
        scene_id: &str,
        name: &str,
        scene_schema_version: u32,
    ) -> Result<(), StateError> {
        let name = sanitize_preset_name(name).ok_or(StateError::PresetName)?;
        let scene = self.scene_mut(scene_id).ok_or(StateError::Scene)?;
        let preset = UserPreset {
            name: name.clone(),
            seed: scene.seed,
            parameters: scene.parameters.clone(),
            scene_schema_version,
        };
        if let Some(existing) = scene
            .user_presets
            .iter_mut()
            .find(|preset| preset.name.eq_ignore_ascii_case(&name))
        {
            *existing = preset;
        } else {
            if scene.user_presets.len() >= 32 {
                return Err(StateError::PresetLimit);
            }
            scene.user_presets.push(preset);
        }
        scene.selected_preset = Some(format!("user.{name}"));
        Ok(())
    }

    pub fn delete_user_preset(&mut self, scene_id: &str, name: &str) -> bool {
        let Some(scene) = self.scene_mut(scene_id) else {
            return false;
        };
        let before = scene.user_presets.len();
        scene
            .user_presets
            .retain(|preset| !preset.name.eq_ignore_ascii_case(name));
        if scene.selected_preset.as_deref().is_some_and(|id| {
            id.strip_prefix("user.")
                .is_some_and(|value| value.eq_ignore_ascii_case(name))
        }) {
            scene.selected_preset = None;
        }
        before != scene.user_presets.len()
    }

    fn validate(&mut self) -> Result<(), StateError> {
        self.schema_version = STATE_SCHEMA_VERSION;
        self.scenes.retain(|id, scene| {
            if !valid_stable_id(id) {
                return false;
            }
            scene
                .parameters
                .retain(|parameter_id, value| valid_stable_id(parameter_id) && value.is_finite());
            scene.user_presets.retain_mut(|preset| {
                preset.name = sanitize_preset_name(&preset.name).unwrap_or_default();
                preset.parameters.retain(|parameter_id, value| {
                    valid_stable_id(parameter_id) && value.is_finite()
                });
                !preset.name.is_empty()
            });
            scene.user_presets.truncate(32);
            true
        });
        if self.scenes.is_empty() {
            return Err(StateError::Scene);
        }
        if !self.scenes.contains_key(&self.selected_scene) {
            self.selected_scene = if self.scenes.contains_key(LEGACY_SCENE_ID) {
                LEGACY_SCENE_ID.to_owned()
            } else {
                self.scenes
                    .keys()
                    .next()
                    .cloned()
                    .ok_or(StateError::Scene)?
            };
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StateError {
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("state schema {0} is newer than this build")]
    FutureVersion(u64),
    #[error("scene identifier is invalid")]
    Scene,
    #[error("preset name must be 1–48 visible characters")]
    PresetName,
    #[error("at most 32 user presets are supported per scene")]
    PresetLimit,
}

#[derive(Clone, Debug, Deserialize)]
struct LegacyUserPreset {
    name: String,
    seed: u64,
    parameters: LegacyParameters,
    scene_schema_version: u32,
}

#[derive(Clone, Debug, Deserialize)]
struct LegacyContentState {
    #[allow(dead_code)]
    #[serde(default)]
    schema_version: u32,
    #[serde(default = "default_scene_id")]
    scene: String,
    seed: u64,
    parameters: LegacyParameters,
    #[serde(default)]
    selected_preset: Option<String>,
    #[serde(default)]
    user_presets: Vec<LegacyUserPreset>,
    #[serde(default)]
    modulation_bindings: Vec<serde_json::Value>,
}

fn default_scene_id() -> String {
    LEGACY_SCENE_ID.to_owned()
}

fn migrate_legacy(legacy: LegacyContentState) -> ContentState {
    let scene_id = if valid_stable_id(&legacy.scene) {
        legacy.scene
    } else {
        LEGACY_SCENE_ID.to_owned()
    };
    let scene = SceneContentState {
        seed: legacy.seed,
        parameters: legacy_parameters(legacy.parameters),
        selected_preset: legacy.selected_preset,
        user_presets: legacy
            .user_presets
            .into_iter()
            .map(|preset| UserPreset {
                name: preset.name,
                seed: preset.seed,
                parameters: legacy_parameters(preset.parameters),
                scene_schema_version: preset.scene_schema_version,
            })
            .collect(),
    };
    ContentState {
        schema_version: STATE_SCHEMA_VERSION,
        selected_scene: scene_id.clone(),
        scenes: BTreeMap::from([(scene_id, scene)]),
        modulation_bindings: legacy.modulation_bindings,
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
struct LegacyParameters {
    density: f32,
    speed: f32,
    flow: f32,
    light: f32,
}

fn legacy_parameters(parameters: LegacyParameters) -> BTreeMap<String, f32> {
    BTreeMap::from([
        ("drift.density".to_owned(), parameters.density),
        ("drift.speed".to_owned(), parameters.speed),
        ("drift.flow".to_owned(), parameters.flow),
        ("drift.light".to_owned(), parameters.light),
    ])
}

fn valid_stable_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 96
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
}

fn sanitize_preset_name(name: &str) -> Option<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 48 || trimmed.chars().any(char::is_control) {
        return None;
    }
    Some(trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_state() -> ContentState {
        ContentState::new(
            "drift",
            SceneContentState::new(
                0x51_71_11,
                BTreeMap::from([
                    ("drift.density".to_owned(), 0.66),
                    ("drift.speed".to_owned(), 0.82),
                    ("drift.flow".to_owned(), 0.72),
                    ("drift.light".to_owned(), 0.58),
                ]),
                Some("builtin.still-water".to_owned()),
            ),
        )
        .unwrap()
    }

    #[test]
    fn round_trip() {
        let state = test_state();
        assert_eq!(
            ContentState::decode(&state.encode().unwrap()).unwrap(),
            state
        );
    }

    #[test]
    fn rejects_corrupt_truncated_and_future() {
        assert!(ContentState::decode(b"{oops").is_err());
        let mut value = serde_json::to_value(test_state()).unwrap();
        value["schema_version"] = 99.into();
        assert!(matches!(
            ContentState::decode(&serde_json::to_vec(&value).unwrap()),
            Err(StateError::FutureVersion(99))
        ));
        value["schema_version"] = (u64::from(u32::MAX) + 4).into();
        assert!(matches!(
            ContentState::decode(&serde_json::to_vec(&value).unwrap()),
            Err(StateError::FutureVersion(_))
        ));
    }

    #[test]
    fn migrates_v0_v1_and_v2_to_per_scene_state() {
        for version in [0, 1, 2] {
            let bytes = format!(
                r#"{{"schema_version":{version},"scene":"drift","seed":9,"parameters":{{"density":0.5,"speed":0.8,"flow":0.7,"light":0.6}}}}"#
            );
            let state = ContentState::decode(bytes.as_bytes()).unwrap();
            assert_eq!(state.schema_version, STATE_SCHEMA_VERSION);
            assert_eq!(state.selected_scene, "drift");
            assert_eq!(
                state.scene("drift").unwrap().parameters["drift.density"],
                0.5
            );
            assert!(state.modulation_bindings.is_empty());
        }
    }

    #[test]
    fn per_scene_state_round_trips_independently() {
        let mut state = test_state();
        state
            .ensure_scene("fog", || SceneContentState {
                seed: 7,
                parameters: BTreeMap::from([("fog.density".to_owned(), 0.72)]),
                selected_preset: Some("builtin.morning-haze".to_owned()),
                user_presets: vec![],
            })
            .unwrap();
        state.selected_scene = "fog".to_owned();
        let decoded = ContentState::decode(&state.encode().unwrap()).unwrap();
        assert_eq!(decoded.selected_scene, "fog");
        assert_eq!(decoded.scene("drift").unwrap().seed, 0x51_71_11);
        assert_eq!(
            decoded.scene("fog").unwrap().parameters["fog.density"],
            0.72
        );
    }

    #[test]
    fn user_preset_round_trip_and_validation() {
        let mut state = test_state();
        state.scene_mut("drift").unwrap().seed = 44;
        state
            .save_user_preset("drift", "  Evening Current  ", 1)
            .unwrap();
        let scene = state.scene("drift").unwrap();
        assert_eq!(scene.user_presets[0].name, "Evening Current");
        assert_eq!(scene.user_presets[0].seed, 44);
        assert!(state.save_user_preset("drift", "\n", 1).is_err());
        let decoded = ContentState::decode(&state.encode().unwrap()).unwrap();
        assert_eq!(decoded, state);
    }
}
