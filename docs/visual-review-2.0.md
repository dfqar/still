# Still 2.0 visual review — 1 October 2026

All twelve built-in presets were rendered after 360 display frames at 60 Hz,
using the actual Metal scene pipelines and the production compositor at
1280 × 840 in Balanced. The development-only `render-catalog` example copies
the result for inspection; the application contains no readback path.

The final catalog is under `build/perf/catalog-final/`. Its Rust/WGSL inputs
match the final source manifest. Native diagnostic memory-counter changes do
not change these pixels. These images verify shader composition and preset
content, not manual accessibility, native-window interaction, or temporal
behavior on unavailable display hardware.

| Space | Reviewed presets | Findings |
| --- | --- | --- |
| Drift | Still Water, Crosswind, Night Current, Quiet Vortex | Original flowing white trails and curl composition retained. GPU implementation, shaders, parameter definitions, and presets are byte-identical to the starting checkout. |
| Fog | Morning Haze, Dense Silence, Cold Layer, After Rain | Restrained dark density, empty pockets, layered light, distinct seed/composition. Ray distance and density integration stay consistent across quality changes. |
| Ocean | Black Tide, Distant Swell, Moon Channel, Deep Weather | Full-width horizon, restrained glints, surface depth. Extended perspective mesh removes exposed side edges; filtered analytic slopes remove distant facets from the choppier preset. |

During development, native Release windows for the spaces were also inspected.
The final preset review above uses offscreen Metal output: Computer Use did not
grant access to Still for a fresh interactive pass. Manual VoiceOver navigation
remains explicitly unverified in the release checklist.

Reproduce:

```sh
cargo run --locked --release -p still-scenes --example render-catalog -- build/perf/catalog-check
python3 scripts/catalog-images.py build/perf/catalog-check
```

Do not run the catalog during the quiet hour soak or a GPU profile; it submits
additional GPU work and would contaminate performance measurements.
