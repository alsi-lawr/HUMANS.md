#!/usr/bin/env python3
"""Validate and aggregate hash-bound skill-verification records."""
from __future__ import annotations

import argparse
import copy
import hashlib
import json
import re
import tomllib
from collections import defaultdict
from datetime import datetime
from pathlib import Path, PurePosixPath


EVIDENCE_CLASSES = {
    "mechanical",
    "sampled_behavior",
    "comparative",
    "model_judgement",
    "human_judgement",
    "unverified",
}
MODES = {"structural", "balanced", "comparative"}
BASELINES = {"none", "no-skill", "immutable-old-skill"}
PARTITIONS = {"positive_trigger", "hard_non_trigger", "task_behavior"}
ARMS = {"candidate", "baseline"}
SHA256 = re.compile(r"^[0-9a-f]{64}$")
NAME = re.compile(r"^[a-z0-9]+(?:-[a-z0-9]+)*$")


class VerificationError(ValueError):
    pass


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def canonical_digest(document: dict, omit: str | None = None) -> str:
    value = copy.deepcopy(document)
    if omit is not None:
        value.pop(omit, None)
    data = json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True).encode(
        "ascii"
    )
    return sha256(data)


def load(path: Path) -> tuple[dict, str]:
    data = path.read_bytes()
    if not data:
        raise VerificationError(f"empty TOML: {path}")
    try:
        document = tomllib.loads(data.decode("utf-8"))
    except (UnicodeError, tomllib.TOMLDecodeError) as error:
        raise VerificationError(f"invalid TOML {path}: {error}") from error
    return document, sha256(data)


def validate_strategy(document: dict) -> list[str]:
    errors: list[str] = []
    if document.get("schema_version") != 1:
        errors.append("strategy schema_version must be 1")
    if not isinstance(document.get("strategy_id"), str) or not NAME.fullmatch(
        document["strategy_id"]
    ):
        errors.append("strategy_id is invalid")
    if document.get("mode") not in MODES:
        errors.append("strategy mode is invalid")
    if document.get("baseline_kind") not in BASELINES:
        errors.append("strategy baseline_kind is invalid")
    if document.get("mode") == "structural" and document.get("baseline_kind") != "none":
        errors.append("structural strategy cannot have a baseline")
    if document.get("mode") != "structural" and document.get("baseline_kind") == "none":
        errors.append("behavioral strategy requires a baseline")
    classes = document.get("required_evidence_classes")
    if (
        not isinstance(classes, list)
        or not classes
        or not all(isinstance(item, str) for item in classes)
        or set(classes) - EVIDENCE_CLASSES
        or len(classes) != len(set(classes))
        or "unverified" in classes
    ):
        errors.append("strategy evidence classes are invalid")
    absolute = document.get("absolute", {})
    rate = absolute.get("minimum_candidate_pass_rate")
    if not isinstance(rate, (int, float)) or isinstance(rate, bool) or not 0 <= rate <= 1:
        errors.append("absolute minimum_candidate_pass_rate must be between 0 and 1")
    if not isinstance(absolute.get("require_all_hard_non_triggers"), bool):
        errors.append("absolute require_all_hard_non_triggers must be boolean")
    comparative = document.get("comparative", {})
    delta = comparative.get("minimum_mean_delta")
    if not isinstance(delta, (int, float)) or isinstance(delta, bool):
        errors.append("comparative minimum_mean_delta must be numeric")
    isolation = document.get("isolation", {})
    for key in ("fresh_context_per_case", "rubric_hidden_from_prompt", "simultaneous_arms"):
        if not isinstance(isolation.get(key), bool):
            errors.append(f"isolation {key} must be boolean")
    if document.get("mode") != "structural" and not isolation.get("simultaneous_arms"):
        errors.append("behavioral strategy must use simultaneous arms")
    return errors


