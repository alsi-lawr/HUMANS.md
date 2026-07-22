from __future__ import annotations

import importlib.util
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


def load(path: Path, name: str):
    specification = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(specification)
    assert specification and specification.loader
    specification.loader.exec_module(module)
    return module


claude_setup = load(
    ROOT / "humans-md/adapters/claude/scripts/setup-claude.py", "claude_setup_test"
)


class ClaudeSetupSafetyTests(unittest.TestCase):
    def test_setup_rejects_symlink_target_without_receipt_mutation(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            plugin = root / "plugin"
            (plugin / "templates").mkdir(parents=True)
            (plugin / "templates/AGENTS.md").write_text("# core\n", encoding="ascii")
            config = root / "claude"
            config.mkdir()
            referent = root / "outside.md"
            referent.write_text("# original\n", encoding="ascii")
            (config / "CLAUDE.md").symlink_to(referent)

            with self.assertRaisesRegex(claude_setup.SetupError, "symbolic-link"):
                claude_setup.preview(config, plugin)
            with self.assertRaisesRegex(claude_setup.SetupError, "symbolic-link"):
                claude_setup.install(config, plugin, "any-fingerprint")

            self.assertTrue((config / "CLAUDE.md").is_symlink())
            self.assertEqual("# original\n", referent.read_text(encoding="ascii"))
            self.assertFalse(claude_setup.config_root(config).exists())
            self.assertFalse(claude_setup.pointer(config).exists())


if __name__ == "__main__":
    unittest.main()
