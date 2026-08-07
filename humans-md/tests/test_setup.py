from __future__ import annotations

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock


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
codex_setup = load(
    ROOT / "humans-md/adapters/codex/scripts/setup-codex.py", "codex_setup_overwrite_test"
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

    def test_plain_reinstall_refuses_with_active_receipt(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            plugin = make_plugin(root)
            config = root / "claude"
            config.mkdir()
            claude_setup.install(config, plugin)
            with self.assertRaisesRegex(claude_setup.SetupError, "already exists"):
                claude_setup.install(config, plugin)

    def test_overwrite_reinstall_carries_original_lineage(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            plugin = make_plugin(root)
            config = root / "claude"
            config.mkdir()
            (config / "settings.json").write_text(
                '{"theme": "dark", "includeGitInstructions": true}\n', encoding="ascii"
            )
            claude_setup.install(config, plugin)
            settings = json.loads((config / "settings.json").read_text(encoding="ascii"))
            settings["theme"] = "light"
            (config / "settings.json").write_text(json.dumps(settings) + "\n", encoding="ascii")

            result = claude_setup.install(config, plugin, overwrite=True)
            receipt_path = Path(result["receipt"])
            receipt = json.loads(receipt_path.read_text(encoding="ascii"))
            self.assertEqual("missing", receipt["before"])
            self.assertTrue((receipt_path.parent / "CLAUDE.md.was-missing").is_file())
            self.assertTrue(receipt["settings_before"]["includeGitInstructions"])
            document = json.loads((config / "settings.json").read_text(encoding="utf-8"))
            self.assertEqual("light", document["theme"])
            self.assertFalse(document["includeGitInstructions"])
            pointer = json.loads(claude_setup.pointer(config).read_text(encoding="ascii"))
            self.assertEqual(str(receipt_path), pointer["receipt"])

    def test_overwrite_tolerates_vintage_receipt_without_settings_lineage(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            plugin = make_plugin(root)
            config = root / "claude"
            config.mkdir()
            first = Path(claude_setup.install(config, plugin)["receipt"])
            vintage = json.loads(first.read_text(encoding="ascii"))
            del vintage["settings_before"]
            first.write_bytes(claude_setup.canonical(vintage))

            result = claude_setup.install(config, plugin, overwrite=True)
            receipt = json.loads(Path(result["receipt"]).read_text(encoding="ascii"))
            self.assertIn("includeGitInstructions", receipt["settings_before"])


class CodexSetupOverwriteTests(unittest.TestCase):
    def fake_codex(self, args, environment):
        if args[1:3] == ["plugin", "list"]:
            return {
                "installed": [
                    {
                        "pluginId": codex_setup.PLUGIN_ID,
                        "version": "0.2.0",
                        "installed": True,
                        "enabled": True,
                    }
                ]
            }
        if args[1:4] == ["plugin", "marketplace", "list"]:
            return {"marketplaces": [{"name": codex_setup.MARKETPLACE}]}
        raise AssertionError(args)

    def test_overwrite_reinstall_carries_original_contract_through_uninstall(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            plugin = root / "codex-plugin"
            (plugin / ".codex-plugin").mkdir(parents=True)
            (plugin / ".codex-plugin/plugin.json").write_text(
                json.dumps({"name": "humans-md", "version": "0.2.0"}), encoding="ascii"
            )
            (plugin / "templates").mkdir()
            (plugin / "templates/AGENTS.md").write_text("# core\n", encoding="ascii")
            home = root / "codex-home"
            home.mkdir()
            (home / "AGENTS.md").write_text("# operator\n", encoding="ascii")
            with mock.patch.object(codex_setup, "checked_json", self.fake_codex):
                plan = codex_setup.prepare(plugin, home, "codex")
                codex_setup.install(plan)
                with self.assertRaisesRegex(codex_setup.SetupError, "already exists"):
                    codex_setup.install(plan)
                second = codex_setup.install(plan, overwrite=True)
                receipt_path = Path(second["receipt"])
                receipt = json.loads(receipt_path.read_text(encoding="ascii"))
                self.assertTrue(receipt["before"][0]["existed"])
                self.assertEqual(
                    "# operator\n",
                    (receipt_path.parent / "before/AGENTS.md").read_text(encoding="ascii"),
                )


if __name__ == "__main__":
    unittest.main()
