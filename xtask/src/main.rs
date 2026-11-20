use std::{
    env,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

fn main() -> ExitCode {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask has workspace parent");
    let command = env::args().nth(1).unwrap_or_else(|| "help".into());
    let result = match command.as_str() {
        "rust" => build_rust(root),
        "headers" => generate_header(root),
        "windows" => build_windows(root),
        "app" | "package" => require_macos()
            .and_then(|()| build_rust(root))
            .and_then(|()| build_xcode(root)),
        "check-rust" => check_rust(root),
        "check" => check(root),
        _ => {
            eprintln!("Usage: cargo xtask <rust|headers|windows|app|package|check-rust|check>");
            Ok(())
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("xtask: {message}");
            ExitCode::FAILURE
        }
    }
}

fn build_rust(root: &Path) -> Result<(), String> {
    let mut encoded_flags = env::var("CARGO_ENCODED_RUSTFLAGS").unwrap_or_default();
    let mut remaps = vec![(root.display().to_string(), "/still")];
    if let Some(cargo_home) = tool_home("CARGO_HOME", ".cargo") {
        remaps.push((cargo_home, "/cargo"));
    }
    if let Some(rustup_home) = tool_home("RUSTUP_HOME", ".rustup") {
        remaps.push((rustup_home, "/rustup"));
    }
    for (source, destination) in remaps {
        if !encoded_flags.is_empty() {
            encoded_flags.push('\u{1f}');
        }
        encoded_flags.push_str(&format!("--remap-path-prefix={source}={destination}"));
    }
    let status = Command::new("cargo")
        .args(["build", "--locked", "--release", "-p", "still-ffi"])
        .env("CARGO_ENCODED_RUSTFLAGS", encoded_flags)
        .current_dir(root)
        .status()
        .map_err(|e| format!("cannot run cargo: {e}"))?;
    if !status.success() {
        return Err(format!("cargo exited with {status}"));
    }
    generate_header(root)
}

fn generate_header(root: &Path) -> Result<(), String> {
    let config =
        cbindgen::Config::from_file(root.join("cbindgen.toml")).map_err(|e| e.to_string())?;
    cbindgen::Builder::new()
        .with_crate(root.join("crates/still-ffi"))
        .with_config(config)
        .with_only_target_dependencies(true)
        .generate()
        .map_err(|e| e.to_string())?
        .write_to_file(root.join("include/still.h"));
    Ok(())
}

fn require_macos() -> Result<(), String> {
    if cfg!(target_os = "macos") {
        Ok(())
    } else {
        Err("app/package builds the macOS shell and requires macOS + Xcode; use windows for the Windows engine bundle".into())
    }
}

fn build_windows(root: &Path) -> Result<(), String> {
    if !cfg!(all(
        target_os = "windows",
        target_arch = "x86_64",
        target_env = "msvc"
    )) {
        return Err("windows requires a native x64 Windows host with x86_64-pc-windows-msvc Rust and the MSVC/Windows SDK build tools".into());
    }
    // Staging paths below intentionally describe a native, default-target build.
    // Refuse overrides rather than copying a stale or wrong-architecture DLL.
    if env::var_os("CARGO_BUILD_TARGET").is_some() || env::var_os("CARGO_TARGET_DIR").is_some() {
        return Err(
            "windows staging requires CARGO_BUILD_TARGET and CARGO_TARGET_DIR to be unset".into(),
        );
    }
    build_rust(root)?;
    let destination = root.join("build/windows/win-x64");
    std::fs::create_dir_all(&destination).map_err(|e| e.to_string())?;
    for (source, name) in [
        (root.join("target/release/still_ffi.dll"), "still_ffi.dll"),
        (root.join("target/release/still_ffi.pdb"), "still_ffi.pdb"),
        (root.join("include/still.h"), "still.h"),
        (root.join("LICENSE"), "LICENSE"),
        (root.join("Cargo.lock"), "Cargo.lock"),
        (root.join("rust-toolchain.toml"), "rust-toolchain.toml"),
    ] {
        std::fs::copy(&source, destination.join(name))
            .map_err(|e| format!("cannot stage {}: {e}", source.display()))?;
    }
    std::fs::write(destination.join("BUILD-STATUS.txt"),
        "Still 2.2 Rust engine preparation\nTarget: x86_64-pc-windows-msvc\nLibrary: still_ffi.dll\nABI: 2; state schema: 3\nBackend: DX12\nWinUI SwapChainPanel attachment: pending W0 integration\nThis bundle is not a runnable Windows application.\n")
        .map_err(|e| e.to_string())?;
    println!(
        "Staged Windows engine preparation at {}",
        destination.display()
    );
    Ok(())
}

fn tool_home(variable: &str, default_suffix: &str) -> Option<String> {
    env::var(variable).ok().or_else(|| {
        env::var("HOME")
            .ok()
            .map(|home| format!("{home}/{default_suffix}"))
    })
}

fn build_xcode(root: &Path) -> Result<(), String> {
    let derived = root.join("build/DerivedData");
    run(
        root,
        "xcodebuild",
        &[
            "-project",
            "Still.xcodeproj",
            "-scheme",
            "Still",
            "-configuration",
            "Release",
            "-derivedDataPath",
            path_string(&derived)?,
            "CODE_SIGNING_ALLOWED=NO",
            "build",
        ],
    )?;
    let product = derived.join("Build/Products/Release/Still.app");
    if !product.exists() {
        return Err(format!("missing product {}", product.display()));
    }
    let product_path = path_string(&product)?;
    // A local ad-hoc signature makes the development package self-consistent.
    // Developer ID signing and notarization are intentionally external when no
    // credentials are installed on the build host.
    run(
        root,
        "codesign",
        &[
            "--force",
            "--deep",
            "--options",
            "runtime",
            "--sign",
            "-",
            product_path,
        ],
    )?;
    run(
        root,
        "codesign",
        &["--verify", "--deep", "--strict", product_path],
    )?;
    audit_product(root, &product)?;
    println!("Built {}", product.display());
    Ok(())
}

fn audit_product(root: &Path, product: &Path) -> Result<(), String> {
    let info = product.join("Contents/Info.plist");
    let binary = product.join("Contents/MacOS/Still");
    let dsym_binary: PathBuf = product
        .with_extension("app.dSYM")
        .join("Contents/Resources/DWARF/Still");
    for required in [&info, &binary, &dsym_binary] {
        if !required.exists() {
            return Err(format!("missing release artifact {}", required.display()));
        }
    }

    run(root, "plutil", &["-lint", path_string(&info)?])?;
    run(
        root,
        "lipo",
        &[path_string(&binary)?, "-verify_arch", "arm64"],
    )?;

    let binary_uuid = macho_uuid(root, &binary)?;
    let dsym_uuid = macho_uuid(root, &dsym_binary)?;
    if binary_uuid != dsym_uuid {
        return Err(format!(
            "binary/dSYM UUID mismatch: {binary_uuid} != {dsym_uuid}"
        ));
    }

    let libraries = output(root, "otool", &["-L", path_string(&binary)?])?;
    for library in libraries.lines().skip(1) {
        if library.contains("libstill_ffi") {
            return Err("the macOS app must link the Still engine statically".into());
        }
    }

    let strings = output(root, "strings", &[path_string(&binary)?])?;
    let root_marker = root.to_string_lossy();
    for forbidden in [root_marker.as_ref(), "/Users/", "target/release", ".wgsl"] {
        if strings.contains(forbidden) {
            return Err(format!(
                "release binary contains forbidden path marker {forbidden:?}"
            ));
        }
    }

    let signature = output(
        root,
        "codesign",
        &["--display", "--verbose=4", path_string(product)?],
    )?;
    if !signature.contains("runtime") {
        return Err("release signature does not enable the hardened runtime".into());
    }
    Ok(())
}

fn macho_uuid(root: &Path, path: &Path) -> Result<String, String> {
    let output = output(root, "dwarfdump", &["--uuid", path_string(path)?])?;
    output
        .split_whitespace()
        .nth(1)
        .map(str::to_owned)
        .ok_or_else(|| format!("cannot read Mach-O UUID for {}", path.display()))
}

fn check(root: &Path) -> Result<(), String> {
    check_rust(root)?;
    build_rust(root)?;
    if cfg!(target_os = "macos") {
        build_xcode(root)?;
    } else {
        println!(
            "Rust checks passed; native Windows UI/GPU presentation requires the separate Windows integration gate."
        );
    }
    Ok(())
}

fn check_rust(root: &Path) -> Result<(), String> {
    run(root, "cargo", &["fmt", "--all", "--", "--check"])?;
    run(
        root,
        "cargo",
        &[
            "clippy",
            "--locked",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
    )?;
    run(root, "cargo", &["test", "--locked", "--workspace"])?;
    Ok(())
}

fn run(root: &Path, program: &str, arguments: &[&str]) -> Result<(), String> {
    let status = Command::new(program)
        .args(arguments)
        .current_dir(root)
        .status()
        .map_err(|e| format!("cannot run {program}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{program} exited with {status}"))
    }
}

fn output(root: &Path, program: &str, arguments: &[&str]) -> Result<String, String> {
    let output = Command::new(program)
        .args(arguments)
        .current_dir(root)
        .output()
        .map_err(|e| format!("cannot run {program}: {e}"))?;
    if !output.status.success() {
        return Err(format!("{program} exited with {}", output.status));
    }
    let mut combined = String::from_utf8_lossy(&output.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    Ok(combined)
}

fn path_string(path: &Path) -> Result<&str, String> {
    path.to_str()
        .ok_or_else(|| format!("non-UTF8 path: {}", path.display()))
}
