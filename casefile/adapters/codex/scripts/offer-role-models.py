#!/usr/bin/env python3
"""Offer provider-backed pairs for named roles in the current Codex session."""
from __future__ import annotations

import argparse
import json
import os
import runpy
import subprocess
import sys
from pathlib import Path


FORK_ARGUMENTS = {"v1": ("fork_context", False), "v2": ("fork_turns", "none")}

EVIDENCE_HELP = """\
Supply current-session evidence as JSON on standard input. The JSON takes
this shape.

  {
    "provider": "provider-id",
    "runtime": "v2",
    "parameters": ["agent_type", "model", "reasoning_effort", "fork_turns"],
    "fork_values": ["none", "all"],
    "named_role_overrides": true,
    "roles": {"unpinned-role": {}, "fixed-role": {
      "model": "model-id", "reasoning_effort": "medium"
    }},
    "models": {"model-id": ["medium", "high"]}
  }

Use the active provider ID from the session's launch settings or thread
metadata. App-server thread/read for the current CODEX_THREAD_ID exposes it
as thread.modelProvider. Use the same Codex home and working directory.

Inspect the current turn's spawn_agent tool, using native tool discovery if
the tool is deferred. V1 has fork_context and V2 has fork_turns. Copy the
available parameter names, permitted fork values, named role registrations
and locks, and advertised model overrides and efforts from that callable.
A role provider override, when present, goes in the role's provider field.

Set named_role_overrides only when the current host's tool contract or an
authorized launch with verified child settings establishes that an unpinned
named role preserves explicit model and effort overrides. If this is unclear,
leave it false and use the reported limitation. Refresh the live evidence
before each spawn. Each pair's spawn object supplies the selection arguments.
Provide the current tool's required task input separately. An offer records
no human selection.
"""


class OfferError(RuntimeError):
    pass


def read_evidence() -> dict:
    evidence = json.load(sys.stdin)
    if not isinstance(evidence, dict):
        raise OfferError("current-session evidence must be a JSON object")
    provider = evidence.get("provider")
    if not isinstance(provider, str) or not provider.strip():
        raise OfferError("current-session evidence must identify the active provider")
    runtime = evidence.get("runtime")
    if not isinstance(runtime, str) or runtime not in FORK_ARGUMENTS:
        raise OfferError("current-session evidence must identify a V1 or V2 spawn tool")
    for key, expected_type in (
        ("parameters", list),
        ("fork_values", list),
        ("roles", dict),
        ("models", dict),
    ):
        if not isinstance(evidence.get(key), expected_type):
            raise OfferError(f"current-session evidence is missing valid {key}")
    return evidence


def role_offer(role: str, evidence: dict, provider_pairs: set[tuple[str, str]]) -> dict:
    def unavailable(reason: str) -> dict:
        return {"pairs": [], "unavailable_reason": reason}

    registration = evidence["roles"].get(role)
    if not isinstance(registration, dict):
        return unavailable("The current spawn tool does not list this named role.")
    if registration.get("provider", evidence["provider"]) != evidence["provider"]:
        return unavailable("The named role selects a different provider.")

    fork_arg, fork_value = FORK_ARGUMENTS[evidence["runtime"]]
    parameters = evidence["parameters"]
    if (
        "agent_type" not in parameters
        or fork_arg not in parameters
        or not any(
            type(value) is type(fork_value) and value == fork_value
            for value in evidence["fork_values"]
        )
    ):
        return unavailable("The current spawn tool lacks a context-free named role path.")

    fixed_model = registration.get("model")
    fixed_effort = registration.get("reasoning_effort")
    spawn = {"agent_type": role, fork_arg: fork_value}
    pairs = []
    if fixed_model is not None and fixed_effort is not None:
        if (
            isinstance(fixed_model, str)
            and isinstance(fixed_effort, str)
            and (fixed_model, fixed_effort) in provider_pairs
        ):
            pairs.append(
                {"model": fixed_model, "reasoning_effort": fixed_effort, "spawn": spawn}
            )
    elif fixed_model is not None or fixed_effort is not None:
        return unavailable("The named role has a partial model or effort lock.")
    elif (
        evidence.get("named_role_overrides") is True
        and "model" in parameters
        and "reasoning_effort" in parameters
    ):
        for model, effort in sorted(provider_pairs):
            active_efforts = evidence["models"].get(model)
            if isinstance(active_efforts, list) and effort in active_efforts:
                pairs.append(
                    {
                        "model": model,
                        "reasoning_effort": effort,
                        "spawn": {**spawn, "model": model, "reasoning_effort": effort},
                    }
                )
    else:
        return unavailable(
            "This spawn tool has no verified override path for an unpinned named role."
        )

    if not pairs:
        return unavailable("No visible model and effort match the provider and the named role.")
    return {"pairs": pairs}


def main() -> int:
    parser = argparse.ArgumentParser(
        description=__doc__,
        epilog=EVIDENCE_HELP,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument("roles", nargs="+", help="named roles in the selected strategy")
    parser.add_argument("--codex-executable", default="codex")
    parser.add_argument("--codex-home", type=Path)
    parser.add_argument("--timeout", type=float, default=20.0)
    arguments = parser.parse_args()
    lister = runpy.run_path(str(Path(__file__).with_name("list-codex-models.py")))
    try:
        evidence = read_evidence()
        environment = None
        if arguments.codex_home is not None:
            environment = {**os.environ, "CODEX_HOME": str(arguments.codex_home.resolve())}
        raw_models = lister["project"](
            arguments.codex_executable,
            arguments.timeout,
            environment,
            provider=evidence["provider"],
        )
        provider_pairs = {
            (model["slug"], level["effort"])
            for model in lister["normalize"](raw_models)
            if model["visibility"] == "list"
            for level in model["supported_reasoning_levels"]
            if isinstance(level["effort"], str) and level["effort"]
        }
        offer = {
            "provider": evidence["provider"],
            "runtime": evidence["runtime"],
            "roles": {role: role_offer(role, evidence, provider_pairs) for role in arguments.roles},
        }
    except (
        OfferError, lister["ProjectionError"], OSError, ValueError, subprocess.SubprocessError
    ) as error:
        print(f"Codex role offer failed: {error}", file=sys.stderr)
        return 1
    print(json.dumps(offer, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
