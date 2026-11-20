#![allow(clippy::missing_safety_doc)] // Safety contracts are expressed by the generated C header and native RAII wrapper.

use std::{
    collections::{BTreeMap, VecDeque},
    ffi::{CStr, CString, c_char, c_void},
    panic::{AssertUnwindSafe, catch_unwind},
    ptr, slice,
    time::Instant,
};
use still_core::{
    AmbientControlFrame, AmbientFrame, AmbientLightControl, AmbientSmoothing, AutoQualityPolicy,
    ContentState, FrameClock, InputMailbox, PointerInput, PowerState, Quality, SceneModulation,
    SceneParameters, SignalBus,
};
use still_renderer::{Renderer, RendererError};
use still_scenes::{
    DRIFT_DESCRIPTOR, SCENE_DESCRIPTORS, Scene, builtin_preset_content, create_scene,
    descriptor as scene_descriptor, preset_descriptors,
};

mod metrics;
use metrics::Metrics;

pub const STILL_ABI_VERSION: u32 = 2;

#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StillResult {
    Ok = 0,
    InvalidArgument = 1,
    InvalidState = 2,
    Renderer = 3,
    BufferTooSmall = 4,
    Panic = 5,
    NotFound = 6,
}

#[repr(C)]
pub struct StillEngineConfig {
    pub abi_version: u32,
    pub struct_size: u32,
    pub quality: u32,
    pub reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct StillPointerEvent {
    pub abi_version: u32,
    pub struct_size: u32,
    pub x: f32,
    pub y: f32,
    pub velocity_x: f32,
    pub velocity_y: f32,
    pub active: u32,
    pub reserved: u32,
}

#[repr(C)]
pub struct StillParameterDescriptor {
    pub abi_version: u32,
    pub struct_size: u32,
    pub stable_id: *const c_char,
    pub display_name: *const c_char,
    pub minimum: f32,
    pub maximum: f32,
    pub default_value: f32,
    pub smoothing_seconds: f32,
}

#[repr(C)]
pub struct StillSceneDescriptor {
    pub abi_version: u32,
    pub struct_size: u32,
    pub stable_id: *const c_char,
    pub display_name: *const c_char,
    pub schema_version: u32,
    pub parameter_count: u32,
}

/// Latest media palette, in normalized sRGB. This never changes saved presets.
/// Each color is RGB; energy is a normalized analyzed envelope, not audio PCM.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct StillAmbientFrame {
    pub abi_version: u32,
    pub struct_size: u32,
    pub color1: [f32; 3],
    pub color2: [f32; 3],
    pub color3: [f32; 3],
    pub energy: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct StillAmbientLightControl {
    pub color: [f32; 3],
    pub gain: f32,
    pub offset: [f32; 2],
    pub scale: [f32; 2],
    pub rotation_degrees: f32,
}

/// General light state, independent of how a caller derives its values.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct StillAmbientControlFrame {
    pub abi_version: u32,
    pub struct_size: u32,
    pub lights: [StillAmbientLightControl; 3],
    pub brightness: f32,
}

/// Code-only timing in seconds. Zero disables smoothing for that property.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct StillAmbientSmoothing {
    pub abi_version: u32,
    pub struct_size: u32,
    pub color_seconds: f32,
    pub attack_seconds: f32,
    pub release_seconds: f32,
    pub transform_seconds: f32,
}

/// Generic transient scene overlay; the caller owns its time base and origin.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct StillModulationFrame {
    pub abi_version: u32,
    pub struct_size: u32,
    pub time_seconds: f64,
    pub motion: f32,
    pub turbulence: f32,
    pub light: f32,
    pub reserved: u32,
}

pub struct StillEngine {
    renderer: Renderer,
    scene: Option<Box<dyn Scene>>,
    transition: Option<SceneTransition>,
    clock: FrameClock,
    state: ContentState,
    input_mailbox: InputMailbox,
    signals: SignalBus,
    quality: AutoQualityPolicy,
    inactive_quality: BTreeMap<String, AutoQualityPolicy>,
    suspended: bool,
    last_error: Option<CString>,
    events: VecDeque<CString>,
    metrics: Metrics,
    last_present_timestamp: Option<f64>,
    ambient_input: Option<AmbientFrame>,
    ambient_controls: Option<AmbientControlFrame>,
    ambient_smoothing: AmbientSmoothing,
    modulation: Option<SceneModulation>,
    last_presented_modulation: Option<SceneModulation>,
}

const SCENE_TRANSITION_SECONDS: f32 = 0.65;

fn scene_transition_blend(elapsed: f32) -> f32 {
    (elapsed / SCENE_TRANSITION_SECONDS).clamp(0.0, 1.0)
}

struct SceneTransition {
    outgoing: Box<dyn Scene>,
    elapsed: f32,
}

impl StillEngine {
    fn build_scene(&self, id: &str, quality: Quality, seed: u64) -> Result<Box<dyn Scene>, String> {
        use still_renderer::wgpu::ErrorFilter;
        self.renderer
            .device
            .push_error_scope(ErrorFilter::OutOfMemory);
        self.renderer
            .device
            .push_error_scope(ErrorFilter::Validation);
        let scene = catch_unwind(AssertUnwindSafe(|| {
            create_scene(
                id,
                &self.renderer.device,
                &self.renderer.adapter,
                quality,
                seed,
            )
        }));
        let validation = pollster::block_on(self.renderer.device.pop_error_scope());
        let memory = pollster::block_on(self.renderer.device.pop_error_scope());
        if let Some(error) = validation.or(memory) {
            return Err(format!("Cannot initialize {id}: {error}"));
        }
        let mut scene = scene
            .map_err(|_| format!("Cannot initialize {id}: GPU pipeline creation failed"))?
            .ok_or_else(|| format!("Unknown scene {id}"))?;
        scene.set_ambient_input(self.ambient_input);
        if self.ambient_controls.is_some() {
            scene.set_ambient_controls(self.ambient_controls);
        }
        scene.set_ambient_smoothing(self.ambient_smoothing);
        scene.set_modulation(self.modulation);
        Ok(scene)
    }

    fn create(quality: Quality) -> Result<Self, String> {
        let state = default_content_state();
        Ok(Self {
            renderer: Renderer::new().map_err(|e| e.to_string())?,
            scene: None,
            transition: None,
            clock: FrameClock::default(),
            state,
            input_mailbox: InputMailbox::default(),
            signals: SignalBus::default(),
            quality: AutoQualityPolicy::new(quality),
            inactive_quality: SCENE_DESCRIPTORS
                .iter()
                .skip(1)
                .map(|d| (d.stable_id.to_owned(), AutoQualityPolicy::new(quality)))
                .collect(),
            suspended: false,
            last_error: None,
            events: VecDeque::with_capacity(16),
            metrics: Metrics::default(),
            last_present_timestamp: None,
            ambient_input: None,
            ambient_controls: None,
            ambient_smoothing: AmbientSmoothing::default(),
            modulation: None,
            last_presented_modulation: None,
        })
    }

    fn fail(&mut self, result: StillResult, message: impl ToString) -> StillResult {
        let clean = message.to_string().replace('\0', " ");
        self.last_error = CString::new(clean).ok();
        result
    }

    fn event(&mut self, message: impl ToString) {
        if self.events.len() == 16 {
            self.events.pop_front();
        }
        if let Ok(message) = CString::new(message.to_string().replace('\0', " ")) {
            self.events.push_back(message);
        }
    }

    fn apply_frame_boundary_updates(&mut self, dt: f32) {
        self.signals.advance(dt, self.input_mailbox.take());
    }

