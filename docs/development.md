# Build, test, and package Still

Start with the requirements and build instructions in the [README](../README.md).
Run commands from the checkout root.

## Source layout

- [macOS](../macOS) contains the SwiftUI/AppKit app, surface lifecycle, and native smoke fixture.
- [still-core](../crates/still-core) defines scene state, timing, signals, and automatic quality.
- [still-scenes](../crates/still-scenes) contains the six scene implementations, WGSL, and presets.
- [still-renderer](../crates/still-renderer) owns the GPU device, presentation surface, and compositor.
- [still-ffi](../crates/still-ffi) connects the engine to native hosts through a versioned C ABI.
- [xtask](../xtask) builds the engine, regenerates its header, and verifies release artifacts.

## Checks on macOS

Run formatting, Clippy with warnings denied, and the Rust tests with:

```sh
cargo xtask check-rust
```

Run the full release check on a Mac with Metal. This requires the GPU tests,
regenerates the [C header](../include/still.h), builds the Xcode Release target,
and audits the app, signature, and matching dSYM.

```sh
STILL_REQUIRE_METAL_TESTS=1 scripts/test.sh
```

If you change the public C API, regenerate the header with `cargo xtask headers`
and include it with the corresponding Rust changes.

For native window, surface, and lifecycle checks, run the smoke fixture from a
macOS desktop session. It opens its own windows and writes results to
`build/still-smoke/latest/native-smoke.json`. Exit code 2 identifies a run where
macOS did not report physical occlusion. The report lists the checks that ran,
including the test with an injected visibility signal.

```sh
scripts/smoke-still.sh
```

The release executable has a separate lifecycle diagnostic across all six
spaces. Close any running Still instance before running it. Check that every
entry in `build/perf/lifecycle-check/lifecycle.json` has `passed: true`.

```sh
python3 scripts/profile-release.py --lifecycle --seconds 207 --label lifecycle-check
```

See [Performance verification](performance.md) for tracing and longer runs.

## Windows engine checks

Use x64 Windows, PowerShell 7, and the Rust MSVC toolchain. Install the MSVC C++
build tools and Windows SDK, then run:

```powershell
rustup toolchain install --no-self-update
./scripts/build-windows.ps1
```

The script builds the x64 engine bundle at `build/windows/win-x64/` and loads
the DLL to check the C ABI. The [Windows port guide](windows/README.md) covers
the planned WinUI host and DX12 presentation checks.

## Source archive

Create and verify the source ZIP on macOS with Python 3:

```sh
python3 scripts/package-source.py --output build/Still-source.zip
(cd build && shasum -a 256 -c Still-source.zip.sha256)
```

The ZIP includes a SHA-256 manifest for its source files. The packager
excludes local toolchains, build products, and editor settings. To verify a
release from the archive, extract it into a fresh directory and run the full
release check from the extracted `Still/` directory.

The local app build is ad-hoc signed. Distributing a macOS binary uses a
separate Developer ID signing and notarization process. See the
[distribution notes](release-checklist.md#distribution-work-requiring-credentials).
