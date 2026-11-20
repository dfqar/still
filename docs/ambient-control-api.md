# Generic external light control

The ambient foundation accepts visual state. Sound analysis, feature extraction,
frequency organization, beat detection and the mapping from those results to
light belong to adapters. There are no named audio channels in this API.

Frost accepts one atomic snapshot for its three existing light sources:

| Value | Meaning / accepted range |
| --- | --- |
| `color` | Three sRGB components, clamped to 0…1 |
| `gain` | Per-light intensity multiplier, 0…4; neutral 1 |
| `offset` | XY displacement from the manual position, −2…2; neutral 0 |
| `scale` | Width/height multipliers, 0.05…4; neutral 1 |
| `rotationDegrees` | Rotation offset from the manual angle, −360…360 |
| `brightness` | Global brightness multiplier, 0…4; neutral 1 |

Offsets use normalized viewport coordinates. These overrides never change the
manual rig, selected preset or saved user state. Manual brightness zero still
produces black. Releasing the input restores the current manual palette and
neutral multipliers. Legacy palette/energy input remains supported; the most
recent input style takes control, so the two styles do not accumulate.

## Smoothing in code

`AmbientSmoothing` in Rust and `configureAmbientSmoothing` in Swift expose four
response times, in seconds:

| Setting | Controls | App default |
| --- | --- | ---: |
| `colorSeconds` | Palette transitions | 0.85 |
| `attackSeconds` | Increasing light gains / global brightness / legacy energy | 0.85 |
| `releaseSeconds` | Decreasing light gains / global brightness / legacy energy | 0.85 |
| `transformSeconds` | Position, scale and rotation | 0.85 |

Zero disables smoothing for that property; finite values from 0 to 60 seconds
are accepted. Values are exponential time constants, independent of display
cadence. Settings reject invalid input atomically, retain the current visual
state when changed and apply on subsequent frames. These settings are transient
and do not enter presets or saved state. They are deliberately absent from the
UI. Manual scene slider smoothing remains unchanged.

For example, an adapter can retain a calm palette but make changes in light
intensity respond quickly:

```swift
engine.configureAmbientSmoothing(
    colorSeconds: 0.85,
    attackSeconds: 0.008,
    releaseSeconds: 0.18,
    transformSeconds: 0.05
)

engine.updateAmbient(lights: [
    AmbientLightInput(color: (0.15, 0.55, 1.0), gain: 1.4),
    AmbientLightInput(color: (1.0, 0.25, 0.1), gain: 0.7,
                      offset: (0.08, 0), scale: (1.2, 0.9)),
    AmbientLightInput(color: (0.6, 0.2, 1.0), gain: 1.1,
                      rotationDegrees: 12)
], brightness: 0.9)

// On detach, return to the normal app response and manual rig.
engine.configureAmbientSmoothing()
engine.updateAmbient(lights: nil)
```

The numbers in a snapshot have no prescribed sound meaning. The adapter may
derive them from a detected event, an analyzed envelope, a timeline, or a
different data source. It can set smoothing to zero if it already shapes those
values itself.

## Delivery and compatibility

Swift keeps one latest snapshot and one latest smoothing configuration under
the frame lock. It consumes settings before the snapshot at a render boundary.
Producer traffic cannot build an unbounded render queue. This is a **state
stream**, not a raw-audio or event queue: intermediate snapshots may be replaced.
Adapters own timing/synchronization and must turn short events into visual
envelopes held across render frames. Pausing rendering does not queue an event
history; only the latest state survives.

For direct Rust use, the types are `AmbientControlFrame`, `AmbientLightControl`
and `AmbientSmoothing`, with `Scene::set_ambient_controls` and
`Scene::set_ambient_smoothing`. The engine forwards these to active/outgoing
scenes and reapplies them when scenes are created. Only Frost consumes them.

The additive C ABI v2 entry points are:

```c
still_engine_ambient_controls_update(engine, &frame);
still_engine_ambient_smoothing_set(engine, &settings);
```

`StillAmbientControlFrame` and `StillAmbientSmoothing` include version/size
headers. Null control input releases overrides; null smoothing resets defaults.
Non-finite input and wrong headers reject the entire update. Finite visual
values are clamped to the ranges above. The older `StillAmbientFrame` layout,
`still_engine_ambient_update`, manual descriptors and state schema v3 remain
compatible. C callers retain the engine's existing serial ownership contract.

## Transient scene modulation

`SceneModulation` is a separate overlay for Drift and Frost. The scene catalog
JSON reports `supportsModulation` for each scene. A host supplies a finite,
nonnegative `time_seconds` on its own timeline and three finite controls,
`motion`, `turbulence`, and `light`, clamped to 0…1. Drift uses them for temporary
changes to speed, flow, and light. Frost uses them for changes to the three
light gains, capped at 18%, 14%, and 22%, plus a bounded brightness boost.
The Frost light positions drift independently on display time and
stay fixed at Motion = 0. The overlay is independent of the palette and is
excluded from presets and saved state.

Send `StillModulationFrame` with ABI version 2, its current size, and zero in
the reserved field to `still_engine_modulation_update`. Pass null to clear the
input. Frost uses `releaseSeconds` to fade out the last rendered boost during
continuous animation. The Swift controller keeps only the latest overlay and
checks its caller-supplied expiration before rendering. Direct ABI callers own
the lifetime of their overlay and must clear it when it expires. The metrics
JSON exposes the accepted input as `modulation` and the input used by the latest
presentation of a supported scene as `last_presented_modulation`.

Use `still_engine_frame_tick_animated` for continuous rendering and
`still_engine_frame_tick` for a single paint. Frost starts with the current
palette on its first frame. The first animated frame after resume preserves the
previously rendered palette, then interpolates toward the latest input on
subsequent ticks. A resumed single paint applies the latest palette immediately.
Both functions use ABI v2 and the same bounded display clock.
Scene crossfades advance with display time. Drive a scene switch with continuous
ticks to complete its transition.

## Historical verification from October 2026

The build/test gate includes 53 tests, real Metal preset/quality/resize checks,
native Release compilation and strict signature/artifact checks. Added tests
cover fast attack with slow release, disabled smoothing, 60/120 Hz response
equivalence, input bounds, atomic rejection and exclusion from saved state.
Development-only Metal fixtures exercise zero external brightness and release
back to the untouched manual rig. A separate generic-control render matches an
equivalent manual rig byte for byte, including color, gain, offset, scale,
rotation and global brightness. All four default preset images also match
the preceding Frost output byte for byte at that revision. Local evidence was written to
`build/ambient-controls-2026-10-22/`; older runtime measurements remain tied to
their original binaries in [ambient-foundation.md](ambient-foundation.md).

The local package is Still 2.2, build 5, with a matching arm64 dSYM and verified
ad-hoc signature. Package identity is recorded in `artifact-audit.json`; the
build log, final Clippy log, source manifest and fixture comparisons are stored
in the same evidence folder. The earlier lifecycle limitations remain open;
this code-only API extension does not claim a new full native lifecycle or soak.
