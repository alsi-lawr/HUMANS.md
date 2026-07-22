from __future__ import annotations

import hashlib
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from _load import ROOT, script


switch = script("casefile/casefile-workflow/scripts/switch-strategy.py")


MATRIX = '''schema_version = 1
strategy_id = "casefile-review-atomic"
phase = "review"
adapter = "test"
[orchestrator]
binding = "root"
[limits]
max_concurrent_subagents = 1
max_depth = 1
[requirements]
capabilities = ["subagents"]
[[workers]]
role = "reviewer"
platform_profile = "reviewer"
minimum_count = 1
maximum_count = 1
can_spawn_subagents = false
[coordination]
batch_when_capacity_exceeded = true
candidate_review_before_ticket = false
shared_ticket_storage_required = true
'''


class SwitchStrategyTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        subprocess.run(
            ["cargo", "build", "-p", "casefile-cli"],
            cwd=ROOT / "casefile",
            check=True,
            capture_output=True,
            text=True,
        )
        cls.environment = os.environ | {"CASEFILE_MATRIX_VALIDATOR": str(ROOT / "casefile/target/debug/casefile")}

    def test_governed_replacement_is_backed_up_and_rolls_back_on_record_failure(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            state = root / "state.toml"
            state.write_text(
                '''schema_version = 1
strategy_id = "current"
phase = "review"
[root]
binding = "root"
[work]
paths = ["tickets/T-001.md"]
''',
                encoding="ascii",
            )
            matrix = root / "matrix.toml"
            matrix.write_text(MATRIX, encoding="ascii")
            output = root / "strategy"
            output.mkdir()
            old = b'schema_version = 1\nstrategy_id = "old"\n'
            selected = output / "review.toml"
            selected.write_bytes(old)
            result = subprocess.run(
                [
                    sys.executable,
                    str(ROOT / "casefile/casefile-workflow/scripts/switch-strategy.py"),
                    "--state",
                    str(state),
                    "--matrix",
                    str(matrix),
                    "--output-dir",
                    str(output),
                    "--mode",
                    "governed",
                    "--capability",
                    "subagents",
                    "--rationale",
                    "human selected",
                    "--timestamp",
                    "2026-07-15T12:00:00Z",
                    "--apply",
                ],
                capture_output=True,
                text=True,
                check=False,
                env=self.environment,
            )
            self.assertEqual(0, result.returncode, result.stdout + result.stderr)
            self.assertEqual(matrix.read_bytes(), selected.read_bytes())
            backup = output / "backups" / f"review-{hashlib.sha256(old).hexdigest()}.toml"
            self.assertEqual(old, backup.read_bytes())

            selected.write_bytes(old)
            before_mtime = selected.stat().st_mtime_ns

            def fail(_path: Path, _data: bytes) -> bool:
                raise OSError("injected record failure")

            with self.assertRaisesRegex(OSError, "injected"):
                switch.apply_transaction(
                    selected,
                    matrix.read_bytes(),
                    root / "failed-transition.toml",
                    b"record\n",
                    root / "failed-backup.toml",
                    record_writer=fail,
                )
            self.assertEqual(old, selected.read_bytes())
            self.assertEqual(before_mtime, selected.stat().st_mtime_ns)
            self.assertFalse((root / "failed-transition.toml").exists())


    def test_switch_uses_rust_validation_for_pipeline_shape(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            state = root / "state.toml"
            state.write_text('''schema_version = 1
strategy_id = "current"
phase = "review"
[root]
binding = "root"
[work]
paths = []
''', encoding="ascii")
            matrix = root / "matrix.toml"
            matrix.write_text(MATRIX + "\n[coordination.pipeline]\nmaximum_active_tickets = 1\n", encoding="ascii")
            result = subprocess.run(
                [sys.executable, str(ROOT / "casefile/casefile-workflow/scripts/switch-strategy.py"),
                 "--state", str(state), "--matrix", str(matrix), "--output-dir", str(root / "strategy"),
                 "--mode", "governed", "--capability", "subagents", "--rationale", "test"],
                capture_output=True, text=True, check=False, env=self.environment,
            )
            self.assertNotEqual(0, result.returncode)
            self.assertIn("canonical Rust matrix validation failed", result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
