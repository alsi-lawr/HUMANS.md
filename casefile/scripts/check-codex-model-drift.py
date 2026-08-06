#!/usr/bin/env python3
"""Compare profile-relevant fields in Codex's stable app-server projection."""
from __future__ import annotations

import argparse
import json
import tomllib
from pathlib import Path


def compare(catalog: dict, profiles: dict) -> list[str]:
    models = catalog.get("models")
    findings: list[str] = []
    if not isinstance(models, list):
        findings.append("Catalog has no model array.")
        models = []
    by_id: dict[str, dict] = {}
    for model in models:
        model_id = model.get("slug") if isinstance(model, dict) else None
        if not isinstance(model_id, str):
            findings.append("A catalog entry has no string slug.")
        elif model_id in by_id:
            findings.append(f"Duplicate profile-relevant model `{model_id}`.")
        else:
            by_id[model_id] = model
    targets = profiles.get("catalog", {}).get("targets", [])
    # A model Codex offers that the packaged catalog does not carry must be added and the agent
    # matrices re-evaluated before it can be selected.
    carried = {target.get("id") for target in targets}
    for model_id in sorted(by_id.keys() - carried):
        findings.append(f"Codex offers `{model_id}`, which the packaged catalog does not carry.")
    for target in targets:
        model_id = target.get("id")
        model = by_id.get(model_id)
        if model is None:
            # A pinned model is carried by this repository, not by Codex's projection.
            if target.get("required") is True and target.get("pinned") is not True:
                findings.append(f"Required model `{model_id}` is missing.")
            continue
        efforts = {
            item.get("effort")
            for item in model.get("supported_reasoning_levels", [])
            if isinstance(item, dict)
        }
        for effort in sorted(set(target.get("required_reasoning", [])) - efforts):
            findings.append(f"Model `{model_id}` no longer declares reasoning `{effort}`.")
        for field, expected in target.get("expected", {}).items():
            if model.get(field) != expected:
                findings.append(
                    f"Model `{model_id}` field `{field}` changed from `{expected}` to `{model.get(field)}`."
                )
    return findings


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--catalog", type=Path, required=True)
    parser.add_argument("--profiles", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    if arguments.catalog.name == "models_cache.json":
        raise SystemExit("refusing models_cache.json")
    catalog = json.loads(arguments.catalog.read_text(encoding="utf-8"))
    profiles = tomllib.loads(arguments.profiles.read_text(encoding="utf-8"))
    findings = compare(catalog, profiles)
    body = [
        "# Codex model profile drift",
        "",
        "This report compares required model IDs, visibility, display names, and required reasoning levels from Codex's stable app-server projection. Raw runtime selectors are intentionally verified by setup lifecycle tests instead. It contains no catalog payload and makes no instruction edits.",
        "",
    ]
    if findings:
        body += ["## Findings", ""] + [f"- {item}" for item in findings]
    else:
        body += ["No profile-relevant drift detected."]
    arguments.output.write_text("\n".join(body) + "\n", encoding="ascii")
    print(f"wrote {len(findings)} finding(s) to {arguments.output}")
    return 1 if findings else 0


if __name__ == "__main__":
    raise SystemExit(main())
