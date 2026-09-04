"""Migrate receipt-owned catalogs and profile registrations without reinstalling the runtime."""
from __future__ import annotations

import copy
import datetime
import hashlib
import json
import os
import tempfile
import tomllib
from pathlib import Path


def profile_config(setup, current: bytes, root: Path) -> bytes:
    fragment = (root / "config/config-fragment.toml.in").read_text(encoding="ascii").encode("ascii")
    profiles = setup.split_owned_tables(fragment, only_agents=True)[1]
    profiles = profiles.replace(
        b"__HUMANS_MD_PLUGIN_ROOT__",
        json.dumps(str(root).replace("\\", "/"), ensure_ascii=True)[1:-1].encode("ascii"),
    )
    result = setup.split_owned_tables(current, only_agents=True)[0]
    if setup.TABLE_END in result:
        return result.replace(setup.TABLE_END, profiles + setup.TABLE_END, 1)
    separator = b"\n" if result and not result.endswith(b"\n") else b""
    return result + separator + profiles


def changes(before: dict, after: dict) -> dict:
    return {
        "added": sorted(after.keys() - before.keys()),
        "removed": sorted(before.keys() - after.keys()),
        "updated": {
            name: sorted(
                field for field in before[name].keys() | after[name].keys()
                if field not in before[name] or field not in after[name]
                or before[name][field] != after[name][field]
            )
            for name in sorted(before.keys() & after.keys()) if before[name] != after[name]
        },
    }


def prepare(setup, root: Path, home: Path, executable: str) -> dict:
    root, manifest = setup.plugin_root(root)
    pointer_path = setup.pointer(home)
    pointer_before = pointer_path.read_bytes()
    receipt_path, receipt = setup.receipt(home, None)
    config_path = home / "config.toml"
    current = config_path.read_bytes()
    document = tomllib.loads(current.decode("utf-8"))
    features = document.get("features", {})
    flags = (features.get("multi_agent"), features.get("multi_agent_v2"))
    if flags == (True, False):
        version = "v1"
    elif flags == (False, True):
        version = "v2"
    else:
        raise setup.SetupError("migration requires exactly one active multi-agent runtime")
    if setup.receipt_multi_agent_version(receipt) != version:
        raise setup.SetupError(
            "active runtime and setup receipt disagree; explicitly reconcile with codex-setup "
            "before migrating models"
        )
    catalog_path = home / f"models-casefile-{version}.json"
    configured = document.get("model_catalog_json")
    if not isinstance(configured, str) or Path(configured).expanduser().resolve() != catalog_path.resolve():
        raise setup.SetupError("migration requires the selected Casefile-owned catalog")
    if catalog_path.name not in {entry["path"] for entry in receipt["before"]}:
        raise setup.SetupError("receipt lacks catalog recovery state; use codex-setup to reconcile")
    previous_catalog = setup.read_catalog(catalog_path)
    setup.catalog_ids(previous_catalog, "selected Casefile catalog")
    observed = {
        config_path: current,
        catalog_path: catalog_path.read_bytes(),
        pointer_path: pointer_before,
        receipt_path: receipt_path.read_bytes(),
    }
    profiles_path = root / "config/profiles.toml"
    catalog, identifiers = setup.catalog_replacement(profiles_path, version)
    config = profile_config(setup, current, root)
    setup.verify_config(config, root, catalog_path, version)
    environment = {**os.environ, "CODEX_HOME": str(home)}
    setup.discover(executable, environment, manifest["version"])
    projection = setup.acquire_models(executable, profiles_path)
    setup.require_available_models(projection, setup.pinned_models(profiles_path))
    for path, data in observed.items():
        if path.read_bytes() != data:
            raise setup.SetupError(f"migration input changed during preview: {path}; preview again")
    candidate = {config_path: config, catalog_path: catalog}
    digest = hashlib.sha256(setup.canonical({
        "root": str(root),
        "plugin_version": manifest["version"],
        "observed": {str(path): hashlib.sha256(data).hexdigest() for path, data in observed.items()},
        "candidate": {str(path): hashlib.sha256(data).hexdigest() for path, data in candidate.items()},
    })).hexdigest()
    return {
        "root": root,
        "home": home,
        "executable": executable,
        "environment": environment,
        "version": manifest["version"],
        "multi_agent_version": version,
        "catalog_models": identifiers,
        "writer_defaults": [
            {"strategy": row["strategy_id"], "model": row["model"], "reasoning": row["reasoning"]}
            for row in tomllib.loads(profiles_path.read_text(encoding="ascii"))["matrix_profiles"]
            if row["role"] == "implementation-writer"
        ],
        "previous": (receipt_path, receipt),
        "observed": observed,
        "candidate": candidate,
        "approval_digest": digest,
    }


