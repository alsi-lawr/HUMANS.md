from __future__ import annotations

import subprocess
import sys
import tempfile
import tomllib
import unittest
from pathlib import Path

from _load import ROOT, script


switch = script("casefile-workflow/scripts/switch-strategy.py")


class SwitchStrategyTests(unittest.TestCase):
    def documents(self):
        state = {
            "schema_version": 1,
            "strategy_id": "current",
            "phase": "review",
            "root": {"binding": "root"},
            "work": {"paths": ["tickets/T-001.md"]},
            "ownership": [{"owner": "writer", "active": True, "paths": ["src/a"]}],
        }
        matrix = {
            "schema_version": 1,
            "strategy_id": "casefile-review-atomic",
            "phase": "review",
            "orchestrator": {"binding": "root"},
            "requirements": {"capabilities": ["subagents"]},
        }
        return state, matrix

    def test_rejects_unavailable_capability_and_overlapping_writers(self):
        state, matrix = self.documents()
        errors = switch.validate(state, matrix, set())
        self.assertTrue(any("unavailable capabilities" in item for item in errors))
        state["ownership"].append({"owner": "other", "active": True, "paths": ["src/a/file"]})
        errors = switch.validate(state, matrix, {"subagents"})
        self.assertTrue(any("overlapping active writers" in item for item in errors))

    def test_governed_apply_preserves_work_and_records_matrix(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            state = root / "state.toml"
            state.write_text('''schema_version = 1
strategy_id = "current"
phase = "review"
[root]
binding = "root"
[work]
paths = ["tickets/T-001.md"]
[[ownership]]
owner = "writer"
active = true
paths = ["src/a"]
''', encoding="ascii")
            matrix = root / "matrix.toml"
            matrix.write_text('''schema_version = 1
strategy_id = "casefile-review-atomic"
phase = "review"
[orchestrator]
binding = "root"
[requirements]
capabilities = ["subagents"]
''', encoding="ascii")
            output = root / "strategy"
            command = [sys.executable, str(ROOT / "casefile-workflow/scripts/switch-strategy.py"), "--state", str(state), "--matrix", str(matrix), "--output-dir", str(output), "--mode", "governed", "--capability", "subagents", "--rationale", "human selected", "--timestamp", "2026-07-15T12:00:00Z", "--apply"]
            result = subprocess.run(command, capture_output=True, text=True, check=False)
            self.assertEqual(0, result.returncode, result.stdout + result.stderr)
            self.assertEqual(matrix.read_bytes(), (output / "review.toml").read_bytes())
            records = list((output / "transitions").glob("*.toml"))
            self.assertEqual(1, len(records))
            record = tomllib.loads(records[0].read_text())
            self.assertEqual(["tickets/T-001.md"], record["preserved_work_paths"])
            self.assertTrue(record["governed_state_updated"])


if __name__ == "__main__":
    unittest.main()
