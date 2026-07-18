from __future__ import annotations

import contextlib
import io
import json
import shutil
import subprocess
import tempfile
import tomllib
import unittest
from pathlib import Path
from unittest import mock

from _load import ROOT, script


setup = script("casefile/adapters/codex/scripts/setup-codex.py")


class FakeCodex:
    def __init__(self, catalog: dict, doctor_ok: bool = True):
        self.catalog = catalog
        self.doctor_ok = doctor_ok
        self.installed = True
        self.marketplace = True

    def result(self, args, value, code=0):
        return subprocess.CompletedProcess(args, code, json.dumps(value), "")

    def __call__(self, args: list[str], environment: dict[str, str]):
        if args[1:3] == ["debug", "models"]:
            if "-c" in args:
                override = args[args.index("-c") + 1]
                key, value = override.split("=", 1)
                if key != "model_catalog_json":
                    raise AssertionError(args)
                return subprocess.CompletedProcess(
                    args, 0, Path(json.loads(value)).read_text(encoding="utf-8"), ""
                )
            config = Path(environment["CODEX_HOME"]) / "config.toml"
            if config.is_file():
                document = tomllib.loads(config.read_text(encoding="ascii"))
                if "model_catalog_json" in document:
                    return subprocess.CompletedProcess(
                        args, 0, Path(document["model_catalog_json"]).read_text(), ""
                    )
            return self.result(args, self.catalog)
        if args[1:4] == ["plugin", "marketplace", "list"]:
            return self.result(args, {"marketplaces": [{"name": "humans-md"}]})
        if args[1:4] == ["plugin", "marketplace", "remove"]:
            self.marketplace = False
            return self.result(args, {})
        if args[1:3] == ["plugin", "list"]:
            values = [
                {
                    "pluginId": "casefile@humans-md",
                    "version": "0.2.0",
                    "installed": True,
                    "enabled": True,
                }
            ] if self.installed else []
            return self.result(args, {"installed": values})
        if args[1:3] == ["plugin", "remove"]:
            self.installed = False
            return self.result(args, {})
        if "doctor" in args:
            output = "Configuration\n  [ok] config loaded\n" if self.doctor_ok else "failed\n"
            return subprocess.CompletedProcess(args, 0 if self.doctor_ok else 2, output, "")
        raise AssertionError(args)


