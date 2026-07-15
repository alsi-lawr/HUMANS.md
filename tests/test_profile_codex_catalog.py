from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path

from _load import ROOT, script


profiler = script("adapters/codex/scripts/profile-codex-catalog.py")


class CatalogProfileTests(unittest.TestCase):
    def files(self, root: Path):
        (root / "instructions").mkdir()
        (root / "instructions/root.md").write_text("bounded instructions\n", encoding="ascii")
        profile = root / "profiles.toml"
        profile.write_text(
            '''schema_version = 1
adapter = "codex"
[catalog]
id_field = "slug"
instruction_fields = ["base_instructions"]
model_message_fields = ["instructions_template"]
selector_fields = ["model_messages.approvals"]
[[catalog.targets]]
id = "model-a"
required_reasoning = ["high"]
instruction_file = "instructions/root.md"
null_selectors = ["model_messages.approvals"]
[catalog.targets.expected]
display_name = "Model A"
''',
            encoding="ascii",
        )
        catalog = root / "fresh-export.json"
        catalog.write_text(
            json.dumps({"models": [{"slug": "model-a", "display_name": "Model A", "supported_reasoning_levels": [{"effort": "high"}], "base_instructions": "old", "model_messages": {"approvals": {"mode": "ask"}, "untouched": 7}, "extra": {"keep": True}}]}),
            encoding="ascii",
        )
        return profile, catalog

    def test_preserves_fields_and_nulls_declared_selector(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            profile, catalog = self.files(root)
            result, stale = profiler.build(json.loads(catalog.read_text()), profiler.load_profile(profile), profile)
            self.assertEqual([], stale)
            model = result["models"][0]
            self.assertEqual("bounded instructions\n", model["base_instructions"])
            self.assertIsNone(model["model_messages"]["approvals"])
            self.assertEqual(7, model["model_messages"]["untouched"])
            self.assertEqual({"keep": True}, model["extra"])

    def test_rejects_duplicate_models(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            profile, catalog = self.files(root)
            document = json.loads(catalog.read_text())
            document["models"].append(dict(document["models"][0]))
            with self.assertRaisesRegex(ValueError, "duplicate model"):
                profiler.build(document, profiler.load_profile(profile), profile)

    def test_apply_backups_permissions_and_idempotence(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            profile, catalog = self.files(root)
            target = root / "profiled.json"
            backups = root / "backups"
            command = [sys.executable, str(ROOT / "adapters/codex/scripts/profile-codex-catalog.py"), "--catalog", str(catalog), "--profile", str(profile), "--target", str(target), "--backup-dir", str(backups), "--apply"]
            first = subprocess.run(command, capture_output=True, text=True, check=False)
            self.assertEqual(0, first.returncode, first.stdout + first.stderr)
            self.assertEqual(0o600, target.stat().st_mode & 0o777)
            self.assertEqual(1, len(list(backups.glob("pristine-*.json"))))
            mtime = target.stat().st_mtime_ns
            second = subprocess.run(command, capture_output=True, text=True, check=False)
            self.assertEqual(0, second.returncode, second.stdout + second.stderr)
            self.assertEqual(mtime, target.stat().st_mtime_ns)


if __name__ == "__main__":
    unittest.main()
