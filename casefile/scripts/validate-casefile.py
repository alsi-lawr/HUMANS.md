#!/usr/bin/env python3
"""Validate Casefile-owned source or generated package boundaries."""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path

SKILLS = {
    "casefile",
    "casefile-investigate",
    "casefile-review",
    "casefile-implement",
    "casefile-switch",
    "casefile-close",
    "casefile-consolidate",
}
FORBIDDEN = {"validator", "writer", "hook", "mcp", "tui", "react", "sqlite"}
EXCLUDED_DIRECTORIES = {".agent-workspace", "build", "node_modules", "target"}
MATRIX = {
    "x86_64-unknown-linux-musl", "aarch64-unknown-linux-musl",
    "x86_64-apple-darwin", "aarch64-apple-darwin",
    "x86_64-pc-windows-msvc", "aarch64-pc-windows-msvc",
}

def owned_markdown(root: Path):
    return (
        path
        for path in root.rglob("*.md")
        if EXCLUDED_DIRECTORIES.isdisjoint(path.relative_to(root).parts)
    )

def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True)
    args = parser.parse_args()
    root = args.source.resolve()
    skills = root / "skills"
    errors = [f"missing Casefile skill: {name}" for name in sorted(SKILLS) if not (skills / name / "SKILL.md").is_file()]
    text = "\n".join(path.read_text(encoding="ascii") for path in owned_markdown(root))
    found = sorted(word for word in FORBIDDEN if f"{word} server" in text or f"{word} placeholder" in text)
    if found:
        errors.append("future Casefile tooling appears in this ticket: " + ", ".join(found))
    codex = root / ".codex-plugin/plugin.json"
    claude = root / ".claude-plugin/plugin.json"
    if codex.exists() or claude.exists():
        metadata = json.loads((codex if codex.exists() else claude).read_text(encoding="ascii"))
        version = metadata.get("version")
        if metadata.get("name") != "casefile" or not isinstance(version, str) or not version:
            errors.append("generated Casefile metadata lacks its package identity")
        for relative in (".mcp.json", "Cargo.toml", "Cargo.lock", "scripts/casefile-mcp-launcher.py"):
            if (root / relative).exists():
                errors.append(f"generated Casefile package retains source-launch input {relative}")
        if "mcpServers" in metadata:
            errors.append("generated Casefile metadata retains an automatic MCP declaration")
        setup = "scripts/setup-codex.py" if codex.exists() else "scripts/setup-claude.py"
        if not (root / setup).is_file() or not (root / "scripts/casefile_runtime.py").is_file():
            errors.append("generated Casefile package lacks receipt-backed runtime setup")
        try:
            raw = (root / "runtime/artifacts.json").read_bytes()
            manifest = json.loads(raw.decode("ascii"))
        except (OSError, UnicodeError, json.JSONDecodeError):
            manifest = None
            errors.append("generated Casefile package lacks a valid artifact manifest")
        if isinstance(manifest, dict):
            rows = manifest.get("artifacts")
            targets = {row.get("target") for row in rows if isinstance(row, dict)} if isinstance(rows, list) else set()
            if manifest.get("version") != version or targets != MATRIX or len(rows or []) != 6:
                errors.append("generated Casefile artifact matrix is incomplete or version-mismatched")
            else:
                for row in rows:
                    path = root / "runtime" / row.get("path", "")
                    if not path.is_file() or path.stat().st_size != row.get("size") or hashlib.sha256(path.read_bytes()).hexdigest() != row.get("sha256"):
                        errors.append(f"generated Casefile artifact is invalid: {row.get('target')}")
    elif not (root / "casefile-workflow").is_dir():
        errors.append("source lacks Casefile workflow assets")
    if errors:
        print("Casefile validation failed:", *errors, sep="\n- ")
        return 1
    print("validated Casefile boundary")
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
