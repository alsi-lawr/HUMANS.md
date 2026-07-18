#!/usr/bin/env python3
"""Restore supported humans-md 0.1.5 Claude state and reseed the core contract."""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
import shutil
import subprocess
import tempfile
from pathlib import Path


def lifecycle():
    path=Path(__file__).with_name("setup-claude.py"); spec=importlib.util.spec_from_file_location("claude_core_setup",path)
    if spec is None or spec.loader is None: raise RuntimeError("cannot load Claude core lifecycle")
    module=importlib.util.module_from_spec(spec); spec.loader.exec_module(module); return module
core=lifecycle()

class MigrationError(RuntimeError): pass

def legacy_receipt(config: Path) -> tuple[Path, Path | None]:
    root=config/"backups/humans-md/claude"; before=root/"CLAUDE.md.before"; missing=root/"CLAUDE.md.was-missing"
    current=config/"CLAUDE.md"
    if current.is_symlink() or (current.exists() and not current.is_file()):
        raise MigrationError("unsafe live Claude target")
    if core.pointer(config).exists(): raise MigrationError("fresh v0.2.0 Claude state is not a migratable v0.1.5 receipt")
    if root.is_symlink() or not root.is_dir() or before.exists()==missing.exists() or (root/"receipt.json").exists():
        raise MigrationError("no supported humans-md 0.1.5 Claude recovery receipt; run legacy recovery first")
    entries={item.name for item in root.iterdir()}
    allowed={"CLAUDE.md.before"} if before.exists() else {"CLAUDE.md.was-missing"}
    if entries != allowed:
        raise MigrationError("unsafe or ambiguous legacy Claude receipt")
    record = before if before.exists() else missing
    if record.is_symlink() or not record.is_file():
        raise MigrationError("unsafe or ambiguous legacy Claude receipt")
    return root, before if before.exists() else None

def fingerprint(path: Path) -> str:
    digest=hashlib.sha256()
    def add(current: Path, relative: str) -> None:
        if current.is_symlink(): digest.update(f"L {relative} ".encode()); digest.update(os.readlink(current).encode()); return
        if current.is_file(): digest.update(f"F {relative} ".encode()); digest.update(current.read_bytes()); return
        if current.is_dir():
            digest.update(f"D {relative}\\n".encode())
            for child in sorted(current.iterdir(),key=lambda item:item.name): add(child,f"{relative}/{child.name}")
            return
        digest.update(f"M {relative}\\n".encode())
    add(path,"."); return digest.hexdigest()

def approval_fingerprint(config: Path, root: Path) -> str:
    return hashlib.sha256(core.canonical({"CLAUDE.md":fingerprint(config/"CLAUDE.md"),"legacy-receipt":fingerprint(root)})).hexdigest()

def show_diff(current: Path, baseline: Path | None) -> None:
    with tempfile.TemporaryDirectory(prefix="humans-md-claude-migration-") as temporary:
        missing=Path(temporary)/"missing"; missing.touch(); left=current if current.exists() else missing; right=baseline if baseline is not None else missing
        result=subprocess.run(["git","diff","--no-index","--",str(left),str(right)],capture_output=True,text=True,encoding="utf-8",errors="strict")
        if result.returncode not in (0,1): raise MigrationError(result.stderr.strip() or "git diff --no-index failed")
        if result.stdout: print(result.stdout,end="" if result.stdout.endswith("\n") else "\n")

def preview(config: Path, plugin_root: Path) -> tuple[dict, Path, Path | None]:
    root,before=legacy_receipt(config); source=core.plugin_source(plugin_root)
    plan={"operation":"migrate-v0.1.5-to-v0.2.0","legacy_receipt":str(root),"restore":str(before) if before else "prior absence","fresh_core_receipt_root":str(core.config_root(config)),"approval_fingerprint":approval_fingerprint(config,root),"marketplace_preserved":True,"install_siblings":False,"source_sha256":hashlib.sha256(source).hexdigest()}
    return plan,root,before

def apply(config: Path, plugin_root: Path, approval: str) -> dict:
    plan,root,before=preview(config,plugin_root)
    if approval != plan["approval_fingerprint"]:
        show_diff(config/"CLAUDE.md",before); raise MigrationError("stale approval: managed state changed; review the recomputed diff")
    # Immediate pre-mutation validation includes the managed file and the complete legacy receipt.
    plan,root,before=preview(config,plugin_root)
    if approval != plan["approval_fingerprint"]:
        show_diff(config/"CLAUDE.md",before); raise MigrationError("stale approval: managed state changed; review the recomputed diff")
    current=config/"CLAUDE.md"; parent=root.parent; rollback=Path(tempfile.mkdtemp(prefix="migration-",dir=parent)); prior_root=rollback/"legacy-receipt"; prior_current=rollback/"CLAUDE.md"; had_current=current.exists(); retired: Path|None=None
    if had_current: core.atomic_write(prior_current,current.read_bytes())
    shutil.copytree(root,prior_root)
    try:
        if before is None: current.unlink(missing_ok=True)
        else: core.atomic_write(current,before.read_bytes())
        retired=parent/"claude-v0.1.5-retired"
        if retired.exists(): raise MigrationError("prior retired legacy receipt requires recovery")
        os.replace(root,retired)
        # Fresh setup owns a distinct versioned receipt and state pointer.
        result=core.install(config,plugin_root)
        return {"status":"migrated","retired_legacy_receipt":str(retired),"fresh_receipt":result["receipt"],"marketplace_preserved":True,"siblings_installed":False}
    except BaseException as error:
        current.unlink(missing_ok=True)
        if had_current: core.atomic_write(current,prior_current.read_bytes())
        if core.config_root(config).exists(): shutil.rmtree(core.config_root(config))
        core.pointer(config).unlink(missing_ok=True)
        if root.exists(): shutil.rmtree(root)
        if retired is not None and retired.exists(): shutil.rmtree(retired)
        shutil.copytree(prior_root,root)
        if not root.is_dir() or (retired is not None and retired.exists()): raise MigrationError("migration rollback left receipt state incomplete") from error
        raise MigrationError(f"migration failed; legacy state rollback verified: {error}") from error

def main() -> int:
    parser=argparse.ArgumentParser(description=__doc__); parser.add_argument("--plugin-root",type=Path,required=True); parser.add_argument("--config-dir",type=Path,default=Path(os.environ.get("CLAUDE_CONFIG_DIR","~/.claude"))); parser.add_argument("--apply",action="store_true"); parser.add_argument("--approval")
    args=parser.parse_args()
    try:
        config=args.config_dir.expanduser().resolve(strict=True); plan,_,before=preview(config,args.plugin_root); print(json.dumps(plan,indent=2,sort_keys=True)); show_diff(config/"CLAUDE.md",before)
        if not args.apply: print("preview only; no files changed"); return 0
        if not args.approval: raise MigrationError("--approval must equal the preview approval_fingerprint")
        print(json.dumps(apply(config,args.plugin_root,args.approval),indent=2,sort_keys=True)); return 0
    except (OSError,UnicodeError,MigrationError,core.SetupError) as error:
        print(f"migration failed: {error}"); return 1
if __name__=="__main__": raise SystemExit(main())
