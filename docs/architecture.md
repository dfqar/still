# Architecture decisions

These ADRs include the historical macOS implementation. The current Windows
handoff and accepted platform decisions are in [windows/README.md](windows/README.md).
ADR-020/021 record the ambient and Windows preparation decisions. ADR-022
describes the current native host and modulation behavior.

## ADR-001 — Ownership and teardown

This decision records the original host. See ADR-022 for the current lifecycle.

`MetalHostView` owns its `CAMetalLayer`. `EngineController` owns the opaque Rust
handle. Frame, state, input, and lifecycle calls execute on one serial dispatch
queue; the initial surface attach runs on the AppKit thread while that queue is
quiescent. Teardown first invalidates the view-linked display link,
synchronously drains the queue, detaches the wgpu surface, and only then
destroys the engine. The native layer therefore always outlives
`wgpu::Surface`.

## ADR-002 — Time and presentation

The display link supplies cadence, not simulation time. `FrameClock` advances a
fixed 1/120-second step, performs at most four ticks, discards accumulated time
after stalls or resume, and exposes interpolation alpha. FIFO presentation and
two desired frames of latency keep motion stable without an unbounded queue.
Parameter smoothing is exponential and uses elapsed time, so it is independent
of 60/120 Hz cadence.

## ADR-003 — Scene and compositor boundary

Core owns validated parameters, state codecs, timing, input, quality, and signals.
The GPU lifecycle contract lives in `still-scenes`. Drift owns its compute/render pipelines and scene
resources. The renderer alone owns the instance, adapter, device, queue,
surface, swapchain recovery, and final linear-to-sRGB compositor. A scene
returns a linear color view; it never presents and never imports AppKit.

## ADR-004 — Drift GPU pipeline

At startup Drift allocates capacity for 30,000 particles. A compute pass evolves
only the active prefix using deterministic three-octave curl noise, damping,
wrapping, and pointer wake. An instanced render pass expands procedural capsules
without CPU particle uploads. No production path maps or reads particle buffers.

## ADR-005 — Trails and color

Two capped internal-resolution history textures ping-pong. Each display frame
decays the prior texture using exponential time-based decay, draws particles,
composites the result to the sRGB surface, and swaps. `RGBA16Float` is preferred
when renderable and filterable; `RGBA8Unorm` is the fallback. The compositor
applies exposure and tone mapping exactly once.

## ADR-006 — Bounded steady state

Particle buffers use High-quality capacity from creation. Changing quality only
changes active counts and internal pixel budgets. Bind groups and history
textures are cached and recreated only for resize, format fallback, or device
generation changes. Input and signal delivery use fixed-capacity latest-value
mailboxes. Diagnostic frame metrics use a fixed 4,096-sample ring.

## ADR-007 — State and presets

Stable namespaced parameter IDs cross persistence and ABI boundaries. State
schema v3 stores the selected scene and an independent seed, parameter map,
selected preset, and custom presets for each scene. The decoder migrates schema
v0/v1/v2 Drift documents, clamps invalid values, defaults missing fields, and
rejects future schemas without destroying their source file. Built-in presets
are immutable; user presets are copied into their owning scene record.

## ADR-008 — Signals and automatic quality

`SignalBus` holds time and normalized pointer input delivered through a
fixed-capacity mailbox. Fog and Ocean add their own 0.85-second pointer response.
Each scene has a separate `AutoQualityPolicy` history; switching spaces restores
that scene's resolved tier and hysteresis. Device preferences, power state, and
display refresh apply to all policies. Auto quality observes CPU update/encode
time and the actual display interval, excluding crossfade frames. It lowers the internal budget after sustained misses,
thermal pressure, or low-power state, then promotes only after a longer stable
hysteresis. User-selected Eco/Balanced/High remain explicit.

## ADR-009 — ABI and failure containment

