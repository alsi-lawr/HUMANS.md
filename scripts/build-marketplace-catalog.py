#!/usr/bin/env python3
"""Render both marketplace catalogs from the package-root manifest inventory."""
from __future__ import annotations

import json
import tomllib
from pathlib import Path


def manifests(root: Path) -> list[dict]:
    paths = sorted(root.glob("*/packaging/plugin.toml"))
    if not paths:
        raise ValueError("no package-root manifests found")
    result = []
    for path in paths:
        value = tomllib.loads(path.read_text(encoding="ascii"))
        if value.get("name") != path.parent.parent.name:
            raise ValueError(f"manifest identity does not match package root: {path}")
        if set(value.get("vendors", {})) != {"codex", "claude"}:
            raise ValueError(f"manifest lacks Codex or Claude adapter: {path}")
        result.append(value)
    versions = {value.get("version") for value in result}
    if len(versions) != 1:
        raise ValueError("marketplace package versions are not synchronized")
    return result


def write(path: Path, value: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True, ensure_ascii=True) + "\n", encoding="ascii")


def main() -> int:
    root = Path(__file__).resolve().parent.parent
    items = manifests(root)
    write(root / "packaging/marketplace/.agents/plugins/marketplace.json", {
        "interface": {"displayName": "humans-md"}, "name": "humans-md",
        "plugins": [{"category": "Coding", "name": item["name"], "policy": {"authentication": "ON_INSTALL", "installation": "AVAILABLE"}, "source": {"path": f"./plugins/codex/{item['name']}", "source": "local"}} for item in items],
    })
    write(root / "packaging/marketplace/.claude-plugin/marketplace.json", {
        "description": "Split humans-md plugins for standing contracts, Casefile, and coding guidance.", "name": "humans-md", "owner": {"name": "alsi-lawr"},
        "plugins": [{"author": {"name": item["publisher"]}, "description": item["description"], "license": item["license"], "name": item["name"], "repository": item["repository_url"], "source": f"./plugins/claude/{item['name']}", "strict": True} for item in items],
    })
    print("rendered marketplace catalogs: " + ", ".join(item["name"] for item in items))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
