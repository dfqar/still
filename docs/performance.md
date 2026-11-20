# Performance verification

Current Frost/ambient foundation evidence for Still 2.2 is recorded in
[ambient-foundation.md](ambient-foundation.md). Glass/Field measurements for
Still 2.1 remain in [release-report-2.1.md](release-report-2.1.md). The 2.0
results below remain historical evidence tied to their recorded executable.

Still 2.0 results were frozen at the user's request on 1 October 2026.
The all-scenes run finished normally after more than one active hour, but its
cadence/memory gate did not pass. Further native/leak/GPU checks were stopped.
The 26 September Still 1.0 measurements below remain a frozen
baseline and are not final Still 2.0 measurements. Diagnostic environment
settings are inactive in normal launches.

## Test system

| Item | Value |
| --- | --- |
| Model | MacBook Air (Mac17,4) |
| SoC | Apple M5, 10-core CPU, 10-core GPU |
| Memory | 16 GB |
| OS | macOS 26.5.2 (25F84) |
| Xcode | 26.6 (17F113) |
| Build | arm64 Release, stripped, ad-hoc signed |
| Profile | Balanced, 60 Hz, Quality First |

## Still 2.0 measurement method

The final executable SHA-256 is
`75e47c2c7c6857017a575029e777b3c8652d92b1670a558397ece4a1982c545d`.
Each diagnostic run records this identity in `run.json`. The source freeze and
actual artifact audit are under `build/audit-2026-10-01/`; the complete build
gate has 41 passing tests, including actual Metal encoding.

`profile-release.py` launches this executable with isolated diagnostic content,
Balanced quality, 60 Hz, and Quality First. `caffeinate -dim` keeps the display
and system awake. An existing Still instance must be closed first. CPU update
plus encoding excludes drawable acquisition wait; it includes scene work and
the compositor. CPU/cadence percentiles describe the most recent 4,096 samples,
while lifetime counts, seconds, maximum gaps, and FPS cover the entire run.
Scene percentiles exclude crossfade frames. Actual display timestamps are not
capped by the simulation clock.

GPU traces use Instruments Metal System Trace after a 15-second warm-up. Frame
cost is the union of GPU intervals for each presented command-buffer group;
overlapping shader stages are not double-counted. FPS/cadence measured during
tracing may include instrument attachment overhead. The quiet hour soak is
recorded separately without Instruments. One read-only `vmmap -summary` snapshot
at about 23 minutes suspended the target for a recorded 719.97 ms gap; this
external measurement interference is preserved in
`build/perf/verified-hour/measurement-notes.json`. The lifetime cadence includes
it. A separate 3,614.67 ms gap appeared near the end. Its cause has not been
established; it is not attributed to the earlier snapshot.

RSS and process CPU are sampled every five seconds. The native writer samples
`MTLDevice.currentAllocatedSize`, cadence, and scene identity every ten seconds.
Memory maxima are sampled peaks; they do not claim instantaneous transition
peaks. `analyze-release-run.py` excludes the first 300 seconds for hour memory
analysis, removes duplicate cached Metal samples, and uses 15-second guards at
cached-scene boundaries for scene-specific RSS. It reports first/last medians
and slopes so growth cannot be hidden by a recent-frame percentile.

```sh
STILL_REQUIRE_METAL_TESTS=1 scripts/test.sh
python3 scripts/profile-release.py --seconds 3665 --cycle 60 --label hour-check
python3 scripts/analyze-release-run.py build/perf/hour-check
python3 scripts/profile-release.py --seconds 207 --lifecycle --label lifecycle-check
python3 scripts/profile-release.py --scene drift --seconds 75 --trace-seconds 30 --label drift-check
python3 scripts/export-metal-trace.py build/perf/drift-check
python3 scripts/analyze-release-run.py build/perf/drift-check --warmup-seconds 15
```

Repeat the final three commands for `fog` and `ocean`, with unique labels.
Run the diagnostics sequentially. The native lifecycle run exercises real
window operations and injected sleep/wake notifications across all six spaces.
Check that every entry has `passed: true` in `build/perf/lifecycle-check/lifecycle.json`.

## Frozen Still 2.0 hour results

Evidence: `build/perf/verified-hour/run.json`, `metrics.json`, `memory.tsv`,
`analysis.json`, and the empty `stderr.log`. The measured binary matches the
final source/artifact freeze. Balanced, 60 Hz, Quality First; surface
2360 × 1520 device pixels. The application exited normally with status 0.

| Space | Frames excluding crossfade | Lifetime FPS excluding crossfade | CPU update+encode p95 ms | Guarded RSS range MiB | Guarded sampled Metal range MiB | Final-binary GPU p95 |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| Drift | 73,462 | 59.976 | 0.818 | 114.375–129.344 | 77.703–78.547 | Not recaptured |
| Fog | 70,684 | 59.795 | 0.643 | 114.516–125.469 | 51.563–52.406 | Not recaptured |
| Ocean | 70,931 | 59.935 | 0.617 | 114.188–129.188 | 56.813–57.656 | Not recaptured |

