#!/usr/bin/env python3
"""Return arguments for one Codex spawn after checking its current explicit assignment."""
from __future__ import annotations

import argparse
import json
import runpy
import subprocess
import sys
from pathlib import Path


class ResolutionError(ValueError):
    pass


def resolve(arguments: argparse.Namespace, evidence: str) -> dict:
    if not arguments.planning_root.is_absolute() or not arguments.casefile_executable.is_absolute():
        raise ResolutionError("Use the absolute planning root and installed Casefile executable path.")
    writer = arguments.phase == "implementation" and arguments.role == "implementation-writer"
    if writer and not arguments.ticket_id:
        raise ResolutionError("An implementation writer requires its exact accepted ticket ID.")

    scripts = Path(__file__).resolve().parent
    runtime = scripts / "casefile_runtime.py"
    if not runtime.is_file():
        runtime = scripts.parent.parent / "shared/casefile_runtime.py"
    run_checked = runpy.run_path(str(runtime))["run_checked"]
    casefile = [arguments.casefile_executable, "--root", arguments.planning_root]
    target = [
        "--investigation", arguments.investigation, "--phase", arguments.phase,
        "--strategy-id", arguments.strategy_id,
    ]
    project = [*casefile, "project-strategy-assignments", *target]
    projection = json.loads(run_checked(project, timeout=15).stdout)
    if projection["adapter"] != "codex":
        raise ResolutionError("This spawn check requires a selected Codex strategy.")
    role = projection["roles"].get(arguments.role)
    if role is None:
        raise ResolutionError(f"{arguments.role} is outside the selected strategy.")
    assignment = role["selection"]
    if assignment is None or assignment["source"] not in {
        "strategy_assignment", "legacy_writer_binding",
    }:
        raise ResolutionError(f"{arguments.role} needs an explicit model and effort selection.")

    profiles = role["platform_profiles"]
    profile = arguments.platform_profile
    if profile is None and len(profiles) == 1:
        profile = profiles[0]
    if profile not in profiles:
        raise ResolutionError(f"Choose a named registration for {arguments.role} from the selected matrix.")

    discovery = [
        sys.executable, scripts / "offer-role-models.py", "--codex-executable", arguments.codex_executable,
    ]
    if arguments.codex_home is not None:
        discovery.extend(["--codex-home", arguments.codex_home])
    offer = json.loads(
        run_checked([*discovery, "--", profile], input_text=evidence, timeout=45).stdout
    )
    current = offer["roles"][profile]
    model, effort = assignment["model"], assignment["reasoning_effort"]
    match = next((
        pair for pair in current["pairs"]
        if (pair["model"], pair["reasoning_effort"]) == (model, effort)
        and pair["spawn"]["agent_type"] == profile
    ), None)
    if match is None:
        diagnostic = current.get("unavailable_reason", "The saved pair is outside the current offer.")
        if current.get("unverified_pair_count", 0):
            diagnostic += " Some provider pairs still need callable eligibility evidence."
        raise ResolutionError(
            f"Cannot launch {arguments.role} ({profile}) with saved {model} / {effort}. "
            f"{diagnostic} Verify the saved pair or obtain an explicit replacement."
        )

    if writer:
        run_checked([
            *casefile, "require-writer-progress", "--investigation",
            projection["target"]["investigation"], "--ticket-id", arguments.ticket_id,
        ], timeout=15)
    if json.loads(run_checked(project, timeout=15).stdout) != projection:
        raise ResolutionError("The selected strategy or assignments changed during the spawn check. Recheck before delegating.")
    return {
        "role": arguments.role,
        "model": model,
        "reasoning_effort": effort,
        "source": assignment["source"],
        "runtime": offer["runtime"],
        "spawn": match["spawn"],
    }


def main() -> int:
    parser = argparse.ArgumentParser(
        description=__doc__,
        epilog="Supply fresh evidence from this caller on stdin in the offer-role-models.py format.",
    )
    parser.add_argument("--casefile-executable", type=Path, required=True)
    parser.add_argument("--planning-root", type=Path, required=True)
    parser.add_argument("--investigation", required=True)
    parser.add_argument("--phase", choices=("investigation", "review", "implementation"), required=True)
    parser.add_argument("--strategy-id", required=True)
    parser.add_argument("--role", required=True)
    parser.add_argument("--platform-profile", help="required when the role has multiple named registrations")
    parser.add_argument("--ticket-id", help="the exact accepted ticket for an implementation writer")
    parser.add_argument("--codex-executable", default="codex")
    parser.add_argument("--codex-home", type=Path)
    arguments = parser.parse_args()
    try:
        result = resolve(arguments, sys.stdin.read())
    except (OSError, ValueError, TypeError, KeyError, subprocess.SubprocessError) as error:
        print(f"Casefile role spawn check failed: {error}", file=sys.stderr)
        return 1
    print(json.dumps(result, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
