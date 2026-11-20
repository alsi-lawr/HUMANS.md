from __future__ import annotations

import json
import subprocess
import sys
import tomllib
import unittest

from _load import ROOT, script


ADAPTER = ROOT / "casefile/adapters/codex"
OFFER = script("casefile/adapters/codex/scripts/offer-role-models.py")
CONVERSATION = ADAPTER / "scripts/recommend-role-assignments.py"


def projection(strategy_id: str, *, adapter: str = "codex") -> dict:
    source = (ADAPTER / "matrices" / f"{strategy_id}.toml").read_text(encoding="ascii")
    matrix = tomllib.loads(source)
    return {
        "adapter": adapter,
        "target": {
            "investigation": "projects/demo/investigations/sample",
            "phase": matrix["phase"],
            "strategy_id": strategy_id,
        },
        "revisions": {"matrix": "selected-revision", "assignments": None, "legacy_writer_binding": None},
        "roles": {
            worker["role"]: {"platform_profiles": [worker["platform_profile"]], "selection": None}
            for worker in matrix.get("workers", [])
        },
    }


def offer(projected, provider_pairs, callable_pairs, *, examples=False, missing=()):
    profiles = {profile for role in projected["roles"].values() for profile in role["platform_profiles"]}
    profiles.add("unrelated-worker")
    evidence = {
        "provider": "test-provider",
        "runtime": "v2",
        "parameters": ["agent_type", "model", "reasoning_effort", "fork_turns"],
        "fork_values": ["none", "all"],
        "named_role_overrides": True,
        "roles": {profile: {} for profile in profiles if profile not in missing},
        "models_scope": "examples" if examples else "exhaustive",
    }
    return {"roles": {
        profile: OFFER.role_offer(profile, evidence, provider_pairs, callable_pairs)
        for profile in profiles
    }}


