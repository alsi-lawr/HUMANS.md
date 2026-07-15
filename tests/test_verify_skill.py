from __future__ import annotations

import copy
import tempfile
import unittest
from pathlib import Path

from _load import script


verify = script("scripts/verify-skill.py")


class VerifySkillTests(unittest.TestCase):
    def artifact(self, root: Path, name: str) -> dict[str, str]:
        path = root / "artifacts" / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(name + "\n", encoding="ascii")
        return {"path": f"artifacts/{name}", "sha256": verify.sha256(path.read_bytes())}

    def fixture(self, root: Path):
        for directory in ("prompts", "rubrics", "suites", "strategies", "runs"):
            (root / directory).mkdir()
        (root / "prompts/case.txt").write_text("Perform the requested task.\n", encoding="ascii")
        (root / "rubrics/rubric.md").write_text("# Rubric\n\nJudge the result.\n", encoding="ascii")
        suite_path = root / "suites/suite.toml"
        suite_path.write_text(
            '''schema_version = 1
suite_id = "suite"
[[cases]]
id = "positive"
skill = "sample-skill"
partition = "positive_trigger"
prompt = "prompts/case.txt"
rubric = "rubrics/rubric.md"
[[cases]]
id = "near"
skill = "sample-skill"
partition = "hard_non_trigger"
prompt = "prompts/case.txt"
rubric = "rubrics/rubric.md"
[[cases]]
id = "behavior"
skill = "sample-skill"
partition = "task_behavior"
prompt = "prompts/case.txt"
rubric = "rubrics/rubric.md"
''',
            encoding="ascii",
        )
        strategy_path = root / "strategies/balanced.toml"
        strategy_path.write_text(
            '''schema_version = 1
strategy_id = "balanced"
mode = "balanced"
baseline_kind = "no-skill"
required_evidence_classes = ["sampled_behavior", "comparative"]
[absolute]
minimum_candidate_pass_rate = 1.0
require_all_hard_non_triggers = true
[comparative]
minimum_mean_delta = 0.1
[isolation]
fresh_context_per_case = true
rubric_hidden_from_prompt = true
simultaneous_arms = true
''',
            encoding="ascii",
        )
        strategy, strategy_hash = verify.load(strategy_path)
        suite, suite_hash = verify.load(suite_path)
        run_root = root / "runs"
        runtime = {
            "platform": "codex",
            "model": "test-model",
            "version": "1.0",
            "configuration": "isolated",
        }
        run = {
            "schema_version": 1,
            "run_id": "run-001",
            "strategy_id": strategy["strategy_id"],
            "strategy_sha256": strategy_hash,
            "suite_id": suite["suite_id"],
            "suite_sha256": suite_hash,
            "runtime": runtime,
            "runtime_sha256": verify.canonical_digest(runtime),
            "runtime_artifact": self.artifact(run_root, "runtime.txt"),
            "candidate_artifact": self.artifact(run_root, "candidate.txt"),
            "baseline_kind": "no-skill",
            "baseline_artifact": {
                "kind": "no-skill",
                **self.artifact(run_root, "baseline.txt"),
            },
            "execution": {
                "window_id": "window-001",
                "started_at": "2026-07-15T12:00:00Z",
                "completed_at": "2026-07-15T12:10:00Z",
                "fresh_context_per_case": True,
                "rubric_hidden_from_prompt": True,
                "simultaneous_arms": True,
                "isolation_artifact": self.artifact(run_root, "isolation.txt"),
            },
            "results": [],
        }
        for case in suite["cases"]:
            for arm, status, evidence_class, score in (
                ("candidate", "pass", "sampled_behavior", 1.0),
                ("baseline", "fail", "comparative", 0.5),
            ):
                run["results"].append(
                    {
                        "case_id": case["id"],
                        "arm": arm,
                        "status": status,
                        "evidence_class": evidence_class,
                        "score": score,
                        "context_id": f"{case['id']}-{arm}",
                        "window_id": "window-001",
                        **self.artifact(run_root, f"{case['id']}-{arm}.txt"),
                    }
                )
        run["run_sha256"] = verify.canonical_digest(run, "run_sha256")
        run_path = run_root / "run.toml"
        run_path.write_text("path anchor\n", encoding="ascii")
        return strategy, strategy_hash, suite, suite_hash, run, run_path

    def test_happy_hash_bound_run_aggregates_by_skill_and_partition(self):
        with tempfile.TemporaryDirectory() as temporary:
            values = self.fixture(Path(temporary))
            strategy, strategy_hash, suite, suite_hash, run, run_path = values
            self.assertEqual(
                [], verify.validate_run(run, strategy, strategy_hash, suite, suite_hash, run_path)
            )
            result = verify.aggregate(strategy, suite, run)
            self.assertEqual("pass", result["status"])
            self.assertEqual(["sample-skill"], list(result["skills"]))
            self.assertEqual(verify.PARTITIONS, set(result["partitions"]))

    def test_hostile_run_rejects_status_class_and_artifact_mismatch(self):
        with tempfile.TemporaryDirectory() as temporary:
            values = self.fixture(Path(temporary))
            strategy, strategy_hash, suite, suite_hash, run, run_path = values
            hostile = copy.deepcopy(run)
            hostile["results"][0]["evidence_class"] = "unverified"
            hostile["results"][0]["sha256"] = "0" * 64
            hostile["run_sha256"] = verify.canonical_digest(hostile, "run_sha256")
            errors = verify.validate_run(
                hostile, strategy, strategy_hash, suite, suite_hash, run_path
            )
            self.assertTrue(any("status and evidence class" in item for item in errors))
            self.assertTrue(any("does not match" in item for item in errors))


if __name__ == "__main__":
    unittest.main()
