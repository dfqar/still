#!/usr/bin/env python3
"""Export the three measured tables without printing captured process env."""
import argparse
from pathlib import Path
import subprocess


parser = argparse.ArgumentParser()
parser.add_argument("directory", type=Path)
args = parser.parse_args()
directory = args.directory.resolve()
for table, filename in [("metal-gpu-intervals", "gpu.xml"),
                        ("ca-client-present-request", "presents.xml"),
                        ("metal-current-allocated-size", "metal-memory.xml")]:
    subprocess.run(["xcrun", "xctrace", "export", "--input", str(directory / "metal.trace"),
                    "--xpath", f'/trace-toc/run[@number="1"]/data/table[@schema="{table}"]',
                    "--output", str(directory / filename)], check=True)
with (directory / "gpu-summary.json").open("w") as destination:
    subprocess.run(["python3", str(Path(__file__).with_name("analyze-metal-trace.py")),
                    str(directory / "gpu.xml"), str(directory / "presents.xml"),
                    str(directory / "metal-memory.xml")], stdout=destination, check=True)
print(f"Measured Metal summary: {directory / 'gpu-summary.json'}")
