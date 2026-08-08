#!/usr/bin/env python3
"""Offer, persist, and resolve the active Codex Casefile writer binding."""
from __future__ import annotations

import argparse
import importlib.util
import json
import os
import shutil
import subprocess
import sys
import tomllib
from pathlib import Path, PurePosixPath

try:
    import list_codex_models
except ModuleNotFoundError:
    _lister_path = Path(__file__).resolve().with_name("list-codex-models.py")
    _lister_spec = importlib.util.spec_from_file_location(
        "list_codex_models", _lister_path
    )
    if _lister_spec is None or _lister_spec.loader is None:
        raise
    list_codex_models = importlib.util.module_from_spec(_lister_spec)
    _lister_spec.loader.exec_module(list_codex_models)


RECOMMENDED_MODEL = "gpt-5.6-sol"
RECOMMENDED_EFFORT = "high"
STRATEGIES = (
    "casefile-implement-ticket-batch",
    "casefile-implement-ticket-batch-look-ahead",
    "casefile-implement-pipeline",
)
RUNTIMES = ("v1", "v2")


class BindingError(RuntimeError):
    pass


def checked(args: list[str], environment: dict[str, str] | None = None) -> str:
    result = subprocess.run(
        args,
        env=environment,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="strict",
    )
    if result.returncode:
        detail = result.stderr.strip() or result.stdout.strip() or "no diagnostic output"
        raise BindingError(f"command failed ({result.returncode}): {' '.join(args)}: {detail}")
    return result.stdout


def load_profiles(path: Path) -> dict:
    try:
        value = tomllib.loads(path.read_text(encoding="ascii"))
    except (OSError, UnicodeError, tomllib.TOMLDecodeError) as error:
        raise BindingError(f"invalid Codex profile catalog: {error}") from error
    if value.get("schema_version") != 1 or value.get("adapter") != "codex":
        raise BindingError("unsupported Codex profile catalog")
    return value


def default_profiles_path() -> Path:
    script = Path(__file__).resolve()
    candidates = (
        script.parent.parent / "config/profiles.toml",
        script.parent.parent / "profiles.toml",
    )
    return next((path for path in candidates if path.is_file()), candidates[0])


def active_runtime(home: Path) -> str:
    try:
        config = tomllib.loads((home / "config.toml").read_text(encoding="utf-8"))
    except (OSError, UnicodeError, tomllib.TOMLDecodeError) as error:
        raise BindingError(f"Codex Casefile setup is not readable: {error}") from error
    features = config.get("features", {})
    state = (features.get("multi_agent"), features.get("multi_agent_v2"))
    if state == (True, False):
        return "v1"
    if state == (False, True):
        return "v2"
    raise BindingError("Codex Casefile setup must enable exactly one supported multi-agent runtime")


def read_json(path: Path, label: str) -> dict:
    if path.is_symlink() or not path.is_file():
        raise BindingError(f"{label} is missing or unsafe: {path}")
    try:
        value = json.loads(path.read_bytes())
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise BindingError(f"{label} is invalid: {error}") from error
    if not isinstance(value, dict):
        raise BindingError(f"{label} is not an object")
    return value


def catalog_models(document: dict, label: str) -> dict[str, dict]:
    models = document.get("models")
    if not isinstance(models, list):
        raise BindingError(f"{label} has no model list")
    by_id: dict[str, dict] = {}
    for model in models:
        identifier = model.get("slug") if isinstance(model, dict) else None
        if not isinstance(identifier, str) or not identifier:
            raise BindingError(f"{label} has a model without an ID")
        if identifier in by_id:
            raise BindingError(f"{label} has duplicate model IDs")
        by_id[identifier] = model
    return by_id


def owned_catalog(home: Path, runtime: str) -> dict:
    config_path = home / "config.toml"
    try:
        config = tomllib.loads(config_path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, tomllib.TOMLDecodeError) as error:
        raise BindingError(f"Codex Casefile setup is not readable: {error}") from error
    expected = (home / f"models-casefile-{runtime}.json").resolve()
    configured = config.get("model_catalog_json")
    if not isinstance(configured, str) or Path(configured).expanduser().resolve() != expected:
        raise BindingError("Codex is not configured with the active Casefile-owned catalog")
    return read_json(expected, "active Casefile catalog")


