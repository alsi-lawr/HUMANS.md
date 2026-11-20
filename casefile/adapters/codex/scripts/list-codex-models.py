#!/usr/bin/env python3
"""List the models reported by the current Codex provider."""
from __future__ import annotations

import argparse
import json
import subprocess
import sys


class ProjectionError(RuntimeError):
    pass


def project(
    executable: str,
    timeout: float,
    environment: dict[str, str] | None = None,
    *,
    provider: str | None = None,
) -> list[dict]:
    command = [executable, "app-server"]
    if provider is not None:
        command.extend(["-c", f"model_provider={json.dumps(provider, ensure_ascii=False)}"])
    process = subprocess.Popen(
        command,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        text=True,
        bufsize=1,
        env=environment,
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
    """Project the app-server model metadata used by role selection."""
    result = []
    for model in models:
        identifier = model.get("model", model.get("id"))
        if not isinstance(identifier, str) or not identifier:
            raise ProjectionError("app-server model/list contains a model without an ID")
        entry = {
            "slug": identifier,
            "display_name": model.get("displayName"),
            "visibility": "hide" if model.get("hidden") else "list",
            "supported_reasoning_levels": [
                {"effort": effort.get("reasoningEffort")}
                for effort in model.get("supportedReasoningEfforts") or []
                if isinstance(effort, dict)
            ],
        }
        if "multiAgentVersion" in model:
            entry["multi_agent_version"] = model["multiAgentVersion"]
        result.append(entry)
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--codex-executable", default="codex")
    parser.add_argument("--provider", help="provider ID to select for app-server discovery")
    parser.add_argument("--timeout", type=float, default=20.0)
    arguments = parser.parse_args()
    try:
        models = normalize(project(arguments.codex_executable, arguments.timeout, provider=arguments.provider))
    except (ProjectionError, OSError, ValueError, subprocess.SubprocessError) as error:
        print(f"Codex model listing failed: {error}", file=sys.stderr)
        return 1
    print(json.dumps({"models": sorted(models, key=lambda model: model["slug"])}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
