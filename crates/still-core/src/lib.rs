mod ambient;
mod clock;
mod mailbox;
mod modulation;
mod quality;
mod scene;
mod signals;
mod state;

pub use ambient::{
    AmbientControlFrame, AmbientFrame, AmbientLightControl, AmbientSmoothing, srgb_to_linear,
};
pub use clock::{FrameClock, FramePlan};
pub use mailbox::InputMailbox;
pub use modulation::SceneModulation;
pub use quality::{AutoQualityPolicy, PowerState};
pub use scene::{
    SceneDescriptor, SceneParameterDescriptor, SceneParameters, SmoothedSceneParameters,
};
pub use signals::SignalBus;
pub use state::{ContentState, STATE_SCHEMA_VERSION, SceneContentState, StateError, UserPreset};

#[derive(Clone, Copy, Debug, Default)]
pub struct PointerInput {
    pub position: [f32; 2],
    pub velocity: [f32; 2],
    pub active: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum Quality {
    #[default]
    Auto = 0,
    Eco = 1,
    Balanced = 2,
    High = 3,
}

impl Quality {
    pub fn from_raw(raw: u32) -> Self {
        match raw {
            1 => Self::Eco,
            2 => Self::Balanced,
            3 => Self::High,
            _ => Self::Auto,
        }
    }
}
