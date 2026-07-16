from __future__ import annotations

import json
import shutil
import subprocess
import tempfile
import tomllib
import unittest
from pathlib import Path

from _load import ROOT, script


setup = script("adapters/codex/scripts/setup-codex.py")


class FakeCodex:
    def __init__(self, catalog: dict, doctor_ok: bool = True):
        self.catalog = catalog
        self.doctor_ok = doctor_ok
        self.installed = True
        self.marketplace = True

    def __call__(self, args: list[str], environment: dict[str, str]):
        if args[-3:] == ["debug", "models", "--bundled"]:
            return subprocess.CompletedProcess(args, 0, json.dumps(self.catalog), "")
        if args[-2:] == ["debug", "models"]:
            home = Path(environment["CODEX_HOME"])
            config = tomllib.loads((home / "config.toml").read_text(encoding="ascii"))
            path = Path(config["model_catalog_json"])
            return subprocess.CompletedProcess(args, 0, path.read_text(), "")
        if args[1:4] == ["plugin", "marketplace", "list"]:
            values = [{"name": "humans-md"}] if self.marketplace else []
            return subprocess.CompletedProcess(args, 0, json.dumps({"marketplaces": values}), "")
        if args[1:4] == ["plugin", "marketplace", "remove"]:
            self.marketplace = False
            return subprocess.CompletedProcess(args, 0, "{}", "")
        if args[1:3] == ["plugin", "list"]:
            values = (
                [
                    {
                        "pluginId": "humans-md@humans-md",
                        "version": "0.1.4",
                        "installed": True,
                        "enabled": True,
                    }
                ]
                if self.installed
                else []
            )
            return subprocess.CompletedProcess(args, 0, json.dumps({"installed": values}), "")
        if args[1:3] == ["plugin", "remove"]:
            self.installed = False
            return subprocess.CompletedProcess(args, 0, "{}", "")
        if "doctor" in args:
            output = "Configuration\n  [ok] config loaded\n" if self.doctor_ok else "failed\n"
            return subprocess.CompletedProcess(args, 2, output, "")
        raise AssertionError(args)


