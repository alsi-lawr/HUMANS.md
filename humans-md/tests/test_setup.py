from __future__ import annotations

import importlib.util
import json
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


CONTRACT_SETTINGS = (
    '{"includeGitInstructions": false, "attribution": {"commit": ""},'
    ' "disableBundledSkills": true}\n'
)


def make_plugin(root: Path) -> Path:
    plugin = root / "plugin"
    (plugin / "templates").mkdir(parents=True)
    (plugin / "templates/AGENTS.md").write_text("# core\n", encoding="ascii")
    (plugin / "templates/settings.json").write_text(CONTRACT_SETTINGS, encoding="ascii")
    return plugin


class ClaudeSetupSafetyTests(unittest.TestCase):
    def test_setup_rejects_symlink_target_without_receipt_mutation(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            plugin = make_plugin(root)
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


class ClaudeSettingsPackagingTests(unittest.TestCase):
    def install(self, root: Path, existing: str | None) -> tuple[Path, dict]:
        plugin = make_plugin(root)
        config = root / "claude"
        config.mkdir()
        if existing is not None:
            (config / "settings.json").write_text(existing, encoding="ascii")
        plan = claude_setup.preview(config, plugin)
        result = claude_setup.install(config, plugin, plan["approval_fingerprint"])
        settings = json.loads((config / "settings.json").read_text(encoding="ascii"))
        receipt = json.loads(Path(result["receipt"]).read_text(encoding="ascii"))
        return config, {"settings": settings, "receipt": receipt}

    def test_install_overwrites_contract_keys_and_preserves_unrelated_state(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            existing = (
                '{"theme": "dark", "includeGitInstructions": true,'
                ' "attribution": {"pr": "keep me"}}\n'
            )
            _, out = self.install(root, existing)

            # Contract wins over the operator's prior value.
            self.assertFalse(out["settings"]["includeGitInstructions"])
            self.assertEqual("", out["settings"]["attribution"]["commit"])
            self.assertTrue(out["settings"]["disableBundledSkills"])
            # Unrelated keys, including siblings under a managed parent, survive.
            self.assertEqual("dark", out["settings"]["theme"])
            self.assertEqual("keep me", out["settings"]["attribution"]["pr"])

    def test_receipt_records_prior_leaf_values_for_restore(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            existing = '{"includeGitInstructions": true}\n'
            _, out = self.install(root, existing)

            before = out["receipt"]["settings_before"]
            self.assertTrue(before["includeGitInstructions"])
            # Absent keys record as null so uninstall removes rather than restores them.
            self.assertIsNone(before["disableBundledSkills"])
            self.assertIsNone(before["attribution.commit"])
            self.assertEqual("settings.json.before", out["receipt"]["settings_file_before"])

    def test_missing_settings_file_is_created_and_marked_in_receipt(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            _, out = self.install(root, None)

            self.assertFalse(out["settings"]["includeGitInstructions"])
            self.assertEqual("missing", out["receipt"]["settings_file_before"])

    def test_approval_goes_stale_when_settings_change_after_preview(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            plugin = make_plugin(root)
            config = root / "claude"
            config.mkdir()
            (config / "settings.json").write_text('{"theme": "dark"}\n', encoding="ascii")
            plan = claude_setup.preview(config, plugin)

            (config / "settings.json").write_text('{"theme": "light"}\n', encoding="ascii")
            with self.assertRaisesRegex(claude_setup.SetupError, "stale approval"):
                claude_setup.install(config, plugin, plan["approval_fingerprint"])

            self.assertFalse(claude_setup.pointer(config).exists())
            self.assertEqual(
                '{"theme": "light"}\n', (config / "settings.json").read_text(encoding="ascii")
            )

    def test_malformed_existing_settings_is_refused_before_mutation(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            plugin = make_plugin(root)
            config = root / "claude"
            config.mkdir()
            (config / "settings.json").write_text("{not json\n", encoding="ascii")

            with self.assertRaisesRegex(claude_setup.SetupError, "not valid JSON"):
                claude_setup.preview(config, plugin)
            self.assertFalse((config / "CLAUDE.md").exists())
            self.assertFalse(claude_setup.pointer(config).exists())


if __name__ == "__main__":
    unittest.main()
