# Still 2.0 release checklist

## Automated gate

Evidence date: 1 October 2026. The source/binary freeze is
`build/audit-2026-10-01/final-source-manifest.json`; gate log is
`build/audit-2026-10-01/final-gate.log`. Current artifact audit is
`build/audit-2026-10-01/final-artifact-audit.json`. Final runtime evidence below
must match that binary before the remaining boxes are checked.

The user requested freezing the current result and stopping further work.
The final hour run returned 0 with no diagnostic errors, but
`continuous_hour_pass=false`: 3,614.67 ms unexplained maximum gap and an
unresolved positive RSS trend. Do not mark the complete runtime release gate
passed. Details and actual measurements are in `docs/performance.md`.

- [x] `cargo fmt --all -- --check`
- [x] `cargo clippy --locked --workspace --all-targets -- -D warnings`
- [x] `STILL_REQUIRE_METAL_TESTS=1 cargo test --locked --workspace` (41 tests; actual Metal integration)
- [x] Generated `include/still.h` from the Rust ABI
- [x] Built an arm64 Release application targeting macOS 14.0
- [x] Compiled and embedded the asset-catalog app icon
- [x] Stripped the application binary
- [x] Generated a dSYM with a UUID matching the application binary
- [x] Verified the local hardened-runtime ad-hoc signature with strict nested-code checking
- [x] Checked that the binary contains no workspace path or external WGSL path
- [ ] Captured and reviewed final per-scene Still 2.0 performance evidence
- [ ] Completed and reviewed the active one-hour all-scenes RSS/CPU/Metal soak
- [x] Captured an active hour on the final binary (3,634.226 display seconds, 217,524 frames); cadence/memory acceptance remains open

Run the complete build gate with `STILL_REQUIRE_METAL_TESTS=1 scripts/test.sh`.
Run the cycling soak with `python3 scripts/profile-release.py --seconds 3665
--cycle 60 --label hour-check`; this runner keeps the display awake, changes
space every 60 seconds, isolates diagnostic content/device settings, and records
the binary identity. Review it with `python3 scripts/analyze-release-run.py
build/perf/hour-check`. Do not check the hour gate from a recent 4,096-frame
percentile: lifetime elapsed display time must itself reach 3,600 seconds.

## Product behavior

- [x] Drift remains a GPU-compute particle scene with its four original presets
- [x] Fog is a standalone non-particle procedural volume with four presets
- [x] Ocean is a standalone displaced mesh surface with four presets
- [x] Native two-level Space/Preset navigation and per-space parameter schema
- [x] Per-space seed, last values, selected preset, and custom presets
- [x] State schema v3 with schema v0/v1/v2 migration and atomic persistence
- [x] Bounded two-output crossfade; outgoing resources released after transition
- [x] Scene-specific Eco/Balanced/High budgets and separate Auto-policy histories
- [x] Pointer response, smoothing, deterministic variations, and display-driven timing
- [x] Hidden, minimized, occluded, inactive, sleep, and wake lifecycle handling
- [x] Zero-size, resize, scale/display move, surface loss/outdated/timeout containment
- [x] VoiceOver labels, native keyboard commands, fullscreen, and Reduce Motion implementation
- [x] Rust panic containment and diagnostic error/event paths across the C ABI
- [x] No ordinary developer/debug UI in the Release product

## Manual coverage

- [x] Real Release Metal visual inspection during development; final shaders/presets reviewed from actual Metal offscreen renders
- [x] Repeated programmatic scene changes covered by registry/runtime tests
- [ ] Final binary native lifecycle diagnostic: resize/fullscreen/minimize/restore, sleep/wake notifications, rapid switches, close during crossfade
- [ ] Manual VoiceOver navigation on release hardware
- [ ] Physical system sleep/wake test (notification-path coverage is tracked separately)
- [ ] External-display move and unplug test
- [ ] Minimum supported macOS 14 launch/persistence smoke test
- [ ] Native-cadence trace on a 120 Hz display

The final shader/preset catalog is reviewed in `docs/visual-review-2.0.md`.
Computer Use did not grant access to Still for a fresh interactive pass;
manual accessibility coverage is not inferred from native labels or offscreen
renders. Deployment target macOS 14 and timing tests at 120 Hz are implementation
facts, not proof of a launch on macOS 14 or measured 120 Hz cadence.

The inherited `block 0.1.6` dependency emits a future-Rust compatibility notice;
the pinned-toolchain Clippy gate passes with warnings denied. Xcode's unused
AppIntents metadata extraction notice does not prevent packaging. No new lint
suppression was added in this finishing pass.

## Distribution work requiring credentials

No valid Developer ID identity is installed on the audit host. `cargo xtask
package` therefore applies a valid ad-hoc signature for local testing. On a
release machine with credentials:

1. Sign with a `Developer ID Application` identity and the hardened runtime.
2. Verify with `codesign --verify --deep --strict --verbose=2`.
3. Submit the archive with `notarytool`.
4. Staple the accepted ticket and validate it with `spctl`.
