#!/usr/bin/env python3
"""Check a Codex plugin list JSON against an explicit expected identity set."""
from __future__ import annotations
import argparse, json, sys
parser=argparse.ArgumentParser(); parser.add_argument("--expected", required=True); parser.add_argument("--marketplace", action="store_true")
args=parser.parse_args(); value=json.load(sys.stdin)
installed={item.get("pluginId","").split("@",1)[0] for item in value.get("installed",[]) if item.get("installed")}
expected=set(filter(None,args.expected.split(",")))
if installed != expected: raise SystemExit(f"installed plugin mismatch: expected {sorted(expected)}, got {sorted(installed)}")
print("plugin state matches: "+", ".join(sorted(expected)))
