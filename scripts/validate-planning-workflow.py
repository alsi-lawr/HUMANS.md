#!/usr/bin/env python3
"""Validate portable workflow source, Codex matrices, and installed-copy parity."""
from __future__ import annotations
import argparse, re, sys, tomllib
from pathlib import Path

ROLES = {"inspector", "detective", "dialogue-review-chair", "dialogue-review-challenger", "atomic-ticket-reviewer", "verification-reviewer", "implementation-writer"}
SKILLS = {"ticketed-repository-investigation", "investigation-solo", "investigation-atomic", "investigation-inspector-tree", "investigation-review-dialogue", "investigation-review-atomic", "investigation-review-two-stage", "ticket-batch-subagent-pipeline", "ticket-scratch-closeout"}
PHASES = {"investigation", "review", "implementation"}

def fail(errors, message): errors.append(message)
def main():
    ap=argparse.ArgumentParser(); ap.add_argument("--source", type=Path, required=True); ap.add_argument("--codex-home", type=Path); ap.add_argument("--model-catalog", type=Path); ns=ap.parse_args()
    root=ns.source.resolve(); errors=[]; workflow=root/"planning-workflow"; skills=root/"skills"; adapter=workflow/"adapters/codex"
    catalog_path=ns.model_catalog or ((ns.codex_home/"models-sol-v1.json") if ns.codex_home else None)
    catalog={}
    if catalog_path and catalog_path.is_file():
        import json
        for model in json.loads(catalog_path.read_text()).get("models",[]):
            catalog[model.get("slug")]=set(x.get("effort") for x in model.get("supported_reasoning_levels",[]))
    for role in ROLES|{"orchestrator"}:
        if not (workflow/f"roles/{role}.md").is_file(): fail(errors,f"missing role: {role}")
    for skill in SKILLS:
        p=skills/skill/"SKILL.md"
        if not p.is_file(): fail(errors,f"missing skill: {skill}"); continue
        text=p.read_text();
        if not text.startswith("---\nname: ") or "\ndescription: " not in text.split("---",2)[1]: fail(errors,f"invalid skill metadata: {skill}")
    portable="\n".join((skills/s/"SKILL.md").read_text() for s in SKILLS)
    banned=[r"gpt-",r"request_user_input",r"\bCodex\b",r"\bGitHub\b",r"max_concurrent_subagents\s*=",r"\bPlan mode\b",r"reasoning\s*="]
    for pattern in banned:
        if re.search(pattern,portable,re.I): fail(errors,f"platform binding leaked into portable skill: {pattern}")
    for p in sorted((adapter/"agents").glob("*.toml")):
        data=tomllib.loads(p.read_text())
        if "developer_instructions" not in data: fail(errors,f"missing role instructions: {p.name}")
        if "model" in data or "model_reasoning_effort" in data: fail(errors,f"model default in role: {p.name}")
    known={p.stem for p in (adapter/"agents").glob("*.toml")}
    for p in sorted((adapter/"matrices").glob("*.toml")):
        try: d=tomllib.loads(p.read_text())
        except Exception as e: fail(errors,f"invalid TOML {p.name}: {e}"); continue
        for key in ("schema_version","strategy_id","phase","platform","orchestrator","limits","coordination"):
            if key not in d: fail(errors,f"{p.name}: missing {key}")
        if d.get("schema_version") != 1 or d.get("platform") != "codex" or d.get("phase") not in PHASES: fail(errors,f"{p.name}: invalid root binding")
        if d.get("orchestrator",{}).get("binding") != "root": fail(errors,f"{p.name}: orchestrator is not root")
        limits=d.get("limits",{}); concurrency=limits.get("max_concurrent_subagents",0); depth=limits.get("max_depth",-1)
        if concurrency < 1 or depth < 0: fail(errors,f"{p.name}: invalid limits")
        spawning=False
        for w in d.get("workers",[]):
            required={"role","platform_profile","model","reasoning","minimum_count","maximum_count","can_spawn_subagents"}
            if required-w.keys(): fail(errors,f"{p.name}: incomplete worker")
            if w.get("role") not in known or w.get("platform_profile") not in known: fail(errors,f"{p.name}: unknown role/profile {w.get('role')}")
            if w.get("minimum_count",0)<1 or w.get("minimum_count",0)>w.get("maximum_count",0): fail(errors,f"{p.name}: invalid counts")
            if not w.get("model") or not w.get("reasoning"): fail(errors,f"{p.name}: empty platform binding")
            if catalog and (w.get("model") not in catalog or w.get("reasoning") not in catalog.get(w.get("model"),set())): fail(errors,f"{p.name}: unsupported model/reasoning {w.get('model')}/{w.get('reasoning')}")
            spawning |= bool(w.get("can_spawn_subagents"))
        if spawning and depth < 2: fail(errors,f"{p.name}: nested worker without depth 2")
        coord=d.get("coordination",{})
        for k in ("batch_when_capacity_exceeded","candidate_review_before_ticket","shared_ticket_storage_required"):
            if not isinstance(coord.get(k),bool): fail(errors,f"{p.name}: missing coordination {k}")
    for schema in ("strategy-matrix.md","ticket.md","decision.md","investigation-layout.md"):
        if not (workflow/"schemas"/schema).is_file(): fail(errors,f"missing schema: {schema}")
    if ns.codex_home:
        try: config=tomllib.loads((ns.codex_home/"config.toml").read_text())
        except Exception as e: fail(errors,f"installed config is invalid TOML: {e}"); config={}
        for skill in SKILLS:
            src=skills/skill/"SKILL.md"; dst=ns.codex_home/"skills"/skill/"SKILL.md"
            if not dst.is_file() or src.read_bytes()!=dst.read_bytes(): fail(errors,f"installed skill mismatch: {skill}")
        for role in ROLES:
            src=adapter/"agents"/f"{role}.toml"; dst=ns.codex_home/"agents"/f"{role}.toml"
            if not dst.is_file() or src.read_bytes()!=dst.read_bytes(): fail(errors,f"installed role mismatch: {role}")
            declaration=config.get("agents",{}).get(role,{})
            if declaration.get("config_file") != str(dst): fail(errors,f"installed role declaration mismatch: {role}")
    if errors:
        print("validation failed:",*errors,sep="\n- "); return 1
    print(f"validated {len(SKILLS)} skills, {len(ROLES)} worker roles, and {len(list((adapter/'matrices').glob('*.toml')))} Codex matrices")
    return 0
if __name__=="__main__": raise SystemExit(main())