def validate_suite(document: dict, suite_path: Path) -> list[str]:
    errors: list[str] = []
    if document.get("schema_version") != 1:
        errors.append("suite schema_version must be 1")
    if not isinstance(document.get("suite_id"), str) or not document["suite_id"]:
        errors.append("suite_id is required")
    cases = document.get("cases")
    if not isinstance(cases, list) or not cases:
        return errors + ["suite cases must be a non-empty array"]
    seen: set[str] = set()
    by_skill: dict[str, set[str]] = defaultdict(set)
    root = suite_path.parent.parent
    for case in cases:
        if not isinstance(case, dict):
            errors.append("suite case must be a table")
            continue
        case_id = case.get("id")
        if not isinstance(case_id, str) or not case_id:
            errors.append("suite case id is missing")
        elif case_id in seen:
            errors.append(f"duplicate suite case: {case_id}")
        else:
            seen.add(case_id)
        skill = case.get("skill")
        if not isinstance(skill, str) or not NAME.fullmatch(skill):
            errors.append(f"case {case_id!r} lacks a valid skill")
        partition = case.get("partition")
        if partition not in PARTITIONS:
            errors.append(f"invalid case partition: {partition!r}")
        elif isinstance(skill, str):
            by_skill[skill].add(partition)
        resolved: dict[str, Path] = {}
        for field in ("prompt", "rubric"):
            value = case.get(field)
            if not isinstance(value, str) or not value:
                errors.append(f"case {case_id!r} lacks {field}")
                continue
            pure = PurePosixPath(value)
            if pure.is_absolute() or ".." in pure.parts or "\\" in value:
                errors.append(f"case {case_id!r} has unsafe {field}: {value}")
                continue
            target = (root / Path(*pure.parts)).resolve()
            if root.resolve() not in target.parents or not target.is_file():
                errors.append(f"case {case_id!r} has unsafe or missing {field}: {value}")
                continue
            resolved[field] = target
            text = target.read_text(encoding="ascii")
            if not text.strip():
                errors.append(f"case {case_id!r} has empty {field}")
            if field == "prompt" and any(
                token in text.lower()
                for token in ("expected answer", "grading rubric", "diagnosis:", "the answer must")
            ):
                errors.append(f"case {case_id!r} prompt leaks evaluation material")
        if resolved.get("prompt") == resolved.get("rubric"):
            errors.append(f"case {case_id!r} prompt and rubric must be separate")
    for skill, partitions in sorted(by_skill.items()):
        if partitions != PARTITIONS:
            errors.append(f"skill {skill} must cover every verification partition")
    return errors


def parse_timestamp(value: object, field: str, errors: list[str]) -> datetime | None:
    if not isinstance(value, str):
        errors.append(f"{field} must be a UTC timestamp")
        return None
    try:
        return datetime.strptime(value, "%Y-%m-%dT%H:%M:%SZ")
    except ValueError:
        errors.append(f"{field} must use YYYY-MM-DDTHH:MM:SSZ")
        return None


def artifact_errors(value: object, field: str, root: Path) -> list[str]:
    errors: list[str] = []
    if not isinstance(value, dict):
        return [f"{field} must be a table"]
    path_value = value.get("path")
    digest = value.get("sha256")
    if not isinstance(path_value, str) or not path_value:
        errors.append(f"{field}.path is required")
        return errors
    pure = PurePosixPath(path_value)
    if pure.is_absolute() or ".." in pure.parts or "\\" in path_value:
        errors.append(f"{field}.path is unsafe")
        return errors
    target = (root / Path(*pure.parts)).resolve()
    if root.resolve() not in target.parents or not target.is_file():
        errors.append(f"{field}.path is missing or escapes the run directory")
        return errors
    if not isinstance(digest, str) or not SHA256.fullmatch(digest):
        errors.append(f"{field}.sha256 is invalid")
    elif sha256(target.read_bytes()) != digest:
        errors.append(f"{field}.sha256 does not match {path_value}")
    return errors


