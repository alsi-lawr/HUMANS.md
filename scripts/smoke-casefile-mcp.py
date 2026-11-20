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

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "casefile/adapters/shared"))
from casefile_runtime import RuntimeError as ProbeError, probe


class SmokeError(RuntimeError):
    pass


def run_checked(arguments: Sequence[str | PathLike[str]]) -> subprocess.CompletedProcess[str]:
    command = [str(argument) for argument in arguments]
    result = subprocess.run(
        command,
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
    with tempfile.TemporaryDirectory(prefix="casefile-mcp-smoke-") as directory:
        root = Path(directory).resolve()
        fixture = ROOT / "casefile/casefile-store/tests/fixtures/minimum"
        shutil.copytree(fixture, root, dirs_exist_ok=True)
        investigation = "projects/demo/investigations/sample"
        validation = run_checked(
            [
                executable,
                "--root",
                root,
                "check",
                "--require-activation",
                "--investigation",
                investigation,
            ]
        )
        check = json.loads(validation.stdout)
        if check.get("activation") != "active" or check.get("valid") is not True:
            raise SystemExit(f"unexpected scoped Casefile validation result: {validation.stdout}")
        try:
            tool_count = probe(executable, args.version, root)
        except ProbeError as error:
            raise SmokeError(str(error)) from error
    print(json.dumps({"identity": "casefile", "tools": tool_count, "version": args.version}, sort_keys=True))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except SmokeError as error:
        print(f"Casefile smoke command failed: {error}", file=sys.stderr)
        raise SystemExit(1) from None
