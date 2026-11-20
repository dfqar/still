# Frost — ambient foundation for a media player

Build 5 extends this foundation with [generic light control and code-only
smoothing](ambient-control-api.md). Its UI and gentle app defaults are preserved;
sound analysis and routing remain in adapters. The build-4 measurements below
retain their original binary identity.

Frost is a full-window matte glass sheet with three independently configurable
area lights behind it. It is the sixth Still space (`frost`, `⌘6`). Existing
spaces retain their renderers and content. Frost presets: Opal, Afterglow,
Blue Hour, Prism.

The native panel exposes three sRGB color pickers, intensity for each light,
and collapsible position/shape controls: horizontal/vertical position, width,
height and rotation. Intensity zero disables that light. Global brightness
zero produces black. Motion zero fixes the lights. Glass controls set
diffusion, frost, grain and a neutral ambient floor. All 33 scalar values,
the seed and custom presets use the existing per-space state schema v3.

## Rendering and quality

Two dedicated Metal/WGSL passes render linear light irradiance and transmission
through the matte sheet. The sheet has stationary surface-normal distortion,
weighted diffusion sampling and restrained static micrograin. Light colors
are converted from sRGB to linear before adding illumination. The shared
compositor performs tone mapping once. There are no particles, CPU geometry,
readback or history targets in the app.

| Quality | Internal target cap | Diffusion samples |
| --- | ---: | ---: |
| Eco | 0.65 MP | 9 |
| Balanced / initial Auto | 1.20 MP | 17 |
| High | 2.00 MP | 29 |

Each scene owns two targets, using filterable Rgba16Float when supported and
Rgba8Unorm otherwise. Resize/quality changes recreate them; zero size releases
them. The existing 0.65-second crossfade, two-scene limit, per-space automatic
quality and display-driven suspension apply to Frost.

## Media adapter boundary

The player adapter supplies an analyzed three-color palette and a normalized
energy envelope. Playback, audio capture, album-art analysis and audio analysis
belong to that future adapter. This change implements the renderer input and
native bridge; it does not yet connect a media source.

The Swift entry point on `EngineController` is:

```swift
engine.updateAmbient(colors: [
    (0.95, 0.24, 0.08),
    (0.12, 0.42, 0.92),
    (0.62, 0.16, 0.88)
], energy: 0.45)

// On media detach/stop: fade back to the saved manual palette.
engine.updateAmbient(colors: nil)
```

Delivery uses one latest-value slot under the existing frame lock. It is
consumed just before the next render tick, so a faster producer cannot build
an unbounded queue, including while rendering is suspended. Colors and energy
are published together. There is no audio-rate callback into Rust.

The additive C ABI v2 function is
`still_engine_ambient_update(engine, const StillAmbientFrame *frame)`.
`StillAmbientFrame` contains the ABI version/size, three RGB triples in
normalized sRGB and one normalized energy value. A null frame releases the
override. Non-finite components or wrong version/size reject the whole update;
finite out-of-range values are clamped. Calls follow the engine's existing
serial ownership contract.

Rust uses `still_core::AmbientFrame` and the optional
`Scene::set_ambient_input` hook. Only Frost consumes it. It follows the
active/outgoing scene during transitions and is reapplied when a scene is
created. Media input is transient: it changes neither manual parameters,
selected preset nor saved content. It holds the latest palette until replaced
or cleared. Color and energy transitions use cadence-independent smoothing
with a time constant of 0.85 seconds, limiting abrupt changes. Energy gently
modulates radiance by up to 35%; manual intensity, shape and material controls
remain in effect.

## Verification and reproduction

The build/test gate covers formatting, Clippy with warnings denied, 50 Rust
tests, actual Metal encoding of all 24 presets in all three tiers, resizing,
zero size/recreation, ABI validation, and per-space persistence. The media
input tests reject invalid frames atomically and verify it never enters saved
state. A regression check also keeps the live palette continuous when manual
presets change. Release Xcode packaging audits arm64, signature and matching dSYM.

