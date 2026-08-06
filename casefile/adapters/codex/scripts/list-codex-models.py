#!/usr/bin/env python3
"""List the models Codex offers, with pinned models added, for catalog comparison."""
from __future__ import annotations

import argparse
import json
import subprocess
import sys
import tomllib
from pathlib import Path


class ProjectionError(RuntimeError):
    pass


def project(executable: str, timeout: float) -> list[dict]:
    process = subprocess.Popen(
        [executable, "app-server"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        text=True,
        bufsize=1,
    )
    try:
        return _exchange(process)
    finally:
        process.stdin.close()
        process.terminate()
        process.wait(timeout=timeout)


def _exchange(process: subprocess.Popen) -> list[dict]:
    def send(message: dict) -> None:
        process.stdin.write(json.dumps(message) + "\n")
        process.stdin.flush()

    def reply(request_id: int) -> dict:
        # Keep stdin open until the reply lands; closing it early ends the server first.
        for line in process.stdout:
            try:
                message = json.loads(line)
            except ValueError:
                continue
            if message.get("id") == request_id:
                if "error" in message:
                    raise ProjectionError(f"app-server error: {message['error']}")
                return message["result"]
        raise ProjectionError(f"app-server closed before answering request {request_id}")

    send(
        {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "clientInfo": {"name": "casefile", "title": "Casefile", "version": "1"},
                "capabilities": {},
            },
        }
    )
    reply(1)
    send({"jsonrpc": "2.0", "method": "initialized"})
    send(
        {
            "jsonrpc": "2.0",
            "id": 2,
            "method": "model/list",
            "params": {"cursor": None, "includeHidden": True, "limit": 1000},
        }
    )
    result = reply(2)
    if result.get("nextCursor") is not None:
        raise ProjectionError("app-server model/list paginated beyond one page")
    models = result.get("data")
    if not isinstance(models, list):
        raise ProjectionError("app-server model/list returned no data array")
    return models


def normalize(models: list[dict]) -> list[dict]:
    """Map the app-server shape onto the field names the catalog comparison reads."""
    result = []
    for model in models:
        identifier = model.get("id")
        if not isinstance(identifier, str) or not identifier:
            raise ProjectionError("app-server model/list contains a model without an ID")
        result.append(
            {
                "slug": identifier,
                "display_name": model.get("displayName"),
                "visibility": "hide" if model.get("hidden") else "list",
                "supported_reasoning_levels": [
                    {"effort": effort.get("reasoningEffort")}
                    for effort in model.get("supportedReasoningEfforts") or []
                    if isinstance(effort, dict)
                ],
            }
        )
    return result


def targets(profile_path: Path) -> list[dict]:
    profile = tomllib.loads(profile_path.read_text(encoding="ascii"))
    return profile.get("catalog", {}).get("targets", [])


def pinned_entries(profile_path: Path) -> list[dict]:
    """Pinned models are account-gated, so an unauthenticated listing omits them."""
    entries = []
    for target in targets(profile_path):
        if target.get("pinned") is not True:
            continue
        expected = target.get("expected", {})
        entries.append(
            {
                "slug": target["id"],
                "display_name": expected.get("display_name"),
                "visibility": expected.get("visibility"),
                "supported_reasoning_levels": [
                    {"effort": effort} for effort in target.get("required_reasoning", [])
                ],
            }
        )
    return entries


def listing(executable: str, profile_path: Path, timeout: float = 20.0) -> dict:
    models = normalize(project(executable, timeout))
    listed = {model["slug"] for model in models}
    models.extend(entry for entry in pinned_entries(profile_path) if entry["slug"] not in listed)
    return {"models": sorted(models, key=lambda model: model["slug"])}


def unsupported(models: dict, profile_path: Path) -> list[str]:
    """Models Codex offers that the packaged catalog does not carry."""
    carried = {target.get("id") for target in targets(profile_path)}
    return sorted({model["slug"] for model in models["models"]} - carried)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--codex-executable", default="codex")
    parser.add_argument("--profiles", type=Path, required=True)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--timeout", type=float, default=20.0)
    arguments = parser.parse_args()
    try:
        document = listing(arguments.codex_executable, arguments.profiles, arguments.timeout)
    except (ProjectionError, OSError, ValueError) as error:
        print(f"Codex model listing failed: {error}", file=sys.stderr)
        return 1
    text = json.dumps(document, indent=2, sort_keys=True) + "\n"
    if arguments.output is None:
        sys.stdout.write(text)
    else:
        arguments.output.write_text(text, encoding="ascii")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
