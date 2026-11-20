# Still

Still is an ambient app for Apple Silicon Macs. It renders six procedural
spaces in real time and gives you controls for their motion, color, and
texture. Save your own presets or generate a new seeded variation.

## Build and run

Use an Apple Silicon Mac, Xcode 16 or later with the macOS SDK, and Rust
installed through rustup. [The toolchain file](rust-toolchain.toml) pins the Rust
version. The app targets macOS 14 and later.

```sh
git clone https://github.com/dfqar/still.git
cd still
rustup toolchain install --no-self-update
cargo xtask package
open build/DerivedData/Build/Products/Release/Still.app
```

The build writes an ad-hoc signed arm64 app and its matching dSYM to
`build/DerivedData/Build/Products/Release/`. See the
[developer guide](docs/development.md) for tests and source packaging.

## Spaces

| Space | What you can explore |
| --- | --- |
| **Drift** | Particles carried by a vector field, leaving trails as they move |
| **Fog** | Layers of procedural mist with adjustable depth and motion |
| **Ocean** | An animated wave surface with light that changes with the viewing angle |
| **Glass** | Shaped glass with refraction, reflection, and changing form |
| **Field** | Ribbons that follow a continuous vector field |
| **Frost** | Three independently moving lights diffused through matte glass |

Each space has four built-in presets. The controls panel lets you select a
space, change its parameters, and save custom presets. Still remembers your
scene settings, quality, frame rate, power preferences, and window placement.

| Shortcut | Action |
| --- | --- |
| `⌘1`–`⌘6` | Choose a space |
| `⌘N` | Generate a new variation |
| `⌘S` | Save a custom preset |
| `⌘F` | Toggle fullscreen |
| `⌘,` | Show controls |

## Platform support

The macOS app uses SwiftUI and AppKit for the window and controls. The Rust
engine owns scene state and rendering through wgpu and Metal. Windows
development currently covers an x64 engine DLL and C ABI smoke test. The WinUI
host and DX12 presentation are planned in the [Windows port guide](docs/windows/README.md).

## Documentation

- [Developer guide](docs/development.md) for building, testing, and preparing a source archive
- [Architecture decisions](docs/architecture.md) for the engine, render loop, and native host
- [Ambient control API](docs/ambient-control-api.md) for Frost light controls and Drift/Frost modulation
- [C ABI header](include/still.h) for integrating the engine
- [Documentation index](docs/README.md) for profiling, porting, and historical verification

Still is a dfqar lab project under the [MIT License](LICENSE).
