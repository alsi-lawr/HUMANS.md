#!/usr/bin/env python3
"""Smoke one built Casefile executable against an activated planning Store."""
from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
import tempfile
from os import PathLike
from pathlib import Path
from typing import Sequence


class SmokeError(RuntimeError):
    pass


def run_checked(
    arguments: Sequence[str | PathLike[str]], input_text: str | None = None
) -> subprocess.CompletedProcess[str]:
    command = [str(argument) for argument in arguments]
    result = subprocess.run(
        command,
        input=input_text,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="backslashreplace",
    )
    if result.returncode:
        raise SmokeError(
            json.dumps(
                {
                    "command": command,
                    "exit_code": result.returncode,
                    "stderr": result.stderr,
                    "stdout": result.stdout,
                },
                ensure_ascii=True,
                sort_keys=True,
            )
        )
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--executable", type=Path, required=True)
    parser.add_argument("--version", required=True)
    args = parser.parse_args()
    executable = args.executable.resolve(strict=True)
    version = run_checked([executable, "--version"])
    if args.version not in version.stdout:
        raise SystemExit(f"version output does not contain {args.version!r}: {version.stdout!r}")
    compatibility = run_checked([executable, "mcp-compatibility"])
    contract = json.loads(compatibility.stdout)
    if contract.get("identity") != "casefile" or contract.get("provider_protocol_version") != 1:
        raise SystemExit("unexpected Casefile compatibility contract")
    with tempfile.TemporaryDirectory(prefix="casefile-mcp-smoke-") as directory:
        root = Path(directory)
        fixture = Path(__file__).resolve().parent.parent / "casefile/casefile-store/tests/fixtures/minimum"
        shutil.copytree(fixture, root, dirs_exist_ok=True)
        requests = "\n".join(
            json.dumps(value, separators=(",", ":"))
            for value in (
                {"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25"}},
                {"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}},
            )
        ) + "\n"
        result = run_checked(
            [executable, "mcp-package", "--planning-root", root.resolve()],
            input_text=requests,
        )
    responses = [json.loads(line) for line in result.stdout.splitlines()]
    if responses[0]["result"]["serverInfo"]["name"] != "casefile":
        raise SystemExit("unexpected MCP server identity")
    if len(responses[1]["result"]["tools"]) != 12:
        raise SystemExit("Casefile MCP did not expose exactly 12 tools")
    print(json.dumps({"identity":"casefile","tools":12,"version":args.version}, sort_keys=True))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except SmokeError as error:
        print(f"Casefile smoke command failed: {error}", file=sys.stderr)
        raise SystemExit(1) from None
