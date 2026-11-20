#!/usr/bin/env python3
"""Create a source-only Still archive without local settings or build output."""
import argparse
import hashlib
import json
from pathlib import Path
import zipfile


def main():
    root = Path(__file__).resolve().parent.parent
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=root / "build/Still-source-windows.zip")
    args = parser.parse_args()
    destination = args.output.resolve()
    temporary = destination.with_suffix(destination.suffix + ".tmp")
    checksum = destination.with_suffix(destination.suffix + ".sha256")
    roots = [
        ".github", "crates", "docs", "include", "macOS", "platforms",
        "scripts", "Still.xcodeproj", "xtask",
    ]
    root_files = [
        ".cargo/config.toml", ".gitignore", "Cargo.toml", "Cargo.lock",
        "cbindgen.toml", "LICENSE", "README.md", "rust-toolchain.toml",
    ]
    excluded = {
        "target", "build", "DerivedData", "bin", "obj", "TestResults",
        "__pycache__", "xcuserdata", "__MACOSX",
    }
    files = [root / name for name in root_files]
    attributes = root / ".gitattributes"
    if attributes.exists() or attributes.is_symlink():
        files.append(attributes)
    for path in files:
        if path.resolve() != path:
            raise SystemExit(f"Refusing source symlink: {path.relative_to(root)}")
    for name in roots:
        source_dir = root / name
        if source_dir.is_symlink():
            raise SystemExit(f"Refusing source symlink: {name}")
        for path in source_dir.rglob("*"):
            relative = path.relative_to(root)
            if (
                excluded.intersection(relative.parts)
                or any(part.startswith(".") for part in relative.parts[1:])
                or relative.as_posix() == "docs/windows.zip"
                or path in {destination, temporary, checksum}
                or path.suffix in {".xcuserstate", ".user", ".suo", ".pyc"}
            ):
                continue
            if path.is_symlink():
                raise SystemExit(f"Refusing source symlink: {relative}")
            if path.is_file():
                files.append(path)
    files.sort(key=lambda path: path.relative_to(root).as_posix())
    payloads = {
        path.relative_to(root).as_posix(): path.read_bytes() for path in files
    }
    manifest = {
        "kind": "Still source archive",
        "files": {name: hashlib.sha256(data).hexdigest() for name, data in payloads.items()},
    }
    payloads["SOURCE-MANIFEST.json"] = (json.dumps(manifest, indent=2) + "\n").encode()
    destination.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(temporary, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for name, data in payloads.items():
            # Stable ZIP metadata makes identical source produce identical bytes.
            entry = zipfile.ZipInfo("Still/" + name)
            entry.create_system = 3
            entry.compress_type = zipfile.ZIP_DEFLATED
            mode = (root / name).stat().st_mode & 0o777 if name != "SOURCE-MANIFEST.json" else 0o644
            entry.external_attr = (0o100000 | mode) << 16
            archive.writestr(entry, data, compresslevel=9)
    with zipfile.ZipFile(temporary) as archive:
        if archive.testzip() is not None:
            raise SystemExit("Archive CRC verification failed")
        for name, digest in manifest["files"].items():
            if hashlib.sha256(archive.read("Still/" + name)).hexdigest() != digest:
                raise SystemExit(f"Archive checksum verification failed: {name}")
    temporary.replace(destination)
    digest = hashlib.sha256(destination.read_bytes()).hexdigest()
    checksum.write_text(f"{digest}  {destination.name}\n", encoding="utf-8")
    print(f"{destination}\n{len(files)} source files; SHA-256 {digest}")


if __name__ == "__main__":
    main()