def active_catalog(executable: str, home: Path) -> dict:
    profile_path = default_profiles_path()
    try:
        projection = list_codex_models.listing(executable, profile_path)
    except list_codex_models.ProjectionError as error:
        raise BindingError(f"Codex model availability failed: {error}") from error
    projected = catalog_models(projection, "Codex model projection")
    configured = catalog_models(
        owned_catalog(home, active_runtime(home)), "active Casefile catalog"
    )
    return {
        "models": [
            model for identifier, model in configured.items() if identifier in projected
        ]
    }


def resolution_rows(profiles: dict, runtime: str, model: str, effort: str) -> list[dict]:
    if runtime == "v1":
        matched = [
            row
            for row in profiles.get("writer_profiles", [])
            if row.get("multi_agent_version") == runtime
            and row.get("model") == model
            and row.get("reasoning") == effort
        ]
        if len(matched) != 1 or set(matched[0].get("strategy_ids", [])) != set(STRATEGIES):
            return []
        rows = [{**matched[0], "strategy_id": strategy} for strategy in STRATEGIES]
    elif runtime == "v2":
        rows = [
            row
            for row in profiles.get("writer_runtime_overrides", [])
            if row.get("multi_agent_version") == runtime
            and row.get("model_override") is True
            and row.get("reasoning_override") is True
            and isinstance(row.get("fork_turns"), int)
            and row["fork_turns"] > 0
        ]
    else:
        raise BindingError(f"unsupported multi-agent runtime: {runtime}")
    by_strategy = {row.get("strategy_id"): row for row in rows}
    if set(by_strategy) != set(STRATEGIES) or len(rows) != len(STRATEGIES):
        return []
    if any(row.get("role") != "implementation-writer" for row in rows):
        return []
    return [by_strategy[strategy] for strategy in STRATEGIES]


def offered_pairs(catalog: dict, profiles: dict, runtime: str) -> list[dict]:
    offered = []
    for model in catalog["models"]:
        if not isinstance(model, dict) or model.get("visibility") != "list":
            continue
        selector = model.get("multi_agent_version")
        if (runtime == "v1" and selector is not None) or (runtime == "v2" and selector != "v2"):
            continue
        levels = model.get("supported_reasoning_levels")
        if not isinstance(levels, list):
            continue
        efforts = []
        for level in levels:
            effort = level.get("effort") if isinstance(level, dict) else None
            if isinstance(effort, str) and effort and effort not in efforts:
                efforts.append(effort)
        for effort in efforts:
            rows = resolution_rows(profiles, runtime, model["slug"], effort)
            if not rows:
                continue
            mode = "named_profile" if runtime == "v1" else "runtime_override"
            value = ";".join(
                f"{row['strategy_id']}={row['profile']}" for row in rows
            )
            offered.append(
                {
                    "model": model["slug"],
                    "display_name": model.get("display_name", model["slug"]),
                    "reasoning_effort": effort,
                    "resolution": {"mode": mode, "value": value},
                    "recommended": model["slug"] == RECOMMENDED_MODEL
                    and effort == RECOMMENDED_EFFORT,
                }
            )
    return offered


def offer(
    executable: str,
    home: Path,
    profiles_path: Path,
) -> dict:
    runtime = active_runtime(home)
    pairs = offered_pairs(active_catalog(executable, home), load_profiles(profiles_path), runtime)
    recommendation = next((pair for pair in pairs if pair["recommended"]), None)
    if not pairs:
        raise BindingError("no model/effort pair is currently selectable")
    return {
        "multi_agent_version": runtime,
        "recommendation": {
            "model": RECOMMENDED_MODEL,
            "reasoning_effort": RECOMMENDED_EFFORT,
            "available": recommendation is not None,
        },
        "pairs": pairs,
        "selection_required": True,
    }