The C ABI is versioned and descriptor-driven; Swift does not duplicate scene
metadata. Opaque handles expose lifecycle, batched input, parameters, presets,
state, quality, power, refresh, events, errors, and metric export. Every entry
point rejects null pointers, invalid configurations, and non-finite inputs,
and contains Rust panics. As with any C ABI, non-null pointers must refer to
valid caller-owned memory and handles must be live and used serially; arbitrary
dangling pointers cannot be made safe by validation. GPU creation is guarded
by validation/out-of-memory error scopes and panic containment. Failed scene
creation preserves the current scene and returns a diagnostic status. Surface
lost/outdated errors reconfigure; timeout skips a frame; out-of-memory suspends
submission. Swift displays an error when initialization or rendering fails.

## ADR-010 — Native lifecycle

This decision records the original display policy. See ADR-022 for static paints.

`CADisplayLink` belongs to the `NSView`, follows the view's current screen, and
updates the engine with that screen's maximum refresh. Notifications cover app
activation, occlusion, miniaturization, sleep/wake, low-power state, and thermal
state. Hidden/inactive views stop submitting frames; resume resets timing and
keeps the previous content state.

## ADR-011 — Still 2.0 multi-scene boundary

The Still 1.0 architectural migration targeted five concrete Drift assumptions
outside the Drift implementation: fixed four-value parameter types in core, particle counts in
the generic quality budget, a flat Drift-only persisted document, an
`Option<Drift>` runtime slot in the ABI layer, and a compositor API shaped
around Drift's two history textures. These are migration targets rather than a
reason to rewrite the working renderer.

Still 2.0 introduces descriptor-indexed `SceneParameters` in core and an
object-safe GPU `Scene` contract in `still-scenes`. A scene owns its parameters,
presets, smoothing, seed, pipelines, simulation, and output texture. The ABI
runtime stores `Box<dyn Scene>` and consumes one neutral linear scene output;
it no longer calls Drift's particle or history API directly. Drift remains the
first registry entry and its existing GPU implementation stays intact behind
`DriftScene`.

The migration is complete: quality budgets and content are per-scene, the
compositor accepts neutral texture views rather than Drift history, and Fog and
Ocean are registry entries beside Drift. No ECS or general render graph was
introduced.

## ADR-012 — Neutral output and bounded transitions

Every scene produces a filterable linear-color texture view plus a stable
source key and an exposure hint. The compositor has two texture bindings and
can present the same output twice for steady state or crossfade two unrelated
outputs. Scene internals may use zero, one, or many compute/render passes.

At a scene switch the runtime retains only the incoming and outgoing scenes for
0.65 seconds. A rapid subsequent switch drops the older outgoing scene and
uses the previously incoming scene as the next outgoing scene, bounding peak
GPU ownership at two scenes. Older outgoing scenes and compositor bindings are
released before the next incoming scene is allocated. Detach, zero-size resize, suspend/resume, quality
changes, and state import explicitly visit or clear both slots.

## ADR-013 — Fog

Fog owns one quality-capped linear render target, a fullscreen pipeline and its
uniform state. Warped 3D value noise builds layered density; a bounded ray loop
integrates extinction and directional scattering. Eco/Balanced/High select
0.45/0.80/1.35 megapixels, 10/14/20 ray steps, and 2/3/4 noise octaves. It has
no particle buffer, compute simulation, or Drift history dependency. The ray
span stays constant across quality tiers and fBm amplitudes use a common
normalization, avoiding brightness/composition changes when Auto adjusts quality.

## ADR-014 — Ocean

Ocean owns an indexed grid, color/depth targets, surface and subdued-sky
pipelines, and uniform state. Both draws share one render pass. The vertex
shader sums directed waves and displaces a perspective-spaced mesh. Fragment
normals use analytic slopes filtered by the pixel's world-space footprint,
followed by restrained Fresnel and specular response. Eco/Balanced/High select 72/128/192 mesh segments,
0.55/0.95/1.55 megapixels, and 3/5/6 waves. Horizon displacement is attenuated
to avoid far-grid aliasing; the mesh extends beyond the view and blends into
the horizon. It has no particle or fullscreen-noise foundation.

