#!/usr/bin/env python3
"""Preview, install, or recover the versioned humans-md Claude core receipt."""
from __future__ import annotations

import argparse
import datetime
import hashlib
import json
import os
import shutil
import tempfile
from pathlib import Path

RECEIPT_SCHEMA = 1
RECEIPT_KIND = "humans-md-claude-core-v0.2.0"


class SetupError(RuntimeError):
    pass


def canonical(value: object) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True, ensure_ascii=True) + "\n").encode("ascii")


def atomic_write(path: Path, data: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    temporary_path = Path(temporary)
    try:
        with os.fdopen(descriptor, "wb") as stream:
            fchmod = getattr(os, "fchmod", None)
            if fchmod is not None:
                fchmod(stream.fileno(), 0o600)
            stream.write(data); stream.flush(); os.fsync(stream.fileno())
        if os.name == "posix": os.chmod(temporary_path, 0o600)
        os.replace(temporary_path, path)
    except BaseException:
        try: os.close(descriptor)
        except OSError: pass
        temporary_path.unlink(missing_ok=True)
        raise


def path_fingerprint(path: Path) -> str:
    digest = hashlib.sha256()
    if path.is_symlink(): raise SetupError(f"symbolic-link managed target is unsupported: {path}")
    if path.is_file(): digest.update(b"file "); digest.update(path.read_bytes())
    elif path.exists(): raise SetupError(f"unsafe managed target: {path}")
    else: digest.update(b"missing")
    return digest.hexdigest()


def config_root(config: Path) -> Path: return config / "backups/humans-md/claude-v0.2.0"
def pointer(config: Path) -> Path: return config / "state/humans-md/claude-v0.2.0.json"
def target(config: Path) -> Path: return config / "CLAUDE.md"
def settings_target(config: Path) -> Path: return config / "settings.json"


def plugin_source(plugin_root: Path) -> bytes:
    path = plugin_root.resolve(strict=True) / "templates/AGENTS.md"
    if path.is_symlink() or not path.is_file(): raise SetupError("core plugin lacks a safe contract template")
    return path.read_bytes()


def plugin_settings(plugin_root: Path) -> dict:
    """Contract settings the core owns, as dotted leaf paths."""
    path = plugin_root.resolve(strict=True) / "templates/settings.json"
    if path.is_symlink() or not path.is_file(): raise SetupError("core plugin lacks a safe settings template")
    try: document = json.loads(path.read_text(encoding="ascii"))
    except (ValueError, UnicodeDecodeError) as error: raise SetupError(f"unreadable settings template: {error}") from error
    if not isinstance(document, dict): raise SetupError("settings template must be a JSON object")
    return dict(flatten(document))


def flatten(document: dict, prefix: str = "") -> list[tuple[str, object]]:
    leaves: list[tuple[str, object]] = []
    for key, value in document.items():
        if not isinstance(key, str) or "." in key: raise SetupError(f"unsupported settings key: {key!r}")
        dotted = f"{prefix}{key}"
        if isinstance(value, dict): leaves.extend(flatten(value, f"{dotted}."))
        else: leaves.append((dotted, value))
    return leaves


_ABSENT = object()


def read_leaf(document: dict, dotted: str) -> object:
    cursor: object = document
    for part in dotted.split("."):
        if not isinstance(cursor, dict) or part not in cursor: return _ABSENT
        cursor = cursor[part]
    return cursor


def write_leaf(document: dict, dotted: str, value: object) -> None:
    parts = dotted.split("."); cursor = document
    for part in parts[:-1]:
        branch = cursor.get(part)
        if not isinstance(branch, dict): branch = {}; cursor[part] = branch
        cursor = branch
    cursor[parts[-1]] = value


def read_settings(path: Path) -> dict:
    if path.is_symlink(): raise SetupError(f"symbolic-link managed target is unsupported: {path}")
    if not path.exists(): return {}
    if not path.is_file(): raise SetupError(f"unsafe managed target: {path}")
    try: document = json.loads(path.read_text(encoding="utf-8"))
    except (ValueError, UnicodeDecodeError) as error: raise SetupError(f"existing settings.json is not valid JSON: {error}") from error
    if not isinstance(document, dict): raise SetupError("existing settings.json must be a JSON object")
    return document


def settings_plan(config: Path, plugin_root: Path) -> dict:
    """Per-key contract value against the value already on disk."""
    contract = plugin_settings(plugin_root); current = read_settings(settings_target(config))
    plan = {}
    for dotted, value in contract.items():
        existing = read_leaf(current, dotted)
        plan[dotted] = {"contract": value, "current": None if existing is _ABSENT else existing, "present": existing is not _ABSENT}
    return plan


def merged_settings(config: Path, plugin_root: Path) -> tuple[dict, dict]:
    """Return (document with contract keys applied, prior leaf values for the receipt)."""
    contract = plugin_settings(plugin_root); document = read_settings(settings_target(config)); before = {}
    for dotted, value in contract.items():
        existing = read_leaf(document, dotted)
        before[dotted] = None if existing is _ABSENT else existing
        write_leaf(document, dotted, value)
    return document, before


def combined_fingerprint(config: Path) -> str:
    """One approval covers both managed targets; either changing invalidates it."""
    digest = hashlib.sha256()
    digest.update(path_fingerprint(target(config)).encode("ascii")); digest.update(b" ")
    digest.update(path_fingerprint(settings_target(config)).encode("ascii"))
    return digest.hexdigest()


def preview(config: Path, plugin_root: Path) -> dict:
    source = plugin_source(plugin_root)
    return {"operation": "claude-core-setup", "receipt_kind": RECEIPT_KIND, "target": str(target(config)), "settings_target": str(settings_target(config)), "approval_fingerprint": combined_fingerprint(config), "source_sha256": hashlib.sha256(source).hexdigest(), "settings_plan": settings_plan(config, plugin_root)}


def install(config: Path, plugin_root: Path, approval: str | None = None) -> dict:
    plan = preview(config, plugin_root)
    if approval is not None and approval != plan["approval_fingerprint"]: raise SetupError("stale approval: managed target changed")
    if pointer(config).exists(): raise SetupError("an active v0.2.0 Claude core receipt already exists")
    source = plugin_source(plugin_root); current = target(config); root = config_root(config)
    settings_current = settings_target(config)
    merged, settings_before = merged_settings(config, plugin_root)
    root.mkdir(parents=True, exist_ok=True); os.chmod(root, 0o700)
    receipt_dir = Path(tempfile.mkdtemp(prefix=datetime.datetime.now(datetime.UTC).strftime("%Y%m%dT%H%M%SZ-"), dir=root))
    before = receipt_dir / "CLAUDE.md.before"; was_missing = not current.exists()
    settings_backup = receipt_dir / "settings.json.before"; settings_was_missing = not settings_current.exists()
    try:
        if current.exists(): atomic_write(before, current.read_bytes())
        else: (receipt_dir / "CLAUDE.md.was-missing").write_text("\n", encoding="ascii")
        if settings_current.exists(): atomic_write(settings_backup, settings_current.read_bytes())
        else: (receipt_dir / "settings.json.was-missing").write_text("\n", encoding="ascii")
        # Last re-read before mutation rejects a review invalidated during setup preparation.
        if approval is not None and approval != combined_fingerprint(config): raise SetupError("stale approval: managed target changed")
        atomic_write(current, source)
        # Preserve the operator's key order; only contract leaves are rewritten.
        atomic_write(settings_current, (json.dumps(merged, indent=2, ensure_ascii=True) + "\n").encode("ascii"))
        receipt_path = receipt_dir / "receipt.json"
        atomic_write(receipt_path, canonical({"schema_version": RECEIPT_SCHEMA, "kind": RECEIPT_KIND, "status": "installed", "plugin_version": "0.2.0", "before": "missing" if was_missing else "CLAUDE.md.before", "settings_before": settings_before, "settings_file_before": "missing" if settings_was_missing else "settings.json.before"}))
        pointer(config).parent.mkdir(parents=True, exist_ok=True)
        atomic_write(pointer(config), canonical({"receipt": str(receipt_path), "kind": RECEIPT_KIND}))
        return {"status": "installed", "receipt": str(receipt_path)}
    except BaseException as error:
        current.unlink(missing_ok=True)
        if before.exists(): atomic_write(current, before.read_bytes())
        settings_current.unlink(missing_ok=True)
        if settings_backup.exists(): atomic_write(settings_current, settings_backup.read_bytes())
        shutil.rmtree(receipt_dir, ignore_errors=True); pointer(config).unlink(missing_ok=True)
        raise SetupError(f"Claude setup failed; rollback verified: {error}") from error


def main() -> int:
    parser=argparse.ArgumentParser(description=__doc__); parser.add_argument("--plugin-root",type=Path,required=True); parser.add_argument("--config-dir",type=Path,default=Path(os.environ.get("CLAUDE_CONFIG_DIR","~/.claude"))); parser.add_argument("--apply",action="store_true"); parser.add_argument("--approval")
    args=parser.parse_args()
    try:
        config=args.config_dir.expanduser().resolve(strict=True); plan=preview(config,args.plugin_root); print(json.dumps(plan,indent=2,sort_keys=True))
        if args.apply:
            if not args.approval: raise SetupError("--approval must equal the preview approval_fingerprint")
            print(json.dumps(install(config,args.plugin_root,args.approval),indent=2,sort_keys=True))
        else: print("preview only; no files changed")
        return 0
    except (OSError, SetupError) as error:
        print(f"Claude setup failed: {error}"); return 1

if __name__ == "__main__": raise SystemExit(main())
