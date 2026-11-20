#!/usr/bin/env python3
"""Summarize RSS and CPU samples emitted by soak-release.sh."""

from __future__ import annotations

import argparse
import csv
import json
import math
import statistics
from pathlib import Path


def percentile(values, fraction):
    ordered = sorted(values)
    return ordered[max(0, math.ceil(fraction * len(ordered)) - 1)]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("samples", type=Path)
    parser.add_argument("--warmup-seconds", type=int, default=300)
    arguments = parser.parse_args()

    with arguments.samples.open(newline="") as source:
        rows = [
            (float(row["elapsed_seconds"]), int(row["rss_kib"]), float(row["cpu_percent"]))
            for row in csv.DictReader(source, delimiter="\t")
        ]
    if len(rows) < 2:
        raise ValueError("at least two soak samples are required")

    steady = [row for row in rows if row[0] >= arguments.warmup_seconds]
    if len(steady) < 2:
        raise ValueError("not enough post-warmup samples")
    times = [row[0] for row in steady]
    rss = [row[1] for row in steady]
    cpu = [row[2] for row in steady]
    mean_time = statistics.fmean(times)
    mean_rss = statistics.fmean(rss)
    denominator = sum((time - mean_time) ** 2 for time in times)
    slope_kib_per_second = (
        sum((time - mean_time) * (value - mean_rss) for time, value in zip(times, rss))
        / denominator
    )
    window = min(30, len(steady) // 2)
    initial_median = statistics.median(rss[:window])
    final_median = statistics.median(rss[-window:])

    result = {
        "duration_seconds": rows[-1][0] - rows[0][0],
        "samples": len(rows),
        "warmup_seconds_excluded": arguments.warmup_seconds,
        "rss_mib": {
            "min": min(rss) / 1024,
            "p50": percentile(rss, 0.50) / 1024,
            "p95": percentile(rss, 0.95) / 1024,
            "max": max(rss) / 1024,
            "initial_window_median": initial_median / 1024,
            "final_window_median": final_median / 1024,
            "linear_slope_mib_per_hour": slope_kib_per_second * 3600 / 1024,
        },
        "cpu_percent": {
            "p50": percentile(cpu, 0.50),
            "p95": percentile(cpu, 0.95),
            "max": max(cpu),
        },
    }
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
