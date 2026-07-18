#!/usr/bin/env python3
"""Check isolated Claude plugin-list JSON against expected installed sibling names."""
from __future__ import annotations
import argparse,json,sys
parser=argparse.ArgumentParser();parser.add_argument("--expected",required=True);parser.add_argument("--absent",required=True);args=parser.parse_args()
value=json.load(sys.stdin); expected=set(filter(None,args.expected.split(","))); found=set()
def walk(item):
 if isinstance(item,dict):
  identifier=item.get("pluginId") or item.get("id") or item.get("name")
  if isinstance(identifier,str) and (item.get("installed",True) is True): found.add(identifier.split("@",1)[0])
  for child in item.values(): walk(child)
 elif isinstance(item,list):
  for child in item: walk(child)
walk(value)
absent=set(filter(None,args.absent.split(",")))
if not expected <= found or absent & found: raise SystemExit(f"installed Claude plugins mismatch: expected {sorted(expected)}, absent {sorted(absent)}, got {sorted(found)}")
print("Claude sibling state matches: "+", ".join(sorted(expected)))
