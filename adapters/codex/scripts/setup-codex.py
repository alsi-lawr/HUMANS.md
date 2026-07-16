#!/usr/bin/env python3
"""Deterministically preview, install, or uninstall humans-md for Codex."""
from __future__ import annotations

import argparse
import copy
import datetime
import hashlib
import json
import os
import re
import shutil
import subprocess
import tempfile
import tomllib
from pathlib import Path, PurePosixPath


PLUGIN_ID = "humans-md@humans-md"
MARKETPLACE = "humans-md"
RECEIPT_SCHEMA = 2
REQUIRED_MODELS = {"gpt-5.6-sol", "gpt-5.6-terra", "gpt-5.6-luna"}
SCALAR_BEGIN = b"# >>> humans-md setup scalars >>>\n"
SCALAR_END = b"# <<< humans-md setup scalars <<<\n"
TABLE_BEGIN = b"\n# >>> humans-md setup tables >>>\n"
TABLE_END = b"# <<< humans-md setup tables <<<\n"
LEGACY_PATHS = (
    "skills/investigation-atomic",
    "skills/investigation-inspector-tree",
    "skills/investigation-review-atomic",
    "skills/investigation-review-dialogue",
    "skills/investigation-review-two-stage",
    "skills/investigation-solo",
    "skills/ticket-batch-subagent-pipeline",
    "skills/ticket-scratch-closeout",
    "skills/ticketed-repository-investigation",
    "skills/git-contribution",
    "skills/readme-generator",
    "skills/skill-generator",
    "skills/skill-packaging",
    "skills/casefile-workflow",
    "skills/casefile-investigate-solo",
    "skills/casefile-investigate-atomic",
    "skills/casefile-investigate-inspector-tree",
    "skills/casefile-review-atomic",
    "skills/casefile-review-dialogue",
    "skills/casefile-review-two-stage",
    "skills/casefile-implement-ticket-batch",
    "skills/casefile-switch-strategy",
    "skills/casefile-closeout",
    "skills/casefile-codex-setup",
    "skills/casefile-codex-cutover",
    "skills/casefile-codex-catalog-profile",
    "skills/casefile-codex-uninstall",
    "agents/inspector.toml",
    "agents/detective.toml",
    "agents/dialogue-review-chair.toml",
    "agents/dialogue-review-challenger.toml",
    "agents/atomic-ticket-reviewer.toml",
    "agents/verification-reviewer.toml",
    "agents/implementation-writer.toml",
    "planning-workflow",
    "casefile-workflow",
    "models-sol-v1.json",
    "models-sol-v1-bak.json",
)


class SetupError(RuntimeError):
    pass


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def canonical(value: object) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True, ensure_ascii=True) + "\n").encode(
        "ascii"
    )


def secure_dir(path: Path) -> None:
    path.mkdir(parents=True, exist_ok=True)
    os.chmod(path, 0o700)


def atomic_write(path: Path, data: bytes, mode: int = 0o600) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    temporary_path = Path(temporary)
    try:
        os.fchmod(descriptor, 0o600)
        with os.fdopen(descriptor, "wb") as stream:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        os.chmod(temporary_path, mode)
        os.replace(temporary_path, path)
    except BaseException:
        temporary_path.unlink(missing_ok=True)
        raise


