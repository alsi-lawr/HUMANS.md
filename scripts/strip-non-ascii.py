#!/usr/bin/env python3
"""Check or replace a narrow allowlist of non-ASCII typography in text files."""
from __future__ import annotations

import argparse
import os
from pathlib import Path


REPLACEMENTS = {
    "\u00a0": " ",
    "\u2013": "-",
    "\u2014": "--",
    "\u2018": "'",
    "\u2019": "'",
    "\u201c": '"',
    "\u201d": '"',
    "\u2026": "...",
}
TEXT_NAMES = {"LICENSE"}
TEXT_SUFFIXES = {".json", ".md", ".py", ".toml", ".txt", ".yaml", ".yml", ".in"}


def files(paths: list[Path]):
    for supplied in paths:
        if supplied.is_file():
            yield supplied
            continue
        for current, directories, names in os.walk(supplied):
            directories[:] = sorted(
                name for name in directories if name not in {".git", ".agent-workspace", "__pycache__"}
            )
            for name in sorted(names):
                path = Path(current) / name
                if path.suffix.lower() in TEXT_SUFFIXES or name in TEXT_NAMES:
                    yield path


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--check", action="store_true")
    mode.add_argument("--write", action="store_true")
    parser.add_argument("paths", type=Path, nargs="+", default=[Path.cwd()])
    arguments = parser.parse_args()
    failures: list[str] = []
    changed = 0
    for path in files(arguments.paths):
        text = path.read_text(encoding="utf-8")
        non_ascii = sorted({character for character in text if ord(character) > 127})
        if not non_ascii:
            continue
        unsupported = [character for character in non_ascii if character not in REPLACEMENTS]
        if unsupported:
            failures.append(
                f"{path}: unsupported code points "
                + ", ".join(f"U+{ord(character):04X}" for character in unsupported)
            )
            continue
        if arguments.check:
            failures.append(f"{path}: contains allowlisted non-ASCII typography")
            continue
        updated = "".join(REPLACEMENTS.get(character, character) for character in text)
        path.write_text(updated, encoding="ascii")
        changed += 1
    if failures:
        print("ASCII check failed:", *failures, sep="\n- ")
        return 1
    print(f"ASCII {'check passed' if arguments.check else 'rewrite completed'}; changed {changed} file(s)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