    fn set_ambient_input(&mut self, input: Option<AmbientFrame>) {
        self.ambient_input = input;
        self.ambient_controls = None;
        if let Some(scene) = &mut self.scene {
            scene.set_ambient_input(input);
        }
        if let Some(transition) = &mut self.transition {
            transition.outgoing.set_ambient_input(input);
        }
    }

    fn set_ambient_controls(&mut self, input: Option<AmbientControlFrame>) {
        self.ambient_input = None;
        self.ambient_controls = input;
        if let Some(scene) = &mut self.scene {
            scene.set_ambient_controls(input);
        }
        if let Some(transition) = &mut self.transition {
            transition.outgoing.set_ambient_controls(input);
        }
    }

    fn set_ambient_smoothing(&mut self, settings: AmbientSmoothing) {
        self.ambient_smoothing = settings;
        if let Some(scene) = &mut self.scene {
            scene.set_ambient_smoothing(settings);
        }
        if let Some(transition) = &mut self.transition {
            transition.outgoing.set_ambient_smoothing(settings);
        }
    }

    fn set_modulation(&mut self, input: Option<SceneModulation>) {
        self.modulation = input;
        if let Some(scene) = &mut self.scene {
            scene.set_modulation(input);
        }
        if let Some(transition) = &mut self.transition {
            transition.outgoing.set_modulation(input);
        }
    }

    fn snapshot_with_pending_parameters(&self) -> ContentState {
        self.state.clone()
    }

    fn apply_content(
        &mut self,
        seed: u64,
        parameters: SceneParameters,
        selected_preset: Option<String>,
    ) {
        let scene_id = self.state.selected_scene.clone();
        let Some(descriptor) = scene_descriptor(&scene_id) else {
            return;
        };
        let Some(content) = self.state.active_scene_mut() else {
            return;
        };
        content.seed = seed;
        content.parameters = parameters.to_persisted(descriptor);
        content.selected_preset = selected_preset.clone();
        if let Some(scene) = &mut self.scene {
            scene.restore_content(seed, parameters, selected_preset);
        }
    }

    fn select_preset(&mut self, id: &str) -> StillResult {
        let scene_id = self.state.selected_scene.clone();
        let Some(descriptor) = scene_descriptor(&scene_id) else {
            return StillResult::InvalidState;
        };
        if let Some((seed, parameters)) = builtin_preset_content(&scene_id, id) {
            self.apply_content(seed, parameters, Some(id.to_owned()));
            self.event(format!("preset-selected:{id}"));
            return StillResult::Ok;
        }
        if let Some(name) = id.strip_prefix("user.")
            && let Some(preset) = self.state.active_scene().and_then(|scene| {
                scene
                    .user_presets
                    .iter()
                    .find(|preset| preset.name.eq_ignore_ascii_case(name))
                    .cloned()
            })
        {
            self.apply_content(
                preset.seed,
                SceneParameters::from_persisted(descriptor, &preset.parameters),
                Some(format!("user.{}", preset.name)),
            );
            self.event(format!("preset-selected:user.{}", preset.name));
            return StillResult::Ok;
        }
        StillResult::NotFound
    }

    fn activate_quality(&mut self, stable_id: &str) {
        if self.state.selected_scene == stable_id {
            return;
        }
        if let Some(next) = self.inactive_quality.remove(stable_id) {
            let previous = std::mem::replace(&mut self.quality, next);
            self.inactive_quality
                .insert(self.state.selected_scene.clone(), previous);
        }
    }