class CodexSetupTests(unittest.TestCase):
    def fixture(self, root: Path):
        plugin = root / "plugin"
        (plugin / ".codex-plugin").mkdir(parents=True)
        (plugin / ".codex-plugin/plugin.json").write_text(
            '{"name":"casefile","version":"0.2.0"}\n', encoding="ascii"
        )
        (plugin / "config").mkdir()
        for name in ("config-fragment.toml.in", "profiles.toml"):
            shutil.copy2(ROOT / "casefile/adapters/codex" / name, plugin / "config" / name)
        shutil.copytree(ROOT / "casefile/adapters/codex/catalog", plugin / "config/catalog")
        shutil.copytree(ROOT / "casefile/adapters/codex/agents", plugin / "agents")
        (plugin / "templates").mkdir()
        shutil.copy2(ROOT / "AGENTS.md", plugin / "templates/AGENTS.md")

        profiles = tomllib.loads((plugin / "config/profiles.toml").read_text(encoding="ascii"))
        catalog = {
            "models": [
                {
                    "slug": target["id"],
                    "display_name": target["expected"]["display_name"],
                    "base_instructions": "upstream",
                    "model_messages": {"instructions_template": "upstream"},
                    "multi_agent_version": "v2",
                    "supported_reasoning_levels": [
                        {"effort": effort} for effort in target["required_reasoning"]
                    ],
                }
                for target in profiles["catalog"]["targets"]
            ]
        }
        home = root / "codex-home"
        home.mkdir()
        original = b'model = "gpt-5.5"\npersonality = "pragmatic"\n'
        (home / "config.toml").write_bytes(original)
        legacy = home / "skills/investigation-solo"
        legacy.mkdir(parents=True)
        (legacy / "SKILL.md").write_text("legacy\n", encoding="ascii")
        return plugin, home, original, catalog, legacy

    @contextlib.contextmanager
    def fake_command(self, fake):
        previous = setup.command
        setup.command = fake
        try:
            yield
        finally:
            setup.command = previous

    def test_active_models_install_and_uninstall_preserve_unowned_config(self):
        with tempfile.TemporaryDirectory() as temporary:
            plugin, home, original, catalog, legacy = self.fixture(Path(temporary))
            fake = FakeCodex(catalog)
            with self.fake_command(fake):
                plan = setup.prepare(plugin, home, "codex")
                self.assertIn("gpt-5.3-codex-spark", plan["patched"])
                result = setup.install(plan)

                selected = {
                    model["slug"]: model
                    for model in json.loads((home / "models-casefile-v1.json").read_bytes())["models"]
                }
                self.assertIsNone(selected["gpt-5.6-sol"]["multi_agent_version"])
                self.assertNotEqual("upstream", selected["gpt-5.3-codex-spark"]["base_instructions"])
                self.assertTrue(legacy.exists())

                config = home / "config.toml"
                config.write_bytes(config.read_bytes().replace(b"pragmatic", b"friendly"))
                receipt_path, receipt = setup.receipt(home, None)
                setup.uninstall(home, "codex", receipt_path, receipt)
                self.assertEqual(original.replace(b"pragmatic", b"friendly"), config.read_bytes())
                self.assertTrue(legacy.is_dir())
                self.assertFalse(setup.pointer(home).exists())
                self.assertFalse(fake.installed)
                self.assertTrue(fake.marketplace)
                self.assertEqual("installed", result["status"])

    def test_model_cache_includes_active_optional_models_missing_from_fallback(self):
        with tempfile.TemporaryDirectory() as temporary:
            plugin, home, _, catalog, _ = self.fixture(Path(temporary))
            fallback = {
                "models": [
                    model
                    for model in catalog["models"]
                    if model["slug"] != "gpt-5.3-codex-spark"
                ]
            }
            (home / "models_cache.json").write_bytes(setup.canonical(catalog))
            with self.fake_command(FakeCodex(fallback)):
                plan = setup.prepare(plugin, home, "codex")
            self.assertIn("gpt-5.3-codex-spark", plan["patched"])
            self.assertNotIn("gpt-5.3-codex-spark", plan["skipped"])

    def test_portable_bytes_write_and_resource_separator(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            target = root / "target"
            with mock.patch.object(setup.os, "fchmod", None):
                setup.atomic_write(target, b"portable")
            self.assertEqual(b"portable", target.read_bytes())

            plugin, home, _, catalog, _ = self.fixture(root / "fixture")
            profile = plugin / "config/profiles.toml"
            profile.write_text(
                profile.read_text(encoding="ascii").replace(
                    "catalog/gpt-5.6-sol/base-instructions.md",
                    r"catalog\\gpt-5.6-sol\\base-instructions.md",
                ),
                encoding="ascii",
            )
            with self.fake_command(FakeCodex(catalog)):
                self.assertIn("gpt-5.6-sol", setup.prepare(plugin, home, "codex")["patched"])

    def test_checked_uses_text_command_output(self):
        result = subprocess.CompletedProcess(["codex"], 0, "caf\u00e9", "")
        with mock.patch.object(setup, "command", return_value=result):
            self.assertEqual("caf\u00e9", setup.checked(["codex"], {}))

    def test_fdopen_failure_and_rollback_restore_original_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            target = root / "target"
            target.write_bytes(b"old")
            error = OSError("fdopen failed")
            with mock.patch.object(setup.os, "fdopen", side_effect=error):
                with self.assertRaises(OSError) as raised:
                    setup.atomic_write(target, b"new")
            self.assertIs(error, raised.exception)
            self.assertEqual(b"old", target.read_bytes())
            self.assertEqual([], list(root.glob(".target.*")))

            plugin, home, original, catalog, legacy = self.fixture(root / "rollback")
            with self.fake_command(FakeCodex(catalog, doctor_ok=False)):
                with self.assertRaisesRegex(setup.SetupError, "rollback verified"):
                    setup.install(setup.prepare(plugin, home, "codex"))
            self.assertEqual(original, (home / "config.toml").read_bytes())
            self.assertTrue(legacy.is_dir())

    def test_current_receipt_and_git_alert(self):
        with tempfile.TemporaryDirectory() as temporary:
            plugin, home, _, catalog, _ = self.fixture(Path(temporary))
            fake = FakeCodex(catalog)
            with self.fake_command(fake):
                result = setup.install(setup.prepare(plugin, home, "codex"))
                receipt_path, receipt = setup.receipt(home, Path(result["receipt"]))
                config = home / "config.toml"
                config.write_bytes(config.read_bytes() + b'local = "pragmatic"\n')
                output = io.StringIO()
                with contextlib.redirect_stdout(output):
                    setup.show_uninstall_diffs(home, receipt_path, receipt)
                self.assertIn("diff --git", output.getvalue())
                self.assertIn("# >>> casefile setup scalars >>>", output.getvalue())
                setup.uninstall(home, "codex", receipt_path, receipt)
                invalid = json.loads(receipt_path.read_bytes())
                invalid["before"][0]["path"] = r"C:\config.toml"
                receipt_path.write_bytes(setup.canonical(invalid))
                with self.assertRaisesRegex(setup.SetupError, "unsafe receipt path"):
                    setup.receipt(home, receipt_path)

    def test_uninstall_aborts_when_a_managed_file_changes_after_snapshot(self):
        with tempfile.TemporaryDirectory() as temporary:
            plugin, home, _, catalog, _ = self.fixture(Path(temporary))
            fake = FakeCodex(catalog)
            with self.fake_command(fake):
                result = setup.install(setup.prepare(plugin, home, "codex"))
                receipt_path, receipt = setup.receipt(home, Path(result["receipt"]))
                contract = home / "config.toml"
                installed = contract.read_bytes()
                original_snapshot = setup.snapshot

                def snapshot(snapshot_home, paths, destination):
                    entries = original_snapshot(snapshot_home, paths, destination)
                    if destination.parent.name.startswith("uninstall-"):
                        contract.write_bytes(contract.read_bytes() + b"\nchanged_after_snapshot = true\n")
                    return entries

                with mock.patch.object(setup, "snapshot", snapshot):
                    with self.assertRaisesRegex(setup.SetupError, "changed after uninstall snapshot"):
                        setup.uninstall(home, "codex", receipt_path, receipt)
                self.assertEqual(installed, contract.read_bytes())


if __name__ == "__main__":
    unittest.main()
