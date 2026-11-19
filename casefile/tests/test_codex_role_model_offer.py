from __future__ import annotations

import contextlib
import io
import json
import sys
import unittest
from unittest import mock

from _load import script


offer = script("casefile/adapters/codex/scripts/offer-role-models.py")
lister = script("casefile/adapters/codex/scripts/list-codex-models.py")
ROLE = "casefile-implementation-writer"


def provider_model(model, efforts, *, hidden=False, selector=None):
    return {
        "id": f"picker-{model}",
        "model": model,
        "hidden": hidden,
        "multiAgentVersion": selector,
        "supportedReasoningEfforts": [{"reasoningEffort": effort} for effort in efforts],
    }


class CodexRoleModelOfferTests(unittest.TestCase):
    def query(self, evidence, provider_models, *roles):
        output = io.StringIO()
        process = mock.Mock()
        process.stdout = iter(
            json.dumps(reply)
            for reply in (
                {"id": 1, "result": {}},
                {"id": 2, "result": {"data": provider_models, "nextCursor": None}},
            )
        )
        arguments = ["offer-role-models.py", "--codex-executable", "fixture-codex", *roles]
        with (
            mock.patch.object(sys, "argv", arguments),
            mock.patch.object(sys, "stdin", io.StringIO(json.dumps(evidence))),
            mock.patch.object(offer.subprocess, "Popen", return_value=process) as start,
            contextlib.redirect_stdout(output),
        ):
            self.assertEqual(0, offer.main())
        self.assertEqual(
            ["fixture-codex", "app-server", "-c", 'model_provider="fixture-provider"'],
            start.call_args.args[0],
        )
        requests = [json.loads(call.args[0]) for call in process.stdin.write.call_args_list]
        self.assertEqual("model/list", requests[-1]["method"])
        return json.loads(output.getvalue())["roles"]

    def test_offer_intersects_the_provider_and_callable_and_builds_legal_spawn_arguments(self):
        provider_models = [
            provider_model("matching", ["high", "medium"], selector="v2"),
            provider_model("provider-only", ["high"]),
            provider_model("hidden", ["high"], hidden=True),
        ]
        for runtime, fork, fresh, full in (
            ("v1", "fork_context", False, True),
            ("v2", "fork_turns", "none", "all"),
        ):
            with self.subTest(runtime=runtime):
                evidence = {
                    "provider": "fixture-provider",
                    "runtime": runtime,
                    "parameters": ["agent_type", "model", "reasoning_effort", fork],
                    "fork_values": [fresh, full],
                    "named_role_overrides": True,
                    "roles": {ROLE: {}},
                    "models": {
                        "matching": ["high", "xhigh"],
                        "hidden": ["high"],
                        "stale": ["high"],
                    },
                }
                choices = self.query(evidence, provider_models, ROLE, "unregistered-role")
                self.assertEqual(
                    [
                        {
                            "model": "matching",
                            "reasoning_effort": "high",
                            "spawn": {
                                "agent_type": ROLE,
                                fork: fresh,
                                "model": "matching",
                                "reasoning_effort": "high",
                            },
                        }
                    ],
                    choices[ROLE]["pairs"],
                )
                self.assertEqual([], choices["unregistered-role"]["pairs"])

    def test_fixed_role_remains_selectable_when_the_host_cannot_override_an_unpinned_role(self):
        fixed = "casefile-fixed-role"
        evidence = {
            "provider": "fixture-provider",
            "runtime": "v1",
            "parameters": ["agent_type", "fork_context", "model", "reasoning_effort"],
            "fork_values": [False, True],
            "named_role_overrides": False,
            "roles": {ROLE: {}, fixed: {"model": "matching", "reasoning_effort": "high"}},
            "models": {"matching": ["high", "medium"]},
        }
        available = [provider_model("matching", ["high", "medium"])]
        choices = self.query(evidence, available, ROLE, fixed)
        self.assertEqual([], choices[ROLE]["pairs"])
        self.assertTrue(choices[ROLE]["unavailable_reason"])
        self.assertEqual(
            [
                {
                    "model": "matching",
                    "reasoning_effort": "high",
                    "spawn": {"agent_type": fixed, "fork_context": False},
                }
            ],
            choices[fixed]["pairs"],
        )
        effort_withdrawn = [provider_model("matching", ["medium"])]
        choices = self.query(evidence, effort_withdrawn, fixed)
        self.assertEqual([], choices[fixed]["pairs"])

    def test_full_history_only_or_no_current_pair_produces_no_offer(self):
        evidence = {
            "provider": "fixture-provider",
            "runtime": "v2",
            "parameters": ["agent_type", "fork_turns", "model", "reasoning_effort"],
            "fork_values": ["all"],
            "named_role_overrides": True,
            "roles": {ROLE: {}},
            "models": {"matching": ["high"]},
        }
        matching = [provider_model("matching", ["high"])]
        self.assertEqual([], self.query(evidence, matching, ROLE)[ROLE]["pairs"])
        evidence["fork_values"].append("none")
        different_effort = [provider_model("matching", ["medium"])]
        self.assertEqual([], self.query(evidence, different_effort, ROLE)[ROLE]["pairs"])

    def test_model_projection_retains_the_provider_runtime_selector_when_supplied(self):
        raw = provider_model("matching", ["high"], selector="v2")
        (model,) = lister.normalize([raw])
        self.assertEqual("v2", model["multi_agent_version"])


if __name__ == "__main__":
    unittest.main()