def validate_run(
    document: dict,
    strategy: dict,
    strategy_hash: str,
    suite: dict,
    suite_hash: str,
    run_path: Path | None = None,
) -> list[str]:
    errors: list[str] = []
    root = run_path.resolve().parent if run_path is not None else Path.cwd().resolve()
    if document.get("schema_version") != 1:
        errors.append("run schema_version must be 1")
    if not isinstance(document.get("run_id"), str) or not document["run_id"]:
        errors.append("run_id is required")
    if document.get("strategy_id") != strategy.get("strategy_id") or document.get(
        "strategy_sha256"
    ) != strategy_hash:
        errors.append("run strategy identity or hash mismatch")
    if document.get("suite_id") != suite.get("suite_id") or document.get(
        "suite_sha256"
    ) != suite_hash:
        errors.append("run suite identity or hash mismatch")
    if document.get("run_sha256") != canonical_digest(document, "run_sha256"):
        errors.append("run_sha256 does not match the canonical run payload")

    runtime = document.get("runtime")
    if not isinstance(runtime, dict):
        errors.append("runtime must be a table")
    else:
        for field in ("platform", "model", "version", "configuration"):
            if not isinstance(runtime.get(field), str) or not runtime[field]:
                errors.append(f"runtime.{field} is required")
        if document.get("runtime_sha256") != canonical_digest(runtime):
            errors.append("runtime_sha256 does not match runtime metadata")
    errors += artifact_errors(document.get("runtime_artifact"), "runtime_artifact", root)
    errors += artifact_errors(document.get("candidate_artifact"), "candidate_artifact", root)

    baseline_kind = document.get("baseline_kind")
    if baseline_kind != strategy.get("baseline_kind"):
        errors.append("run baseline kind differs from strategy")
    baseline = document.get("baseline_artifact")
    if baseline_kind == "none":
        if baseline != {"kind": "none"}:
            errors.append("no-baseline run must record baseline_artifact.kind = 'none'")
    else:
        if not isinstance(baseline, dict) or baseline.get("kind") != baseline_kind:
            errors.append("baseline artifact kind differs from strategy")
        else:
            errors += artifact_errors(baseline, "baseline_artifact", root)
            if baseline_kind == "immutable-old-skill" and not isinstance(
                baseline.get("immutable_ref"), str
            ):
                errors.append("immutable old-skill baseline requires immutable_ref")

    execution = document.get("execution")
    if not isinstance(execution, dict):
        errors.append("execution must be a table")
        execution = {}
    for field in ("window_id",):
        if not isinstance(execution.get(field), str) or not execution[field]:
            errors.append(f"execution.{field} is required")
    started = parse_timestamp(execution.get("started_at"), "execution.started_at", errors)
    completed = parse_timestamp(execution.get("completed_at"), "execution.completed_at", errors)
    if started is not None and completed is not None and completed < started:
        errors.append("execution completion precedes its start")
    for field in ("fresh_context_per_case", "rubric_hidden_from_prompt", "simultaneous_arms"):
        expected = strategy.get("isolation", {}).get(field)
        if execution.get(field) is not expected:
            errors.append(f"execution.{field} does not match the strategy")
    errors += artifact_errors(execution.get("isolation_artifact"), "execution.isolation_artifact", root)

    case_ids = {
        case["id"] for case in suite.get("cases", []) if isinstance(case, dict) and "id" in case
    }
    required_arms = {"candidate"} if strategy.get("mode") == "structural" else ARMS
    seen: set[tuple[str, str]] = set()
    contexts: set[str] = set()
    evidence_used: set[str] = set()
    results = document.get("results")
    if not isinstance(results, list):
        return errors + ["run results must be an array"]
    for index, result in enumerate(results):
        if not isinstance(result, dict):
            errors.append("run result must be a table")
            continue
        key = (result.get("case_id"), result.get("arm"))
        if key in seen:
            errors.append(f"duplicate run result: {key}")
        seen.add(key)
        if result.get("case_id") not in case_ids:
            errors.append(f"unknown run case: {result.get('case_id')!r}")
        if result.get("arm") not in required_arms:
            errors.append(f"invalid run arm: {result.get('arm')!r}")
        status = result.get("status")
        evidence_class = result.get("evidence_class")
        if status not in {"pass", "fail", "unverified"}:
            errors.append(f"invalid result status for {key}")
        if evidence_class not in EVIDENCE_CLASSES:
            errors.append(f"invalid evidence class for {key}")
        else:
            evidence_used.add(evidence_class)
        if (status == "unverified") != (evidence_class == "unverified"):
            errors.append(f"status and evidence class are inconsistent for {key}")
        score = result.get("score")
        if not isinstance(score, (int, float)) or isinstance(score, bool) or not 0 <= score <= 1:
            errors.append(f"score must be between 0 and 1 for {key}")
        elif status == "unverified" and score != 0:
            errors.append(f"unverified result score must be zero for {key}")
        context_id = result.get("context_id")
        if not isinstance(context_id, str) or not context_id:
            errors.append(f"context_id is required for {key}")
        elif execution.get("fresh_context_per_case"):
            if context_id in contexts:
                errors.append(f"fresh-context claim reuses context {context_id!r}")
            contexts.add(context_id)
        if result.get("window_id") != execution.get("window_id"):
            errors.append(f"result window differs from execution window for {key}")
        errors += artifact_errors(result, f"results[{index}]", root)
    expected = {(case_id, arm) for case_id in case_ids for arm in required_arms}
    for missing in sorted(expected - seen):
        errors.append(f"missing run result: {missing}")
    for extra in sorted(seen - expected):
        errors.append(f"extra run result: {extra}")
    required_classes = set(strategy.get("required_evidence_classes", []))
    missing_classes = sorted(required_classes - evidence_used)
    if missing_classes:
        errors.append(f"run lacks required evidence classes: {', '.join(missing_classes)}")
    return errors


