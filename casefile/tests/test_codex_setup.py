from __future__ import annotations

import contextlib
import io
import json
import shutil
import struct
import subprocess
import sys
import tempfile
import tomllib
import unittest
from pathlib import Path
from unittest import mock

from _load import ROOT, script


setup = script("casefile/adapters/codex/scripts/setup-codex.py")
PLUGIN_VERSION = tomllib.loads(
    (ROOT / "casefile/packaging/plugin.toml").read_text(encoding="ascii")
)["version"]


def native_stub(target: str) -> bytes:
    if target.endswith("linux-musl"):
        data = bytearray(64); data[:4] = b"\x7fELF"; data[4:6] = b"\x02\x01"
        struct.pack_into("<H", data, 18, 183 if target.startswith("aarch64") else 62)
    elif target.endswith("darwin"):
        data = bytearray(64); data[:4] = b"\xcf\xfa\xed\xfe"
        struct.pack_into("<I", data, 4, 0x0100000C if target.startswith("aarch64") else 0x01000007)
    else:
        data = bytearray(128); data[:2] = b"MZ"; struct.pack_into("<I", data, 0x3C, 64)
        data[64:68] = b"PE\0\0"; struct.pack_into("<H", data, 68, 0xAA64 if target.startswith("aarch64") else 0x8664)
    return bytes(data)


class FakeCodex:
    def __init__(self, doctor_ok: bool = True, version: str = "0.145.0"):
        self.doctor_ok = doctor_ok
        self.version = version
        self.installed = True
        self.marketplace = True
        self.calls: list[list[str]] = []

    def result(self, args, value, code=0):
        return subprocess.CompletedProcess(args, code, json.dumps(value), "")

    def __call__(self, args: list[str], environment: dict[str, str]):
        self.calls.append(args)
        if args[1:] == ["--version"]:
            return subprocess.CompletedProcess(args, 0, f"codex-cli {self.version}\n", "")
        if args[1:4] == ["plugin", "marketplace", "list"]:
            return self.result(args, {"marketplaces": [{"name": "humans-md"}]})
        if args[1:4] == ["plugin", "marketplace", "remove"]:
            self.marketplace = False
            return self.result(args, {})
        if args[1:3] == ["plugin", "list"]:
            values = [
                {
                    "pluginId": "casefile@humans-md",
                    "version": PLUGIN_VERSION,
                    "installed": True,
                    "enabled": True,
                }
            ] if self.installed else []
            return self.result(args, {"installed": values})
        if args[1:3] == ["plugin", "remove"]:
            self.installed = False
            return self.result(args, {})
        if "doctor" in args:
            check = {
                "status": "warning" if self.doctor_ok else "error",
                "summary": "config loaded" if self.doctor_ok else "failed",
                "details": {"config.toml parse": "ok"} if self.doctor_ok else {},
            }
            return self.result(args, {"checks": {"config.load": check}}, 1 if self.doctor_ok else 2)
        raise AssertionError(args)


