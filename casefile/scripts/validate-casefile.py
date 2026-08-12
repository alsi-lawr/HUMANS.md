#!/usr/bin/env python3
"""Validate Casefile-owned source or generated package boundaries."""
from __future__ import annotations
import argparse
import json
import re
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

def artifact_path(value: object, target: object) -> Path | None:
    if not isinstance(value, str) or not isinstance(target, str) or not value or "\0" in value:
        return None
    if value.startswith(("/", "\\")) or re.match(r"^[A-Za-z]:", value):
        return None
    parts = [part for part in value.replace("\\", "/").split("/") if part]
    if not parts or any(part in {".", ".."} for part in parts):
        return None
    name = "casefile.exe" if target.endswith("windows-msvc") else "casefile"
    if parts != ["bin", target, name]:
        return None
    return Path(*parts)

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
        metadata = json.loads((codex if codex.exists() else claude).read_text(encoding="utf-8"))
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
        if codex.exists() and not (root / "scripts/list-codex-models.py").is_file():
            errors.append("generated Codex Casefile package lacks stable model discovery")
        try:
            manifest_path = root / "runtime/artifacts.json"
            if (
                manifest_path.is_symlink()
                or not manifest_path.is_file()
                or manifest_path.stat().st_size <= 0
            ):
                raise OSError("artifact manifest is missing, empty, or unsafe")
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
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
                    relative = artifact_path(row.get("path"), row.get("target"))
                    path = root / "runtime" / relative if relative is not None else None
                    contained = False
                    if path is not None:
                        try:
                            path.resolve(strict=True).relative_to((root / "runtime").resolve(strict=True))
                            contained = True
                        except (OSError, ValueError):
                            pass
                    if (
                        path is None
                        or not contained
                        or path.is_symlink()
                        or not path.is_file()
                        or path.stat().st_size <= 0
                    ):
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
