#!/usr/bin/env python3
"""Validate and aggregate deterministic skill-verification records."""
from __future__ import annotations

import argparse
import hashlib
import json
import tomllib
from collections import defaultdict
from pathlib import Path


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


class VerificationError(ValueError):
    pass


def load(path: Path) -> tuple[dict, str]:
    data = path.read_bytes()
    if not data:
        raise VerificationError(f"empty TOML: {path}")
    try:
        document = tomllib.loads(data.decode("utf-8"))
    except (UnicodeError, tomllib.TOMLDecodeError) as error:
        raise VerificationError(f"invalid TOML {path}: {error}") from error
    return document, hashlib.sha256(data).hexdigest()


def validate_strategy(document: dict) -> list[str]:
    errors: list[str] = []
    if document.get("schema_version") != 1:
        errors.append("strategy schema_version must be 1")
    if document.get("mode") not in MODES:
        errors.append("strategy mode is invalid")
    if document.get("baseline_kind") not in BASELINES:
        errors.append("strategy baseline_kind is invalid")
    if document.get("mode") == "structural" and document.get("baseline_kind") != "none":
        errors.append("structural strategy cannot have a baseline")
    if document.get("mode") != "structural" and document.get("baseline_kind") == "none":
        errors.append("behavioral strategy requires a baseline")
    classes = document.get("required_evidence_classes")
    if not isinstance(classes, list) or not classes or set(classes) - EVIDENCE_CLASSES:
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
    cases = document.get("cases")
    if not isinstance(cases, list) or not cases:
        return errors + ["suite cases must be a non-empty array"]
    seen: set[str] = set()
    partitions: set[str] = set()
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
        partition = case.get("partition")
        if partition not in PARTITIONS:
            errors.append(f"invalid case partition: {partition!r}")
        else:
            partitions.add(partition)
        if not isinstance(case.get("skill"), str) or not case["skill"]:
            errors.append(f"case {case_id!r} lacks a skill")
        for field in ("prompt", "rubric"):
            value = case.get(field)
            if not isinstance(value, str) or not value:
                errors.append(f"case {case_id!r} lacks {field}")
                continue
            target = (root / value).resolve()
            if root.resolve() not in target.parents or not target.is_file():
                errors.append(f"case {case_id!r} has unsafe or missing {field}: {value}")
                continue
            text = target.read_text(encoding="ascii")
            if not text.strip():
                errors.append(f"case {case_id!r} has empty {field}")
            if field == "prompt" and any(token in text.lower() for token in ("expected answer", "grading rubric", "diagnosis:")):
                errors.append(f"case {case_id!r} prompt leaks evaluation material")
    if partitions != PARTITIONS:
        errors.append("suite must cover positive, hard non-trigger, and task-behavior partitions")
    return errors


def validate_run(
    document: dict,
    strategy: dict,
    strategy_hash: str,
    suite: dict,
    suite_hash: str,
) -> list[str]:
    errors: list[str] = []
    if document.get("schema_version") != 1:
        errors.append("run schema_version must be 1")
    if document.get("strategy_id") != strategy.get("strategy_id") or document.get("strategy_sha256") != strategy_hash:
        errors.append("run strategy identity or hash mismatch")
    if document.get("suite_id") != suite.get("suite_id") or document.get("suite_sha256") != suite_hash:
        errors.append("run suite identity or hash mismatch")
    if document.get("baseline_kind") != strategy.get("baseline_kind"):
        errors.append("run baseline kind differs from strategy")
    if document.get("baseline_kind") == "immutable-old-skill" and not document.get("baseline_immutable_ref"):
        errors.append("immutable old-skill baseline requires a reference")
    if document.get("baseline_kind") == "no-skill" and document.get("baseline_artifact") != "none":
        errors.append("no-skill baseline artifact must be 'none'")

    case_ids = {case["id"] for case in suite.get("cases", []) if isinstance(case, dict) and "id" in case}
    required_arms = {"candidate"} if strategy.get("mode") == "structural" else {"candidate", "baseline"}
    seen: set[tuple[str, str]] = set()
    results = document.get("results")
    if not isinstance(results, list):
        return errors + ["run results must be an array"]
    for result in results:
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
        if result.get("status") not in {"pass", "fail", "unverified"}:
            errors.append(f"invalid result status for {key}")
        if result.get("evidence_class") not in EVIDENCE_CLASSES:
            errors.append(f"invalid evidence class for {key}")
        score = result.get("score")
        if not isinstance(score, (int, float)) or isinstance(score, bool) or not 0 <= score <= 1:
            errors.append(f"score must be between 0 and 1 for {key}")
        if not isinstance(result.get("artifact_ref"), str) or not result["artifact_ref"]:
            errors.append(f"artifact_ref is required for {key}")
    expected = {(case_id, arm) for case_id in case_ids for arm in required_arms}
    for missing in sorted(expected - seen):
        errors.append(f"missing run result: {missing}")
    for extra in sorted(seen - expected):
        errors.append(f"extra run result: {extra}")
    return errors


def aggregate(strategy: dict, suite: dict, run: dict) -> dict:
    by_case: dict[str, dict[str, dict]] = defaultdict(dict)
    for result in run["results"]:
        by_case[result["case_id"]][result["arm"]] = result
    candidate = [arms["candidate"] for arms in by_case.values()]
    verified = all(item["status"] != "unverified" for arms in by_case.values() for item in arms.values())
    pass_rate = sum(item["status"] == "pass" for item in candidate) / len(candidate)
    partition = {case["id"]: case["partition"] for case in suite["cases"]}
    hard_non_trigger_pass = all(
        by_case[case_id]["candidate"]["status"] == "pass"
        for case_id, value in partition.items()
        if value == "hard_non_trigger"
    )
    absolute = pass_rate >= strategy["absolute"]["minimum_candidate_pass_rate"]
    if strategy["absolute"]["require_all_hard_non_triggers"]:
        absolute = absolute and hard_non_trigger_pass
    delta: float | None = None
    comparative = True
    if strategy["baseline_kind"] != "none":
        delta = sum(
            arms["candidate"]["score"] - arms["baseline"]["score"]
            for arms in by_case.values()
        ) / len(by_case)
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
        "case_count": len(by_case),
    }


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
            errors += validate_run(run, strategy, strategy_hash, suite, suite_hash)
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
