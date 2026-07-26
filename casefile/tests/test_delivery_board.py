from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
import urllib.request
from pathlib import Path

from _load import ROOT


provision = ROOT / "casefile/casefile-workflow/scripts/provision-delivery-board.py"
investigation = "projects/demo/investigations/sample"


class DeliveryBoardScriptTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        subprocess.run(["cargo", "build", "-p", "casefile-cli"], cwd=ROOT / "casefile", check=True)
        target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "casefile/target"))
        cls.binary = target / "debug/casefile"

    def fixture(self, root: Path) -> None:
        shutil.copytree(ROOT / "casefile/casefile-store/tests/fixtures/minimum", root, dirs_exist_ok=True)
        for arguments in (
            ["init", "-q"],
            ["config", "user.email", "casefile@example.test"],
            ["config", "user.name", "Casefile Test"],
            ["add", "."],
            ["commit", "-qm", "fixture"],
        ):
            subprocess.run(["git", *arguments], cwd=root, check=True)

    def run_script(self, root: Path, preview: Path, apply: bool = False) -> subprocess.CompletedProcess[str]:
        command = [
            sys.executable,
            str(provision),
            "--root",
            str(root),
            "--casefile",
            str(self.binary),
            "--preview-file",
            str(preview),
            "--investigation",
            investigation,
        ]
        if apply:
            command.append("--apply")
        return subprocess.run(command, text=True, capture_output=True, check=False)

    def derived_boards(self, root: Path, index: Path) -> list[dict]:
        process = subprocess.Popen(
            [str(self.binary), "--root", str(root), "serve", "--index", str(index)],
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        try:
            assert process.stdout is not None
            lines = [process.stdout.readline().strip() for _ in range(4)]
            port = lines[0].removeprefix("Casefile server: http://127.0.0.1:")
            request = urllib.request.Request(
                f"http://127.0.0.1:{port}/api/query",
                data=json.dumps({
                    "query": "boards",
                    "scope": {"project": "demo", "investigation": "sample"},
                }).encode(),
                headers={"Content-Type": "application/json"},
                method="POST",
            )
            with urllib.request.urlopen(request) as response:
                return json.load(response)["Current"]["value"]
        finally:
            process.terminate()
            process.wait(timeout=10)
            if process.stdout is not None:
                process.stdout.close()
            if process.stderr is not None:
                process.stderr.close()

    def test_absent_board_is_canonical_governed_and_progress_derived(self):
        with tempfile.TemporaryDirectory() as workspace, tempfile.TemporaryDirectory() as scratch:
            root, preview = Path(workspace) / "root", Path(scratch) / "preview.json"
            self.fixture(root)
            ticket = root / f"{investigation}/tickets/accepted/HMD-011.md"
            ticket_before = ticket.read_bytes()
            progress = root / f"{investigation}/progress/log.toml"
            result = self.run_script(root, preview)
            self.assertEqual(0, result.returncode, result.stderr)
            self.assertFalse((root / f"{investigation}/boards/delivery.toml").exists())
            self.assertFalse(json.loads(result.stdout)["no_op"])

            applied = self.run_script(root, preview, apply=True)
            self.assertEqual(0, applied.returncode, applied.stderr)
            board = root / f"{investigation}/boards/delivery.toml"
            self.assertEqual(
                'schema_version = 1\nid = "HMD-delivery"\ntitle = "Delivery"\n'
                'status_source = "progress"\nfilter_kinds = ["ticket"]\n\n'
                '[[columns]]\nname = "Unknown"\nstatuses = ["unknown"]\n\n'
                '[[columns]]\nname = "In progress"\nstatuses = ["in_progress"]\n\n'
                '[[columns]]\nname = "In review"\nstatuses = ["in_review"]\n\n'
                '[[columns]]\nname = "Verifying"\nstatuses = ["verifying"]\n\n'
                '[[columns]]\nname = "Blocked"\nstatuses = ["blocked"]\n\n'
                '[[columns]]\nname = "Complete"\nstatuses = ["complete"]\n',
                board.read_text(encoding="utf-8"),
            )
            scan = subprocess.run(
                [str(self.binary), "--root", str(root), "scan"],
                text=True,
                capture_output=True,
                check=True,
            )
            record = next(item for item in json.loads(scan.stdout)["snapshot"]["entries"] if item["path"] == f"{investigation}/boards/delivery.toml")
            self.assertEqual(("governed", "board"), (record["classification"], record["kind"]))
            boards = self.derived_boards(root, Path(scratch) / "index.sqlite")
            delivery = next(board for board in boards if board["identity"]["identity"] == "HMD-delivery")
            self.assertEqual("HMD-011", delivery["columns"][0]["cards"][0]["identity"]["identity"])
            self.assertEqual(ticket_before, ticket.read_bytes())
            self.assertFalse(progress.exists())

    def test_exact_repeat_is_no_op_and_different_target_is_preserved(self):
        with tempfile.TemporaryDirectory() as workspace, tempfile.TemporaryDirectory() as scratch:
            root, preview = Path(workspace) / "root", Path(scratch) / "preview.json"
            self.fixture(root)
            self.assertEqual(0, self.run_script(root, preview).returncode)
            self.assertEqual(0, self.run_script(root, preview, apply=True).returncode)
            exact = self.run_script(root, preview)
            self.assertEqual(0, exact.returncode, exact.stderr)
            self.assertTrue(json.loads(exact.stdout)["no_op"])
            inode = (root / f"{investigation}/boards/delivery.toml").stat().st_ino
            retry = self.run_script(root, preview, apply=True)
            self.assertEqual(0, retry.returncode, retry.stderr)
            self.assertTrue(json.loads(retry.stdout)["no_op"])
            self.assertEqual(inode, (root / f"{investigation}/boards/delivery.toml").stat().st_ino)

            board = root / f"{investigation}/boards/delivery.toml"
            different = board.read_text(encoding="utf-8").replace('title = "Delivery"', 'title = "Custom"')
            board.write_text(different, encoding="utf-8")
            refused = self.run_script(root, preview)
            self.assertNotEqual(0, refused.returncode)
            self.assertIn("already differs", refused.stderr)
            self.assertTrue(json.loads(refused.stdout)["diff"])
            self.assertEqual(different, board.read_text(encoding="utf-8"))

    def test_external_saved_preview_enforces_root_and_store_revisions(self):
        with tempfile.TemporaryDirectory() as workspace, tempfile.TemporaryDirectory() as scratch:
            root, preview = Path(workspace) / "root", Path(scratch) / "preview.json"
            self.fixture(root)
            in_root = self.run_script(root, root / "preview.json")
            self.assertNotEqual(0, in_root.returncode)
            self.assertIn("outside --root", in_root.stderr)

            self.assertEqual(0, self.run_script(root, preview).returncode)
            (root / "unrelated.txt").write_text("changed", encoding="utf-8")
            stale = self.run_script(root, preview, apply=True)
            self.assertNotEqual(0, stale.returncode)
            self.assertFalse((root / f"{investigation}/boards/delivery.toml").exists())

    def test_invalid_unactivated_missing_and_ambiguous_mappings_are_refused(self):
        cases = {
            "invalid": "schema_version = 2\n",
            "unactivated": None,
            "missing": 'schema_version = 1\n\n[projects.demo]\nprefix = "HMD"\ninvestigations = []\n',
            "ambiguous": 'schema_version = 1\n\n[projects.demo]\nprefix = "HMD"\ninvestigations = ["projects/demo/investigations/sample", "projects/demo/investigations/sample"]\n',
        }
        for name, activation in cases.items():
            with self.subTest(name=name), tempfile.TemporaryDirectory() as workspace, tempfile.TemporaryDirectory() as scratch:
                root, preview = Path(workspace) / "root", Path(scratch) / "preview.json"
                self.fixture(root)
                path = root / "casefile.toml"
                if activation is None:
                    path.unlink()
                else:
                    path.write_text(activation, encoding="utf-8")
                result = self.run_script(root, preview)
                self.assertNotEqual(0, result.returncode)
                self.assertFalse((root / f"{investigation}/boards/delivery.toml").exists())

    def test_symlink_and_non_file_targets_are_refused_without_mutation(self):
        for collision in ("symlink", "directory"):
            with self.subTest(collision=collision), tempfile.TemporaryDirectory() as workspace, tempfile.TemporaryDirectory() as scratch:
                root, preview = Path(workspace) / "root", Path(scratch) / "preview.json"
                self.fixture(root)
                target = root / f"{investigation}/boards/delivery.toml"
                target.parent.mkdir(parents=True, exist_ok=True)
                if collision == "symlink":
                    source = Path(scratch) / "outside.toml"
                    source.write_text("preserve\n", encoding="utf-8")
                    target.symlink_to(source)
                else:
                    target.mkdir()
                result = self.run_script(root, preview)
                self.assertNotEqual(0, result.returncode)
                if collision == "symlink":
                    self.assertEqual("preserve\n", source.read_text(encoding="utf-8"))
                else:
                    self.assertTrue(target.is_dir())

    def test_workflow_contracts_call_the_packaged_wrapper_at_the_required_gates(self):
        startup = (ROOT / "casefile/skills/casefile/SKILL.md").read_text(encoding="utf-8")
        consolidate = (ROOT / "casefile/skills/casefile-consolidate/SKILL.md").read_text(encoding="utf-8")
        script = "casefile-workflow/scripts/provision-delivery-board.py"
        self.assertIn(script, startup)
        self.assertLess(startup.index(script), startup.index("resolve-writer-binding.py offer"))
        self.assertIn(script, consolidate)
        self.assertIn("progress-log outcome", consolidate)


if __name__ == "__main__":
    unittest.main()
