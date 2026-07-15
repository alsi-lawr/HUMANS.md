#!/usr/bin/env python3
"""Confirm isolated Codex marketplace JSON contains humans-md."""
from __future__ import annotations

import json
import sys


def contains(value: object) -> bool:
    if isinstance(value, dict):
        if value.get("name") == "humans-md" or value.get("pluginName") == "humans-md":
            return True
        return any(contains(item) for item in value.values())
    if isinstance(value, list):
        return any(contains(item) for item in value)
    return value == "humans-md"


def main() -> int:
    try:
        document = json.load(sys.stdin)
    except json.JSONDecodeError as error:
        print(f"invalid Codex discovery JSON: {error}")
        return 1
    if not contains(document):
        print("humans-md was not discovered")
        return 1
    print("isolated Codex marketplace discovered humans-md")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
