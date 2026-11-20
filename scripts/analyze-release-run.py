#!/usr/bin/env python3
"""Review sampled RSS, CPU, Metal memory, and actual cadence of a release run."""

import argparse
import csv
import hashlib
import json
import math
from pathlib import Path
import statistics


def distribution(samples, divisor=1):
    values = sorted(samples)
    if not values:
        return None
    return {key: value / divisor for key, value in {
        "min": values[0], "p50": statistics.median(values),
        "p95": values[max(0, math.ceil(len(values) * .95) - 1)],
        "max": values[-1],
    }.items()}


def memory_trend(rows, field, divisor):
    samples = [(row["time"], row[field]) for row in rows if row[field] is not None]
    if len(samples) < 2:
        return None
    times, values = zip(*samples)
    mean_time = statistics.fmean(times)
    mean_value = statistics.fmean(values)
    denominator = sum((value - mean_time) ** 2 for value in times)
    slope = (sum((time - mean_time) * (value - mean_value)
                 for time, value in samples) / denominator) if denominator else 0
    window = min(30, len(values) // 2)
    first = statistics.median(values[:window]) / divisor
    last = statistics.median(values[-window:]) / divisor
    return dict(distribution(values, divisor), samples=len(values),
                first_window_median=first, last_window_median=last,
                window_samples=window, window_delta=last - first,
                linear_slope_per_hour=slope * 3600 / divisor)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("run_directory", type=Path)
    parser.add_argument("--warmup-seconds", type=float, default=300)
    parser.add_argument("--partial", action="store_true")
    args = parser.parse_args()
    directory = args.run_directory
    metadata_path = directory / "run.json"
    if not metadata_path.exists() and not args.partial:
        parser.error("run.json is required; --partial only produces provisional evidence")
    metadata = json.loads(metadata_path.read_text()) if metadata_path.exists() else {}
    metrics = json.loads((directory / "metrics.json").read_text())
    with (directory / "memory.tsv").open(newline="") as stream:
        rows = [dict(time=float(row["elapsed_seconds"]), rss=int(row["rss_kib"]),
                     cpu=float(row["cpu_percent"]), scene=row.get("scene", ""),
                     frames=int(row.get("presented_frames", "0")),
                     metal=int(row["metal_allocated_bytes"]) if row.get("metal_allocated_bytes") else None)
                for row in csv.DictReader(stream, delimiter="\t")]
    steady = [row for row in rows if row["time"] >= args.warmup_seconds]
    if len(steady) < 2:
        parser.error("not enough post-warmup samples")
    groups = []
    for row in rows:
        if not groups or groups[-1][0]["scene"] != row["scene"]:
            groups.append([])
        groups[-1].append(row)
    # Metrics are refreshed every ten seconds and ps every five. The 15-second
    # guard at both cached-scene boundaries avoids assigning live RSS to the
    # previous scene and excludes crossfade/initial allocation from steady data.
    cycling = metadata.get("cycle", 0) > 0 or len(groups) > 1
    guarded = []
    for group in groups:
        lower = group[0]["time"] + (15 if cycling else 0)
        upper = group[-1]["time"] - (15 if cycling else 0)
        guarded.extend(row for row in group if max(args.warmup_seconds, lower) <= row["time"] <= upper)
    per_scene = {}
    for scene in metrics.get("scenes", {}):
        scene_rows = [row for row in guarded if row["scene"] == scene]
        if not scene_rows:
            continue
        # Repeated frame counts mean repeated cached Metal samples.
        seen = set()
        metal_rows = []
        for row in scene_rows:
            if row["frames"] not in seen:
                seen.add(row["frames"])
                metal_rows.append(row)
        per_scene[scene] = dict(rss_mib=memory_trend(scene_rows, "rss", 1024),
                               metal_mib=memory_trend(metal_rows, "metal", 1024 ** 2),
                               process_cpu_percent=distribution([row["cpu"] for row in scene_rows]),
                               cadence=metrics.get("scenes", {}).get(scene))
    log = (directory / "stderr.log").read_text(errors="replace")
    notes_path = directory / "measurement-notes.json"
    notes = json.loads(notes_path.read_text()) if notes_path.exists() else {}
    binary = directory.parents[1] / "DerivedData/Build/Products/Release/Still.app/Contents/MacOS/Still"
    current_hash = hashlib.sha256(binary.read_bytes()).hexdigest() if binary.is_file() else None
    result = dict(provisional=not metadata_path.exists(),
                  binary_sha256=metadata.get("binary_sha256"),
                  matches_current_binary=metadata.get("binary_sha256") == current_hash,
                  app_returncode=metadata.get("app_returncode"),
                  runner_continuous_hour_pass=metadata.get("continuous_hour_pass", False),
                  sampled_seconds=rows[-1]["time"] - rows[0]["time"],
                  samples=len(rows), warmup_seconds=args.warmup_seconds,
                  scene_boundary_guard_seconds=15 if cycling else 0,
                  rss_mib=memory_trend(steady, "rss", 1024),
                  sampled_metal_mib=distribution([row["metal"] for row in steady if row["metal"] is not None], 1024 ** 2),
                  process_cpu_percent=distribution([row["cpu"] for row in steady]),
                  scenes=per_scene, metrics=metrics,
                  measurement_notes=notes,
                  stderr_bytes=len(log.encode()), stderr=log.strip(),
                  measurement_note="RSS/CPU sampled about every 5 s; Metal/cadence about every 10 s. Peaks are sampled, not instantaneous. Memory trends require review; no leak-free guarantee is inferred from a percentile.")
    print(json.dumps(result, indent=2) + "\n", end="")


if __name__ == "__main__":
    main()
