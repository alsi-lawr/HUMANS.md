from __future__ import annotations

import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from _load import ROOT


class CodexModelMigrationTests(unittest.TestCase):
    def test_retired_migration_cannot_restore_catalog_ownership(self):
        with tempfile.TemporaryDirectory() as temporary:
            home = Path(temporary)
            original = b'model_catalog_json = "foreign.json"\n'
            (home / "config.toml").write_bytes(original)
            result = subprocess.run([
                sys.executable, ROOT / "casefile/adapters/codex/scripts/setup-codex.py",
                "migrate-models", "--plugin-root", home, "--codex-home", home,
                "--codex-executable", "codex", "--apply",
            ], capture_output=True, text=True)
            self.assertEqual(1, result.returncode, result.stdout + result.stderr)
            self.assertIn("use Casefile setup", result.stdout)
            self.assertEqual(original, (home / "config.toml").read_bytes())
            self.assertEqual({"config.toml"}, {path.name for path in home.iterdir()})
