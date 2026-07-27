from __future__ import annotations

import json
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

from _load import ROOT


class ScratchStrategyTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        subprocess.run(["cargo", "build", "-p", "casefile-cli"], cwd=ROOT / "casefile", check=True)
        target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "casefile/target"))
        cls.binary = target / "debug/casefile"

    def test_explicit_scratch_target_changes_only_scratch_and_refuses_store_overlap(self):
        with tempfile.TemporaryDirectory() as temporary:
            outer = Path(temporary)
            store = outer / "store"
            shutil.copytree(ROOT / "casefile/casefile-store/tests/fixtures/minimum", store)
            matrix = ROOT / "casefile/adapters/codex/matrices/casefile-implement-ticket-batch.toml"
            scratch = outer / "scratch" / "selected.toml"
            before = {path.relative_to(store): path.read_bytes() for path in store.rglob("*") if path.is_file()}
            result = subprocess.run(
                [str(self.binary), "--root", str(store), "scratch-strategy", "--matrix", str(matrix), "--target", str(scratch)],
                capture_output=True, text=True, check=False,
            )
            self.assertEqual(0, result.returncode, result.stderr)
            self.assertEqual(matrix.read_bytes(), scratch.read_bytes())
            after = {path.relative_to(store): path.read_bytes() for path in store.rglob("*") if path.is_file()}
            self.assertEqual(before, after)
            refused = subprocess.run(
                [str(self.binary), "--root", str(store), "scratch-strategy", "--matrix", str(matrix), "--target", str(store / "new" / "nested" / "strategy.toml")],
                capture_output=True, text=True, check=False,
            )
            self.assertNotEqual(0, refused.returncode)
            self.assertIn("must not overlap", refused.stderr)
            self.assertFalse((store / "new").exists())
            final = {path.relative_to(store): path.read_bytes() for path in store.rglob("*") if path.is_file()}
            self.assertEqual(before, final)

    def test_scratch_is_absent_from_provider_and_mcp_discovery(self):
        provider = (ROOT / "casefile/casefile-store/src/provider.rs").read_text(encoding="utf-8")
        mcp = (ROOT / "casefile/casefile-cli/src/mcp.rs").read_text(encoding="utf-8")
        self.assertNotIn("ScratchStrategy", provider)
        self.assertNotIn("scratch_strategy", mcp)


if __name__ == "__main__":
    unittest.main()
