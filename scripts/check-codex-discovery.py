#!/usr/bin/env python3
"""Confirm isolated Codex marketplace JSON contains the exact split plugin inventory."""
from __future__ import annotations

import json
import sys

EXPECTED = {"humans-md", "casefile", "coding"}


def names(value: object) -> set[str]:
    if isinstance(value, dict):
        found = {item for key in ("name", "pluginName") if isinstance((item := value.get(key)), str)}
        for child in value.values():
            found |= names(child)
        return found
    if isinstance(value, list):
        return set().union(*(names(item) for item in value)) if value else set()
    return set()


def main() -> int:
    try:
        document = json.load(sys.stdin)
    except json.JSONDecodeError as error:
        print(f"invalid Codex discovery JSON: {error}")
        return 1
    missing = EXPECTED - names(document)
    if missing:
        print("missing Codex marketplace identities: " + ", ".join(sorted(missing)))
        return 1
    print("isolated Codex marketplace discovered: " + ", ".join(sorted(EXPECTED)))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
