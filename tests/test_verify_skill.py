from __future__ import annotations

import hashlib
import tempfile
import unittest
from pathlib import Path

from _load import script


verify = script("scripts/verify-skill.py")


class VerifySkillTests(unittest.TestCase):
    def fixture(self, root: Path):
        (root / "prompts").mkdir()
        (root / "rubrics").mkdir()
        (root / "prompts/case.txt").write_text("Perform the bounded task.\n", encoding="ascii")
        (root / "rubrics/rubric.md").write_text("# Rubric\n\nJudge bounded behavior.\n", encoding="ascii")
        suite = {
            "schema_version": 1,
            "suite_id": "suite",
            "cases": [
                {"id": "positive", "skill": "sample", "partition": "positive_trigger", "prompt": "prompts/case.txt", "rubric": "rubrics/rubric.md"},
                {"id": "near", "skill": "sample", "partition": "hard_non_trigger", "prompt": "prompts/case.txt", "rubric": "rubrics/rubric.md"},
                {"id": "behavior", "skill": "sample", "partition": "task_behavior", "prompt": "prompts/case.txt", "rubric": "rubrics/rubric.md"},
            ],
        }
        strategy = {
            "schema_version": 1,
            "strategy_id": "balanced",
            "mode": "balanced",
            "baseline_kind": "no-skill",
            "required_evidence_classes": ["sampled_behavior", "comparative"],
            "absolute": {"minimum_candidate_pass_rate": 1.0, "require_all_hard_non_triggers": True},
            "comparative": {"minimum_mean_delta": 0.1},
            "isolation": {"fresh_context_per_case": True, "rubric_hidden_from_prompt": True, "simultaneous_arms": True},
        }
        return strategy, suite

    def test_balanced_absolute_and_comparative_aggregation(self):
        with tempfile.TemporaryDirectory() as temporary:
            strategy, suite = self.fixture(Path(temporary))
            run = {"results": []}
            for case in suite["cases"]:
                run["results"].append({"case_id": case["id"], "arm": "candidate", "status": "pass", "evidence_class": "sampled_behavior", "score": 1.0, "artifact_ref": "candidate"})
                run["results"].append({"case_id": case["id"], "arm": "baseline", "status": "fail", "evidence_class": "comparative", "score": 0.5, "artifact_ref": "baseline"})
            result = verify.aggregate(strategy, suite, run)
            self.assertEqual("pass", result["status"])
            self.assertTrue(result["absolute_acceptance"])
            self.assertGreaterEqual(result["mean_delta"], 0.1)

    def test_unverified_result_stays_unverified(self):
        with tempfile.TemporaryDirectory() as temporary:
            strategy, suite = self.fixture(Path(temporary))
            run = {"results": []}
            for case in suite["cases"]:
                for arm in ("candidate", "baseline"):
                    run["results"].append({"case_id": case["id"], "arm": arm, "status": "unverified", "evidence_class": "unverified", "score": 0.0, "artifact_ref": "not-run"})
            self.assertEqual("unverified", verify.aggregate(strategy, suite, run)["status"])


if __name__ == "__main__":
    unittest.main()