Overall: **3,634.226 seconds of observed display time** (60 min 34.2 s),
**217,524 presented frames**, **59.854 lifetime FPS**, and **2,447 crossfade
frames**. Display-interval p50/p95 remained 16.666 ms. Two gaps exceeded 100 ms:
the known 719.97 ms snapshot interruption and the unexplained 3,614.67 ms gap.
The runner's `continuous_hour_pass` is **false**, because the maximum gap
exceeded its 1,000 ms continuity limit. An active hour was exercised; a clean
cadence gate is not claimed.

After excluding 300 seconds of warm-up, RSS ranged **114.156–134.891 MiB**.
The first/last 30-sample medians were **114.359 / 129.305 MiB** (+14.945 MiB);
linear slope **+14.524 MiB/hour**. This residual growth is not explained, and
leak-free or indefinitely stable RAM is not claimed. Metal allocations sampled
over the whole interval ranged **51.563–85.078 MiB**. Per-scene first/last Metal
medians matched (77.703 / 51.563 / 56.813 MiB), supporting bounded scene GPU
allocation in the observed interval. Sampling does not measure every transient
peak.

No final per-scene GPU traces, final-binary native lifecycle rerun, or leak
scan were performed after the user requested this freeze. Earlier traces of
other executable hashes remain developmental evidence, not substitute final
measurements. Further release certification requires investigating the long
gap and RSS trend, then repeating these outstanding checks.

## Still 1.0 frozen baseline — Balanced at 60 Hz

The run used a 30-second Metal System Trace after warm-up. Built-in metrics use
a fixed 4,096-frame ring and measure CPU update plus command encoding without
including drawable acquisition wait. GPU frame time is the union of Still GPU
intervals belonging to every presented command-buffer group, so overlapping
vertex and fragment stages are not double-counted.

| Metric | Result | Release target |
| --- | ---: | ---: |
| Frames in Metal trace | 1,819 | — |
| Effective frame rate | 59.69 fps | 60 fps |
| CPU update + encode p50 | 0.115 ms | — |
| CPU update + encode p95 | 0.586 ms | ≤ 1.5 ms |
| CPU update + encode p99 | 0.729 ms | — |
| GPU frame p50 | 1.740 ms | — |
| GPU frame p95 | 10.986 ms | approximately 10 ms |
| GPU frame p99 | 12.109 ms | — |
| GPU frame maximum | 14.425 ms | — |
| Present interval p50 | 16.655 ms | 16.667 ms |
| Present interval p95 | 17.249 ms | — |
| Metal allocated memory | 72.75–72.75 MiB | bounded |

The GPU p95 is approximately 1 ms above the guidance target but remains below
the 16.67 ms 60 Hz display budget; the measured cadence stayed at 59.69 fps.
One 33 ms display interval occurred in both the application's sample ring and
the trace.

## Historical trace reproduction

Set the app's device preferences to Balanced and 60 Hz, package the Release
build, and launch the executable with:

```sh
STILL_PROFILE_WHEN_INACTIVE=1 \
STILL_METRICS_PATH="$PWD/build/perf/final-balanced-60-metrics.json" \
"$PWD/build/DerivedData/Build/Products/Release/Still.app/Contents/MacOS/Still"
```

Attach Instruments using the Metal System Trace template. Export these tables
from the resulting trace with `xcrun xctrace export`:

- `metal-gpu-intervals`
- `ca-client-present-request`
- `metal-current-allocated-size`

Then run:

```sh
scripts/analyze-metal-trace.py \
  build/perf/final-balanced-60-gpu.xml \
  build/perf/final-balanced-60-presents.xml \
  build/perf/final-balanced-60-metal-memory.xml
```

The executable contains no `map_async`, mapped-buffer, texture-to-buffer copy,
or readback path. Particle buffers are allocated once at High capacity, and
quality changes only alter the active prefix and internal texture budget.

## Historical interrupted soak

The release gate uses `scripts/soak-release.sh 3600 10`. It renders while the
app is diagnostically inactive, samples RSS and CPU every ten seconds, exits
through normal AppKit termination, and writes fixed-ring frame metrics. The
longest uninterrupted active run in this audit presented 83,370 frames at
59.985 fps with CPU p95 0.684 ms (about 23 minutes). The display then entered
system sleep, so this is not represented as a completed one-hour memory soak.
For a full unattended run, keep the display awake (for example with
`caffeinate -dim`) and run the command again.

The interrupted historical run is not used to satisfy the Still 2.0 hour gate.
The final gate requires at least 3,600 seconds of actual display time, all three
scenes exercised, clean normal termination, and review of both RSS and Metal
allocation. Merely requesting a 3,600-second timer does not prove this.

## Limitations

The test system's attached display reports a 60 Hz cadence. The view-linked
display path, 120 Hz fixed-step simulation, and Native cadence option are
implemented, but no 120 Hz hardware measurement is claimed. Before advertising
120 Hz performance, repeat the same profile on a ProMotion or 120 Hz display.
Minimum macOS 14, external-display movement/unplugging, physical system
sleep/wake, and manual VoiceOver navigation are not verified on this host.
Power policy responds to system thermal/low-power state, but no watt or battery
consumption measurement is claimed. See the release checklist for coverage.