def preview(setup, plan: dict) -> dict:
    home = plan["home"]
    config, catalog = setup.managed(home, plan["multi_agent_version"])
    old_profiles = tomllib.loads(plan["observed"][config].decode("utf-8"))["agents"]
    new_profiles = tomllib.loads(plan["candidate"][config].decode("utf-8"))["agents"]
    before = json.loads(plan["observed"][catalog])["models"]
    after = json.loads(plan["candidate"][catalog])["models"]
    return {
        "operation": "migrate-models",
        "plugin_root": str(plan["root"]),
        "plugin_version": plan["version"],
        "catalog": str(catalog),
        "config": str(config),
        "multi_agent_version": plan["multi_agent_version"],
        "writer_defaults": plan["writer_defaults"],
        "catalog_changes": changes({m["slug"]: m for m in before}, {m["slug"]: m for m in after}),
        "profile_changes": changes(
            {k: v for k, v in old_profiles.items() if k.startswith("casefile-")},
            {k: v for k, v in new_profiles.items() if k.startswith("casefile-")},
        ),
        "changed_files": [
            str(path) for path, data in plan["candidate"].items()
            if data != plan["observed"][path]
        ],
        "approval_digest": plan["approval_digest"],
        "restart_required": any(
            data != plan["observed"][path] for path, data in plan["candidate"].items()
        ),
        "preserved": [
            "multi-agent runtime", "MCP binding and executable", "root model and effort",
            "unrelated configuration", "other catalog variant", "investigation writer bindings",
            "original pre-install recovery state", "Codex-owned models_cache.json",
        ],
    }


def apply(setup, plan: dict, expected_digest: str | None) -> dict:
    if expected_digest != plan["approval_digest"]:
        raise setup.SetupError("migration requires --expect-digest from the approved current preview")
    for path, data in plan["observed"].items():
        if path.read_bytes() != data:
            raise setup.SetupError(f"migration input changed after preview: {path}; preview again")
    previous_path, previous = plan["previous"]
    changed = [
        path for path, data in plan["candidate"].items() if data != plan["observed"][path]
    ]
    if not changed:
        return {"status": "unchanged", "receipt": str(previous_path), "restart_required": False}
    home = plan["home"]
    receipt_dir = Path(tempfile.mkdtemp(
        prefix=datetime.datetime.now(datetime.UTC).strftime("%Y%m%dT%H%M%SZ-models-"),
        dir=home / "backups/casefile",
    ))
    setup.secure_dir(receipt_dir)
    rollback = setup.snapshot(home, [*changed, setup.pointer(home)], receipt_dir / "rollback")
    setup.copy_path(previous_path.parent / "before", receipt_dir / "before")
    for path, data in plan["observed"].items():
        if path.read_bytes() != data:
            raise setup.SetupError(f"migration input changed before apply: {path}; preview again")
    try:
        for path in changed:
            setup.atomic_write(path, plan["candidate"][path])
        config, catalog = setup.managed(home, plan["multi_agent_version"])
        setup.verify_config(config.read_bytes(), plan["root"], catalog, plan["multi_agent_version"])
        setup.verify_effective_catalog(plan)
        setup.doctor(plan)
        for path, data in plan["candidate"].items():
            if path.read_bytes() != data:
                raise setup.SetupError(f"migration output changed during verification: {path}")
        receipt = copy.deepcopy(previous)
        receipt.update({
            "schema_version": setup.RECEIPT_SCHEMA,
            "install_id": receipt_dir.name,
            "operation": "migrate-models",
            "model_plugin_version": plan["version"],
        })
        receipt_path = receipt_dir / "receipt.json"
        setup.atomic_write(receipt_path, setup.canonical(receipt))
        setup.atomic_write(setup.pointer(home), setup.canonical({"receipt": str(receipt_path)}))
        return {"status": "migrated", "receipt": str(receipt_path), "restart_required": True}
    except BaseException as error:
        setup.restore(home, receipt_dir / "rollback", rollback)
        setup.atomic_write(receipt_dir / "failure.json", setup.canonical({
            "status": "failed", "error": str(error), "rollback_verified": True,
        }))
        raise setup.SetupError(f"model migration failed; rollback verified: {error}") from error
