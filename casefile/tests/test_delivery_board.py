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

    def run_script(
        self,
        root: Path,
        preview: Path,
        apply: bool = False,
        selected_investigation: str = investigation,
    ) -> subprocess.CompletedProcess[str]:
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
            selected_investigation,
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
                'schema_version = 1\nid = "HMD-sample-delivery"\ntitle = "Delivery"\n'
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
            delivery = next(board for board in boards if board["identity"]["identity"] == "HMD-sample-delivery")
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

    def test_two_investigations_receive_distinct_directory_scoped_identities(self):
        with tempfile.TemporaryDirectory() as workspace, tempfile.TemporaryDirectory() as scratch:
            root = Path(workspace) / "root"
            self.fixture(root)
            second = "projects/demo/investigations/second-casefile"
            activation = (root / "casefile.toml").read_text(encoding="utf-8")
            (root / "casefile.toml").write_text(
                activation.replace(
                    f'investigations = ["{investigation}"]',
                    f'investigations = ["{investigation}", "{second}"]',
                ),
                encoding="utf-8",
            )

            identities = []
            for index, selected in enumerate((investigation, second)):
                preview = Path(scratch) / f"preview-{index}.json"
                result = self.run_script(root, preview, selected_investigation=selected)
                self.assertEqual(0, result.returncode, result.stderr)
                applied = self.run_script(root, preview, apply=True, selected_investigation=selected)
                self.assertEqual(0, applied.returncode, applied.stderr)
                board = root / selected / "boards/delivery.toml"
                identities.append(
                    next(
                        line.removeprefix('id = "').removesuffix('"')
                        for line in board.read_text(encoding="utf-8").splitlines()
                        if line.startswith("id = ")
                    )
                )

            self.assertEqual(["HMD-sample-delivery", "HMD-second-casefile-delivery"], identities)
            self.assertEqual(len(identities), len(set(identities)))
            scan = subprocess.run(
                [str(self.binary), "--root", str(root), "scan"],
                text=True,
                capture_output=True,
                check=True,
            )
            self.assertFalse(
                any(item["code"] == "duplicate_identity" for item in json.loads(scan.stdout)["diagnostics"])
            )

    def test_nested_identity_collision_refuses_preview_and_apply_without_touching_targets(self):
        alpha = "projects/demo/investigations/alpha/shared"
        beta = "projects/demo/investigations/beta/shared"
        with tempfile.TemporaryDirectory() as workspace, tempfile.TemporaryDirectory() as scratch:
            root, preview = Path(workspace) / "root", Path(scratch) / "preview.json"
            self.fixture(root)
            (root / "casefile.toml").write_text(
                'schema_version = 1\n\n[projects.demo]\nprefix = "HMD"\n'
                f'investigations = ["{alpha}", "{beta}"]\n',
                encoding="utf-8",
            )
            refused = self.run_script(root, preview, selected_investigation=alpha)
            self.assertNotEqual(0, refused.returncode)
            self.assertIn("must map to exactly one", refused.stderr)
            self.assertFalse(preview.exists())
            self.assertFalse((root / alpha / "boards/delivery.toml").exists())
            self.assertFalse((root / beta / "boards/delivery.toml").exists())

        with tempfile.TemporaryDirectory() as workspace, tempfile.TemporaryDirectory() as scratch:
            root, preview = Path(workspace) / "root", Path(scratch) / "preview.json"
            self.fixture(root)
            (root / "casefile.toml").write_text(
                'schema_version = 1\n\n[projects.demo]\nprefix = "HMD"\n'
                f'investigations = ["{alpha}"]\n',
                encoding="utf-8",
            )
            prepared = self.run_script(root, preview, selected_investigation=alpha)
            self.assertEqual(0, prepared.returncode, prepared.stderr)
            (root / "casefile.toml").write_text(
                'schema_version = 1\n\n[projects.demo]\nprefix = "HMD"\n'
                f'investigations = ["{alpha}", "{beta}"]\n',
                encoding="utf-8",
            )
            alpha_target = root / alpha / "boards/delivery.toml"
            beta_target = root / beta / "boards/delivery.toml"
            alpha_target.parent.mkdir(parents=True, exist_ok=True)
            beta_target.parent.mkdir(parents=True, exist_ok=True)
            alpha_target.write_text("preserve alpha\n", encoding="utf-8")
            beta_target.write_text("preserve beta\n", encoding="utf-8")

            refused = self.run_script(root, preview, apply=True, selected_investigation=alpha)
            self.assertNotEqual(0, refused.returncode)
            self.assertIn("must map to exactly one", refused.stderr)
            self.assertEqual("preserve alpha\n", alpha_target.read_text(encoding="utf-8"))
            self.assertEqual("preserve beta\n", beta_target.read_text(encoding="utf-8"))

    def test_unchanged_store_diagnostics_do_not_block_but_introduced_diagnostics_do(self):
        with tempfile.TemporaryDirectory() as workspace, tempfile.TemporaryDirectory() as scratch:
            root, preview = Path(workspace) / "root", Path(scratch) / "preview.json"
            self.fixture(root)
            historical = "projects/demo/investigations/z-historical"
            activation = (root / "casefile.toml").read_text(encoding="utf-8")
            (root / "casefile.toml").write_text(
                activation.replace(
                    f'investigations = ["{investigation}"]',
                    f'investigations = ["{investigation}", "{historical}"]',
                ),
                encoding="utf-8",
            )
            historical_ticket = root / historical / "tickets/accepted/HMD-011.md"
            historical_ticket.parent.mkdir(parents=True)
            historical_ticket.write_bytes(
                (root / investigation / "tickets/accepted/HMD-011.md").read_bytes()
            )
            before = json.loads(
                subprocess.run(
                    [str(self.binary), "--root", str(root), "scan"],
                    text=True,
                    capture_output=True,
                    check=True,
                ).stdout
            )["diagnostics"]
            self.assertTrue(before)

            result = self.run_script(root, preview)
            self.assertEqual(0, result.returncode, result.stderr)
            applied = self.run_script(root, preview, apply=True)
            self.assertEqual(0, applied.returncode, applied.stderr)
            after = json.loads(
                subprocess.run(
                    [str(self.binary), "--root", str(root), "scan"],
                    text=True,
                    capture_output=True,
                    check=True,
                ).stdout
            )["diagnostics"]
            self.assertEqual(before, after)

        with tempfile.TemporaryDirectory() as workspace, tempfile.TemporaryDirectory() as scratch:
            root, preview = Path(workspace) / "root", Path(scratch) / "preview.json"
            self.fixture(root)
            existing = root / investigation / "boards/existing.toml"
            existing.write_text(
                (root / investigation / "boards/main.toml")
                .read_text(encoding="utf-8")
                .replace("HMD-board", "HMD-sample-delivery"),
                encoding="utf-8",
            )
            refused = self.run_script(root, preview)
            self.assertNotEqual(0, refused.returncode)
            self.assertIn("identity also appears", refused.stderr)
            self.assertFalse((root / investigation / "boards/delivery.toml").exists())

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

    def test_symlinked_root_is_refused_before_preview_and_apply(self):
        with tempfile.TemporaryDirectory() as workspace, tempfile.TemporaryDirectory() as scratch:
            real_root = Path(workspace) / "root"
            linked_root = Path(workspace) / "linked-root"
            preview = Path(scratch) / "preview.json"
            self.fixture(real_root)
            linked_root.symlink_to(real_root, target_is_directory=True)

            refused_preview = self.run_script(linked_root, preview)
            self.assertNotEqual(0, refused_preview.returncode)
            self.assertIn("planning root must not be a symlink", refused_preview.stderr)
            self.assertFalse((real_root / f"{investigation}/boards/delivery.toml").exists())

            self.assertEqual(0, self.run_script(real_root, preview).returncode)
            refused_apply = self.run_script(linked_root, preview, apply=True)
            self.assertNotEqual(0, refused_apply.returncode)
            self.assertIn("planning root must not be a symlink", refused_apply.stderr)
            self.assertFalse((real_root / f"{investigation}/boards/delivery.toml").exists())

    def test_board_ancestor_collisions_cannot_redirect_preview_or_apply(self):
        for external_target in (False, True):
            with self.subTest(external_target=external_target), tempfile.TemporaryDirectory() as workspace, tempfile.TemporaryDirectory() as scratch:
                root, preview = Path(workspace) / "root", Path(scratch) / "preview.json"
                self.fixture(root)
                boards = root / f"{investigation}/boards"
                external = Path(scratch) / "external-boards"
                external.mkdir()
                outside = external / "delivery.toml"
                if external_target:
                    outside.write_text("preserve external board\n", encoding="utf-8")
                shutil.rmtree(boards)
                boards.symlink_to(external, target_is_directory=True)

                refused = self.run_script(root, preview)
                self.assertNotEqual(0, refused.returncode)
                self.assertIn("ancestors must not be symlinks", refused.stderr)
                if external_target:
                    self.assertEqual("preserve external board\n", outside.read_text(encoding="utf-8"))
                else:
                    self.assertFalse(outside.exists())

        with tempfile.TemporaryDirectory() as workspace, tempfile.TemporaryDirectory() as scratch:
            root, preview = Path(workspace) / "root", Path(scratch) / "preview.json"
            self.fixture(root)
            self.assertEqual(0, self.run_script(root, preview).returncode)
            boards = root / f"{investigation}/boards"
            moved = root / f"{investigation}/boards-before-redirect"
            boards.rename(moved)
            external = Path(scratch) / "external-boards"
            external.mkdir()
            boards.symlink_to(external, target_is_directory=True)
            refused = self.run_script(root, preview, apply=True)
            self.assertNotEqual(0, refused.returncode)
            self.assertIn("ancestors must not be symlinks", refused.stderr)
            self.assertFalse((external / "delivery.toml").exists())

    def test_non_directory_board_ancestor_is_refused(self):
        with tempfile.TemporaryDirectory() as workspace, tempfile.TemporaryDirectory() as scratch:
            root, preview = Path(workspace) / "root", Path(scratch) / "preview.json"
            self.fixture(root)
            boards = root / f"{investigation}/boards"
            shutil.rmtree(boards)
            boards.write_text("collision\n", encoding="utf-8")
            refused = self.run_script(root, preview)
            self.assertNotEqual(0, refused.returncode)
            self.assertIn("ancestors must be directories", refused.stderr)
            self.assertEqual("collision\n", boards.read_text(encoding="utf-8"))

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