def command(args: list[str], environment: dict[str, str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(args, env=environment, capture_output=True, text=True)


def checked(args: list[str], environment: dict[str, str]) -> str:
    result = command(args, environment)
    if result.returncode:
        detail = result.stderr.strip() or result.stdout.strip() or "no diagnostic output"
        raise SetupError(f"command failed ({result.returncode}): {' '.join(args)}: {detail}")
    return result.stdout


def checked_json(args: list[str], environment: dict[str, str]) -> dict:
    try:
        value = json.loads(checked(args, environment))
    except json.JSONDecodeError as error:
        raise SetupError(f"command returned invalid JSON: {' '.join(args)}") from error
    if not isinstance(value, dict):
        raise SetupError(f"command returned a non-object: {' '.join(args)}")
    return value


def plugin_root(path: Path) -> tuple[Path, dict]:
    root = path.expanduser().resolve(strict=True)
    try:
        manifest = json.loads((root / ".codex-plugin/plugin.json").read_text(encoding="ascii"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise SetupError(f"invalid installed plugin: {error}") from error
    if manifest.get("name") != "humans-md" or not isinstance(manifest.get("version"), str):
        raise SetupError("installed plugin identity is not humans-md")
    for relative in (
        "config/config-fragment.toml.in",
        "config/profiles.toml",
        "templates/AGENTS.md",
    ):
        if not (root / relative).is_file():
            raise SetupError(f"installed plugin lacks {relative}")
    return root, manifest


def discover(executable: str, environment: dict[str, str], version: str) -> dict:
    plugins = checked_json([executable, "plugin", "list", "--json"], environment)
    plugin = next(
        (item for item in plugins.get("installed", []) if item.get("pluginId") == PLUGIN_ID),
        None,
    )
    if not isinstance(plugin, dict) or not plugin.get("installed") or not plugin.get("enabled"):
        raise SetupError("humans-md must be installed and enabled before setup")
    if plugin.get("version") != version:
        raise SetupError(
            f"installed plugin version {plugin.get('version')!r} differs from package {version!r}"
        )
    markets = checked_json(
        [executable, "plugin", "marketplace", "list", "--json"], environment
    )
    if not any(item.get("name") == MARKETPLACE for item in markets.get("marketplaces", [])):
        raise SetupError("humans-md marketplace is not configured")
    return plugin


def resource(profile: Path, target: dict, path_key: str, hash_key: str) -> bytes:
    value = target.get(path_key)
    if not isinstance(value, str) or not value:
        raise SetupError(f"catalog target lacks {path_key}")
    path = (profile.parent / value).resolve(strict=True)
    if profile.parent.resolve() not in path.parents or not path.is_file() or path.is_symlink():
        raise SetupError(f"unsafe catalog resource: {value}")
    data = path.read_bytes()
    if not data or sha(data) != target.get(hash_key):
        raise SetupError(f"catalog resource hash mismatch: {value}")
    return data


def set_selector(model: dict, dotted: str) -> None:
    current = model
    parts = dotted.split(".")
    for part in parts[:-1]:
        current = current.get(part)
        if not isinstance(current, dict):
            raise SetupError(f"catalog selector is missing: {dotted}")
    if parts[-1] not in current:
        raise SetupError(f"catalog selector is missing: {dotted}")
    current[parts[-1]] = None


def catalog_override(raw: dict, profile_path: Path) -> tuple[bytes, list[str], list[str]]:
    profile = tomllib.loads(profile_path.read_text(encoding="ascii"))
    policy = profile.get("catalog", {})
    models = raw.get("models")
    if profile.get("schema_version") != 1 or not isinstance(models, list):
        raise SetupError("unsupported catalog or profile schema")
    if policy.get("id_field") != "slug" or policy.get("selector_fields") != [
        "multi_agent_version"
    ]:
        raise SetupError("unsupported catalog policy")
    by_id = {model.get("slug"): model for model in models if isinstance(model, dict)}
    if None in by_id or len(by_id) != len(models):
        raise SetupError("bundled catalog has missing or duplicate model IDs")
    missing = sorted(REQUIRED_MODELS - by_id.keys())
    if missing:
        raise SetupError(f"bundled catalog lacks required models: {', '.join(missing)}")

    result = copy.deepcopy(raw)
    output = {model["slug"]: model for model in result["models"]}
    patched: list[str] = []
    skipped: list[str] = []
    for target in policy.get("targets", []):
        model_id = target.get("id")
        if model_id not in output:
            skipped.append(str(model_id))
            continue
        model = output[model_id]
        model["base_instructions"] = resource(
            profile_path, target, "base_instructions_file", "base_instructions_sha256"
        ).decode("ascii")
        messages = json.loads(
            resource(
                profile_path, target, "model_messages_file", "model_messages_sha256"
            ).decode("ascii")
        )
        if not isinstance(messages, dict):
            raise SetupError(f"invalid model messages: {model_id}")
        model["model_messages"] = messages
        selectors = target.get("null_selectors", [])
        if not isinstance(selectors, list) or set(selectors) - {"multi_agent_version"}:
            raise SetupError(f"unsupported selector for {model_id}")
        for selector in selectors:
            set_selector(model, selector)
        patched.append(model_id)
    if not REQUIRED_MODELS <= set(patched):
        raise SetupError("required models were not patched")
    if any(output[model].get("multi_agent_version") is not None for model in REQUIRED_MODELS):
        raise SetupError("required V1 selectors were not cleared")
    return canonical(result), sorted(patched), sorted(skipped)


def marked(begin: bytes, payload: bytes, end: bytes) -> bytes:
    return begin + payload.rstrip(b"\n") + b"\n" + end


def config_candidate(current: bytes, root: Path, catalog: Path) -> tuple[bytes, dict[str, str]]:
    document = tomllib.loads(current.decode("utf-8")) if current else {}
    conflicts = {
        key
        for key in ("model_catalog_json", "features", "agents")
        if key in document
    }
    if conflicts:
        raise SetupError(
            "managed config already exists; clean it before setup: " + ", ".join(sorted(conflicts))
        )
    fragment = (root / "config/config-fragment.toml.in").read_text(encoding="ascii").replace(
        "__HUMANS_MD_PLUGIN_ROOT__", str(root).replace("\\", "/")
    )
    scalar_block = marked(
        SCALAR_BEGIN,
        f"model_catalog_json = {json.dumps(str(catalog))}\n".encode("utf-8"),
        SCALAR_END,
    )
    table_block = marked(TABLE_BEGIN, fragment.encode("ascii"), TABLE_END)
    data = scalar_block + current + table_block
    verify_config(data, root, catalog)
    return data, {"scalars": sha(scalar_block), "tables": sha(table_block)}


def unowned_config(data: bytes, expected: dict) -> bytes:
    ranges = []
    for name, begin, end in (
        ("scalars", SCALAR_BEGIN, SCALAR_END),
        ("tables", TABLE_BEGIN, TABLE_END),
    ):
        if data.count(begin) != 1 or data.count(end) != 1:
            raise SetupError(f"managed config block is missing or duplicated: {name}")
        start = data.index(begin)
        stop = data.index(end, start) + len(end)
        block = data[start:stop]
        if not isinstance(expected.get(name), str) or sha(block) != expected[name]:
            raise SetupError(f"managed config block changed after setup: {name}")
        ranges.append((start, stop))
    ranges.sort()
    if ranges[0][1] > ranges[1][0]:
        raise SetupError("managed config blocks overlap")
    result = data[: ranges[0][0]] + data[ranges[0][1] : ranges[1][0]] + data[ranges[1][1] :]
    if result:
        tomllib.loads(result.decode("utf-8"))
    return result


def verify_config(data: bytes, root: Path, catalog: Path) -> None:
    document = tomllib.loads(data.decode("utf-8"))
    profiles = tomllib.loads((root / "config/profiles.toml").read_text(encoding="ascii"))
    if document.get("model_catalog_json") != str(catalog):
        raise SetupError("catalog path is incorrect")
    features = document.get("features", {})
    if features.get("multi_agent") is not True or features.get("multi_agent_v2") is not False:
        raise SetupError("V1 feature flags are incorrect")
    agents = document.get("agents", {})
    for row in profiles.get("matrix_profiles", []):
        expected = root / row["agent_file"]
        actual = agents.get(row["profile"], {}).get("config_file")
        if actual != str(expected) or not expected.is_file():
            raise SetupError(f"role binding is incorrect: {row['profile']}")


def remove(path: Path) -> None:
    if path.is_symlink() or path.is_file():
        path.unlink(missing_ok=True)
    elif path.is_dir():
        shutil.rmtree(path)


def copy_path(source: Path, target: Path) -> None:
    target.parent.mkdir(parents=True, exist_ok=True)
    if source.is_symlink():
        target.symlink_to(os.readlink(source))
    elif source.is_dir():
        shutil.copytree(source, target, symlinks=True, copy_function=shutil.copy2)
    else:
        shutil.copy2(source, target)


def tree_hash(path: Path) -> str:
    if not path.exists() and not path.is_symlink():
        return "missing"
    values = []
    paths = [path]
    if path.is_dir() and not path.is_symlink():
        paths += sorted(path.rglob("*"), key=lambda item: item.relative_to(path).as_posix())
    for item in paths:
        relative = "." if item == path else item.relative_to(path).as_posix()
        if item.is_symlink():
            values.append((relative, "symlink", os.readlink(item)))
        elif item.is_dir():
            values.append((relative, "directory"))
        elif item.is_file():
            values.append((relative, "file", sha(item.read_bytes())))
    return sha(canonical(values))


def snapshot(home: Path, paths: list[Path], destination: Path) -> list[dict]:
    secure_dir(destination)
    entries = []
    for path in paths:
        relative = path.relative_to(home)
        existed = path.exists() or path.is_symlink()
        entries.append(
            {"path": relative.as_posix(), "existed": existed, "sha256": tree_hash(path)}
        )
        if existed:
            copy_path(path, destination / relative)
    return entries


def restore(home: Path, source: Path, entries: list[dict]) -> None:
    for entry in entries:
        pure = PurePosixPath(entry["path"])
        if pure.is_absolute() or ".." in pure.parts:
            raise SetupError("unsafe receipt path")
        path = home / Path(*pure.parts)
        remove(path)
        if entry["existed"]:
            copy_path(source / Path(*pure.parts), path)
        if tree_hash(path) != entry["sha256"]:
            raise SetupError(f"restore verification failed: {path}")


def managed(home: Path) -> list[Path]:
    return [
        home / "config.toml",
        home / "AGENTS.md",
        home / "models-humans-md-v1.json",
        *(home / relative for relative in LEGACY_PATHS),
    ]


def pointer(home: Path) -> Path:
    return home / "state/humans-md/current.json"


def prepare(root: Path, home: Path, executable: str) -> dict:
    root, manifest = plugin_root(root)
    environment = {**os.environ, "CODEX_HOME": str(home)}
    plugin = discover(executable, environment, manifest["version"])
    try:
        raw = json.loads(checked([executable, "debug", "models", "--bundled"], environment))
    except json.JSONDecodeError as error:
        raise SetupError("bundled catalog export is invalid JSON") from error
    catalog, patched, skipped = catalog_override(raw, root / "config/profiles.toml")
    catalog_path = home / "models-humans-md-v1.json"
    config_path = home / "config.toml"
    current = config_path.read_bytes() if config_path.is_file() else b""
    contract = (root / "templates/AGENTS.md").read_bytes()
    contract.decode("ascii")
    config, config_blocks = config_candidate(current, root, catalog_path)
    return {
        "root": root,
        "home": home,
        "executable": executable,
        "environment": environment,
        "version": plugin["version"],
        "config": config,
        "config_blocks": config_blocks,
        "contract": contract,
        "catalog": catalog,
        "patched": patched,
        "skipped": skipped,
        "legacy": [str(path) for path in managed(home)[3:] if path.exists()],
    }


def preview(plan: dict) -> dict:
    home = plan["home"]
    return {
        "operation": "install",
        "plugin_version": plan["version"],
        "config": str(home / "config.toml"),
        "contract": str(home / "AGENTS.md"),
        "catalog": str(home / "models-humans-md-v1.json"),
        "receipt_root": str(home / "backups/humans-md"),
        "patched_models": plan["patched"],
        "skipped_optional_models": plan["skipped"],
        "legacy_paths_removed": plan["legacy"],
        "config_ownership": "hash-bound blocks",
        "restart_required": True,
    }


def doctor(plan: dict) -> None:
    result = command(
        [
            plan["executable"],
            "--strict-config",
            "doctor",
            "--summary",
            "--ascii",
            "--no-color",
        ],
        plan["environment"],
    )
    if not re.search(r"\[ok\]\s+config\s+loaded", result.stdout + result.stderr):
        raise SetupError(result.stderr.strip() or result.stdout.strip() or "config load failed")


def verify_effective_catalog(plan: dict) -> None:
    document = checked_json(
        [plan["executable"], "debug", "models"], plan["environment"]
    )
    models = document.get("models")
    if not isinstance(models, list):
        raise SetupError("effective catalog has no model list")
    selected = {
        model.get("slug"): model for model in models if isinstance(model, dict)
    }
    for model_id in REQUIRED_MODELS:
        model = selected.get(model_id)
        if not isinstance(model, dict) or model.get("multi_agent_version") is not None:
            raise SetupError(f"effective catalog did not activate V1 for {model_id}")


def install(plan: dict) -> dict:
    home = plan["home"]
    if pointer(home).exists():
        raise SetupError("an active humans-md receipt already exists")
    install_id = datetime.datetime.now(datetime.UTC).strftime("%Y%m%dT%H%M%SZ-") + sha(
        plan["config"] + plan["contract"] + plan["catalog"]
    )[:12]
    receipt_dir = home / "backups/humans-md" / install_id
    secure_dir(receipt_dir.parent)
    if receipt_dir.exists():
        raise SetupError(f"receipt already exists: {receipt_dir}")
    secure_dir(receipt_dir)
    paths = managed(home)
    before = snapshot(home, paths, receipt_dir / "before")
    try:
        config, contract, catalog = paths[:3]
        atomic_write(config, plan["config"], 0o600)
        atomic_write(contract, plan["contract"], 0o600)
        atomic_write(catalog, plan["catalog"], 0o600)
        for path in paths[3:]:
            remove(path)
        verify_config(config.read_bytes(), plan["root"], catalog)
        if contract.read_bytes() != plan["contract"] or catalog.read_bytes() != plan["catalog"]:
            raise SetupError("written setup bytes differ from preview")
        doctor(plan)
        verify_effective_catalog(plan)
        if config.read_bytes() != plan["config"]:
            raise SetupError("Codex changed config during verification")
        receipt = {
            "schema_version": RECEIPT_SCHEMA,
            "status": "installed",
            "install_id": install_id,
            "plugin_version": plan["version"],
            "before": before,
            "after": {
                "config": sha(config.read_bytes()),
                "contract": sha(contract.read_bytes()),
                "catalog": sha(catalog.read_bytes()),
            },
            "config_blocks": plan["config_blocks"],
            "remove_plugin": True,
            "remove_marketplace": True,
        }
        receipt_data = canonical(receipt)
        receipt_path = receipt_dir / "receipt.json"
        atomic_write(receipt_path, receipt_data)
        secure_dir(pointer(home).parent)
        atomic_write(
            pointer(home),
            canonical({"receipt": str(receipt_path), "sha256": sha(receipt_data)}),
        )
        return {"status": "installed", "receipt": str(receipt_path), "restart_required": True}
    except BaseException as error:
        restore(home, receipt_dir / "before", before)
        pointer(home).unlink(missing_ok=True)
        atomic_write(
            receipt_dir / "failure.json",
            canonical({"status": "failed", "error": str(error), "rollback_verified": True}),
        )
        raise SetupError(f"setup failed; rollback verified: {error}") from error


def read_json(path: Path) -> tuple[dict, bytes]:
    try:
        data = path.read_bytes()
        value = json.loads(data)
    except (OSError, json.JSONDecodeError) as error:
        raise SetupError(f"invalid receipt: {error}") from error
    if not isinstance(value, dict):
        raise SetupError("invalid receipt object")
    return value, data


def receipt(home: Path, explicit: Path | None) -> tuple[Path, dict]:
    if explicit is None:
        selected, _ = read_json(pointer(home))
        path = Path(selected.get("receipt", ""))
        expected = selected.get("sha256")
    else:
        path = explicit.expanduser().resolve(strict=True)
        expected = None
    path = path.resolve(strict=True)
    root = (home / "backups/humans-md").resolve()
    if root not in path.parents or path.name != "receipt.json":
        raise SetupError("receipt is outside the durable backup root")
    value, data = read_json(path)
    if expected is not None and expected != sha(data):
        raise SetupError("receipt pointer hash mismatch")
    if (
        value.get("status") != "installed"
        or value.get("schema_version") != RECEIPT_SCHEMA
    ):
        raise SetupError("receipt is not an installed receipt")
    return path, value


def uninstall_preview(path: Path, value: dict) -> dict:
    return {
        "operation": "uninstall",
        "receipt": str(path),
        "restore_snapshot": str(path.parent / "before"),
        "managed_path_count": len(value["before"]),
        "preserve_unowned_config": True,
        "remove_plugin": value["remove_plugin"],
        "remove_marketplace": value["remove_marketplace"],
    }


def uninstall(home: Path, executable: str, path: Path, value: dict) -> dict:
    current = managed(home)
    expected = value["after"]
    config = current[0]
    if not config.is_file():
        raise SetupError(f"managed config is missing: {config}")
    restored_config = unowned_config(config.read_bytes(), value.get("config_blocks", {}))
    for target, key in zip(current[1:3], ("contract", "catalog"), strict=True):
        if not target.is_file() or sha(target.read_bytes()) != expected[key]:
            raise SetupError(f"managed file changed after setup: {target}")
    if any(target.exists() for target in current[3:]):
        raise SetupError("a superseded direct path reappeared after setup")
    rollback_dir = home / "backups/humans-md" / (
        "uninstall-" + datetime.datetime.now(datetime.UTC).strftime("%Y%m%dT%H%M%SZ")
    )
    secure_dir(rollback_dir.parent)
    rollback_paths = current + [
        home / "plugins/cache/humans-md",
        home / ".tmp/marketplaces/humans-md",
        pointer(home),
    ]
    rollback = snapshot(home, rollback_paths, rollback_dir / "before")
    environment = {**os.environ, "CODEX_HOME": str(home)}
    try:
        before = value["before"]
        if not before or before[0].get("path") != "config.toml":
            raise SetupError("receipt config inventory is invalid")
        restore(home, path.parent / "before", before[1:])
        if restored_config or before[0]["existed"]:
            atomic_write(config, restored_config, 0o600)
        else:
            remove(config)
        if value["remove_plugin"]:
            checked([executable, "plugin", "remove", PLUGIN_ID, "--json"], environment)
        if value["remove_marketplace"]:
            checked(
                [executable, "plugin", "marketplace", "remove", MARKETPLACE, "--json"],
                environment,
            )
        pointer(home).unlink(missing_ok=True)
        result = {"status": "uninstalled", "install_receipt": str(path)}
        atomic_write(rollback_dir / "receipt.json", canonical(result))
        return result
    except BaseException as error:
        restore(home, rollback_dir / "before", rollback)
        atomic_write(
            rollback_dir / "failure.json",
            canonical({"status": "failed", "error": str(error), "rollback_verified": True}),
        )
        raise SetupError(f"uninstall failed; rollback verified: {error}") from error


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="operation", required=True)
    install_parser = subparsers.add_parser("install")
    install_parser.add_argument("--plugin-root", type=Path, required=True)
    default_home = Path(os.environ.get("CODEX_HOME", "~/.codex"))
    install_parser.add_argument("--codex-home", type=Path, default=default_home)
    install_parser.add_argument("--codex-executable", default=shutil.which("codex"))
    install_parser.add_argument("--apply", action="store_true")
    uninstall_parser = subparsers.add_parser("uninstall")
    uninstall_parser.add_argument("--codex-home", type=Path, default=default_home)
    uninstall_parser.add_argument("--codex-executable", default=shutil.which("codex"))
    uninstall_parser.add_argument("--receipt", type=Path)
    uninstall_parser.add_argument("--apply", action="store_true")
    arguments = parser.parse_args()
    try:
        home = arguments.codex_home.expanduser().resolve(strict=True)
        if not arguments.codex_executable:
            raise SetupError("Codex executable was not found")
        if arguments.operation == "install":
            plan = prepare(arguments.plugin_root, home, arguments.codex_executable)
            print(json.dumps(preview(plan), indent=2, sort_keys=True))
            if not arguments.apply:
                print("preview only; no files changed")
                return 0
            print(json.dumps(install(plan), indent=2, sort_keys=True))
            return 0
        receipt_path, value = receipt(home, arguments.receipt)
        print(json.dumps(uninstall_preview(receipt_path, value), indent=2, sort_keys=True))
        if not arguments.apply:
            print("preview only; no files changed")
            return 0
        print(json.dumps(uninstall(home, arguments.codex_executable, receipt_path, value), indent=2))
        return 0
    except (OSError, UnicodeError, ValueError, SetupError, tomllib.TOMLDecodeError) as error:
        print(f"{arguments.operation} failed: {error}")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