def safe_investigation(value: str) -> str:
    path = PurePosixPath(value.replace("\\", "/"))
    if path.is_absolute() or ".." in path.parts or not path.parts:
        raise BindingError("investigation path must be a contained relative path")
    return path.as_posix().rstrip("/")


def selected_pair(result: dict, model: str, effort: str) -> dict:
    pair = next(
        (
            pair
            for pair in result["pairs"]
            if pair["model"] == model and pair["reasoning_effort"] == effort
        ),
        None,
    )
    if pair is None:
        raise BindingError(
            f"{model}/{effort} is not currently selectable; rerun offer and obtain a new "
            "explicit choice"
        )
    return pair


def binding_source(pair: dict) -> str:
    resolution = pair["resolution"]
    values = {
        "adapter": "codex",
        "role": "implementation-writer",
        "model": pair["model"],
        "reasoning_effort": pair["reasoning_effort"],
        "mode": resolution["mode"],
        "value": resolution["value"],
    }
    return (
        "schema_version = 1\n"
        f"adapter = {json.dumps(values['adapter'])}\n"
        f"role = {json.dumps(values['role'])}\n"
        f"model = {json.dumps(values['model'])}\n"
        f"reasoning_effort = {json.dumps(values['reasoning_effort'])}\n\n"
        "[resolution]\n"
        f"mode = {json.dumps(values['mode'])}\n"
        f"value = {json.dumps(values['value'])}\n"
    )


def selection_request(
    investigation: str,
    pair: dict,
) -> dict:
    source = binding_source(pair)
    return {
        "path": f"{investigation}/strategy/bindings.toml",
        "model": pair["model"],
        "reasoning_effort": pair["reasoning_effort"],
        "resolution": pair["resolution"],
        "request": {
            "investigation": investigation,
            "binding_source": source,
        },
        "persisted": False,
        "provider_preview_tool": "casefile_preview_writer_binding",
        "provider_apply_tool": "casefile_apply_writer_binding",
        "approval_required": False,
    }


def require_writer_progress(
    casefile_executable: str,
    planning_root: Path,
    investigation: str,
    ticket_id: str,
) -> None:
    checked(
        [
            casefile_executable,
            "--root",
            str(planning_root),
            "require-writer-progress",
            "--investigation",
            investigation,
            "--ticket-id",
            ticket_id,
        ]
    )


def binding_projection(
    casefile_executable: str,
    planning_root: Path,
    investigation: str,
    strategy_id: str,
) -> dict:
    try:
        result = json.loads(
            checked(
                [
                    casefile_executable,
                    "--root",
                    str(planning_root),
                    "project-writer-binding",
                    "--investigation",
                    investigation,
                    "--strategy-id",
                    strategy_id,
                ]
            )
        )
    except json.JSONDecodeError as error:
        raise BindingError("Casefile writer projection returned invalid JSON") from error
    if not isinstance(result, dict):
        raise BindingError("Casefile writer projection is not an object")
    return result


