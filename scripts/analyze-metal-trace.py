#!/usr/bin/env python3
"""Summarize exported Metal System Trace tables for Still."""

from __future__ import annotations

import argparse
import json
import math
import xml.etree.ElementTree as ET
from pathlib import Path


def read_table(path: Path):
    document = ET.parse(path)
    ids = {element.get("id"): element for element in document.iter() if element.get("id")}

    def resolve(element):
        seen = set()
        while element.get("ref"):
            reference = element.get("ref")
            if reference in seen or reference not in ids:
                break
            seen.add(reference)
            element = ids[reference]
        return element

    def value(element):
        element = resolve(element)
        if (element.text or "").strip():
            return element.text.strip()
        for child in element.iter():
            if child is element:
                continue
            child = resolve(child)
            if (child.text or "").strip():
                return child.text.strip()
        return ""

    node = document.find(".//node")
    if node is None or node.find("schema") is None:
        raise ValueError(f"{path} has no exported xctrace table")
    columns = [column.findtext("mnemonic") for column in node.find("schema").findall("col")]
    rows = [dict(zip(columns, map(resolve, list(row)))) for row in node.findall("row")]
    return rows, value


def integer(element, value):
    return int(value(element).replace("\N{NO-BREAK SPACE}", "").replace(" ", ""))


def percentile(values, fraction):
    ordered = sorted(values)
    return ordered[max(0, math.ceil(fraction * len(ordered)) - 1)]


def union_duration(spans):
    if not spans:
        return 0
    spans.sort()
    total = 0
    left, right = spans[0]
    for start, end in spans[1:]:
        if start <= right:
            right = max(right, end)
        else:
            total += right - left
            left, right = start, end
    return total + right - left


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("gpu", type=Path, help="metal-gpu-intervals XML")
    parser.add_argument("presents", type=Path, help="ca-client-present-request XML")
    parser.add_argument("memory", type=Path, help="metal-current-allocated-size XML")
    parser.add_argument("--process", default="Still")
    arguments = parser.parse_args()

    gpu_rows, gpu_value = read_table(arguments.gpu)
    present_rows, present_value = read_table(arguments.presents)
    memory_rows, memory_value = read_table(arguments.memory)

    def owned(row):
        return arguments.process in row["process"].get("fmt", "")

    present_ids = [integer(row["cmdbuffer-id"], present_value) for row in present_rows if owned(row)]
    present_times = [integer(row["timestamp"], present_value) for row in present_rows if owned(row)]
    gpu_intervals = [
        (
            integer(row["cmdbuffer-id"], gpu_value),
            integer(row["start"], gpu_value),
            integer(row["duration"], gpu_value),
        )
        for row in gpu_rows
        if owned(row)
    ]

    per_frame_ms = []
    previous_present_id = -1
    for present_id in present_ids:
        spans = [
            (start, start + duration)
            for command_buffer_id, start, duration in gpu_intervals
            if previous_present_id < command_buffer_id <= present_id
        ]
        if spans:
            per_frame_ms.append(union_duration(spans) / 1_000_000)
        previous_present_id = present_id

    if not per_frame_ms or len(present_times) < 2:
        raise ValueError("trace does not contain enough matching frames")

    display_ms = [(end - start) / 1_000_000 for start, end in zip(present_times, present_times[1:])]
    memory_bytes = [
        integer(row["current-allocated-size"], memory_value)
        for row in memory_rows
        if owned(row)
    ]
    result = {
        "gpu_frames": len(per_frame_ms),
        "gpu_ms": {
            "p50": percentile(per_frame_ms, 0.50),
            "p95": percentile(per_frame_ms, 0.95),
            "p99": percentile(per_frame_ms, 0.99),
            "max": max(per_frame_ms),
        },
        "present_count": len(present_times),
        "present_interval_ms": {
            "p50": percentile(display_ms, 0.50),
            "p95": percentile(display_ms, 0.95),
            "max": max(display_ms),
        },
        "metal_memory_mib": {
            "min": min(memory_bytes) / 1_048_576,
            "max": max(memory_bytes) / 1_048_576,
        },
    }
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
