from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from _load import ROOT


transition = ROOT / "casefile/casefile-workflow/scripts/transition-ticket-progress.py"


class TicketProgressScriptTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        subprocess.run(["cargo", "build", "-p", "casefile-cli"], cwd=ROOT / "casefile", check=True)
        cls.binary = ROOT / "casefile/target/debug/casefile"

    def fixture(self, root: Path) -> None:
        shutil.copytree(ROOT / "casefile/casefile-store/tests/fixtures/minimum", root, dirs_exist_ok=True)
        for arguments in (["init", "-q"], ["config", "user.email", "casefile@example.test"], ["config", "user.name", "Casefile Test"], ["add", "."], ["commit", "-qm", "fixture"]):
            subprocess.run(["git", *arguments], cwd=root, check=True)

    def run_script(self, root: Path, preview: Path, *arguments: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run([sys.executable, str(transition), "--root", str(root), "--casefile", str(self.binary), "--preview-file", str(preview), *arguments], text=True, capture_output=True, check=False)

    def test_saved_preview_is_stale_after_an_intervening_change_and_exact_retry_is_no_op(self):
        with tempfile.TemporaryDirectory() as workspace, tempfile.TemporaryDirectory() as scratch:
            root, preview = Path(workspace) / "root", Path(scratch) / "preview.json"
            self.fixture(root)
            command = ("transition", "--investigation", "projects/demo/investigations/sample", "--recorded-by", "root", "--recorded-at", "2026-07-26T10:00:00Z", "--ticket", "HMD-011", "--from", "unknown", "--to", "in_progress", "--operation-id", "start-001")
            self.assertEqual(0, self.run_script(root, preview, *command).returncode)
            (root / "unrelated.txt").write_text("changed", encoding="utf-8")
            stale = self.run_script(root, preview, "--apply", *command)
            self.assertNotEqual(0, stale.returncode)
            self.assertFalse((root / "projects/demo/investigations/sample/progress/log.toml").exists())
            (root / "unrelated.txt").unlink()
            self.assertEqual(0, self.run_script(root, preview, *command).returncode)
            applied = self.run_script(root, preview, "--apply", *command)
            self.assertEqual(0, applied.returncode, applied.stderr)
            retry = self.run_script(root, preview, "--apply", *command)
            self.assertEqual(0, retry.returncode, retry.stderr)
            self.assertTrue(json.loads(retry.stdout)["no_op"])

    def test_preview_file_under_the_planning_root_is_rejected(self):
        with tempfile.TemporaryDirectory() as workspace:
            root = Path(workspace) / "root"
            self.fixture(root)
            result = self.run_script(root, root / "preview.json", "bootstrap-unknown", "--investigation", "projects/demo/investigations/sample")
            self.assertNotEqual(0, result.returncode)
            self.assertIn("outside --root", result.stderr)

    def test_bootstrap_note_and_replace_are_previewed_then_applied(self):
        with tempfile.TemporaryDirectory() as workspace, tempfile.TemporaryDirectory() as scratch:
            root, preview = Path(workspace) / "root", Path(scratch) / "preview.json"
            self.fixture(root)
            investigation = "projects/demo/investigations/sample"
            log = root / f"{investigation}/progress/log.toml"

            bootstrap = self.run_script(root, preview, "bootstrap-unknown", "--investigation", investigation)
            self.assertEqual(0, bootstrap.returncode, bootstrap.stderr)
            self.assertEqual(["HMD-011"], json.loads(bootstrap.stdout)["bootstrap_ticket_ids"])
            self.assertFalse(log.exists())
            applied = self.run_script(root, preview, "--apply", "bootstrap-unknown", "--investigation", investigation)
            self.assertEqual(0, applied.returncode, applied.stderr)
            self.assertEqual("schema_version = 1\n", log.read_text(encoding="utf-8"))

            existing = self.run_script(root, preview, "bootstrap-unknown", "--investigation", investigation)
            self.assertEqual(0, existing.returncode, existing.stderr)
            existing_preview = json.loads(existing.stdout)
            self.assertTrue(existing_preview["no_op"])
            self.assertEqual([], existing_preview.get("bootstrap_ticket_ids", []))

            note = (
                "note", "--investigation", investigation, "--recorded-by", "root",
                "--recorded-at", "2026-07-26T10:00:00Z", "--ticket", "HMD-011",
                "--category", "quirk", "--message", "Checked through the sole workflow.",
                "--operation-id", "note-001",
            )
            self.assertEqual(0, self.run_script(root, preview, *note).returncode)
            note_apply = self.run_script(root, preview, "--apply", *note)
            self.assertEqual(0, note_apply.returncode, note_apply.stderr)
            self.assertIn('kind = "note"', log.read_text(encoding="utf-8"))

            replacement = Path(scratch) / "replacement.toml"
            replacement.write_text("schema_version = 1\n", encoding="utf-8")
            replace = (
                "replace", "--investigation", investigation, "--recorded-by", "root",
                "--recorded-at", "2026-07-26T10:01:00Z", "--replacement", str(replacement),
            )
            self.assertEqual(0, self.run_script(root, preview, *replace).returncode)
            replace_apply = self.run_script(root, preview, "--apply", *replace)
            self.assertEqual(0, replace_apply.returncode, replace_apply.stderr)
            self.assertEqual("schema_version = 1\n", log.read_text(encoding="utf-8"))

    def test_invalid_activation_refuses_progress_preview(self):
        with tempfile.TemporaryDirectory() as workspace, tempfile.TemporaryDirectory() as scratch:
            root, preview = Path(workspace) / "root", Path(scratch) / "preview.json"
            self.fixture(root)
            (root / "casefile.toml").write_text("schema_version = 2\n", encoding="utf-8")
            result = self.run_script(
                root, preview, "bootstrap-unknown", "--investigation", "projects/demo/investigations/sample"
            )
            self.assertNotEqual(0, result.returncode)
            self.assertIn("active Casefile activation", result.stderr)


if __name__ == "__main__":
    unittest.main()
