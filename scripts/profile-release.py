#!/usr/bin/env python3
"""Run only the local Release Still app, preserving saved user state.

Requires a macOS GUI session. Measurements and screenshots stay in the repo.
The optional Metal trace is sampled after warmup and never overlaps another run.
"""
import argparse
import csv
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time


ROOT = Path(__file__).resolve().parent.parent
APP = ROOT / "build/DerivedData/Build/Products/Release/Still.app/Contents/MacOS/Still"


def capture(pid, destination):
    window = subprocess.check_output(["xcrun", "swift", "-module-cache-path", str(ROOT / "build/module-cache"),
                                      str(ROOT / "scripts/window-id.swift"), str(pid)], text=True, timeout=30).strip()
    subprocess.run(["screencapture", "-x", "-o", "-l", window, str(destination)], check=True, timeout=15)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--scene", choices=["drift", "fog", "ocean", "glass", "field", "frost"], default="drift")
    parser.add_argument("--preset")
    parser.add_argument("--quality", choices=["0", "1", "2", "3"], default="2")
    parser.add_argument("--seconds", type=int, default=100)
    parser.add_argument("--cycle", type=float, default=0)
    parser.add_argument("--trace-seconds", type=int, default=0)
    parser.add_argument("--warmup", type=int, default=15)
    parser.add_argument("--capture", action="store_true")
    parser.add_argument("--lifecycle", action="store_true")
    parser.add_argument("--label", required=True)
    args = parser.parse_args()
    if not APP.is_file() or args.seconds <= 0 or args.warmup < 0:
        parser.error("build the app first and supply positive duration")
    if args.trace_seconds and args.seconds < args.warmup + args.trace_seconds + 5:
        parser.error("duration must cover warmup, trace, and shutdown")
    if not args.label.replace("-", "").replace("_", "").isalnum():
        parser.error("label must contain letters, digits, hyphens, or underscores")
    existing = subprocess.run(["pgrep", "-x", "Still"], capture_output=True, text=True)
    if existing.returncode == 0:
        raise RuntimeError("Still is already running; close it before a diagnostic run")
    output = ROOT / "build/perf" / args.label
    output.mkdir(parents=True, exist_ok=True)
    metrics_path = output / "metrics.json"
    environment = dict(os.environ, STILL_PROFILE_RUN="1", STILL_PROFILE_WHEN_INACTIVE="1",
                       STILL_INITIAL_SCENE=args.scene, STILL_QUALITY=args.quality, STILL_FRAME_RATE="60",
                       STILL_EXIT_AFTER_SECONDS=str(args.seconds), STILL_METRICS_PATH=str(metrics_path),
                       STILL_SCENE_CYCLE_SECONDS=str(args.cycle))
    if args.preset:
        environment["STILL_INITIAL_PRESET"] = args.preset
    if args.lifecycle:
        if args.seconds < 207:
            parser.error("six-space lifecycle coverage needs at least 207 seconds")
        environment["STILL_LIFECYCLE_PATH"] = str(output / "lifecycle.json")
        environment["STILL_PROFILE_WHEN_INACTIVE"] = "0"
    identity = hashlib.sha256(APP.read_bytes()).hexdigest()
    awake = subprocess.Popen(["caffeinate", "-dim"])
    start = time.monotonic()
    trace = None
    captured = False
    maximum_frames = 0
    rows = []
    app = None
    try:
        with (output / "stderr.log").open("w") as error_log, (output / "memory.tsv").open("w", newline="") as samples:
            app = subprocess.Popen([str(APP)], env=environment, stderr=error_log, stdout=error_log)
            writer = csv.writer(samples, delimiter="\t")
            writer.writerow(["elapsed_seconds", "rss_kib", "cpu_percent", "scene", "presented_frames", "metal_allocated_bytes"])
            while app.poll() is None:
                elapsed = time.monotonic() - start
                if elapsed > args.seconds + 30:
                    app.terminate()
                    raise RuntimeError("Still did not terminate at the requested deadline")
                metrics = {}
                if metrics_path.exists():
                    metrics = json.loads(metrics_path.read_text())
                    maximum_frames = max(maximum_frames, metrics.get("presented_frames", 0))
                sample = subprocess.check_output(["ps", "-o", "rss=,%cpu=", "-p", str(app.pid)], text=True).split()
                if sample:
                    row = [round(elapsed, 3), int(sample[0]), float(sample[1]), metrics.get("active_scene", args.scene), maximum_frames, metrics.get("metal_allocated_bytes", "")]
                    writer.writerow(row)
                    rows.append(row)
                    samples.flush()
                if elapsed >= args.warmup and args.capture and not captured:
                    capture(app.pid, output / "window.png")
                    captured = True
                if elapsed >= args.warmup and args.trace_seconds and trace is None:
                    log = (output / "trace.log").open("w")
                    trace = subprocess.Popen(["xcrun", "xctrace", "record", "--template", "Metal System Trace",
                                              "--attach", str(app.pid), "--time-limit", f"{args.trace_seconds}s",
                                              "--output", str(output / "metal.trace")], stdout=log, stderr=log)
                    log.close()
                time.sleep(min(5, max(0.1, args.seconds + 1 - elapsed)))
            if app.returncode != 0:
                raise RuntimeError(f"Still exited with {app.returncode}; see stderr.log")
            if trace is not None and trace.wait(timeout=60) != 0:
                raise RuntimeError("Metal trace failed; see trace.log")
        metrics = json.loads(metrics_path.read_text())
        summary = dict(vars(args), binary_sha256=identity, wall_seconds=time.monotonic() - start,
                       app_returncode=app.returncode, metrics=metrics)
        summary["rss_mib"] = dict(min=min(r[1] for r in rows) / 1024, max=max(r[1] for r in rows) / 1024)
        scene_metrics = list(metrics["scenes"].values())
        minimum_scene_seconds = 3600 / max(len(scene_metrics), 1) * 0.8
        summary["continuous_hour_pass"] = (args.seconds >= 3600 and metrics["observed_seconds"] >= 3600
                                           and metrics["lifetime_effective_fps"] >= 59.0
                                           and metrics["max_present_gap_ms"] < 1000
                                           and all(s["observed_seconds"] >= minimum_scene_seconds
                                                   and s["lifetime_effective_fps"] >= 59.0 for s in scene_metrics))
        (output / "run.json").write_text(json.dumps(summary, indent=2) + "\n")
        print(json.dumps({"output": str(output), "frames": metrics["presented_frames"],
                          "fps": metrics["lifetime_effective_fps"], "continuous_hour_pass": summary["continuous_hour_pass"]}))
    finally:
        if app is not None and app.poll() is None:
            subprocess.run(["osascript", "-e", 'tell application id "app.still.Drift" to quit'], timeout=15)
            app.wait(timeout=15)
        awake.terminate()
        awake.wait()


if __name__ == "__main__":
    main()