    fn select_scene(&mut self, stable_id: &str) -> StillResult {
        let Some(descriptor) = scene_descriptor(stable_id) else {
            return StillResult::NotFound;
        };
        if self.state.scene(stable_id).is_none() {
            let Some(default) = default_scene_state(stable_id) else {
                return StillResult::InvalidState;
            };
            if let Err(error) = self.state.ensure_scene(stable_id, || default) {
                return self.fail(StillResult::InvalidState, error);
            }
        }
        if self.state.selected_scene == stable_id {
            return StillResult::Ok;
        }
        self.renderer.release_compositor_sources();
        // Drop the older outgoing space before allocating the next one.
        // A rapid switch must never temporarily own three complete GPU scenes.
        if let Some(mut interrupted) = self.transition.take() {
            self.renderer.release_compositor_sources();
            interrupted.outgoing.release_gpu();
        }
        if self.scene.is_some() {
            let Some(content) = self.state.scene(stable_id).cloned() else {
                return StillResult::InvalidState;
            };
            let quality = self
                .inactive_quality
                .get(stable_id)
                .map_or(self.quality.resolved(), AutoQualityPolicy::resolved);
            let mut scene = match self.build_scene(stable_id, quality, content.seed) {
                Ok(scene) => scene,
                Err(error) => return self.fail(StillResult::Renderer, error),
            };
            scene.restore_content(
                content.seed,
                SceneParameters::from_persisted(descriptor, &content.parameters),
                content.selected_preset,
            );
            let outgoing = self.scene.replace(scene);
            self.transition = outgoing.map(|outgoing| SceneTransition {
                outgoing,
                elapsed: 0.0,
            });
        }
        self.activate_quality(stable_id);
        self.state.selected_scene = stable_id.to_owned();
        self.clock.reset();
        self.event(format!("scene-selected:{stable_id}"));
        StillResult::Ok
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn still_abi_version() -> u32 {
    STILL_ABI_VERSION
}

/// Replace a transient visual overlay on the engine's serial owner. Null clears
/// immediately; invalid input is rejected atomically. Saved parameters are untouched.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn still_engine_modulation_update(
    engine: *mut StillEngine,
    frame: *const StillModulationFrame,
) -> StillResult {
    with_engine(engine, |e| {
        if frame.is_null() {
            e.set_modulation(None);
            return StillResult::Ok;
        }
        let frame = unsafe { &*frame };
        if frame.abi_version != STILL_ABI_VERSION
            || frame.struct_size < std::mem::size_of::<StillModulationFrame>() as u32
        {
            return StillResult::InvalidArgument;
        }
        let Some(input) = (SceneModulation {
            time_seconds: frame.time_seconds,
            motion: frame.motion,
            turbulence: frame.turbulence,
            light: frame.light,
        })
        .validated() else {
            return StillResult::InvalidArgument;
        };
        e.set_modulation(Some(input));
        StillResult::Ok
    })
}

/// Atomically replace the transient palette/envelope. A null frame clears it
/// and fades back to manual colors. Call on the same serial queue as the engine.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn still_engine_ambient_update(
    engine: *mut StillEngine,
    frame: *const StillAmbientFrame,
) -> StillResult {
    with_engine(engine, |e| {
        if frame.is_null() {
            e.set_ambient_input(None);
            return StillResult::Ok;
        }
        let frame = unsafe { &*frame };
        if frame.abi_version != STILL_ABI_VERSION
            || frame.struct_size < std::mem::size_of::<StillAmbientFrame>() as u32
        {
            return StillResult::InvalidArgument;
        }
        let Some(input) =
            AmbientFrame::validated([frame.color1, frame.color2, frame.color3], frame.energy)
        else {
            return StillResult::InvalidArgument;
        };
        e.set_ambient_input(Some(input));
        StillResult::Ok
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn still_engine_ambient_controls_update(
    engine: *mut StillEngine,
    frame: *const StillAmbientControlFrame,
) -> StillResult {
    with_engine(engine, |e| {
        if frame.is_null() {
            e.set_ambient_controls(None);
            return StillResult::Ok;
        }
        let frame = unsafe { &*frame };
        if frame.abi_version != STILL_ABI_VERSION
            || frame.struct_size < std::mem::size_of::<StillAmbientControlFrame>() as u32
        {
            return StillResult::InvalidArgument;
        }
        let input = AmbientControlFrame {
            lights: frame.lights.map(|light| AmbientLightControl {
                color: light.color,
                gain: light.gain,
                offset: light.offset,
                scale: light.scale,
                rotation_degrees: light.rotation_degrees,
            }),
            brightness: frame.brightness,
        };
        let Some(input) = input.validated() else {
            return StillResult::InvalidArgument;
        };
        e.set_ambient_controls(Some(input));
        StillResult::Ok
    })
}

/// Set external response times without changing manual scene controls or state.
/// Null resets the application-friendly defaults. Invalid settings are atomic.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn still_engine_ambient_smoothing_set(
    engine: *mut StillEngine,
    settings: *const StillAmbientSmoothing,
) -> StillResult {
    with_engine(engine, |e| {
        if settings.is_null() {
            e.set_ambient_smoothing(AmbientSmoothing::default());
            return StillResult::Ok;
        }
        let settings = unsafe { &*settings };
        if settings.abi_version != STILL_ABI_VERSION
            || settings.struct_size < std::mem::size_of::<StillAmbientSmoothing>() as u32
        {
            return StillResult::InvalidArgument;
        }
        let settings = AmbientSmoothing {
            color_seconds: settings.color_seconds,
            attack_seconds: settings.attack_seconds,
            release_seconds: settings.release_seconds,
            transform_seconds: settings.transform_seconds,
        };
        let Some(settings) = settings.validated() else {
            return StillResult::InvalidArgument;
        };
        e.set_ambient_smoothing(settings);
        StillResult::Ok
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn still_engine_create(
    config: *const StillEngineConfig,
    out_engine: *mut *mut StillEngine,
) -> StillResult {
    ffi(|| {
        if config.is_null() || out_engine.is_null() {
            return StillResult::InvalidArgument;
        }
        unsafe {
            *out_engine = ptr::null_mut();
        }
        let config = unsafe { &*config };
        if config.abi_version != STILL_ABI_VERSION
            || config.struct_size < std::mem::size_of::<StillEngineConfig>() as u32
        {
            return StillResult::InvalidArgument;
        }
        match StillEngine::create(Quality::from_raw(config.quality)) {
            Ok(engine) => {
                unsafe {
                    *out_engine = Box::into_raw(Box::new(engine));
                }
                StillResult::Ok
            }
            Err(error) => {
                eprintln!("Still renderer initialization failed: {error}");
                StillResult::Renderer
            }
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn still_engine_destroy(engine: *mut StillEngine) {
    if !engine.is_null() {
        let _ = catch_unwind(AssertUnwindSafe(|| unsafe {
            drop(Box::from_raw(engine));
        }));
    }
}

/// Attach a retained AppKit NSView with a CAMetalLayer on macOS.
/// This entry point does not accept HWND, IInspectable or SwapChainPanel pointers.
/// Windows panel interop has a separate planned contract; see docs/windows/interop.md.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn still_engine_surface_attach(
    engine: *mut StillEngine,
    ns_view: *mut c_void,
    width: u32,
    height: u32,
) -> StillResult {
    with_engine(engine, |e| {
        if let Err(err) = unsafe { e.renderer.attach(ns_view, width, height) } {
            return e.fail(StillResult::Renderer, err);
        }
        let scene_id = e.state.selected_scene.clone();
        let Some(content) = e.state.active_scene().cloned() else {
            e.renderer.detach();
            return e.fail(StillResult::InvalidState, "selected scene has no state");
        };
        let Some(descriptor) = scene_descriptor(&scene_id) else {
            e.renderer.detach();
            return e.fail(StillResult::InvalidState, "unsupported scene");
        };
        let mut scene = match e.build_scene(&scene_id, e.quality.resolved(), content.seed) {
            Ok(scene) => scene,
            Err(error) => {
                e.scene = None;
                e.renderer.detach();
                return e.fail(StillResult::Renderer, error);
            }
        };
        scene.restore_content(
            content.seed,
            SceneParameters::from_persisted(descriptor, &content.parameters),
            content.selected_preset,
        );
        e.scene = Some(scene);
        e.transition = None;
        e.clock.reset();
        e.event("surface-attached");
        StillResult::Ok
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn still_engine_surface_detach(engine: *mut StillEngine) -> StillResult {
    with_engine(engine, |e| {
        e.scene = None;
        e.transition = None;
        e.renderer.detach();
        e.clock.reset();
        e.event("surface-detached");
        StillResult::Ok
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn still_engine_resize(
    engine: *mut StillEngine,
    width: u32,
    height: u32,
) -> StillResult {
    with_engine(engine, |e| {
        e.renderer.resize(width, height);
        if (width == 0 || height == 0)
            && let Some(scene) = &mut e.scene
        {
            scene.release_gpu();
        }
        if (width == 0 || height == 0)
            && let Some(transition) = &mut e.transition
        {
            transition.outgoing.release_gpu();
        }
        StillResult::Ok
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn still_engine_suspend(engine: *mut StillEngine) -> StillResult {
    with_engine(engine, |e| {
        e.suspended = true;
        e.renderer.set_suspended(true);
        if let Some(scene) = &mut e.scene {
            scene.suspend();
        }
        if let Some(transition) = &mut e.transition {
            transition.outgoing.suspend();
        }
        e.clock.reset();
        e.signals.reset_transients();
        StillResult::Ok
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn still_engine_resume(engine: *mut StillEngine) -> StillResult {
    with_engine(engine, |e| {
        e.suspended = false;
        e.renderer.set_suspended(false);
        if let Some(scene) = &mut e.scene {
            scene.resume();
        }
        if let Some(transition) = &mut e.transition {
            transition.outgoing.resume();
        }
        e.clock.reset();
        StillResult::Ok
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn still_engine_frame_tick(
    engine: *mut StillEngine,
    timestamp_seconds: f64,
) -> StillResult {
    frame_tick(engine, timestamp_seconds, false)
}

/// Continuous display frames preserve palette interpolation across resume.
/// Use the ordinary frame tick for a single inactive-window paint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn still_engine_frame_tick_animated(
    engine: *mut StillEngine,
    timestamp_seconds: f64,
) -> StillResult {
    frame_tick(engine, timestamp_seconds, true)
}

fn frame_tick(engine: *mut StillEngine, timestamp_seconds: f64, animated: bool) -> StillResult {
    with_engine(engine, |e| {
        if !timestamp_seconds.is_finite() {
            return StillResult::InvalidArgument;
        }
        if e.suspended || !e.renderer.is_renderable() {
            return StillResult::Ok;
        }
        let mut plan = e.clock.advance(timestamp_seconds);
        plan.animated = animated;
        let update_start = Instant::now();
        e.apply_frame_boundary_updates(plan.display_dt);
        let update_cpu = update_start.elapsed().as_secs_f32();
        let frame = match e.renderer.acquire() {
            Ok(frame) => frame,
            Err(RendererError::Suspended | RendererError::Detached) => return StillResult::Ok,
            Err(err @ RendererError::Frame(still_renderer::wgpu::SurfaceError::OutOfMemory)) => {
                e.suspended = true;
                e.renderer.set_suspended(true);
                return e.fail(StillResult::Renderer, err);
            }
            Err(err) => return e.fail(StillResult::Renderer, err),
        };
        let encode_start = Instant::now();
        let Some(scene) = &mut e.scene else {
            return StillResult::InvalidState;
        };
        let viewport = e.renderer.size();
        scene.prepare(&e.renderer.device, viewport);
        if let Some(transition) = &mut e.transition {
            transition.outgoing.prepare(&e.renderer.device, viewport);
            transition.elapsed += plan.display_dt.clamp(0.0, 0.1);
        }
        let mut encoder = e.renderer.create_encoder();
        let pointer = e.signals.pointer();
        let in_transition = e.transition.is_some();
        let transition_finished = if let Some(transition) = &mut e.transition {
            let Some(outgoing) =
                transition
                    .outgoing
                    .encode(&e.renderer.queue, &mut encoder, plan, pointer)
            else {
                return StillResult::Ok;
            };
            let Some(incoming) = scene.encode(&e.renderer.queue, &mut encoder, plan, pointer)
            else {
                return StillResult::Ok;
            };
            if let Err(err) = e.renderer.prepare_compositor_sources(
                (outgoing.source_key, outgoing.view),
                (incoming.source_key, incoming.view),
            ) {
                return e.fail(StillResult::Renderer, err);
            }
            let blend = scene_transition_blend(transition.elapsed);
            if let Err(err) = e.renderer.encode_composite(
                &mut encoder,
                &frame.view,
                outgoing.exposure,
                incoming.exposure,
                blend,
            ) {
                return e.fail(StillResult::Renderer, err);
            }
            blend >= 1.0
        } else {
            let Some(output) = scene.encode(&e.renderer.queue, &mut encoder, plan, pointer) else {
                return StillResult::Ok;
            };
            if let Err(err) = e.renderer.prepare_compositor_sources(
                (output.source_key, output.view),
                (output.source_key, output.view),
            ) {
                return e.fail(StillResult::Renderer, err);
            }
            if let Err(err) = e.renderer.encode_composite(
                &mut encoder,
                &frame.view,
                output.exposure,
                output.exposure,
                0.0,
            ) {
                return e.fail(StillResult::Renderer, err);
            }
            false
        };
        e.renderer.queue.submit(Some(encoder.finish()));
        frame.texture.present();
        e.last_presented_modulation = if scene.descriptor().supports_modulation {
            e.modulation
        } else {
            None
        };
        if transition_finished {
            e.renderer.release_compositor_sources();
            if let Some(mut transition) = e.transition.take() {
                transition.outgoing.release_gpu();
            }
            e.event("scene-transition-complete");
        }
        let encode_cpu = encode_start.elapsed().as_secs_f32();
        // Cadence uses the uncapped display timestamp, independent of simulation
        // catch-up limits and resets on scene changes. Long gaps stay visible.
        let display_dt =
            e.last_present_timestamp
                .map_or(0.0, |previous| (timestamp_seconds - previous).max(0.0)) as f32;
        e.last_present_timestamp = Some(timestamp_seconds);
        let scene_id = scene_descriptor(&e.state.selected_scene).map_or("drift", |d| d.stable_id);
        e.metrics
            .record(scene_id, update_cpu + encode_cpu, display_dt, in_transition);
        let before = e.quality.resolved();
        if !in_transition && e.quality.observe(update_cpu + encode_cpu, display_dt) {
            let resolved = e.quality.resolved();
            if let Some(scene) = &mut e.scene {
                scene.set_quality(resolved);
            }
            if let Some(transition) = &mut e.transition {
                transition.outgoing.set_quality(resolved);
            }
            e.event(format!(
                "quality-changed:{}->{}",
                before as u32, resolved as u32
            ));
        }
        StillResult::Ok
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn still_engine_input_batch(
    engine: *mut StillEngine,
    events: *const StillPointerEvent,
    count: usize,
) -> StillResult {
    with_engine(engine, |e| {
        if count == 0 {
            return StillResult::Ok;
        }
        if events.is_null() {
            return StillResult::InvalidArgument;
        }
        for event in unsafe { slice::from_raw_parts(events, count) } {
            if event.abi_version != STILL_ABI_VERSION
                || event.struct_size < std::mem::size_of::<StillPointerEvent>() as u32
            {
                return StillResult::InvalidArgument;
            }
            if [event.x, event.y, event.velocity_x, event.velocity_y]
                .into_iter()
                .all(f32::is_finite)
            {
                e.input_mailbox.push(PointerInput {
                    position: [event.x.clamp(-1.0, 1.0), event.y.clamp(-1.0, 1.0)],
                    velocity: [event.velocity_x, event.velocity_y],
                    active: event.active != 0,
                });
            }
        }
        StillResult::Ok
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn still_engine_parameter_set(
    engine: *mut StillEngine,
    stable_id: *const c_char,
    value: f32,
) -> StillResult {
    with_engine(engine, |e| {
        if !value.is_finite() {
            return StillResult::InvalidArgument;
        }
        let Some(stable_id) = read_string(stable_id) else {
            return StillResult::InvalidArgument;
        };
        let Some(descriptor) = scene_descriptor(&e.state.selected_scene) else {
            return StillResult::InvalidState;
        };
        let Some(parameter) = descriptor
            .parameters
            .iter()
            .find(|parameter| parameter.stable_id == stable_id)
        else {
            return StillResult::InvalidArgument;
        };
        let value = parameter.validate(value);
        let Some(content) = e.state.active_scene_mut() else {
            return StillResult::InvalidState;
        };
        content.parameters.insert(stable_id.to_owned(), value);
        content.selected_preset = None;
        if let Some(scene) = &mut e.scene {
            let _ = scene.set_parameter(stable_id, value);
        }
        StillResult::Ok
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn still_engine_parameter_get(
    engine: *mut StillEngine,
    stable_id: *const c_char,
    out_value: *mut f32,
) -> StillResult {
    with_engine(engine, |e| {
        if out_value.is_null() {
            return StillResult::InvalidArgument;
        }
        let Some(stable_id) = read_string(stable_id) else {
            return StillResult::InvalidArgument;
        };
        let Some(descriptor) = scene_descriptor(&e.state.selected_scene) else {
            return StillResult::InvalidState;
        };
        let Some(parameter) = descriptor
            .parameters
            .iter()
            .find(|parameter| parameter.stable_id == stable_id)
        else {
            return StillResult::InvalidArgument;
        };
        let value = e
            .state
            .active_scene()
            .and_then(|scene| scene.parameters.get(stable_id).copied())
            .unwrap_or(parameter.default_value);
        unsafe {
            *out_value = value;
        }
        StillResult::Ok
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn still_parameter_count() -> usize {
    DRIFT_DESCRIPTOR.parameters.len()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn still_parameter_descriptor(
    index: usize,
    out: *mut StillParameterDescriptor,
) -> StillResult {
    ffi(|| {
        if out.is_null() || index >= DRIFT_DESCRIPTOR.parameters.len() {
            return StillResult::InvalidArgument;
        }
        let descriptor = DRIFT_DESCRIPTOR.parameters[index];
        let ids = [
            c"drift.density".as_ptr(),
            c"drift.speed".as_ptr(),
            c"drift.flow".as_ptr(),
            c"drift.light".as_ptr(),
        ];
        let names = [
            c"Density".as_ptr(),
            c"Speed".as_ptr(),
            c"Flow".as_ptr(),
            c"Light".as_ptr(),
        ];
        unsafe {
            *out = StillParameterDescriptor {
                abi_version: STILL_ABI_VERSION,
                struct_size: std::mem::size_of::<StillParameterDescriptor>() as u32,
                stable_id: ids[index],
                display_name: names[index],
                minimum: descriptor.minimum,
                maximum: descriptor.maximum,
                default_value: descriptor.default_value,
                smoothing_seconds: descriptor.smoothing_seconds,
            };
        }
        StillResult::Ok
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn still_scene_count() -> usize {
    SCENE_DESCRIPTORS.len()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn still_scene_descriptor(
    index: usize,
    out: *mut StillSceneDescriptor,
) -> StillResult {
    ffi(|| {
        if out.is_null() || index >= SCENE_DESCRIPTORS.len() {
            return StillResult::InvalidArgument;
        }
        let descriptor = SCENE_DESCRIPTORS[index];
        let (stable_id, display_name) = match descriptor.stable_id {
            "drift" => (c"drift".as_ptr(), c"Drift".as_ptr()),
            "fog" => (c"fog".as_ptr(), c"Fog".as_ptr()),
            "ocean" => (c"ocean".as_ptr(), c"Ocean".as_ptr()),
            "glass" => (c"glass".as_ptr(), c"Glass".as_ptr()),
            "field" => (c"field".as_ptr(), c"Field".as_ptr()),
            "frost" => (c"frost".as_ptr(), c"Frost".as_ptr()),
            _ => return StillResult::InvalidState,
        };
        unsafe {
            *out = StillSceneDescriptor {
                abi_version: STILL_ABI_VERSION,
                struct_size: std::mem::size_of::<StillSceneDescriptor>() as u32,
                stable_id,
                display_name,
                schema_version: descriptor.schema_version,
                parameter_count: descriptor.parameters.len() as u32,
            };
        }
        StillResult::Ok
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn still_engine_scene_select(
    engine: *mut StillEngine,
    stable_id: *const c_char,
) -> StillResult {
    with_engine(engine, |e| {
        let Some(stable_id) = read_string(stable_id) else {
            return StillResult::InvalidArgument;
        };
        e.select_scene(stable_id)
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn still_engine_scene_catalog_export(
    engine: *mut StillEngine,
    buffer: *mut u8,
    capacity: usize,
    out_length: *mut usize,
) -> StillResult {
    with_engine(engine, |e| {
        let scenes = SCENE_DESCRIPTORS
            .iter()
            .map(|scene| {
                serde_json::json!({
                    "id": scene.stable_id,
                    "name": scene.display_name,
                    "schemaVersion": scene.schema_version,
                    "supportsModulation": scene.supports_modulation,
                })
            })
            .collect::<Vec<_>>();
        let catalog = serde_json::json!({
            "selected": e.state.selected_scene,
            "scenes": scenes,
        });
        match serde_json::to_vec(&catalog) {
            Ok(bytes) => copy_bytes(&bytes, buffer, capacity, out_length),
            Err(error) => e.fail(StillResult::InvalidState, error),
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn still_engine_parameter_catalog_export(
    engine: *mut StillEngine,
    buffer: *mut u8,
    capacity: usize,
    out_length: *mut usize,
) -> StillResult {
    with_engine(engine, |e| {
        let Some(descriptor) = scene_descriptor(&e.state.selected_scene) else {
            return StillResult::InvalidState;
        };
        let parameters = descriptor
            .parameters
            .iter()
            .map(|parameter| {
                serde_json::json!({
                    "id": parameter.stable_id,
                    "name": parameter.display_name,
                    "minimum": parameter.minimum,
                    "maximum": parameter.maximum,
                    "defaultValue": parameter.default_value,
                    "smoothingSeconds": parameter.smoothing_seconds,
                })
            })
            .collect::<Vec<_>>();
        let catalog = serde_json::json!({
            "scene": descriptor.stable_id,
            "parameters": parameters,
        });
        match serde_json::to_vec(&catalog) {
            Ok(bytes) => copy_bytes(&bytes, buffer, capacity, out_length),
            Err(error) => e.fail(StillResult::InvalidState, error),
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn still_engine_new_variation(engine: *mut StillEngine) -> StillResult {
    with_engine(engine, |e| {
        let Some(content) = e.state.active_scene_mut() else {
            return StillResult::InvalidState;
        };
        content.seed = splitmix64(content.seed);
        content.selected_preset = None;
        let seed = content.seed;
        if let Some(scene) = &mut e.scene {
            scene.set_seed(seed);
        }
        e.event("variation-created");
        StillResult::Ok
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn still_engine_quality_set(
    engine: *mut StillEngine,
    quality: u32,
) -> StillResult {
    with_engine(engine, |e| {
        e.quality.set_selected(Quality::from_raw(quality));
        for policy in e.inactive_quality.values_mut() {
            policy.set_selected(Quality::from_raw(quality));
        }
        if let Some(scene) = &mut e.scene {
            scene.set_quality(e.quality.resolved());
        }
        if let Some(transition) = &mut e.transition {
            transition.outgoing.set_quality(e.quality.resolved());
        }
        StillResult::Ok
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn still_engine_display_refresh_set(
    engine: *mut StillEngine,
    refresh_hz: f32,
) -> StillResult {
    with_engine(engine, |e| {
        if !refresh_hz.is_finite() {
            return StillResult::InvalidArgument;
        }
        e.quality.set_refresh_hz(refresh_hz);
        for policy in e.inactive_quality.values_mut() {
            policy.set_refresh_hz(refresh_hz);
        }
        StillResult::Ok
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn still_engine_power_state_set(
    engine: *mut StillEngine,
    low_power: u32,
    thermal_level: u32,
) -> StillResult {
    with_engine(engine, |e| {
        let before = e.quality.resolved();
        let power = PowerState {
            low_power: low_power != 0,
            thermal_level: thermal_level.min(3) as u8,
        };
        e.quality.set_power(power);
        for policy in e.inactive_quality.values_mut() {
            policy.set_power(power);
        }
        if before != e.quality.resolved()
            && let Some(scene) = &mut e.scene
        {
            scene.set_quality(e.quality.resolved());
        }
        if before != e.quality.resolved()
            && let Some(transition) = &mut e.transition
        {
            transition.outgoing.set_quality(e.quality.resolved());
        }
        StillResult::Ok
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn still_engine_preset_select(
    engine: *mut StillEngine,
    preset_id: *const c_char,
) -> StillResult {
    with_engine(engine, |e| {
        let Some(id) = read_string(preset_id) else {
            return StillResult::InvalidArgument;
        };
        e.select_preset(id)
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn still_engine_user_preset_save(
    engine: *mut StillEngine,
    name: *const c_char,
) -> StillResult {
    with_engine(engine, |e| {
        let Some(name) = read_string(name) else {
            return StillResult::InvalidArgument;
        };
        e.state = e.snapshot_with_pending_parameters();
        let scene_id = e.state.selected_scene.clone();
        let Some(descriptor) = scene_descriptor(&scene_id) else {
            return StillResult::InvalidState;
        };
        match e
            .state
            .save_user_preset(&scene_id, name, descriptor.schema_version)
        {
            Ok(()) => {
                e.event(format!("preset-saved:{name}"));
                StillResult::Ok
            }
            Err(err) => e.fail(StillResult::InvalidState, err),
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn still_engine_user_preset_delete(
    engine: *mut StillEngine,
    name: *const c_char,
) -> StillResult {
    with_engine(engine, |e| {
        let Some(name) = read_string(name) else {
            return StillResult::InvalidArgument;
        };
        let scene_id = e.state.selected_scene.clone();
        if e.state.delete_user_preset(&scene_id, name) {
            e.event(format!("preset-deleted:{name}"));
            StillResult::Ok
        } else {
            StillResult::NotFound
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn still_engine_preset_catalog_export(
    engine: *mut StillEngine,
    buffer: *mut u8,
    capacity: usize,
    out_length: *mut usize,
) -> StillResult {
    with_engine(engine, |e| {
        let Some(content) = e.state.active_scene() else {
            return StillResult::InvalidState;
        };
        let Some(presets) = preset_descriptors(&e.state.selected_scene) else {
            return StillResult::InvalidState;
        };
        let builtins = presets.iter().map(|preset| {
            serde_json::json!({
                "id": preset.stable_id,
                "name": preset.display_name,
                "builtin": true
            })
        });
        let users = content.user_presets.iter().map(|preset| serde_json::json!({"id": format!("user.{}", preset.name), "name": preset.name, "builtin": false}));
        let catalog = serde_json::json!({"selected": content.selected_preset, "presets": builtins.chain(users).collect::<Vec<_>>()});
        match serde_json::to_vec(&catalog) {
            Ok(bytes) => copy_bytes(&bytes, buffer, capacity, out_length),
            Err(err) => e.fail(StillResult::InvalidState, err),
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn still_engine_state_export(
    engine: *mut StillEngine,
    buffer: *mut u8,
    capacity: usize,
    out_length: *mut usize,
) -> StillResult {
    with_engine(engine, |e| {
        match e.snapshot_with_pending_parameters().encode() {
            Ok(bytes) => copy_bytes(&bytes, buffer, capacity, out_length),
            Err(err) => e.fail(StillResult::InvalidState, err),
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn still_engine_state_import(
    engine: *mut StillEngine,
    buffer: *const u8,
    length: usize,
) -> StillResult {
    with_engine(engine, |e| {
        if buffer.is_null() || length == 0 {
            return StillResult::InvalidArgument;
        }
        match ContentState::decode(unsafe { slice::from_raw_parts(buffer, length) }) {
            Ok(state) => {
                let state = normalize_supported_state(state);
                if let Some(mut interrupted) = e.transition.take() {
                    e.renderer.release_compositor_sources();
                    interrupted.outgoing.release_gpu();
                }
                let next = if e.scene.is_some() {
                    let id = &state.selected_scene;
                    let Some(content) = state.active_scene() else {
                        return StillResult::InvalidState;
                    };
                    let Some(descriptor) = scene_descriptor(id) else {
                        return StillResult::InvalidState;
                    };
                    let quality = e
                        .inactive_quality
                        .get(id)
                        .map_or(e.quality.resolved(), AutoQualityPolicy::resolved);
                    let mut scene = match e.build_scene(id, quality, content.seed) {
                        Ok(scene) => scene,
                        Err(error) => return e.fail(StillResult::Renderer, error),
                    };
                    scene.restore_content(
                        content.seed,
                        SceneParameters::from_persisted(descriptor, &content.parameters),
                        content.selected_preset.clone(),
                    );
                    Some(scene)
                } else {
                    None
                };
                e.activate_quality(&state.selected_scene);
                e.state = state;
                e.scene = next;
                e.clock.reset();
                StillResult::Ok
            }
            Err(err) => e.fail(StillResult::InvalidState, err),
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn still_engine_metrics_export(
    engine: *mut StillEngine,
    buffer: *mut u8,
    capacity: usize,
    out_length: *mut usize,
) -> StillResult {
    with_engine(engine, |e| {
        let mut metrics = e.metrics.export();
        metrics["active_scene"] = e.state.selected_scene.clone().into();
        metrics["quality_mode"] = (e.quality.selected() as u32).into();
        metrics["quality_tier"] = (e.quality.resolved() as u32).into();
        metrics["viewport_pixels"] = serde_json::json!(e.renderer.size());
        metrics["modulation"] = serde_json::json!(e.modulation);
        metrics["last_presented_modulation"] = serde_json::json!(e.last_presented_modulation);
        match serde_json::to_vec_pretty(&metrics) {
            Ok(bytes) => copy_bytes(&bytes, buffer, capacity, out_length),
            Err(err) => e.fail(StillResult::InvalidState, err),
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn still_engine_error_drain(
    engine: *mut StillEngine,
    buffer: *mut c_char,
    capacity: usize,
    out_length: *mut usize,
) -> StillResult {
    with_engine(engine, |e| {
        drain_message(&mut e.last_error, buffer, capacity, out_length)
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn still_engine_event_drain(
    engine: *mut StillEngine,
    buffer: *mut c_char,
    capacity: usize,
    out_length: *mut usize,
) -> StillResult {
    with_engine(engine, |e| {
        let mut event = e.events.pop_front();
        let result = drain_message(&mut event, buffer, capacity, out_length);
        if result == StillResult::BufferTooSmall
            && let Some(event) = event
        {
            e.events.push_front(event);
        }
        result
    })
}

fn default_scene_state(stable_id: &str) -> Option<still_core::SceneContentState> {
    let descriptor = scene_descriptor(stable_id)?;
    let first_preset = preset_descriptors(stable_id).and_then(|presets| presets.first());
    let (seed, parameters, selected_preset) = if let Some(preset) = first_preset {
        let (seed, parameters) = builtin_preset_content(stable_id, preset.stable_id)?;
        (seed, parameters, Some(preset.stable_id.to_owned()))
    } else {
        (
            splitmix64(stable_id.bytes().fold(0_u64, |hash, byte| {
                hash.wrapping_mul(16_777_619) ^ u64::from(byte)
            })),
            SceneParameters::defaults(descriptor),
            None,
        )
    };
    Some(still_core::SceneContentState::new(
        seed,
        parameters.to_persisted(descriptor),
        selected_preset,
    ))
}

fn default_content_state() -> ContentState {
    ContentState::new(
        "drift",
        default_scene_state("drift").expect("Drift defaults are registered"),
    )
    .expect("the built-in Drift identifier is valid")
}

fn normalize_supported_state(mut state: ContentState) -> ContentState {
    if state.scene("drift").is_none() {
        state.scenes.insert(
            "drift".to_owned(),
            default_scene_state("drift").expect("Drift defaults are registered"),
        );
    }
    if scene_descriptor(&state.selected_scene).is_none()
        || state.scene(&state.selected_scene).is_none()
    {
        state.selected_scene = "drift".to_owned();
    }
    for descriptor in SCENE_DESCRIPTORS {
        if let Some(content) = state.scene_mut(descriptor.stable_id) {
            content.parameters = SceneParameters::from_persisted(&descriptor, &content.parameters)
                .to_persisted(&descriptor);
            for preset in &mut content.user_presets {
                preset.parameters =
                    SceneParameters::from_persisted(&descriptor, &preset.parameters)
                        .to_persisted(&descriptor);
            }
        }
    }
    state
}

fn read_string<'a>(ptr: *const c_char) -> Option<&'a str> {
    if ptr.is_null() {
        None
    } else {
        unsafe { CStr::from_ptr(ptr) }.to_str().ok()
    }
}

fn copy_bytes(
    bytes: &[u8],
    buffer: *mut u8,
    capacity: usize,
    out_length: *mut usize,
) -> StillResult {
    if out_length.is_null() {
        return StillResult::InvalidArgument;
    }
    unsafe {
        *out_length = bytes.len();
    }
    if buffer.is_null() || capacity < bytes.len() {
        return StillResult::BufferTooSmall;
    }
    unsafe {
        ptr::copy_nonoverlapping(bytes.as_ptr(), buffer, bytes.len());
    }
    StillResult::Ok
}

fn drain_message(
    slot: &mut Option<CString>,
    buffer: *mut c_char,
    capacity: usize,
    out_length: *mut usize,
) -> StillResult {
    if out_length.is_null() {
        return StillResult::InvalidArgument;
    }
    let Some(message) = slot.take() else {
        unsafe {
            *out_length = 0;
        }
        return StillResult::Ok;
    };
    let bytes = message.as_bytes_with_nul();
    unsafe {
        *out_length = bytes.len();
    }
    if buffer.is_null() || capacity < bytes.len() {
        *slot = Some(message);
        return StillResult::BufferTooSmall;
    }
    unsafe {
        ptr::copy_nonoverlapping(bytes.as_ptr().cast(), buffer, bytes.len());
    }
    StillResult::Ok
}

fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e3779b97f4a7c15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z ^ (z >> 31)
}
fn ffi(f: impl FnOnce() -> StillResult) -> StillResult {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(StillResult::Panic)
}
fn with_engine(
    engine: *mut StillEngine,
    f: impl FnOnce(&mut StillEngine) -> StillResult,
) -> StillResult {
    if engine.is_null() {
        return StillResult::InvalidArgument;
    }
    let engine = unsafe { &mut *engine };
    match catch_unwind(AssertUnwindSafe(|| f(engine))) {
        Ok(result) => result,
        Err(_) => {
            let _ = catch_unwind(AssertUnwindSafe(|| {
                engine.renderer.recover_after_frame_failure()
            }));
            engine.fail(
                StillResult::Panic,
                "Rust panic contained at the C ABI boundary",
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modulation_abi_is_atomic_transient_and_independent_of_manual_state() {
        let Ok(mut engine) = StillEngine::create(Quality::Eco) else {
            assert_ne!(
                std::env::var("STILL_REQUIRE_METAL_TESTS").as_deref(),
                Ok("1")
            );
            return;
        };
        let saved = serde_json::to_vec(&engine.snapshot_with_pending_parameters()).unwrap();
        let mut frame = StillModulationFrame {
            abi_version: STILL_ABI_VERSION,
            struct_size: std::mem::size_of::<StillModulationFrame>() as u32,
            time_seconds: 12.25,
            motion: 0.8,
            turbulence: 0.3,
            light: 0.6,
            reserved: 0,
        };
        unsafe {
            assert_eq!(
                still_engine_modulation_update(&mut engine, &frame),
                StillResult::Ok
            );
            let accepted = engine.modulation;
            frame.motion = f32::NAN;
            assert_eq!(
                still_engine_modulation_update(&mut engine, &frame),
                StillResult::InvalidArgument
            );
            assert_eq!(engine.modulation, accepted);
            frame.motion = 0.5;
            frame.struct_size = 0;
            assert_eq!(
                still_engine_modulation_update(&mut engine, &frame),
                StillResult::InvalidArgument
            );
            assert_eq!(engine.modulation, accepted);
            assert_eq!(
                serde_json::to_vec(&engine.snapshot_with_pending_parameters()).unwrap(),
                saved
            );
            assert_eq!(
                still_engine_modulation_update(&mut engine, ptr::null()),
                StillResult::Ok
            );
            assert!(engine.modulation.is_none());
        }
        assert_eq!(
            SCENE_DESCRIPTORS
                .iter()
                .filter(|s| s.supports_modulation)
                .map(|s| s.stable_id)
                .collect::<Vec<_>>(),
            vec!["drift", "frost"]
        );
    }

    #[test]
    fn registries_have_presets_and_default_state_is_legacy_compatible() {
        let state = default_content_state();
        for scene in SCENE_DESCRIPTORS {
            assert_eq!(preset_descriptors(scene.stable_id).unwrap().len(), 4);
            assert!(default_scene_state(scene.stable_id).is_some());
        }
        assert_eq!(state.selected_scene, "drift");
        assert!(state.scene("drift").unwrap().user_presets.is_empty());
    }

    #[test]
    fn native_scene_descriptor_catalog_matches_registry() {
        assert_eq!(still_scene_count(), 6);
        for (index, expected) in SCENE_DESCRIPTORS.iter().enumerate() {
            let mut descriptor = std::mem::MaybeUninit::<StillSceneDescriptor>::uninit();
            unsafe {
                assert_eq!(
                    still_scene_descriptor(index, descriptor.as_mut_ptr()),
                    StillResult::Ok
                );
                let actual = descriptor.assume_init();
                assert_eq!(
                    CStr::from_ptr(actual.stable_id).to_str().unwrap(),
                    expected.stable_id
                );
                assert_eq!(
                    CStr::from_ptr(actual.display_name).to_str().unwrap(),
                    expected.display_name
                );
                assert_eq!(actual.parameter_count as usize, expected.parameters.len());
            }
        }
    }

    #[test]
    fn scenes_can_switch_rapidly_before_surface_attachment() {
        let Ok(mut engine) = StillEngine::create(Quality::Balanced) else {
            return;
        };
        for id in [
            "fog", "ocean", "glass", "field", "frost", "drift", "field", "glass", "frost",
        ] {
            assert_eq!(engine.select_scene(id), StillResult::Ok);
            assert_eq!(engine.state.selected_scene, id);
            assert!(engine.state.scene(id).is_some());
            assert!(engine.transition.is_none());
        }
        assert_eq!(engine.select_scene("unknown"), StillResult::NotFound);
    }

    #[test]
    fn transition_blend_is_bounded_and_monotonic() {
        assert_eq!(scene_transition_blend(-1.0), 0.0);
        assert_eq!(scene_transition_blend(0.0), 0.0);
        assert!(scene_transition_blend(0.2) < scene_transition_blend(0.4));
        assert_eq!(scene_transition_blend(SCENE_TRANSITION_SECONDS), 1.0);
        assert_eq!(scene_transition_blend(5.0), 1.0);
    }

    #[test]
    fn auto_quality_history_belongs_to_each_scene() {
        let Ok(mut engine) = StillEngine::create(Quality::Auto) else {
            return;
        };
        for _ in 0..200 {
            engine.quality.observe(0.040, 1.0 / 30.0);
        }
        assert_eq!(engine.quality.resolved(), Quality::Eco);
        assert_eq!(engine.select_scene("fog"), StillResult::Ok);
        assert_eq!(engine.quality.resolved(), Quality::Balanced);
        assert_eq!(engine.select_scene("ocean"), StillResult::Ok);
        assert_eq!(engine.quality.resolved(), Quality::Balanced);
        assert_eq!(engine.select_scene("drift"), StillResult::Ok);
        assert_eq!(engine.quality.resolved(), Quality::Eco);
        unsafe {
            still_engine_quality_set(&mut engine, Quality::High as u32);
        }
        engine.select_scene("fog");
        assert_eq!(engine.quality.resolved(), Quality::High);
    }

    #[test]
    fn ffi_rejects_null_handles_invalid_configs_and_nonfinite_inputs() {
        unsafe {
            assert_eq!(
                still_engine_frame_tick(ptr::null_mut(), 0.0),
                StillResult::InvalidArgument
            );
            assert_eq!(
                still_engine_scene_select(ptr::null_mut(), ptr::null()),
                StillResult::InvalidArgument
            );
            still_engine_destroy(ptr::null_mut());
            let config = StillEngineConfig {
                abi_version: 0,
                struct_size: 0,
                quality: 0,
                reserved: 0,
            };
            let mut handle = ptr::dangling_mut();
            assert_eq!(
                still_engine_create(&config, &mut handle),
                StillResult::InvalidArgument
            );
            assert!(handle.is_null());
            let Ok(mut engine) = StillEngine::create(Quality::Balanced) else {
                return;
            };
            assert_eq!(
                still_engine_frame_tick(&mut engine, f64::NAN),
                StillResult::InvalidArgument
            );
            assert_eq!(
                still_engine_display_refresh_set(&mut engine, f32::INFINITY),
                StillResult::InvalidArgument
            );
            let parameter = c"drift.density";
            assert_eq!(
                still_engine_parameter_set(&mut engine, parameter.as_ptr(), f32::NAN),
                StillResult::InvalidArgument
            );
            assert_eq!(
                still_engine_state_import(&mut engine, ptr::null(), 10),
                StillResult::InvalidArgument
            );
            assert_eq!(
                still_engine_input_batch(&mut engine, ptr::null(), 1),
                StillResult::InvalidArgument
            );
        }
    }

    #[test]
    fn ffi_roundtrip_preserves_independent_presets_parameters_and_seeds() {
        let Ok(mut engine) = StillEngine::create(Quality::Balanced) else {
            return;
        };
        engine.select_scene("fog");
        engine.select_preset("builtin.after-rain");
        let fog = engine.state.active_scene().unwrap().clone();
        engine.select_scene("ocean");
        engine.select_preset("builtin.moon-channel");
        let ocean = engine.state.active_scene().unwrap().clone();
        engine.select_scene("glass");
        engine.select_preset("builtin.liquid-fold");
        engine
            .state
            .save_user_preset("glass", "Evening Glass", 1)
            .unwrap();
        let glass = engine.state.active_scene().unwrap().clone();
        engine.select_scene("field");
        engine.select_preset("builtin.woven-silence");
        engine
            .state
            .save_user_preset("field", "Evening Field", 1)
            .unwrap();
        let field = engine.state.active_scene().unwrap().clone();
        engine.select_scene("frost");
        engine.select_preset("builtin.afterglow");
        unsafe {
            assert_eq!(
                still_engine_parameter_set(&mut engine, c"frost.light2.red".as_ptr(), 0.43),
                StillResult::Ok
            );
            assert_eq!(
                still_engine_parameter_set(&mut engine, c"frost.light3.rotation".as_ptr(), 173.0),
                StillResult::Ok
            );
        }
        engine
            .state
            .save_user_preset("frost", "Evening Frost", 1)
            .unwrap();
        let frost = engine.state.active_scene().unwrap().clone();
        engine.select_scene("drift");
        engine.select_preset("builtin.crosswind");
        let bytes = engine.state.encode().unwrap();
        unsafe {
            assert_eq!(
                still_engine_state_import(&mut engine, bytes.as_ptr(), bytes.len()),
                StillResult::Ok
            );
        }
        assert_eq!(engine.state.scene("fog"), Some(&fog));
        assert_eq!(engine.state.scene("ocean"), Some(&ocean));
        assert_eq!(engine.state.scene("glass"), Some(&glass));
        assert_eq!(engine.state.scene("field"), Some(&field));
        assert_eq!(engine.state.scene("frost"), Some(&frost));
        engine.select_scene("frost");
        assert_eq!(engine.state.active_scene(), Some(&frost));
        engine.select_scene("glass");
        assert_eq!(engine.state.active_scene(), Some(&glass));
        engine.select_scene("field");
        assert_eq!(engine.state.active_scene(), Some(&field));
        engine.select_scene("fog");
        assert_eq!(engine.state.active_scene(), Some(&fog));
        assert_eq!(
            engine.select_preset("builtin.moon-channel"),
            StillResult::NotFound
        );
    }

    #[test]
    fn ambient_input_is_transient_atomic_and_can_be_cleared() {
        let Ok(mut engine) = StillEngine::create(Quality::Balanced) else {
            return;
        };
        engine.select_scene("frost");
        let saved = engine.state.encode().unwrap();
        let mut input = StillAmbientFrame {
            abi_version: STILL_ABI_VERSION,
            struct_size: std::mem::size_of::<StillAmbientFrame>() as u32,
            color1: [1.0, 0.0, 0.2],
            color2: [0.2, 0.6, 1.0],
            color3: [0.6, 1.0, 0.2],
            energy: 0.75,
        };
        unsafe {
            assert_eq!(
                still_engine_ambient_update(&mut engine, &input),
                StillResult::Ok
            );
            let previous = engine.ambient_input;
            input.color2[1] = f32::NAN;
            assert_eq!(
                still_engine_ambient_update(&mut engine, &input),
                StillResult::InvalidArgument
            );
            assert_eq!(engine.ambient_input, previous);
            input.color2[1] = 0.5;
            input.struct_size = 4;
            assert_eq!(
                still_engine_ambient_update(&mut engine, &input),
                StillResult::InvalidArgument
            );
            assert_eq!(engine.ambient_input, previous);
            assert_eq!(engine.state.encode().unwrap(), saved);
            assert_eq!(
                still_engine_ambient_update(&mut engine, ptr::null()),
                StillResult::Ok
            );
            assert!(engine.ambient_input.is_none());
            assert_eq!(engine.state.encode().unwrap(), saved);
            assert_eq!(
                still_engine_ambient_update(ptr::null_mut(), &input),
                StillResult::InvalidArgument
            );
        }
    }

    #[test]
    fn generic_light_controls_and_response_settings_are_atomic_and_transient() {
        let Ok(mut engine) = StillEngine::create(Quality::Balanced) else {
            return;
        };
        engine.select_scene("frost");
        let saved = engine.state.encode().unwrap();
        let light = StillAmbientLightControl {
            color: [0.5, 0.2, 1.0],
            gain: 1.4,
            offset: [-0.1, 0.2],
            scale: [1.2, 0.8],
            rotation_degrees: 15.0,
        };
        let mut frame = StillAmbientControlFrame {
            abi_version: STILL_ABI_VERSION,
            struct_size: std::mem::size_of::<StillAmbientControlFrame>() as u32,
            lights: [light; 3],
            brightness: 0.7,
        };
        let mut settings = StillAmbientSmoothing {
            abi_version: STILL_ABI_VERSION,
            struct_size: std::mem::size_of::<StillAmbientSmoothing>() as u32,
            color_seconds: 0.8,
            attack_seconds: 0.008,
            release_seconds: 0.2,
            transform_seconds: 0.05,
        };
        unsafe {
            assert_eq!(
                still_engine_ambient_controls_update(&mut engine, &frame),
                StillResult::Ok
            );
            let previous = engine.ambient_controls;
            frame.lights[1].gain = f32::NAN;
            assert_eq!(
                still_engine_ambient_controls_update(&mut engine, &frame),
                StillResult::InvalidArgument
            );
            assert_eq!(engine.ambient_controls, previous);
            assert_eq!(
                still_engine_ambient_smoothing_set(&mut engine, &settings),
                StillResult::Ok
            );
            let previous = engine.ambient_smoothing;
            settings.attack_seconds = -1.0;
            assert_eq!(
                still_engine_ambient_smoothing_set(&mut engine, &settings),
                StillResult::InvalidArgument
            );
            assert_eq!(engine.ambient_smoothing, previous);
            settings.attack_seconds = 0.01;
            settings.struct_size = 4;
            assert_eq!(
                still_engine_ambient_smoothing_set(&mut engine, &settings),
                StillResult::InvalidArgument
            );
            frame.struct_size = 4;
            assert_eq!(
                still_engine_ambient_controls_update(&mut engine, &frame),
                StillResult::InvalidArgument
            );
            assert_eq!(engine.state.encode().unwrap(), saved);
            assert_eq!(
                still_engine_ambient_controls_update(&mut engine, ptr::null()),
                StillResult::Ok
            );
            assert_eq!(
                still_engine_ambient_smoothing_set(&mut engine, ptr::null()),
                StillResult::Ok
            );
            assert!(engine.ambient_controls.is_none());
            assert_eq!(engine.ambient_smoothing, AmbientSmoothing::default());
            assert_eq!(engine.state.encode().unwrap(), saved);
            assert_eq!(
                still_engine_ambient_smoothing_set(ptr::null_mut(), ptr::null()),
                StillResult::InvalidArgument
            );
        }
    }
}
