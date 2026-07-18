#!/usr/bin/env python3
"""Run package-local and shared standard-library tests."""
from __future__ import annotations
import subprocess, sys
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
paths=[ROOT/"tests", ROOT/"humans-md/tests", ROOT/"casefile/tests", ROOT/"coding/tests"]
for path in paths:
    if path.is_dir() and any(path.glob("test_*.py")):
        result=subprocess.run([sys.executable,"-m","unittest","discover","-s",str(path),"-v"],cwd=ROOT)
        if result.returncode: raise SystemExit(result.returncode)