## ADR-015 — M0 baseline and removed coupling

The frozen Still 1.0 baseline was the 1,819-frame Balanced/60 trace documented
in `docs/performance.md`: 0.586 ms CPU p95, 10.986 ms GPU p95, 59.69 fps, and
72.75 MiB stable Metal allocation. The audit identified and then removed these
cross-layer assumptions:

- core's fixed Drift parameter struct;
- generic quality budgets expressed as particle counts;
- flat single-scene persistence;
- `Option<Drift>` in the ABI runtime;
- compositor input shaped around two Drift history textures;
- native controls and preset catalog assuming one scene.

Particle buffers, compute dispatch, trails, and vector-field code now remain
inside Drift. Fog and Ocean prove that neither compute, fullscreen rendering,
mesh rendering, nor a fixed pass count is imposed by the runtime.

## ADR-016 — Contract and identifiers

`Scene` exposes descriptors, parameter targets/current values, the preset
catalog, seed/content restoration, quality changes, `prepare(device, viewport)`,
`encode(queue, encoder, FramePlan, PointerInput)`, suspend/resume, and GPU release.
`prepare` handles viewport and quality-dependent reallocation; `encode` handles
scene-specific time and returns an optional `SceneOutput`. No mandatory CPU
simulation callback or fixed pass count exists. The runtime passes device-pixel
dimensions; the native boundary owns Retina scale conversion.

Scene identifiers (`drift`, `fog`, `ocean`) select implementations. Preset
identifiers such as `builtin.still-water` select content within one scene.
Parameters are namespaced (`fog.depth`, `ocean.calm`); Quality chooses a technical
budget and does not select content. Fog/Ocean seed conversion uses SplitMix64
before reducing to a representable shader phase, so adjacent built-in seeds
remain distinct and deterministic.

## ADR-017 — Resource budgets and diagnostics

The following caps describe scene resources at a normal viewport aspect ratio.
Textures are rounded to integer sizes and have a 64-pixel minimum dimension.
There is one common compositor draw and no additional per-scene post-process.

| Space | Eco / Balanced / High | History | Main allocation budget at RGBA16Float |
| --- | --- | --- | --- |
| Drift | 1 / 2 / 4 MP; 10k / 20k / 30k active-particle ceilings | Two color targets | 16 / 32 / 64 MB of history plus fixed 30k-particle capacity |
| Fog | 0.45 / 0.80 / 1.35 MP; 10 / 14 / 20 ray steps; 2 / 3 / 4 octaves | None | 3.6 / 6.4 / 10.8 MB of color |
| Ocean | 0.55 / 0.95 / 1.55 MP; 72 / 128 / 192 grid segments; 3 / 5 / 6 waves | None | 6.6 / 11.4 / 18.6 MB of color+depth; 0.12 / 0.39 / 0.88 MB of indices |

MB here means decimal resource bytes. These are allocation budgets, not caps on
all process/driver/Metal memory; swapchain, alignment, pipelines, and driver
caches are measured separately. Transitions retain at most two complete scenes,
then drop the outgoing scene and replace its compositor binding. No scene cache
retains all three spaces.

Diagnostic metrics retain 4,096 recent samples globally and per scene. Lifetime
frame counts, elapsed display time, maximum gap, and gaps above 100 ms use the
uncapped display timestamp; simulation's capped dt cannot hide a stall.
Crossfade frames contribute to overall cadence but not scene CPU percentiles.
The native diagnostic writer also samples `MTLDevice.currentAllocatedSize`.
These diagnostics are opt-in environment settings; the normal Release UI
contains no profiler controls or readback path.


## ADR-018 — Glass and Field (Still 2.1)