def metrics(strategy: dict, suite: dict, by_case: dict[str, dict[str, dict]], case_ids: list[str]) -> dict:
    candidate = [by_case[case_id]["candidate"] for case_id in case_ids]
    results = [item for case_id in case_ids for item in by_case[case_id].values()]
    verified = all(item["status"] != "unverified" for item in results)
    pass_rate = sum(item["status"] == "pass" for item in candidate) / len(candidate)
    partition = {case["id"]: case["partition"] for case in suite["cases"]}
    hard_ids = [case_id for case_id in case_ids if partition[case_id] == "hard_non_trigger"]
    hard_non_trigger_pass = all(
        by_case[case_id]["candidate"]["status"] == "pass" for case_id in hard_ids
    )
    absolute = pass_rate >= strategy["absolute"]["minimum_candidate_pass_rate"]
    if strategy["absolute"]["require_all_hard_non_triggers"] and hard_ids:
        absolute = absolute and hard_non_trigger_pass
    delta: float | None = None
    comparative = True
    if strategy["baseline_kind"] != "none":
        delta = sum(
            by_case[case_id]["candidate"]["score"]
            - by_case[case_id]["baseline"]["score"]
            for case_id in case_ids
        ) / len(case_ids)
        comparative = delta >= strategy["comparative"]["minimum_mean_delta"]
    status = "unverified" if not verified else "pass" if absolute and comparative else "fail"
    return {
        "status": status,
        "verified": verified,
        "absolute_acceptance": absolute,
        "comparative_acceptance": comparative,
        "candidate_pass_rate": pass_rate,
        "mean_delta": delta,
        "hard_non_trigger_pass": hard_non_trigger_pass,
        "case_count": len(case_ids),
    }


def aggregate(strategy: dict, suite: dict, run: dict) -> dict:
    by_case: dict[str, dict[str, dict]] = defaultdict(dict)
    for result in run["results"]:
        by_case[result["case_id"]][result["arm"]] = result
    case_ids = [case["id"] for case in suite["cases"]]
    skills: dict[str, list[str]] = defaultdict(list)
    partitions: dict[str, list[str]] = defaultdict(list)
    for case in suite["cases"]:
        skills[case["skill"]].append(case["id"])
        partitions[case["partition"]].append(case["id"])
    overall = metrics(strategy, suite, by_case, case_ids)
    overall["skills"] = {
        name: metrics(strategy, suite, by_case, ids) for name, ids in sorted(skills.items())
    }
    overall["partitions"] = {
        name: metrics(strategy, suite, by_case, ids)
        for name, ids in sorted(partitions.items())
    }
    return overall


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("validate", "aggregate"))
    parser.add_argument("--strategy", type=Path, required=True)
    parser.add_argument("--suite", type=Path, required=True)
    parser.add_argument("--run", type=Path)
    arguments = parser.parse_args()
    try:
        strategy, strategy_hash = load(arguments.strategy)
        suite, suite_hash = load(arguments.suite)
        errors = validate_strategy(strategy) + validate_suite(suite, arguments.suite.resolve())
        run = None
        if arguments.run:
            run, _ = load(arguments.run)
            errors += validate_run(
                run,
                strategy,
                strategy_hash,
                suite,
                suite_hash,
                arguments.run,
            )
        if arguments.command == "aggregate" and run is None:
            errors.append("aggregate requires --run")
        if errors:
            raise VerificationError("\n".join(errors))
        if arguments.command == "aggregate":
            print(json.dumps(aggregate(strategy, suite, run), sort_keys=True))
        else:
            print(
                f"validated strategy {strategy['strategy_id']}, suite {suite['suite_id']}"
                + (f", and run {run['run_id']}" if run else "")
            )
    except (OSError, VerificationError, UnicodeError, tomllib.TOMLDecodeError) as error:
        print(f"verification {arguments.command} failed: {error}")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
