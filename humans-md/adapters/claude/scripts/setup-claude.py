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


def plugin_source(plugin_root: Path) -> bytes:
    path = plugin_root.resolve(strict=True) / "templates/AGENTS.md"
    if path.is_symlink() or not path.is_file(): raise SetupError("core plugin lacks a safe contract template")
    return path.read_bytes()


def preview(config: Path, plugin_root: Path) -> dict:
    source = plugin_source(plugin_root)
    return {"operation": "claude-core-setup", "receipt_kind": RECEIPT_KIND, "target": str(target(config)), "approval_fingerprint": path_fingerprint(target(config)), "source_sha256": hashlib.sha256(source).hexdigest()}


def install(config: Path, plugin_root: Path, approval: str | None = None) -> dict:
    plan = preview(config, plugin_root)
    if approval is not None and approval != plan["approval_fingerprint"]: raise SetupError("stale approval: managed target changed")
    if pointer(config).exists(): raise SetupError("an active v0.2.0 Claude core receipt already exists")
    source = plugin_source(plugin_root); current = target(config); root = config_root(config)
    root.mkdir(parents=True, exist_ok=True); os.chmod(root, 0o700)
    receipt_dir = Path(tempfile.mkdtemp(prefix=datetime.datetime.now(datetime.UTC).strftime("%Y%m%dT%H%M%SZ-"), dir=root))
    before = receipt_dir / "CLAUDE.md.before"; was_missing = not current.exists()
    try:
        if current.exists(): atomic_write(before, current.read_bytes())
        else: (receipt_dir / "CLAUDE.md.was-missing").write_text("\n", encoding="ascii")
        # Last re-read before mutation rejects a review invalidated during setup preparation.
        if approval is not None and approval != path_fingerprint(current): raise SetupError("stale approval: managed target changed")
        atomic_write(current, source)
        receipt_path = receipt_dir / "receipt.json"
        atomic_write(receipt_path, canonical({"schema_version": RECEIPT_SCHEMA, "kind": RECEIPT_KIND, "status": "installed", "plugin_version": "0.2.0", "before": "missing" if was_missing else "CLAUDE.md.before"}))
        pointer(config).parent.mkdir(parents=True, exist_ok=True)
        atomic_write(pointer(config), canonical({"receipt": str(receipt_path), "kind": RECEIPT_KIND}))
        return {"status": "installed", "receipt": str(receipt_path)}
    except BaseException as error:
        current.unlink(missing_ok=True)
        if before.exists(): atomic_write(current, before.read_bytes())
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