The registry adds `glass` and `field` after the existing three identifiers,
preserving keyboard positions and content identifiers. ABI version 2 and state
schema v3 remain compatible: new records are created lazily on selection, and
all scenes retain independent seeds, parameters, built-in/custom presets and
Auto histories. Native controls are descriptor-driven; no scene schema is
duplicated in Swift.

Glass owns two filterable linear color targets, one uniform buffer and two
fullscreen render pipelines. A procedural backdrop pass is followed by a
3D distance-surface optical pass. The shader traces the incoming ray through a
seeded folded ellipsoid, bisects its exit surface, refracts at both boundaries,
attenuates transmission by thickness and combines Fresnel reflection with a
restrained studio environment. It uses no Drift particles, Fog volume or Ocean
mesh. Form, Motion, Refraction and Light shape the material and composition.

Field owns a fixed 328,320-byte storage buffer containing sampled streamline
geometry, one uniform buffer and one capped color target. One compute invocation
per line integrates a spatial vector field with midpoint steps; an instanced
render pass expands adjacent samples into antialiased ribbon segments. This is
geometry construction every frame, not a persistent particle simulation. No
velocity/age buffer, history target, readback or CPU geometry upload exists.
Density, Motion, Bend and Light control independent content dimensions.

The two adapters share only descriptor/preset/state plumbing. GPU pipelines
and resources remain inside each implementation. The compositor and bounded
0.65-second transition mechanism continue to consume neutral scene outputs.

| Space | Eco / Balanced / High | Main scene resource budgets (RGBA16Float) |
| --- | --- | --- |
| Glass | 0.55 / 0.95 / 1.50 MP; 28 / 44 / 64 entry ray steps; 12 exit bisections | Two color targets: 8.8 / 15.2 / 24 MB |
| Field | 0.75 / 1.40 / 2.20 MP; 128 / 240 / 360 line ceilings; 24 / 40 / 56 segments per line | Color: 6.0 / 11.2 / 17.6 MB; fixed 0.328 MB geometry |

Budgets are decimal scene resource bytes, excluding swapchain and driver
allocations. Target sizes preserve the viewport aspect ratio and can reach
one pixel for tiny surfaces; zero-sized surfaces release targets.
The profiling analyzer and native lifecycle diagnostic derive scene coverage
from the registry. Hour acceptance allocates its minimum per-scene time across
all registered scenes instead of assuming three spaces.

## ADR-019 — Frost and transient media input (Still 2.2)

Frost adds a sixth registry entry without changing the existing scene IDs or
implementations. Two dedicated passes render three independently shaped area
lights and a flat frosted sheet. Each light has sRGB color, intensity, position,
two radii and rotation. Global controls set brightness, motion, diffusion,
frost, grain and the ambient floor. All 33 manual parameters persist through
the existing schema v3 and built-in/custom preset mechanisms.

The scalar descriptor contract stays generic. Swift uses a specialized Frost
presentation to group RGB components into native color pickers and disclose
the shape/material controls. Numeric bounds/defaults still come from Rust.
RGB components are sent in one serial engine operation, preventing a render
tick from observing only part of a picker update.

`still-core::AmbientFrame` defines a three-color sRGB palette and normalized
energy envelope. `Scene::set_ambient_input` is optional; Frost consumes it and
the other scenes ignore it. The runtime keeps the latest input outside saved
content, applies it to newly created scenes and forwards it to active/outgoing
scenes. The additive ABI v2 function `still_engine_ambient_update` validates
whole frames; null releases the override. No state schema or existing C struct
layout changes. The Swift bridge coalesces producer traffic into one slot and
consumes it before frame encoding. Playback/analysis adapters are future work.

Frost smooths palette and energy with a 0.85-second exponential time constant;
clearing the input fades back to the manual palette. Manual brightness, light
geometry/power and material controls remain active with media input. Linear
light conversion happens before illumination is summed. Surface noise stays
stationary, and Motion zero fixes the lights. Scene outputs use the existing
compositor, crossfade and quality/lifecycle infrastructure.