class RoleAssignmentPromptTests(unittest.TestCase):
    def run_helper(self, command, projected, current, **inputs):
        return subprocess.run(
            [sys.executable, CONVERSATION, command],
            input=json.dumps({"projection": projected, "offer": current, **inputs}),
            capture_output=True, text=True, check=False,
        )

    def recommend(self, projected, current):
        result = self.run_helper("recommend", projected, current)
        self.assertEqual(0, result.returncode, result.stderr)
        return json.loads(result.stdout)

    def test_strategy_prompt_recommends_preferred_live_pairs_when_model_versions_change(self):
        projected = projection("casefile-implement-ticket-batch")
        for version in (3, 42):
            with self.subTest(version=version):
                astra = f"fixture-{version}-astra"
                terra = f"fixture-{version}-terra"
                pairs = {(astra, "high"), (terra, "xhigh"), (terra, "medium"), ("fixture-999-sol", "high")}
                prompt = self.recommend(projected, offer(projected, pairs, pairs))
                self.assertEqual("confirm", prompt["state"])
                self.assertEqual(["Use these assignments", "Change assignments"], prompt["actions"])
                self.assertEqual(
                    {
                        "implementation-writer": (astra, "high", "recommended"),
                        "atomic-ticket-reviewer": (terra, "xhigh", "recommended"),
                        "verification-reviewer": (terra, "medium", "recommended"),
                    },
                    {
                        row["role"]: (row["choice"]["model"], row["choice"]["reasoning_effort"], row["status"])
                        for row in prompt["rows"]
                    },
                )
                self.assertTrue(all(row["reason"] for row in prompt["rows"]))
                self.assertNotIn("request", prompt)

    def test_provider_only_luna_suggests_eligible_sol_and_discloses_example_coverage(self):
        projected = projection("casefile-implement-ticket-batch-look-ahead")
        sol = ("fixture-3-sol", "medium")
        luna = ("fixture-4-luna", "medium")
        pairs = {sol, ("fixture-999-astra", "medium")}
        for examples in (False, True):
            with self.subTest(examples=examples):
                current = offer(projected, pairs | {luna}, pairs, examples=examples)
                prompt = self.recommend(projected, current)
                look_ahead = next(row for row in prompt["rows"] if row["role"] == "look-ahead-investigator")
                self.assertEqual("alternative", look_ahead["status"])
                self.assertEqual({"model": sol[0], "reasoning_effort": sol[1]}, look_ahead["choice"])
                self.assertIn("Luna / medium", look_ahead["reason"])
                self.assertEqual(examples, bool(look_ahead["registrations_with_unverified_pairs"]))
                self.assertNotIn(luna[0], json.dumps(look_ahead["eligible_choices"]))
        verified = offer(projected, pairs | {luna}, pairs | {luna}, examples=True)
        rows = self.recommend(projected, verified)["rows"]
        look_ahead = next(row for row in rows if row["role"] == "look-ahead-investigator")
        self.assertEqual("recommended", look_ahead["status"])
        self.assertEqual(luna[0], look_ahead["choice"]["model"])

    def test_partial_named_registration_coverage_leaves_role_unassigned(self):
        projected = projection("casefile-investigate-atomic")
        projected["roles"]["detective"]["platform_profiles"].append("detector-secondary")
        pair = ("fixture-terra", "medium")
        current = offer(projected, {pair}, {pair}, missing=("detector-secondary",))
        prompt = self.recommend(projected, current)
        self.assertEqual("blocked", prompt["state"])
        self.assertNotIn("Use these assignments", prompt["actions"])
        (row,) = prompt["rows"]
        self.assertIsNone(row["choice"])
        self.assertEqual("unassigned", row["status"])
        self.assertIn("detector-secondary", row["reason"])

    def test_available_effort_is_an_alternative_when_the_preferred_effort_is_missing(self):
        projected = projection("casefile-review-atomic")
        pair = ("fixture-terra", "medium")
        prompt = self.recommend(projected, offer(projected, {pair}, {pair}))
        (row,) = prompt["rows"]
        self.assertEqual("alternative", row["status"])
        self.assertEqual({"model": pair[0], "reasoning_effort": pair[1]}, row["choice"])
        self.assertIn("Terra / xhigh", row["reason"])

    def test_edited_confirmed_choices_keep_projection_revisions_and_require_full_live_eligibility(self):
        projected = projection("casefile-review-two-stage")
        terra = ("fixture-1-terra", "xhigh")
        alternate = ("fixture-2-sol", "medium")
        pairs = {terra, (terra[0], "medium"), alternate}
        current = offer(projected, pairs, pairs)
        proposed = self.recommend(projected, current)
        self.assertEqual("confirm", proposed["state"])
        self.assertTrue(all(alternate[0] in json.dumps(row["eligible_choices"]) for row in proposed["rows"]))
        confirmed = [
            {"role": "atomic-ticket-reviewer", "model": alternate[0], "reasoning_effort": alternate[1]},
            {"role": "verification-reviewer", "model": terra[0], "reasoning_effort": "medium"},
        ]
        result = self.run_helper("select", projected, current, confirmed_roles=confirmed)
        self.assertEqual(0, result.returncode, result.stderr)
        self.assertEqual(
            {"request": {"target": projected["target"], "expected_revisions": projected["revisions"], "roles": confirmed}},
            json.loads(result.stdout),
        )
        refreshed = offer(projected, pairs, pairs - {alternate})
        for selection, live_offer in ((confirmed[:1], current), (confirmed, refreshed)):
            refusal = self.run_helper("select", projected, live_offer, confirmed_roles=selection)
            self.assertEqual(1, refusal.returncode)
            self.assertEqual("", refusal.stdout)

    def test_single_role_reuses_only_a_current_explicit_pair_and_solo_and_claude_skip(self):
        projected = projection("casefile-investigate-atomic")
        pair = ("saved-sol", "medium")
        live = offer(projected, {pair}, {pair})
        first = self.recommend(projected, live)
        self.assertEqual("confirm", first["state"])
        self.assertNotIn("request", first)
        projected["roles"]["detective"]["selection"] = {
            "model": pair[0], "reasoning_effort": pair[1], "source": "strategy_assignment",
        }
        reused = self.recommend(projected, live)
        self.assertEqual("reuse", reused["state"])
        self.assertEqual([], reused["actions"])
        withdrew = self.recommend(projected, offer(projected, {pair}, set()))
        self.assertEqual("blocked", withdrew["state"])
        self.assertIn(pair[0], withdrew["rows"][0]["reason"])

        for skipped in (projection("casefile-investigate-solo"), {**projected, "adapter": "claude"}):
            self.assertEqual({"state": "skip", "actions": [], "rows": []}, self.recommend(skipped, {}))
