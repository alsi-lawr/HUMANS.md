#!/usr/bin/env python3
"""Restore supported humans-md 0.1.5 Claude state and reseed the core contract."""
from __future__ import annotations

import argparse
import datetime
import json
import os
import shutil
import subprocess
import tempfile
from pathlib import Path


class MigrationError(RuntimeError):
    pass


def atomic_write(path: Path, data: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    temporary_path = Path(temporary)
    try:
        with os.fdopen(descriptor, "wb") as stream:
            fchmod = getattr(os, "fchmod", None)
            if fchmod is not None:
                fchmod(stream.fileno(), 0o600)
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        if os.name == "posix":
            os.chmod(temporary_path, 0o600)
        os.replace(temporary_path, path)
    except BaseException:
        try:
            os.close(descriptor)
        except OSError:
            pass
        temporary_path.unlink(missing_ok=True)
        raise


def receipt(config: Path) -> tuple[Path, Path | None]:
    root = config / "backups/humans-md/claude"
    before = root / "CLAUDE.md.before"
    missing = root / "CLAUDE.md.was-missing"
    if root.is_symlink() or not root.is_dir() or before.exists() == missing.exists():
        raise MigrationError("no supported humans-md 0.1.5 Claude recovery receipt; run legacy recovery first")
    if before.exists() and (before.is_symlink() or not before.is_file()):
        raise MigrationError("unsafe legacy Claude receipt")
    return root, before if before.exists() else None


def show_diff(current: Path, baseline: Path | None) -> None:
    with tempfile.TemporaryDirectory(prefix="humans-md-claude-migration-") as temporary:
        missing = Path(temporary) / "missing"
        missing.touch()
        left = current if current.exists() else missing
        right = baseline if baseline is not None else missing
        result = subprocess.run(["git", "diff", "--no-index", "--", str(left), str(right)], capture_output=True, text=True, encoding="utf-8", errors="strict")
        if result.returncode not in (0, 1):
            raise MigrationError(result.stderr.strip() or "git diff --no-index failed")
        if result.stdout:
            print(result.stdout, end="" if result.stdout.endswith("\n") else "\n")


def preview(config: Path, plugin_root: Path) -> tuple[dict, Path, Path | None, bytes]:
    root, before = receipt(config)
    source = plugin_root.resolve(strict=True) / "templates/AGENTS.md"
    if not source.is_file() or source.is_symlink():
        raise MigrationError("core plugin lacks a safe CLAUDE.md contract template")
    return ({"operation": "migrate-v0.1.5-to-v0.2.0", "legacy_receipt": str(root), "restore": str(before) if before else "prior absence", "fresh_core_receipt": str(root), "marketplace_preserved": True, "install_siblings": False}, root, before, source.read_bytes())


def apply(config: Path, plugin_root: Path) -> dict:
    _, root, before, source = preview(config, plugin_root)
    # Validate all state again after human approval and before mutation.
    _, root, before, source = preview(config, plugin_root)
    current = config / "CLAUDE.md"
    rollback = Path(tempfile.mkdtemp(prefix="migration-", dir=config / "backups/humans-md"))
    prior_current = rollback / "CLAUDE.md"
    prior_root = rollback / "legacy-receipt"
    had_current = current.exists()
    if had_current:
        shutil.copy2(current, prior_current)
    shutil.copytree(root, prior_root)
    try:
        if before is None:
            current.unlink(missing_ok=True)
        else:
            atomic_write(current, before.read_bytes())
        retired = root.parent / ("claude-v0.1.5-retired-" + datetime.datetime.now(datetime.UTC).strftime("%Y%m%dT%H%M%SZ"))
        os.replace(root, retired)
        root.mkdir(mode=0o700)
        if current.exists():
            atomic_write(root / "CLAUDE.md.before", current.read_bytes())
        else:
            (root / "CLAUDE.md.was-missing").write_text("\n", encoding="ascii")
        atomic_write(current, source)
        if current.read_bytes() != source:
            raise MigrationError("fresh contract write verification failed")
        return {"status": "migrated", "retired_legacy_receipt": str(retired), "marketplace_preserved": True, "siblings_installed": False}
    except BaseException as error:
        current.unlink(missing_ok=True)
        if had_current:
            atomic_write(current, prior_current.read_bytes())
        if root.exists():
            shutil.rmtree(root)
        shutil.copytree(prior_root, root)
        raise MigrationError(f"migration failed; legacy state rollback verified: {error}") from error


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plugin-root", type=Path, required=True)
    parser.add_argument("--config-dir", type=Path, default=Path(os.environ.get("CLAUDE_CONFIG_DIR", "~/.claude")))
    parser.add_argument("--apply", action="store_true")
    arguments = parser.parse_args()
    try:
        config = arguments.config_dir.expanduser().resolve(strict=True)
        plan, _, before, _ = preview(config, arguments.plugin_root)
        print(json.dumps(plan, indent=2, sort_keys=True))
        show_diff(config / "CLAUDE.md", before)
        if arguments.apply:
            print(json.dumps(apply(config, arguments.plugin_root), indent=2, sort_keys=True))
        else:
            print("preview only; no files changed")
        return 0
    except (OSError, UnicodeError, MigrationError) as error:
        print(f"migration failed: {error}")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