class CodexSetupTests(unittest.TestCase):
    def fixture(self, root: Path):
        plugin = root / "plugin"
        (plugin / ".codex-plugin").mkdir(parents=True)
        (plugin / ".codex-plugin/plugin.json").write_text(
            '{"name":"humans-md","version":"0.1.4"}\n', encoding="ascii"
        )
        (plugin / "config").mkdir()
        for name in ("config-fragment.toml.in", "profiles.toml"):
            shutil.copy2(ROOT / "adapters/codex" / name, plugin / "config" / name)
        shutil.copytree(ROOT / "adapters/codex/catalog", plugin / "config/catalog")
        shutil.copytree(ROOT / "adapters/codex/agents", plugin / "agents")
        (plugin / "templates").mkdir()
        shutil.copy2(ROOT / "AGENTS.md", plugin / "templates/AGENTS.md")

        profiles = tomllib.loads((plugin / "config/profiles.toml").read_text(encoding="ascii"))
        models = []
        for target in profiles["catalog"]["targets"]:
            if target["id"] == "gpt-5.3-codex-spark":
                continue
            models.append(
                {
                    "slug": target["id"],
                    "display_name": target["expected"]["display_name"],
                    "base_instructions": "upstream",
                    "model_messages": {"instructions_template": "upstream"},
                    "multi_agent_version": "v2",
                    "supported_reasoning_levels": [
                        {"effort": effort} for effort in target["required_reasoning"]
                    ],
                    "preserved": True,
                }
            )
        catalog = {"models": models, "unrelated": {"preserved": True}}

        home = root / "codex-home"
        home.mkdir()
        original = b'''model = "gpt-5.5"
model_reasoning_effort = "high"
personality = "pragmatic"
[plugins."humans-md@humans-md"]
enabled = true
[marketplaces.humans-md]
source = "fixture"
'''
        (home / "config.toml").write_bytes(original)
        legacy = home / "skills/investigation-solo"
        legacy.mkdir(parents=True)
        (legacy / "SKILL.md").write_text("legacy\n", encoding="ascii")
        unrelated_agent = home / "agents/unrelated.toml"
        unrelated_agent.parent.mkdir()
        unrelated_agent.write_text('model = "unrelated"\n', encoding="ascii")
        return plugin, home, original, catalog, legacy, unrelated_agent

    def test_install_generates_v1_catalog_and_durable_uninstall_receipt(self):
        with tempfile.TemporaryDirectory() as temporary:
            plugin, home, original, catalog, legacy, unrelated_agent = self.fixture(
                Path(temporary)
            )
            fake = FakeCodex(catalog)
            previous = setup.command
            setup.command = fake
            try:
                plan = setup.prepare(plugin, home, "codex")
                self.assertIn("gpt-5.3-codex-spark", plan["skipped"])
                result = setup.install(plan)
                receipt_path = Path(result["receipt"])
                self.assertEqual(home / "backups/humans-md", receipt_path.parents[1])
                self.assertEqual(0o600, receipt_path.stat().st_mode & 0o777)
                self.assertEqual(0o700, receipt_path.parent.stat().st_mode & 0o777)
                config = tomllib.loads((home / "config.toml").read_text(encoding="ascii"))
                self.assertEqual("gpt-5.5", config["model"])
                self.assertEqual("high", config["model_reasoning_effort"])
                self.assertEqual("pragmatic", config["personality"])
                self.assertTrue(config["features"]["multi_agent"])
                self.assertFalse(config["features"]["multi_agent_v2"])
                self.assertEqual(
                    str(home / "models-humans-md-v1.json"),
                    config["model_catalog_json"],
                )
                profiled = json.loads((home / "models-humans-md-v1.json").read_text())
                selected = {item["slug"]: item for item in profiled["models"]}
                self.assertIsNone(selected["gpt-5.6-sol"]["multi_agent_version"])
                self.assertIsNone(selected["gpt-5.6-terra"]["multi_agent_version"])
                self.assertIsNone(selected["gpt-5.6-luna"]["multi_agent_version"])
                self.assertTrue(profiled["unrelated"]["preserved"])
                self.assertFalse(legacy.exists())
                self.assertTrue(unrelated_agent.is_file())

                path, receipt = setup.receipt(home, None)
                active_config = home / "config.toml"
                installed_config = active_config.read_bytes()
                changed_owned_config = installed_config.replace(
                    b"multi_agent_v2 = false", b"multi_agent_v2 = true", 1
                )
                active_config.write_bytes(changed_owned_config)
                with self.assertRaisesRegex(setup.SetupError, "managed config block changed"):
                    setup.uninstall(home, "codex", path, receipt)
                self.assertEqual(changed_owned_config, active_config.read_bytes())

                active_config.write_bytes(
                    installed_config.replace(
                        b'personality = "pragmatic"', b'personality = "friendly"', 1
                    )
                )
                setup.uninstall(home, "codex", path, receipt)
                expected = original.replace(
                    b'personality = "pragmatic"', b'personality = "friendly"', 1
                )
                self.assertEqual(expected, active_config.read_bytes())
                self.assertTrue(legacy.is_dir())
                self.assertTrue(unrelated_agent.is_file())
                self.assertFalse((home / "AGENTS.md").exists())
                self.assertFalse((home / "models-humans-md-v1.json").exists())
                self.assertFalse(setup.pointer(home).exists())
                self.assertFalse(fake.installed)
                self.assertFalse(fake.marketplace)
            finally:
                setup.command = previous

    def test_failed_mechanical_verification_rolls_back(self):
        with tempfile.TemporaryDirectory() as temporary:
            plugin, home, original, catalog, legacy, _ = self.fixture(Path(temporary))
            fake = FakeCodex(catalog, doctor_ok=False)
            previous = setup.command
            setup.command = fake
            try:
                plan = setup.prepare(plugin, home, "codex")
                with self.assertRaisesRegex(setup.SetupError, "rollback verified"):
                    setup.install(plan)
                self.assertEqual(original, (home / "config.toml").read_bytes())
                self.assertTrue(legacy.is_dir())
                self.assertFalse(setup.pointer(home).exists())
            finally:
                setup.command = previous


if __name__ == "__main__":
    unittest.main()
