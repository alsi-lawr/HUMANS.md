from __future__ import annotations

import json
import subprocess
import unittest
from pathlib import Path
from unittest import mock

from _load import script


runtime = script("casefile/adapters/shared/casefile_runtime.py")


class RuntimeProbeTests(unittest.TestCase):
    def probe(self, tools: set[str]) -> int:
        compatibility = {
            "identity": "casefile",
            "provider_protocol_version": 6,
            "required_provider_operations": sorted(runtime.REQUIRED_OPERATIONS),
        }
        messages = [
            {"id": 1, "result": {"serverInfo": {"name": "casefile"}}},
            {"id": 2, "result": {"tools": [{"name": name} for name in sorted(tools)]}},
        ]
        replies = [
            "casefile 0.10.0\n",
            json.dumps(compatibility),
            "\n".join(map(json.dumps, messages)),
        ]
        with mock.patch.object(
            runtime.subprocess, "run",
            side_effect=[subprocess.CompletedProcess([], 0, stdout=reply) for reply in replies],
        ):
            return runtime.probe(Path("casefile"), "0.10.0", Path("planning"))

    def test_probe_accepts_role_assignment_tools_and_refuses_same_size_substitution(self):
        tools = runtime.REQUIRED_MCP_TOOLS | {
            "casefile_project_strategy_assignments",
            "casefile_preview_strategy_assignments",
            "casefile_apply_strategy_assignments",
        }
        self.assertEqual(len(tools), self.probe(tools))

        substituted = tools - {"casefile_apply_strategy_assignments"} | {"casefile_unrelated"}
        with self.assertRaises(runtime.RuntimeError):
            self.probe(substituted)
