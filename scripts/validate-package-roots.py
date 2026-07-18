#!/usr/bin/env python3
"""Validate the three explicit v0.2.1 package roots and catalog inventory."""
from __future__ import annotations
import json, tomllib
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
EXPECTED={"humans-md","casefile","coding"}
def main() -> int:
    manifests=sorted(ROOT.glob("*/packaging/plugin.toml")); errors=[]; names=set(); versions=set()
    for path in manifests:
        item=tomllib.loads(path.read_text(encoding="ascii")); names.add(item.get("name")); versions.add(item.get("version"))
        if item.get("name") != path.parents[1].name: errors.append(f"root identity mismatch: {path}")
        if set(item.get("vendors",{})) != {"codex","claude"}: errors.append(f"vendor parity mismatch: {path}")
    if names != EXPECTED: errors.append(f"package inventory mismatch: {sorted(names)}")
    if versions != {"0.2.1"}: errors.append(f"package versions not synchronized: {sorted(versions)}")
    for catalog_path in (ROOT/"packaging/marketplace/.agents/plugins/marketplace.json",ROOT/"packaging/marketplace/.claude-plugin/marketplace.json"):
        catalog=json.loads(catalog_path.read_text(encoding="ascii")); catalog_names={entry.get("name") for entry in catalog.get("plugins",[])}
        if catalog_names != EXPECTED: errors.append(f"catalog parity mismatch: {catalog_path}")
    for vendor, marker in (("codex", ".codex-plugin/plugin.json"), ("claude", ".claude-plugin/plugin.json")):
        generated=ROOT / "build/marketplace/plugins" / vendor
        if generated.exists():
            identities={path.name for path in generated.iterdir() if path.is_dir()}
            if identities != EXPECTED: errors.append(f"generated {vendor} inventory mismatch: {sorted(identities)}")
            for name in EXPECTED & identities:
                metadata=json.loads((generated/name/marker).read_text(encoding="ascii"))
                if metadata.get("name") != name or metadata.get("version") != "0.2.1": errors.append(f"generated {vendor} metadata mismatch: {name}")
    if any((ROOT/name).exists() for name in ("adapters","skills","casefile-workflow","verification")): errors.append("superseded monolithic plugin roots remain")
    if errors: print("package-root validation failed:",*errors,sep="\n- "); return 1
    print("validated package roots and marketplace inventory: casefile, coding, humans-md"); return 0
if __name__=="__main__": raise SystemExit(main())