Eco/Balanced/High cap each of the two RGBA16Float targets at 0.65/1.20/2.00 MP
and use 9/17/29 diffusion samples. Main target budgets total 10.4/19.2/32.0 MB,
excluding swapchain/driver memory. Zero size releases targets. Uniform updates
are fixed-size (192 bytes); no history, per-frame heap allocations or readback
are added to rendering. Developer catalog fixtures separately exercise zero
brightness, manual red, a media blue override and release to the manual palette.
Product/API details and current evidence: [ambient-foundation.md](ambient-foundation.md).

## ADR-020 — General external controls and hidden response settings

The ambient foundation accepts source-independent visual snapshots, not named
audio channels. Each of three lights has color, gain, offset, scale and rotation;
a frame adds global brightness. Separate color/attack/release/transform time
constants are configurable in code, remain absent from UI and are transient.
The default is 0.85 seconds; zero disables that property's smoothing. Adapters
own analysis, sound-feature grouping, synchronization and pulse envelopes.

The runtime forwards the latest accepted input/configuration to active/outgoing
scenes and reapplies them on creation. Frost currently consumes this contract;
other scenes may ignore it. Legacy palette/energy input remains compatible;
the most recent input style wins. Manual state and schema v3 are unchanged.
Details: [ambient-control-api.md](ambient-control-api.md).

## ADR-021 — Native Windows host and repository transfer preparation

The accepted Windows direction is C#/.NET 10 LTS, WinUI 3/Windows App SDK,
SwapChainPanel, C ABI/PInvoke and the shared Rust/wgpu engine on DX12. Begin
with x64, SDR, opaque FIFO presentation, existing scenes/WGSL/state/quality,
and serialized CompositionTarget.Rendering callbacks on the UI thread.
Windows owns XAML, windows, lifecycle, native events and file I/O; Rust remains
authoritative for scene state and rendering. Audio analysis stays in adapters.

Native handle creation is isolated under platform cfg in still-renderer.
still-ffi produces both staticlib and cdylib; Windows artifacts use the actual
crate output name still_ffi.dll. Rust-only checks/header generation are separate
from Xcode packaging. Windows engine staging, DLL ABI smoke and CI are prepared.
The old surface_attach export stays AppKit-only and rejects Windows pointers.

This preparation does not implement the Windows host. W0 must prove native
COM lifetime, real Present, a surface-compatible adapter, correct color and DPI
before freezing the Windows surface API. Current create-before-surface ordering,
runtime orchestration in still-ffi and device-loss recovery are explicit porting
work. No speculative wgpu upgrade, scene rewrite or macOS layout move is required.
Full handoff, source reconciliation and acceptance gates: [windows/README.md](windows/README.md).

## ADR-022 — Surface generations and transient visual inputs

The macOS host prepares the graphics device off the main actor and publishes
the controller when it is ready. Retry restores the latest scene state and
rejects stale preparation results. The engine queue is held during the AppKit
surface attach. Each attachment gets a generation so callbacks from retired
views cannot resize, draw on, or detach a replacement surface. Detach is
synchronous while AppKit still owns the layer. Handle destruction runs on the
engine queue and is awaited on app termination.

A visible inactive window receives a bounded paint for initial display and
content revisions. A hidden or occluded window defers that work until reveal.
Continuous frames use the animated C ABI tick. Both modes share the bounded
display clock and the latest pending visual inputs. Frost initializes its
palette from the current input and maintains palette continuity when animation
resumes. Refresh and power updates follow the current surface generation.

The Swift host coalesces transient modulation and drops expired values at the
next paint. Drift and Frost read modulation during encode. The C ABI version
and save schema stay at 2 and 3 respectively. The media host owns the meaning
and lifetime of the inputs. [Generic control API](ambient-control-api.md)
