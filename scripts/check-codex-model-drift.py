#!/usr/bin/env python3
"""Compare only profile-relevant fields in a fresh Codex model export."""
from __future__ import annotations

import argparse
import json
import tomllib
from pathlib import Path


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
    for target in profiles.get("catalog", {}).get("targets", []):
        model_id = target.get("id")
        model = by_id.get(model_id)
        if model is None:
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
        for selector in target.get("null_selectors", []):
            current: object = model
            found = True
            for part in selector.split("."):
                if not isinstance(current, dict) or part not in current:
                    findings.append(f"Model `{model_id}` no longer exposes selector `{selector}`.")
                    found = False
                    break
                current = current[part]
            if found and current is not None:
                findings.append(
                    f"Model `{model_id}` selector `{selector}` is `{current}` instead of JSON null."
                )
    body = [
        "# Codex model profile drift",
        "",
        "This report compares only model IDs, required reasoning levels, declared expected fields, and declared selectors. It contains no catalog payload and makes no instruction edits.",
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