```sh
STILL_REQUIRE_METAL_TESTS=1 scripts/test.sh
cargo run --locked --release -p still-scenes --example render-catalog -- build/perf/catalog-frost frost
python3 scripts/catalog-images.py build/perf/catalog-frost
```

The catalog also supports a third argument, limited to Frost: `black`, `red`,
`media-blue`, `media-clear`. Each uses Opal to verify a zero-brightness frame,
manual red colors, a transient blue palette and release back to manual colors.
Those cases render for twelve seconds of simulation; the clear case releases
the override after three seconds. The ordinary preset catalog renders six
seconds. Readback exists only in this development example.

Evidence for this change is stored under `build/frost-2026-10-22/`. All four
presets were visually inspected using actual Metal output and the production
compositor at 1280 × 840 in Balanced. Render fixtures confirmed all-black RGB
at brightness zero, pure red for a manual red rig, a blue-dominant live media
palette and return to the original multicolor rig. Per-image hashes and channel
statistics are in `visual-evidence.json`.

Native UI inspection confirmed the three color wells, grouped shape/material
controls, brightness editing, shape disclosure and rotation editing. It used
isolated diagnostic state; saved user preferences were not modified. Full
interactive editing inside the separate system color panel was not verified.

The six-space native lifecycle exercise completed 50 checks; 45 passed. Five
resume-frame checks failed with `app_active=false` and no renderer error, so
the complete lifecycle gate is not claimed. Selection, fullscreen/windowed,
minimize suspension, rapid switches and close during crossfade were exercised.
That UI/lifecycle evidence belongs to binary SHA-256
`d599fe2faecbb630cbdb9f0076747498f96917472432c088784b1b36666dbfd7`,
before the final Rust-only live-palette continuity correction. Native source
is unchanged by that correction. The log is
`build/perf/lifecycle-2-2-final/lifecycle.json`.

Runtime host: Mac17,4, Apple M5, macOS 27.0.1, 60 Hz. The instrumented
75-second development run measured 59.344 FPS and GPU p95 7.312 ms. It had
three present gaps over 100 ms (max 466.648 ms). A separate 45-second run
without Instruments measured 59.801 FPS, with one 139.481 ms gap. Both exited
normally with empty stderr. These short runs do not certify uninterrupted
cadence, long-term memory behavior, power use or a full release soak.

## Prior local package, build 4 — 22 October 2026

Still 2.2, build 4, arm64, stripped, hardened-runtime ad-hoc signed. The full
build/test gate passed 50 tests (16 core, 10 FFI, 3 renderer, 21 scenes), required
real Metal and passed strict signature/artifact checks. The matching binary
and dSYM UUID is `D8528F93-9A29-39C6-B1BA-4CCDC5BD0805`.

Executable SHA-256:
`e4993696e193a16d45e860865b535675310b2e3560789c572aa011351af5f20a`.
Package: `build/DerivedData/Build/Products/Release/Still.app`.
Build log: `build/frost-2026-10-22/final-gate.log`. Source hashes, preserved
prior-scene GPU content and package audit are recorded in that evidence folder.

The fresh 75-second Balanced run of this exact binary, viewport 2360 × 1520,
presented 4,468 frames at 59.685 FPS. CPU update+encode p95 was 1.146 ms. Its
30-second Metal trace after 15 seconds of warmup included 1,824 GPU frames:
GPU p95 4.647 ms, p99 5.508 ms, maximum 7.818 ms. Sampled Metal memory during
the trace ranged from 50.281 to 78.250 MiB. The process exited normally with
empty stderr. One present gap exceeded 100 ms (121.082 ms), so uninterrupted
cadence is not certified. Raw data: `build/perf/frost-2-2-verified/`.

Full native lifecycle is still unverified as described above. Minimum macOS 14,
hardware 120 Hz, physical sleep/wake, display disconnection, manual VoiceOver
navigation, long soak/leak behavior and public signing/notarization are outside
this extension's verified scope. The local app is ready for manual use; a media
source still needs to be connected by a player adapter.

The older reports for 2.0 and 2.1 retain their original scope and limitations.
