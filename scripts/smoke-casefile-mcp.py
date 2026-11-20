#!/usr/bin/env python3
"""Smoke one built Casefile executable against an activated planning Store."""
from __future__ import annotations

import argparse
import json
import shutil
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "casefile/adapters/shared"))
from casefile_runtime import RuntimeError as SmokeError, probe, run_checked


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
        tool_count = probe(executable, args.version, root)
    print(json.dumps({"identity": "casefile", "tools": tool_count, "version": args.version}, sort_keys=True))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except SmokeError as error:
        print(f"Casefile smoke command failed: {error}", file=sys.stderr)
        raise SystemExit(1) from None
