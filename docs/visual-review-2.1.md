# Still 2.1 visual review — 4 October 2026

The eight Glass and Field presets were rendered after 360 display frames at
60 Hz, using their real Metal pipelines and the production compositor at
1280 × 840 in Balanced. All eight PNGs were inspected. The development-only
catalog performs the readback; the application has no readback path.

| Space | Reviewed presets | Findings |
| --- | --- | --- |
| Glass | Clear Form, Smoked Lens, Liquid Fold, Quiet Prism | Distinct seeded lens/fold compositions, refracted backdrop, restrained studio highlights and dark glass absorption. The bounded exit search removes the optical discontinuity seen during initial development. |
| Field | Soft Current, Contour, Long Form, Woven Silence | Continuous fine flow lines on a dark background; distinct density, curvature and composition. The line geometry is rebuilt from a spatial field each frame. |

Images and raw RGBA outputs are in `build/perf/catalog-glass-field/`.
Per-image hashes and nonblack pixel counts are in
`build/glass-field-2026-10-04/visual-evidence.json`. Rust/WGSL inputs match the
final source manifest. These still images verify preset content and shader
composition; native lifecycle and performance are recorded separately in
[release-report-2.1.md](release-report-2.1.md).

Reproduce with the isolated toolchain used by this checkout:

```sh
CARGO_HOME="$PWD/.toolchains/cargo" RUSTUP_HOME="$PWD/.toolchains/rustup" PATH="$PWD/.toolchains/cargo/bin:$PATH" cargo run --locked --release -p still-scenes --example render-catalog -- build/perf/catalog-glass-field glass,field
python3 scripts/catalog-images.py build/perf/catalog-glass-field
```

Run the catalog separately from performance measurements because it submits
additional GPU work.