def resolve_spawn(
    executable: str,
    home: Path,
    profiles_path: Path,
    casefile_executable: str,
    planning_root: Path,
    investigation: str,
    strategy_id: str,
    ticket_id: str,
) -> dict:
    investigation = safe_investigation(investigation)
    if strategy_id not in STRATEGIES:
        raise BindingError(f"unsupported implementation strategy: {strategy_id}")
    require_writer_progress(
        casefile_executable,
        planning_root,
        investigation,
        ticket_id,
    )
    profiles = load_profiles(profiles_path)
    projection = binding_projection(
        casefile_executable,
        planning_root,
        investigation,
        strategy_id,
    )
    if projection.get("strategy_id") != strategy_id or projection.get("adapter") != "codex":
        raise BindingError("Casefile writer projection does not match the selected strategy")
    state = projection.get("binding")
    state_name = state.get("state") if isinstance(state, dict) else None
    if state_name not in {"absent", "resolved"}:
        if state_name in {"pending", "unresolved", "invalid"}:
            raise BindingError(
                f"canonical writer binding state is {state_name}; stop before delegation and "
                "repair or explicitly reselect while implementation is inactive"
            )
        raise BindingError("Casefile writer projection has an invalid binding state")
    effective = state.get("effective")
    if not isinstance(effective, dict):
        raise BindingError("Casefile writer projection lacks an effective pair")
    model, effort, source = (
        effective.get("model"),
        effective.get("reasoning_effort"),
        effective.get("source"),
    )
    expected_source = "matrix" if state_name == "absent" else "binding"
    if (
        not isinstance(model, str)
        or not model
        or not isinstance(effort, str)
        or not effort
        or source != expected_source
    ):
        raise BindingError("Casefile writer projection has an invalid effective pair")
    current = offer(executable, home, profiles_path)
    try:
        pair = selected_pair(current, model, effort)
    except BindingError as error:
        raise BindingError(
            f"effective writer {model}/{effort} is unavailable; stop before delegation, "
            "rerun offer, "
            "and obtain explicit reselection while implementation is inactive"
        ) from error
    rows = resolution_rows(profiles, current["multi_agent_version"], model, effort)
    row = next(item for item in rows if item["strategy_id"] == strategy_id)
    spawn = {"agent_type": row["profile"]}
    if current["multi_agent_version"] == "v2":
        spawn.update(
            {
                "model": model,
                "reasoning_effort": effort,
                "fork_turns": str(row["fork_turns"]),
            }
        )
    return {
        "model": model,
        "reasoning_effort": effort,
        "binding_source": source,
        "multi_agent_version": current["multi_agent_version"],
        "spawn": spawn,
        "revalidated": True,
    }


def parser() -> argparse.ArgumentParser:
    value = argparse.ArgumentParser(description=__doc__)
    value.add_argument(
        "--codex-home",
        type=Path,
        default=Path(os.environ.get("CODEX_HOME", "~/.codex")),
    )
    value.add_argument("--codex-executable", default=shutil.which("codex"))
    value.add_argument("--profiles", type=Path, default=default_profiles_path())
    subparsers = value.add_subparsers(dest="operation", required=True)
    subparsers.add_parser("offer")
    select = subparsers.add_parser("select")
    select.add_argument("--casefile-executable", default=shutil.which("casefile"))
    select.add_argument("--planning-root", type=Path, required=True)
    select.add_argument("--investigation", required=True)
    select.add_argument("--model", required=True)
    select.add_argument("--reasoning-effort", required=True)
    resolve = subparsers.add_parser("resolve")
    resolve.add_argument("--casefile-executable", default=shutil.which("casefile"))
    resolve.add_argument("--planning-root", type=Path, required=True)
    resolve.add_argument("--investigation", required=True)
    resolve.add_argument("--strategy-id", choices=STRATEGIES, required=True)
    resolve.add_argument("--ticket-id", required=True)
    return value


def main() -> int:
    arguments = parser().parse_args()
    try:
        if not arguments.codex_executable:
            raise BindingError("Codex executable was not found")
        home = arguments.codex_home.expanduser().resolve(strict=True)
        profiles = arguments.profiles.expanduser().resolve(strict=True)
        if arguments.operation == "offer":
            result = offer(arguments.codex_executable, home, profiles)
        elif arguments.operation == "select":
            investigation = safe_investigation(arguments.investigation)
            pair = selected_pair(
                offer(arguments.codex_executable, home, profiles),
                arguments.model,
                arguments.reasoning_effort,
            )
            result = selection_request(
                investigation,
                pair,
            )
        else:
            if not arguments.casefile_executable:
                raise BindingError("Casefile executable was not found")
            result = resolve_spawn(
                arguments.codex_executable,
                home,
                profiles,
                arguments.casefile_executable,
                arguments.planning_root.expanduser().resolve(strict=True),
                arguments.investigation,
                arguments.strategy_id,
                arguments.ticket_id,
            )
        print(json.dumps(result, indent=2, sort_keys=True))
        return 0
    except (BindingError, OSError, UnicodeError, ValueError) as error:
        print(f"writer binding {arguments.operation} failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
