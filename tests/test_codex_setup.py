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


setup = script("adapters/codex/scripts/setup-codex.py")


class FakeCodex:
    def __init__(self, catalog: dict, doctor_ok: bool = True):
        self.catalog = catalog
        self.doctor_ok = doctor_ok
        self.installed = True
        self.marketplace = True

    def result(self, args, value, code=0):
        return subprocess.CompletedProcess(args, code, json.dumps(value).encode(), b"")

    def __call__(self, args: list[str], environment: dict[str, str]):
        if args[-2:] == ["debug", "models"]:
            config = Path(environment["CODEX_HOME"]) / "config.toml"
            if config.is_file():
                document = tomllib.loads(config.read_text(encoding="ascii"))
                if "model_catalog_json" in document:
                    return subprocess.CompletedProcess(
                        args, 0, Path(document["model_catalog_json"]).read_bytes(), b""
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
                    "pluginId": "humans-md@humans-md",
                    "version": "0.1.4",
                    "installed": True,
                    "enabled": True,
                }
            ] if self.installed else []
            return self.result(args, {"installed": values})
        if args[1:3] == ["plugin", "remove"]:
            self.installed = False
            return self.result(args, {})
        if "doctor" in args:
            output = b"Configuration\n  [ok] config loaded\n" if self.doctor_ok else b"failed\n"
            return subprocess.CompletedProcess(args, 0 if self.doctor_ok else 2, output, b"")
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
                    for model in json.loads((home / "models-humans-md-v1.json").read_bytes())["models"]
                }
                self.assertIsNone(selected["gpt-5.6-sol"]["multi_agent_version"])
                self.assertNotEqual("upstream", selected["gpt-5.3-codex-spark"]["base_instructions"])
                self.assertFalse(legacy.exists())

                config = home / "config.toml"
                config.write_bytes(config.read_bytes().replace(b"pragmatic", b"friendly"))
                receipt_path, receipt = setup.receipt(home, None)
                setup.uninstall(home, "codex", receipt_path, receipt)
                self.assertEqual(original.replace(b"pragmatic", b"friendly"), config.read_bytes())
                self.assertTrue(legacy.is_dir())
                self.assertFalse(setup.pointer(home).exists())
                self.assertFalse(fake.installed)
                self.assertFalse(fake.marketplace)
                self.assertEqual("installed", result["status"])

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

    def test_checked_decodes_utf8_strictly(self):
        result = subprocess.CompletedProcess(["codex"], 0, "caf\u00e9".encode(), b"")
        with mock.patch.object(setup, "command", return_value=result):
            self.assertEqual("caf\u00e9", setup.checked(["codex"], {}))
        malformed = subprocess.CompletedProcess(["codex"], 0, b"\xff", b"")
        with mock.patch.object(setup, "command", return_value=malformed):
            with self.assertRaisesRegex(setup.SetupError, "invalid UTF-8"):
                setup.checked(["codex"], {})

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

    def test_legacy_v2_receipt_and_git_alert(self):
        with tempfile.TemporaryDirectory() as temporary:
            plugin, home, _, catalog, _ = self.fixture(Path(temporary))
            fake = FakeCodex(catalog)
            with self.fake_command(fake):
                result = setup.install(setup.prepare(plugin, home, "codex"))
                receipt_path = Path(result["receipt"])
                receipt = json.loads(receipt_path.read_bytes())
                receipt["schema_version"] = 2
                receipt["after"] = {"obsolete": "digest"}
                receipt["config_blocks"] = {"scalars": "obsolete", "tables": "obsolete"}
                receipt.pop("installed")
                receipt_path.write_bytes(setup.canonical(receipt))

                contract = home / "AGENTS.md"
                contract.write_bytes(contract.read_bytes() + b"# local change\n")
                default_path, default = setup.receipt(home, None)
                explicit_path, explicit = setup.receipt(home, receipt_path)
                self.assertEqual(default_path, explicit_path)
                self.assertEqual(default, explicit)
                output = io.StringIO()
                with contextlib.redirect_stdout(output):
                    setup.show_uninstall_diffs(home, default_path, default)
                self.assertIn("diff --git", output.getvalue())
                self.assertIn("-# >>> humans-md setup scalars >>>", output.getvalue())
                setup.uninstall(home, "codex", default_path, default)

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
                contract = home / "AGENTS.md"
                installed = contract.read_bytes()
                original_snapshot = setup.snapshot

                def snapshot(snapshot_home, paths, destination):
                    entries = original_snapshot(snapshot_home, paths, destination)
                    if destination.parent.name.startswith("uninstall-"):
                        contract.write_bytes(b"changed after snapshot\n")
                    return entries

                with mock.patch.object(setup, "snapshot", snapshot):
                    with self.assertRaisesRegex(setup.SetupError, "changed after uninstall snapshot"):
                        setup.uninstall(home, "codex", receipt_path, receipt)
                self.assertEqual(installed, contract.read_bytes())


if __name__ == "__main__":
    unittest.main()