class CodexSetupTests(unittest.TestCase):
    def fixture(self, root: Path):
        plugin = root / "plugin"
        (plugin / ".codex-plugin").mkdir(parents=True)
        (plugin / ".codex-plugin/plugin.json").write_text(
            json.dumps({"name": "casefile", "version": PLUGIN_VERSION}) + "\n",
            encoding="ascii",
        )
        (plugin / "casefile.toml").write_text("schema_version = 1\n", encoding="ascii")
        (plugin / "projects.toml").write_text("schema_version = 1\nprojects = []\n", encoding="ascii")
        (plugin / "config").mkdir()
        for name in ("config-fragment.toml.in", "profiles.toml"):
            shutil.copy2(ROOT / "casefile/adapters/codex" / name, plugin / "config" / name)
        shutil.copytree(ROOT / "casefile/adapters/codex/agents", plugin / "agents")
        shutil.copytree(ROOT / "casefile/adapters/codex/matrices", plugin / "matrices")
        (plugin / "scripts").mkdir()
        shutil.copy2(ROOT / "casefile/adapters/shared/casefile_runtime.py", plugin / "scripts/casefile_runtime.py")
        for name in ("list-codex-models.py", "offer-role-models.py", "resolve-role-assignment.py"):
            shutil.copy2(ROOT / "casefile/adapters/codex/scripts" / name, plugin / "scripts" / name)
        rows = []
        for target in (
            "aarch64-apple-darwin", "aarch64-pc-windows-msvc", "aarch64-unknown-linux-musl",
            "x86_64-apple-darwin", "x86_64-pc-windows-msvc", "x86_64-unknown-linux-musl",
        ):
            name = "casefile.exe" if target.endswith("windows-msvc") else "casefile"
            binary = plugin / "runtime/bin" / target / name
            binary.parent.mkdir(parents=True, exist_ok=True)
            runtime_source = native_stub(target)
            binary.write_bytes(runtime_source)
            binary.chmod(0o755)
            rows.append({"path": binary.relative_to(plugin / "runtime").as_posix().replace("/", "\\") + "///", "sha256": "representation-only", "size": -1, "target": target})
        (plugin / "runtime/artifacts.json").write_bytes((json.dumps({"artifacts": rows, "source_commit": "1" * 40, "version": PLUGIN_VERSION, "schema_version": 1}, separators=(", ", ": ")) + "\r\n").encode("ascii"))
        (plugin / "templates").mkdir()
        shutil.copy2(ROOT / "AGENTS.md", plugin / "templates/AGENTS.md")

        home = root / "codex-home"
        home.mkdir()
        original = (
            b'model = "gpt-5.5"\npersonality = "pragmatic"\n'
            b'\n[mcp_servers.unrelated]\ncommand = "/unrelated/server"\n'
            b'\n[features]\ndefault_mode_request_user_input = true\n'
            b'\n[agents]\nmax_threads = 12\n'
        )
        (home / "config.toml").write_bytes(original)
        legacy = home / "skills/investigation-solo"
        legacy.mkdir(parents=True)
        (legacy / "SKILL.md").write_text("legacy\n", encoding="ascii")
        return plugin, home, original, legacy

    @contextlib.contextmanager
    def fake_command(self, fake):
        previous = setup.command
        previous_probe = setup.casefile_runtime.probe
        setup.command = fake
        setup.casefile_runtime.probe = lambda *_: None
        try:
            yield
        finally:
            setup.command = previous
            setup.casefile_runtime.probe = previous_probe

    def legacy_install(self, plugin, home, *, version, catalogs, schema=6):
        config = home / "config.toml"
        receipt_dir = home / "backups/casefile/legacy-install"
        paths = [home / name for name in catalogs]
        binary = home / "casefile/runtime/0.9.0/local/casefile"
        before = setup.snapshot(home, [config, *paths, binary] if schema == 6 else [config, *paths], receipt_dir / "before")
        binary.parent.mkdir(parents=True)
        binary.write_bytes(b"old runtime")
        configuration = config.read_bytes()
        configuration, _ = setup.detach_catalog_setting(configuration, tomllib.loads(configuration.decode()))
        candidate = setup.config_candidate(configuration, plugin, version, binary, home)
        selected = home / f"models-casefile-{version}.json"
        setting = f"model_catalog_json = {json.dumps(str(selected))}\n".encode()
        config.write_bytes(setup.marked(setup.SCALAR_BEGIN, setting, setup.SCALAR_END) + candidate)
        for path in paths:
            path.write_bytes(catalogs[path.name])
        receipt = {
            "schema_version": schema,
            "status": "installed",
            "install_id": receipt_dir.name,
            "plugin_version": "0.9.0",
            "before": before,
            "remove_plugin": True,
            "remove_marketplace": False,
        }
        if schema >= 5:
            receipt["multi_agent_version"] = version
        if schema == 6:
            receipt.update({
                "casefile_binary": binary.relative_to(home).as_posix(),
                "owned_binaries": [binary.relative_to(home).as_posix()],
                "planning_root": str(home),
            })
        receipt_path = receipt_dir / "receipt.json"
        receipt_path.write_bytes(setup.canonical(receipt))
        pointer = setup.pointer(home)
        pointer.parent.mkdir(parents=True)
        pointer.write_bytes(setup.canonical({"receipt": str(receipt_path)}))
        return receipt_path, receipt

    def test_fresh_install_and_uninstall_preserve_unowned_config(self):
        with tempfile.TemporaryDirectory() as temporary:
            plugin, home, original, legacy = self.fixture(Path(temporary))
            fake = FakeCodex()
            cache = home / "models_cache.json"
            cache.write_bytes(b"native Codex cache\n")
            with self.fake_command(fake):
                plan = setup.prepare(plugin, home, "codex")
                result = setup.install(plan)
                config = home / "config.toml"
                self.assertNotIn("model_catalog_json", tomllib.loads(config.read_text()))
                self.assertFalse(list(home.glob("models-casefile-*.json")))
                config.write_bytes(config.read_bytes().replace(b"pragmatic", b"friendly"))
                marketplace = (
                    b"\n[marketplaces.humans-md]\n"
                    b'source = "https://github.com/alsi-lawr/humans-md-marketplace.git"\n'
                    b'ref = "v0.4.0"\n'
                )
                config.write_bytes(
                    config.read_bytes().replace(setup.TABLE_END, marketplace + setup.TABLE_END)
                )
                receipt_path, receipt = setup.receipt(home, None)
                setup.uninstall(home, "codex", receipt_path, receipt)
                self.assertEqual(
                    original.replace(b"pragmatic", b"friendly")
                    + marketplace.lstrip(b"\n"),
                    config.read_bytes(),
                )
                self.assertEqual(
                    "v0.4.0",
                    tomllib.loads(config.read_text(encoding="ascii"))["marketplaces"]["humans-md"][
                        "ref"
                    ],
                )
                self.assertTrue(legacy.is_dir())
                self.assertFalse(setup.pointer(home).exists())
                self.assertFalse(fake.installed)
                self.assertTrue(fake.marketplace)
                self.assertEqual("installed", result["status"])
                self.assertEqual(b"native Codex cache\n", cache.read_bytes())
                self.assertFalse(any("debug" in call for call in fake.calls))

    def test_fresh_v1_and_v2_setup_never_claim_existing_catalogs(self):
        for version in ("v1", "v2"):
            with self.subTest(version=version), tempfile.TemporaryDirectory() as temporary:
                plugin, home, original, _ = self.fixture(Path(temporary))
                config = home / "config.toml"
                original = b'model_catalog_json = "models-casefile-v1.json"\n' + original
                config.write_bytes(original)
                unowned = {
                    home / f"models-casefile-{variant}.json": variant.encode()
                    for variant in ("v1", "v2")
                }
                for path, data in unowned.items():
                    path.write_bytes(data)
                fake = FakeCodex()
                with self.fake_command(fake):
                    plan = setup.prepare(plugin, home, "codex", version=version)
                    self.assertEqual([], setup.preview(plan)["legacy_catalog_recovery"])
                    result = setup.install(plan)
                    receipt_path, receipt = setup.receipt(home, Path(result["receipt"]))
                    self.assertEqual(version, receipt["multi_agent_version"])
                    self.assertFalse(setup.LEGACY_CATALOGS & {entry["path"] for entry in receipt["before"]})
                    document = tomllib.loads(config.read_text())
                    self.assertEqual("models-casefile-v1.json", document["model_catalog_json"])
                    self.assertEqual(version == "v1", document["features"]["multi_agent"])
                    self.assertEqual(version == "v2", document["features"]["multi_agent_v2"])
                    self.assertEqual(2, document["agents"]["max_depth"])
                    self.assertEqual(12, document["agents"]["max_threads"])
                    setup.uninstall(home, "codex", receipt_path, receipt)
                self.assertEqual(original, config.read_bytes())
                self.assertEqual(unowned, {path: path.read_bytes() for path in unowned})

    def test_verification_rejects_mixed_feature_flags(self):
        with tempfile.TemporaryDirectory() as temporary:
            plugin, home, _, _ = self.fixture(Path(temporary))
            with self.fake_command(FakeCodex()):
                plan = setup.prepare(plugin, home, "codex", version="v2")
            mixed = plan["config"].replace(b"multi_agent_v2 = true", b"multi_agent_v2 = false")
            with self.assertRaisesRegex(setup.SetupError, "V2 feature flags"):
                setup.verify_config(mixed, plugin, "v2")

    def test_legacy_v1_uninstall_restores_displaced_setting_and_file(self):
        with tempfile.TemporaryDirectory() as temporary:
            plugin, home, original, _ = self.fixture(Path(temporary))
            config = home / "config.toml"
            original = b'model_catalog_json = "foreign.json" # original host setting\n' + original
            config.write_bytes(original)
            v1 = home / "models-casefile-v1.json"
            v1.write_bytes(b"")  # An empty original is still recovery data.
            v2 = home / "models-casefile-v2.json"
            v2.write_bytes(b"untracked v2")
            receipt_path, receipt = self.legacy_install(
                plugin, home, version="v1", catalogs={v1.name: b"legacy v1"}, schema=4,
            )
            config.write_bytes(config.read_bytes().replace(b"pragmatic", b"friendly"))
            before = config.read_bytes()
            self.assertEqual("v1", setup.receipt_multi_agent_version(receipt))
            with self.fake_command(FakeCodex()):
                setup.uninstall(home, "codex", receipt_path, receipt)
            self.assertEqual(original.replace(b"pragmatic", b"friendly"), config.read_bytes())
            self.assertEqual(b"", v1.read_bytes())
            self.assertEqual(b"untracked v2", v2.read_bytes())
            backups = list((home / "backups/casefile").glob("uninstall-*"))
            self.assertEqual(before, (backups[0] / "before/config.toml").read_bytes())
            self.assertEqual(b"legacy v1", (backups[0] / "before" / v1.name).read_bytes())

    def test_v2_upgrade_preserves_host_removed_or_replaced_override(self):
        for override in (None, "replacement.json"):
            with self.subTest(override=override), tempfile.TemporaryDirectory() as temporary:
                plugin, home, original, _ = self.fixture(Path(temporary))
                config = home / "config.toml"
                config.write_bytes(b'model_catalog_json = "displaced.json"\n' + original)
                v1 = home / "models-casefile-v1.json"
                v1.write_bytes(b"unproven v1")
                v2 = home / "models-casefile-v2.json"
                _, old = self.legacy_install(
                    plugin, home, version="v2", catalogs={v2.name: b"later regular bytes"},
                )
                self.assertNotIn(v1.name, {entry["path"] for entry in old["before"]})
                current, _ = setup.detach_catalog_setting(
                    config.read_bytes(), tomllib.loads(config.read_text()),
                )
                setting = f"model_catalog_json = {json.dumps(override)}\n".encode() if override else b""
                config.write_bytes(setting + current)
                cache = home / "models_cache.json"
                cache.write_bytes(b"native cache")
                with self.fake_command(FakeCodex()):
                    plan = setup.prepare(plugin, home, "codex")
                    self.assertEqual("v2", plan["multi_agent_version"])
                    self.assertEqual(
                        [{"path": str(v2), "action": "remove"}],
                        setup.preview(plan)["legacy_catalog_recovery"],
                    )
                    upgrade = setup.install(plan)
                    self.assertEqual(override, tomllib.loads(config.read_text()).get("model_catalog_json"))
                    self.assertFalse(v2.exists())
                    self.assertEqual(b"later regular bytes", (Path(upgrade["receipt"]).parent / "rollback" / v2.name).read_bytes())
                    repeat = setup.prepare(plugin, home, "codex")
                    self.assertEqual([], setup.preview(repeat)["legacy_catalog_recovery"])
                    setup.install(repeat)
                    receipt_path, receipt = setup.receipt(home, None)
                    self.assertFalse(setup.LEGACY_CATALOGS & {entry["path"] for entry in receipt["before"]})
                    setup.uninstall(home, "codex", receipt_path, receipt)
                self.assertEqual(setting + original, config.read_bytes())
                self.assertEqual(b"unproven v1", v1.read_bytes())
                self.assertFalse(v2.exists())
                self.assertEqual(b"native cache", cache.read_bytes())

    def test_mixed_variant_upgrade_recovers_earlier_state_or_preserves_live_selection(self):
        for host_selected in (False, True):
            with self.subTest(host_selected=host_selected), tempfile.TemporaryDirectory() as temporary:
                plugin, home, original, _ = self.fixture(Path(temporary))
                config = home / "config.toml"
                config.write_bytes(b'model_catalog_json = "earlier.json" # host original\n' + original)
                v1, v2 = (home / f"models-casefile-{variant}.json" for variant in ("v1", "v2"))
                v1.write_bytes(b"original v1")
                self.legacy_install(
                    plugin, home, version="v2", catalogs={v1.name: b"legacy v1", v2.name: b"legacy v2"},
                )
                if host_selected:
                    config.write_bytes(config.read_bytes().replace(
                        json.dumps(str(v2)).encode(), json.dumps(str(v1)).encode(),
                    ))
                with self.fake_command(FakeCodex()):
                    plan = setup.prepare(plugin, home, "codex", version="v1")
                    expected = str(v1) if host_selected else "earlier.json"
                    self.assertEqual(expected, tomllib.loads(plan["config"].decode()).get("model_catalog_json"))
                    setup.install(plan)
                    self.assertEqual(expected, tomllib.loads(config.read_text()).get("model_catalog_json"))
                    self.assertEqual(b"legacy v1" if host_selected else b"original v1", v1.read_bytes())
                    self.assertFalse(v2.exists())
                    receipt_path, receipt = setup.receipt(home, None)
                    setup.uninstall(home, "codex", receipt_path, receipt)
                self.assertEqual(expected, tomllib.loads(config.read_text()).get("model_catalog_json"))
                self.assertEqual(b"legacy v1" if host_selected else b"original v1", v1.read_bytes())

    def test_legacy_catalog_recovery_rolls_back_on_failure_and_retries(self):
        for operation in ("install", "uninstall"):
            with self.subTest(operation=operation), tempfile.TemporaryDirectory() as temporary:
                plugin, home, original, _ = self.fixture(Path(temporary))
                config = home / "config.toml"
                original = b'model_catalog_json = "foreign.json"\n' + original
                config.write_bytes(original)
                v1, v2 = (home / f"models-casefile-{variant}.json" for variant in ("v1", "v2"))
                v1.write_bytes(b"original v1")
                receipt_path, receipt = self.legacy_install(
                    plugin, home, version="v2", catalogs={v1.name: b"managed v1", v2.name: b"managed v2"},
                )
                plugin_cache = home / "plugins/cache/casefile"
                plugin_cache.mkdir(parents=True)
                (plugin_cache / "plugin.json").write_bytes(b"retained plugin cache")
                pointer = setup.pointer(home)
                tracked = (config, v1, v2, pointer, receipt_path)
                before = {path: path.read_bytes() for path in tracked}
                fake = FakeCodex()
                with self.fake_command(fake):
                    if operation == "install":
                        plan = setup.prepare(plugin, home, "codex", version="v2")
                        fake.doctor_ok = False
                        with self.assertRaisesRegex(setup.SetupError, "rollback verified"):
                            setup.install(plan)
                        fake.doctor_ok = True
                        self.assertEqual(before, {path: path.read_bytes() for path in tracked})
                        setup.install(plan)
                        receipt_path, receipt = setup.receipt(home, None)
                    else:
                        with mock.patch.object(setup, "checked", side_effect=setup.SetupError("plugin removal failed")):
                            with self.assertRaisesRegex(setup.SetupError, "rollback verified"):
                                setup.uninstall(home, "codex", receipt_path, receipt)
                        self.assertEqual(before, {path: path.read_bytes() for path in tracked})
                    setup.uninstall(home, "codex", receipt_path, receipt)
                self.assertEqual(original, config.read_bytes())
                self.assertEqual(b"original v1", v1.read_bytes())
                self.assertFalse(v2.exists())
                self.assertFalse(pointer.exists())

    def test_uninstall_rejects_a_stale_receipt_without_tearing_down_current_install(self):
        with tempfile.TemporaryDirectory() as temporary:
            plugin, home, _, _ = self.fixture(Path(temporary))
            catalog_path = home / "models-casefile-v1.json"
            old_path, old = self.legacy_install(
                plugin, home, version="v1", catalogs={catalog_path.name: b"owned"},
            )
            fake = FakeCodex()
            with self.fake_command(fake):
                setup.install(setup.prepare(plugin, home, "codex"))
                self.assertFalse(catalog_path.exists())
                catalog_path.write_bytes(b"new host file")
                active_path, active = setup.receipt(home, None)
                paths = [catalog_path, home / "config.toml", home / active["casefile_binary"], setup.pointer(home), active_path]
                before = {path: path.read_bytes() for path in paths}
                with self.assertRaisesRegex(setup.SetupError, "uninstall receipt is stale"):
                    setup.receipt(home, old_path)
                with self.assertRaisesRegex(setup.SetupError, "uninstall receipt is stale"):
                    setup.uninstall(home, "codex", old_path, old)
                self.assertTrue(fake.installed)
                self.assertEqual(before, {path: path.read_bytes() for path in paths})

    def test_foreign_catalog_symlink_preserves_setting_and_managed_target(self):
        with tempfile.TemporaryDirectory() as temporary:
            plugin, home, source, _ = self.fixture(Path(temporary))
            config = home / "config.toml"
            config.write_bytes(b'model_catalog_json = "earlier.json"\n' + source)
            target = home / "models-casefile-v2.json"
            target.write_bytes(b"displaced original bytes")
            self.legacy_install(plugin, home, version="v2", catalogs={target.name: b"live target"})
            alias = home / "user-selected.json"
            try:
                alias.symlink_to(target.name)
            except OSError:
                self.skipTest("file symlinks are unavailable")
            config.write_bytes(config.read_bytes().replace(
                json.dumps(str(target)).encode(), json.dumps(str(alias)).encode(),
            ))
            with self.fake_command(FakeCodex()):
                plan = setup.prepare(plugin, home, "codex")
                self.assertEqual([], setup.preview(plan)["legacy_catalog_recovery"])
                setup.install(plan)
                self.assertEqual(str(alias), tomllib.loads(config.read_text())["model_catalog_json"])
                setup.uninstall(home, "codex", *setup.receipt(home, None))
            self.assertTrue(alias.is_symlink())
            self.assertEqual(b"live target", alias.read_bytes())
            self.assertEqual(str(alias), tomllib.loads(config.read_text())["model_catalog_json"])

    def test_inline_and_file_profiles_keep_their_referenced_catalogs(self):
        for operation in ("install", "uninstall"):
            with self.subTest(operation=operation), tempfile.TemporaryDirectory() as temporary:
                plugin, home, _, _ = self.fixture(Path(temporary))
                config = home / "config.toml"
                v1, v2 = (home / f"models-casefile-{version}.json" for version in ("v1", "v2"))
                old = self.legacy_install(
                    plugin, home, version="v2", catalogs={v1.name: b"legacy v1", v2.name: b"legacy v2"},
                )
                inline = f'\n[profiles.inline]\nmodel_catalog_json = "{v1.name}"\n'.encode()
                config.write_bytes(config.read_bytes() + inline)
                named = home / "named.config.toml"
                named.write_text(f'model_catalog_json = {json.dumps(str(v2))}\n')
                with self.fake_command(FakeCodex()):
                    if operation == "install":
                        plan = setup.prepare(plugin, home, "codex")
                        self.assertEqual([], setup.preview(plan)["legacy_catalog_recovery"])
                        setup.install(plan)
                    else:
                        setup.uninstall(home, "codex", *old)
                document = tomllib.loads(config.read_text())
                self.assertNotIn("model_catalog_json", document)
                self.assertEqual(v1.name, document["profiles"]["inline"]["model_catalog_json"])
                self.assertEqual(str(v2), tomllib.loads(named.read_text())["model_catalog_json"])
                self.assertEqual(b"legacy v1", v1.read_bytes())
                self.assertEqual(b"legacy v2", v2.read_bytes())

    def test_uninstall_recovers_missing_config_but_keeps_an_existing_override_removal(self):
        for missing_config in (False, True):
            with self.subTest(missing_config=missing_config), tempfile.TemporaryDirectory() as temporary:
                plugin, home, source, _ = self.fixture(Path(temporary))
                config = home / "config.toml"
                original = b'model_catalog_json = "earlier.json"\n' + source
                config.write_bytes(original)
                old = self.legacy_install(
                    plugin, home, version="v1", catalogs={"models-casefile-v1.json": b"legacy"},
                )
                if missing_config:
                    config.unlink()
                else:
                    current = config.read_bytes()
                    detached, _ = setup.detach_catalog_setting(current, tomllib.loads(current.decode()))
                    config.write_bytes(detached.replace(b"pragmatic", b"friendly"))
                with self.fake_command(FakeCodex()):
                    setup.uninstall(home, "codex", *old)
                self.assertEqual(original if missing_config else source.replace(b"pragmatic", b"friendly"), config.read_bytes())

    def test_surviving_agent_preserves_its_relative_catalog_during_recovery(self):
        for operation in ("install", "uninstall"):
            with self.subTest(operation=operation), tempfile.TemporaryDirectory() as temporary:
                plugin, home, _, _ = self.fixture(Path(temporary))
                config = home / "config.toml"
                v1, v2 = (home / f"models-casefile-{version}.json" for version in ("v1", "v2"))
                old = self.legacy_install(
                    plugin, home, version="v1", catalogs={v1.name: b"used by custom agent", v2.name: b"unused"},
                )
                role = home / "roles/nested/custom.toml"
                role.parent.mkdir(parents=True)
                role.write_text('model_catalog_json = "../../models-casefile-v1.json"\n')
                registration = b'\n[agents.custom]\nconfig_file = "roles/nested/custom.toml"\n'
                removed = b'\n[agents.casefile-retired]\nconfig_file = "missing.toml"\n'
                config.write_bytes(config.read_bytes() + removed)
                if operation == "install":
                    config.write_bytes(config.read_bytes() + registration)
                else:
                    (home / "custom.config.toml").write_bytes(registration)
                with self.fake_command(FakeCodex()):
                    if operation == "install":
                        setup.install(setup.prepare(plugin, home, "codex"))
                    else:
                        setup.uninstall(home, "codex", *old)
                source = config if operation == "install" else home / "custom.config.toml"
                agents = tomllib.loads(source.read_text())["agents"]
                self.assertEqual("roles/nested/custom.toml", agents["custom"]["config_file"])
                self.assertEqual(b"used by custom agent", v1.read_bytes())
                self.assertFalse(v2.exists())

    def test_unavailable_surviving_agent_config_keeps_receipt_catalog_recoverable(self):
        with tempfile.TemporaryDirectory() as temporary:
            plugin, home, _, _ = self.fixture(Path(temporary))
            config = home / "config.toml"
            v1 = home / "models-casefile-v1.json"
            old = self.legacy_install(plugin, home, version="v1", catalogs={v1.name: b"legacy catalog"})
            config.write_bytes(config.read_bytes() + b'\n[agents.custom]\nconfig_file = "missing.toml"\n')
            with self.fake_command(FakeCodex()):
                setup.uninstall(home, "codex", *old)
            document = tomllib.loads(config.read_text())
            self.assertNotIn("model_catalog_json", document)
            self.assertEqual("missing.toml", document["agents"]["custom"]["config_file"])
            self.assertEqual(b"legacy catalog", v1.read_bytes())

    def test_upgrade_preserves_active_runtime_when_it_differs_from_the_receipt(self):
        with tempfile.TemporaryDirectory() as temporary:
            plugin, home, _, _ = self.fixture(Path(temporary))
            self.legacy_install(plugin, home, version="v1", catalogs={"models-casefile-v1.json": b"legacy"})
            config = home / "config.toml"
            config.write_bytes(config.read_bytes().replace(
                b"multi_agent = true\nmulti_agent_v2 = false", b"multi_agent = false\nmulti_agent_v2 = true",
            ))
            with self.fake_command(FakeCodex()):
                self.assertEqual("v1", setup.receipt_multi_agent_version(setup.receipt(home, None)[1]))
                plan = setup.prepare(plugin, home, "codex")
                self.assertEqual("v2", setup.preview(plan)["multi_agent_version"])
                setup.install(plan)
                self.assertEqual("v2", setup.receipt_multi_agent_version(setup.receipt(home, None)[1]))
                self.assertTrue(tomllib.loads(config.read_text())["features"]["multi_agent_v2"])

    def test_upgrade_requires_explicit_runtime_when_active_flags_are_ambiguous(self):
        with tempfile.TemporaryDirectory() as temporary:
            plugin, home, _, _ = self.fixture(Path(temporary))
            self.legacy_install(plugin, home, version="v1", catalogs={"models-casefile-v1.json": b"legacy"})
            config = home / "config.toml"
            with self.fake_command(FakeCodex()):
                plan = setup.prepare(plugin, home, "codex")
                ambiguous = config.read_bytes().replace(b"multi_agent_v2 = false", b"multi_agent_v2 = true")
                config.write_bytes(ambiguous)
                with self.assertRaisesRegex(setup.SetupError, "active multi-agent runtime is ambiguous"):
                    setup.prepare(plugin, home, "codex")
                with self.assertRaisesRegex(setup.SetupError, "active multi-agent runtime is ambiguous"):
                    setup.install(plan)
                self.assertEqual(ambiguous, config.read_bytes())
                setup.install(setup.prepare(plugin, home, "codex", version="v2"))
                features = tomllib.loads(config.read_text())["features"]
                self.assertIs(features["multi_agent"], False)
                self.assertIs(features["multi_agent_v2"], True)

    def test_config_replaces_owned_keys_but_preserves_host_thread_count(self):
        prior = (
            b'model_catalog_json = "other.json"\n'
            b'keep = "me"\n'
            b'\n[mcp_servers.casefile]\ncommand = "/unowned"\n'
            b'\n[mcp_servers.unrelated]\ncommand = "/unrelated/server"\n'
            b'\n[features]\nmulti_agent = false\nother_feature = true\n'
            b'\n[agents]\nmax_threads = 12\n'
        )
        with tempfile.TemporaryDirectory() as temporary:
            plugin, home, _, _ = self.fixture(Path(temporary))
            (home / "config.toml").write_bytes(prior)
            with self.fake_command(FakeCodex()):
                plan = setup.prepare(plugin, home, "codex")
            document = tomllib.loads(plan["config"].decode("utf-8"))
            self.assertEqual("other.json", document["model_catalog_json"])
            self.assertEqual(str(plan["binary"]), document["mcp_servers"]["casefile"]["command"])
            self.assertTrue(document["features"]["multi_agent"])
            self.assertFalse(document["features"]["multi_agent_v2"])
            self.assertEqual(2, document["agents"]["max_depth"])
            self.assertEqual(12, document["agents"]["max_threads"])
            self.assertEqual("me", document["keep"])
            self.assertTrue(document["features"]["other_feature"])
            self.assertEqual("/unrelated/server", document["mcp_servers"]["unrelated"]["command"])

    def test_thread_capacity_changes_after_preview_cannot_be_overwritten_by_install(self):
        with tempfile.TemporaryDirectory() as temporary:
            plugin, home, _, _ = self.fixture(Path(temporary))
            config = home / "config.toml"
            with self.fake_command(FakeCodex()):
                setup.install(setup.prepare(plugin, home, "codex"))
                plan = setup.prepare(plugin, home, "codex")
                insufficient = setup.MINIMUM_AGENT_THREADS - 1
                config.write_bytes(config.read_bytes().replace(
                    b"max_threads = 12", f"max_threads = {insufficient}".encode("ascii")
                ))
                before = {path: path.read_bytes() for path in home.rglob("*") if path.is_file()}
                with self.assertRaisesRegex(setup.SetupError, "agents.max_threads.*at least"):
                    setup.install(plan)
                self.assertEqual(before, {
                    path: path.read_bytes() for path in home.rglob("*") if path.is_file()
                })
                receipt_path, receipt = setup.receipt(home, None)
                setup.uninstall(home, "codex", receipt_path, receipt)
                self.assertEqual(insufficient, tomllib.loads(config.read_text())["agents"]["max_threads"])

    def test_missing_thread_count_is_initialized_but_remains_host_owned(self):
        for config_source in (b"", b"[agents]\n"):
            with self.subTest(config=config_source), tempfile.TemporaryDirectory() as temporary:
                plugin, home, _, _ = self.fixture(Path(temporary))
                config = home / "config.toml"
                config.write_bytes(config_source)
                with self.fake_command(FakeCodex()):
                    setup.install(setup.prepare(plugin, home, "codex"))
                    threads = tomllib.loads(config.read_text())["agents"]["max_threads"]
                    self.assertGreaterEqual(threads, setup.MINIMUM_AGENT_THREADS)
                    setup.install(setup.prepare(plugin, home, "codex"))
                    config.write_bytes(config.read_bytes().replace(
                        f"max_threads = {threads}".encode("ascii"), b"max_threads = 24"
                    ))
                    setup.install(setup.prepare(plugin, home, "codex", version="v2"))
                    self.assertEqual(24, tomllib.loads(config.read_text())["agents"]["max_threads"])
                    receipt_path, receipt = setup.receipt(home, None)
                    setup.uninstall(home, "codex", receipt_path, receipt)
                restored = tomllib.loads(config.read_text())
                self.assertEqual(24, restored["agents"]["max_threads"])
                self.assertFalse(
                    {"multi_agent", "multi_agent_v2"} & restored.get("features", {}).keys()
                )

    def test_thread_count_inside_old_owned_markers_survives_reinstall_and_uninstall(self):
        for reinstall in (False, True):
            with self.subTest(reinstall=reinstall), tempfile.TemporaryDirectory() as temporary:
                plugin, home, original, _ = self.fixture(Path(temporary))
                config = home / "config.toml"
                with self.fake_command(FakeCodex()):
                    setup.install(setup.prepare(plugin, home, "codex"))
                    installed = config.read_bytes().replace(b"max_threads = 12\n", b"")
                    config.write_bytes(installed.replace(
                        setup.AGENT_END, b"max_threads = 24\n" + setup.AGENT_END
                    ))
                    if reinstall:
                        setup.install(setup.prepare(plugin, home, "codex", version="v2"))
                        self.assertEqual(24, tomllib.loads(config.read_text())["agents"]["max_threads"])
                    receipt_path, receipt = setup.receipt(home, None)
                    setup.uninstall(home, "codex", receipt_path, receipt)
                self.assertEqual(original.replace(b"max_threads = 12", b"max_threads = 24"), config.read_bytes())

    def test_dotted_host_thread_count_survives_the_complete_setup_lifecycle(self):
        with tempfile.TemporaryDirectory() as temporary:
            plugin, home, original, _ = self.fixture(Path(temporary))
            original = b"agents.max_threads = 12 # host capacity\n" + original.replace(
                b"\n[agents]\nmax_threads = 12\n", b""
            )
            config = home / "config.toml"
            config.write_bytes(original)
            with self.fake_command(FakeCodex()):
                setup.install(setup.prepare(plugin, home, "codex"))
                setup.install(setup.prepare(plugin, home, "codex", version="v2"))
                self.assertEqual(12, tomllib.loads(config.read_text())["agents"]["max_threads"])
                receipt_path, receipt = setup.receipt(home, None)
                setup.uninstall(home, "codex", receipt_path, receipt)
            self.assertEqual(original, config.read_bytes())

    def test_incoherent_role_registration_does_not_change_host_config(self):
        for drift, diagnostic in (
            ("agent model", "fixes a model or effort"),
            ("registered effort", "fixes a model or effort"),
            ("missing instructions", "lacks developer instructions"),
            ("missing registration", "roles differ from the packaged profiles"),
        ):
            with self.subTest(drift=drift), tempfile.TemporaryDirectory() as temporary:
                plugin, home, original, _ = self.fixture(Path(temporary))
                name = "casefile-implement-ticket-batch-implementation-writer"
                agent = plugin / "agents" / f"{name}.toml"
                fragment = plugin / "config/config-fragment.toml.in"
                header = f"[agents.{name}]\n"
                if drift == "agent model":
                    agent.write_text(agent.read_text() + 'model = "gpt-5.6-sol"\n')
                elif drift == "registered effort":
                    fragment.write_text(fragment.read_text().replace(
                        header, header + 'model_reasoning_effort = "medium"\n',
                    ))
                elif drift == "missing instructions":
                    agent.write_text('developer_instructions = " "\n')
                else:
                    fragment.write_text(fragment.read_text().replace(header, "[agents.custom]\n"))
                fake = FakeCodex()
                with self.fake_command(fake):
                    with self.assertRaisesRegex(setup.SetupError, diagnostic):
                        setup.prepare(plugin, home, "codex", version="v2")
                self.assertEqual(original, (home / "config.toml").read_bytes())

    def test_portable_bytes_write_and_resource_separator(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            target = root / "target"
            with mock.patch.object(setup.os, "fchmod", None):
                setup.atomic_write(target, b"portable")
            self.assertEqual(b"portable", target.read_bytes())

            plugin, home, _, _ = self.fixture(root / "fixture")
            profile = plugin / "config/profiles.toml"
            profile.write_text(
                profile.read_text(encoding="ascii").replace(
                    'agent_file = "agents/',
                    r'agent_file = "agents\\',
                ),
                encoding="ascii",
            )
            with self.fake_command(FakeCodex()):
                plan = setup.prepare(plugin, home, "codex")

                def reformat_written_configuration(current):
                    config = current["home"] / "config.toml"
                    config.write_bytes(config.read_bytes().replace(b"\n", b"\r\n"))

                with mock.patch.object(setup, "doctor", side_effect=reformat_written_configuration):
                    self.assertEqual("installed", setup.install(plan)["status"])

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

            plugin, home, original, legacy = self.fixture(root / "rollback")
            with self.fake_command(FakeCodex(doctor_ok=False)):
                with self.assertRaisesRegex(setup.SetupError, "rollback verified"):
                    setup.install(setup.prepare(plugin, home, "codex"))
            self.assertEqual(original, (home / "config.toml").read_bytes())
            self.assertTrue(legacy.is_dir())

    def test_current_receipt_and_git_alert(self):
        with tempfile.TemporaryDirectory() as temporary:
            plugin, home, _, _ = self.fixture(Path(temporary))
            fake = FakeCodex()
            with self.fake_command(fake):
                result = setup.install(setup.prepare(plugin, home, "codex"))
                receipt_path, receipt = setup.receipt(home, Path(result["receipt"]))
                config = home / "config.toml"
                config.write_bytes(config.read_bytes() + b'local = "pragmatic"\n')
                output = io.StringIO()
                with contextlib.redirect_stdout(output):
                    setup.show_uninstall_diffs(home, receipt_path, receipt)
                self.assertIn("diff --git", output.getvalue())
                self.assertIn("# >>> casefile setup tables >>>", output.getvalue())
                setup.uninstall(home, "codex", receipt_path, receipt)

    def test_uninstall_aborts_when_a_managed_file_changes_after_snapshot(self):
        with tempfile.TemporaryDirectory() as temporary:
            plugin, home, _, _ = self.fixture(Path(temporary))
            fake = FakeCodex()
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
                self.assertEqual(installed + b"\nchanged_after_snapshot = true\n", contract.read_bytes())
                self.assertTrue(setup.pointer(home).exists())


if __name__ == "__main__":
    unittest.main()
