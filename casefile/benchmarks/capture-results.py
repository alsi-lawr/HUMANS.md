#!/usr/bin/env python3
"""Retain Criterion estimates and raw samples outside a disposable Cargo target."""

import argparse
import json
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("criterion_directory", type=Path)
parser.add_argument("baseline")
parser.add_argument("output", type=Path)
args = parser.parse_args()

results = {}
for estimates_path in sorted(args.criterion_directory.rglob(f"{args.baseline}/estimates.json")):
    directory = estimates_path.parent
    benchmark = json.loads((directory / "benchmark.json").read_text())
    results[benchmark["full_id"]] = {
        "benchmark": benchmark,
        "estimates": json.loads(estimates_path.read_text()),
        "sample": json.loads((directory / "sample.json").read_text()),
    }
if not results:
    parser.error(f"no Criterion results for {args.baseline!r}")
args.output.parent.mkdir(parents=True, exist_ok=True)
args.output.write_text(json.dumps({"baseline": args.baseline, "scenarios": results}, indent=2, sort_keys=True) + "\n")
print(f"Captured {len(results)} scenarios to {args.output}")
