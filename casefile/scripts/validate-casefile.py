#!/usr/bin/env python3
"""Validate Casefile-owned source or generated package boundaries."""
from __future__ import annotations
import argparse
import json
from pathlib import Path

SKILLS = {"casefile", "casefile-investigate", "casefile-review", "casefile-implement", "casefile-switch", "casefile-close"}
FORBIDDEN = {"validator", "writer", "hook", "mcp", "tui", "react", "sqlite"}

def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True)
    args = parser.parse_args()
    root = args.source.resolve()
    skills = root / "skills"
    errors = [f"missing Casefile skill: {name}" for name in sorted(SKILLS) if not (skills / name / "SKILL.md").is_file()]
    text = "\n".join(path.read_text(encoding="ascii") for path in root.rglob("*.md"))
    found = sorted(word for word in FORBIDDEN if f"{word} server" in text or f"{word} placeholder" in text)
    if found:
        errors.append("future Casefile tooling appears in this ticket: " + ", ".join(found))
    codex = root / ".codex-plugin/plugin.json"
    claude = root / ".claude-plugin/plugin.json"
    if codex.exists() or claude.exists():
        metadata = json.loads((codex if codex.exists() else claude).read_text(encoding="ascii"))
        if metadata.get("name") != "casefile" or metadata.get("version") != "0.2.1":
            errors.append("generated Casefile metadata is not casefile 0.2.1")
    elif not (root / "casefile-workflow").is_dir():
        errors.append("source lacks Casefile workflow assets")
    if errors:
        print("Casefile validation failed:", *errors, sep="\n- ")
        return 1
    print("validated Casefile boundary")
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
