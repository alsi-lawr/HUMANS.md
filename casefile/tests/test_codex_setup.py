from __future__ import annotations

import contextlib
import io
import json
import hashlib
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
    def __init__(self, catalog: dict, doctor_ok: bool = True, version: str = "0.145.0"):
        self.catalog = catalog
        self.doctor_ok = doctor_ok
        self.version = version
        self.installed = True
        self.marketplace = True
        self.calls: list[list[str]] = []
        self.model_acquisition_calls = 0

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
            output = "Configuration\n  [ok] config loaded\n" if self.doctor_ok else "failed\n"
            return subprocess.CompletedProcess(args, 0 if self.doctor_ok else 2, output, "")
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
        shutil.copytree(ROOT / "casefile/adapters/codex/catalog", plugin / "config/catalog")
        shutil.copytree(ROOT / "casefile/adapters/codex/agents", plugin / "agents")
        (plugin / "scripts").mkdir()
        shutil.copy2(ROOT / "casefile/adapters/shared/casefile_runtime.py", plugin / "scripts/casefile_runtime.py")
        shutil.copy2(
            ROOT / "casefile/adapters/codex/scripts/resolve-writer-binding.py",
            plugin / "scripts/resolve-writer-binding.py",
        )
        shutil.copy2(
            ROOT / "casefile/adapters/codex/scripts/list-codex-models.py",
            plugin / "scripts/list-codex-models.py",
        )
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
            rows.append({"path":binary.relative_to(plugin / 'runtime').as_posix(),"sha256":hashlib.sha256(runtime_source).hexdigest(),"size":len(runtime_source),"target":target})
        (plugin / "runtime/artifacts.json").write_text(json.dumps({"schema_version":1,"version":PLUGIN_VERSION,"source_commit":"1"*40,"artifacts":rows}, indent=2, sort_keys=True)+"\n", encoding="ascii")
        (plugin / "templates").mkdir()
        shutil.copy2(ROOT / "AGENTS.md", plugin / "templates/AGENTS.md")

        profiles = tomllib.loads((plugin / "config/profiles.toml").read_text(encoding="ascii"))
        catalog = {
            "models": [
                {
                    "slug": target["id"],
                    "display_name": target["expected"]["display_name"],
                    "visibility": target["expected"]["visibility"],
                    "base_instructions": "upstream",
                    "model_messages": {"instructions_template": "upstream"},
                    "supported_reasoning_levels": [
                        {"effort": effort} for effort in target["required_reasoning"]
                    ],
                    **(
                        {"multi_agent_version": "v2"}
                        if target["id"] in setup.V1_SELECTOR_MODELS
                        else {}
                    ),
                }
                for target in profiles["catalog"]["targets"]
            ]
        }
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
        return plugin, home, original, catalog, legacy

    @contextlib.contextmanager
    def fake_command(self, fake):
        previous = setup.command
        previous_probe = setup.casefile_runtime.probe
        previous_listing = setup.list_codex_models.listing

        def listing(_executable, _profile_path, _timeout=20.0):
            fake.model_acquisition_calls += 1
            return {
                "models": [
                    {
                        "slug": model["slug"],
                        "display_name": model["display_name"],
                        "visibility": model["visibility"],
                        "supported_reasoning_levels": model["supported_reasoning_levels"],
                    }
                    for model in fake.catalog["models"]
                ]
            }

        setup.command = fake
        setup.casefile_runtime.probe = lambda *_: None
        setup.list_codex_models.listing = listing
        try:
            yield
        finally:
            setup.command = previous
            setup.casefile_runtime.probe = previous_probe
            setup.list_codex_models.listing = previous_listing

    def test_active_models_install_and_uninstall_preserve_unowned_config(self):
        with tempfile.TemporaryDirectory() as temporary:
            plugin, home, original, catalog, legacy = self.fixture(Path(temporary))
            fake = FakeCodex(catalog)
            with self.fake_command(fake):
                plan = setup.prepare(plugin, home, "codex")
                self.assertIn("gpt-5.3-codex-spark", plan["catalog_models"])
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
                    original.replace(b"pragmatic", b"friendly").replace(b"max_threads = 12\n", b"")
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
                self.assertEqual(3, fake.model_acquisition_calls)
                self.assertFalse(any("debug" in call for call in fake.calls))

    def test_v1_and_v2_lifecycle_record_selected_catalog_and_preserve_other_variant(self):
        for version in ("v1", "v2"):
            with self.subTest(version=version), tempfile.TemporaryDirectory() as temporary:
                plugin, home, original, catalog, _ = self.fixture(Path(temporary))
                other = home / f"models-casefile-{'v2' if version == 'v1' else 'v1'}.json"
                other.write_bytes(b'{"unowned": true}\n')
                fake = FakeCodex(catalog)
                with self.fake_command(fake):
                    plan = setup.prepare(plugin, home, "codex", version=version)
                    self.assertEqual(version, setup.preview(plan)["multi_agent_version"])
                    self.assertIn("gpt-5.3-codex-spark", plan["catalog_models"])
                    result = setup.install(plan)
                    receipt_path, receipt = setup.receipt(home, Path(result["receipt"]))
                    self.assertEqual(version, receipt["multi_agent_version"])
                    self.assertEqual(version, setup.uninstall_preview(receipt_path, receipt)["multi_agent_version"])
                    selected = json.loads((home / f"models-casefile-{version}.json").read_bytes())["models"]
                    selectors = {
                        model["slug"]: model.get("multi_agent_version") for model in selected
                    }
                    if version == "v1":
                        self.assertTrue(all(selectors[model] is None for model in setup.V1_SELECTOR_MODELS))
                    else:
                        self.assertTrue(all(value == "v2" for value in selectors.values()))
                    document = tomllib.loads((home / "config.toml").read_text(encoding="ascii"))
                    self.assertEqual(version == "v1", document["features"]["multi_agent"])
                    self.assertEqual(version == "v2", document["features"]["multi_agent_v2"])
                    self.assertEqual(2, document["agents"]["max_depth"])
                    self.assertEqual(6, document["agents"]["max_threads"])
                    setup.uninstall(home, "codex", receipt_path, receipt)
                self.assertEqual(original.replace(b"max_threads = 12\n", b""), (home / "config.toml").read_bytes())
                self.assertEqual(b'{"unowned": true}\n', other.read_bytes())
                self.assertFalse((home / f"models-casefile-{version}.json").exists())


    def test_verification_rejects_v2_mixed_feature_and_catalog_states(self):
        with tempfile.TemporaryDirectory() as temporary:
            plugin, home, _, catalog, _ = self.fixture(Path(temporary))
            with self.fake_command(FakeCodex(catalog)):
                plan = setup.prepare(plugin, home, "codex", version="v2")
            mixed = plan["config"].replace(b"multi_agent_v2 = true", b"multi_agent_v2 = false")
            with self.assertRaisesRegex(setup.SetupError, "V2 feature flags"):
                setup.verify_config(mixed, plugin, home / "models-casefile-v2.json", "v2")
            catalog["models"][0]["multi_agent_version"] = None
            with self.fake_command(FakeCodex(catalog)):
                (home / "models-casefile-v2.json").write_bytes(setup.canonical(catalog))
                with self.assertRaisesRegex(setup.SetupError, "did not activate V2"):
                    setup.verify_effective_catalog(plan)

    def test_verification_rejects_invalid_v1_owned_selector(self):
        with tempfile.TemporaryDirectory() as temporary:
            plugin, home, _, catalog, _ = self.fixture(Path(temporary))
            with self.fake_command(FakeCodex(catalog)):
                plan = setup.prepare(plugin, home, "codex", version="v1")
                written = json.loads(plan["catalog"])
                selected = {
                    model["slug"]: model for model in written["models"]
                }
                selected["gpt-5.6-sol"]["multi_agent_version"] = "v2"
                (home / "models-casefile-v1.json").write_bytes(setup.canonical(written))
                with self.assertRaisesRegex(setup.SetupError, "did not activate V1"):
                    setup.verify_effective_catalog(plan)

    def test_existing_v1_receipt_without_version_remains_uninstallable(self):
        with tempfile.TemporaryDirectory() as temporary:
            plugin, home, original, catalog, _ = self.fixture(Path(temporary))
            fake = FakeCodex(catalog)
            with self.fake_command(fake):
                result = setup.install(setup.prepare(plugin, home, "codex"))
                receipt_path = Path(result["receipt"])
                legacy = json.loads(receipt_path.read_bytes())
                legacy["schema_version"] = 4
                del legacy["multi_agent_version"]
                receipt_path.write_bytes(setup.canonical(legacy))
                checked_path, receipt = setup.receipt(home, receipt_path)
                self.assertEqual("v1", setup.receipt_multi_agent_version(receipt))
                setup.uninstall(home, "codex", checked_path, receipt)
            self.assertEqual(original.replace(b"max_threads = 12\n", b""), (home / "config.toml").read_bytes())

    def test_legacy_receipt_upgrades_side_by_side_and_uninstalls_to_original(self):
        with tempfile.TemporaryDirectory() as temporary:
            plugin, home, original, catalog, _ = self.fixture(Path(temporary))
            fake = FakeCodex(catalog)
            with self.fake_command(fake):
                first = setup.install(setup.prepare(plugin, home, "codex"))
                first_path = Path(first["receipt"])
                legacy = json.loads(first_path.read_bytes())
                legacy["schema_version"] = 5
                legacy["plugin_version"] = "0.3.4"
                legacy.pop("casefile_binary")
                legacy.pop("planning_root")
                legacy.pop("artifact_sha256")
                legacy.pop("owned_binaries")
                legacy["before"] = legacy["before"][:2]
                first_path.write_bytes(setup.canonical(legacy))
                owned_catalog = home / "models-casefile-v1.json"
                owned = json.loads(owned_catalog.read_bytes())
                owned["upgrade_owned_marker"] = "retained"
                owned_catalog.write_bytes(setup.canonical(owned))
                (home / "models_cache.json").write_bytes(b"not JSON\n")
                upgrade_plan = setup.prepare(plugin, home, "codex")
                self.assertNotIn("upgrade_owned_marker", json.loads(upgrade_plan["catalog"]))
                self.assertEqual(b"not JSON\n", (home / "models_cache.json").read_bytes())
                upgraded = setup.install(upgrade_plan)
                receipt_path, receipt = setup.receipt(home, Path(upgraded["receipt"]))
                self.assertEqual(6, receipt["schema_version"])
                self.assertTrue((home / receipt["casefile_binary"]).is_file())
                setup.uninstall(home, "codex", receipt_path, receipt)
            self.assertEqual(original.replace(b"max_threads = 12\n", b""), (home / "config.toml").read_bytes())


    def test_missing_each_required_model_is_rejected_before_v1_or_v2_mutation(self):
        for version in ("v1", "v2"):
            # A pinned model is exempt: Codex may stop projecting it while it is still required.
            pinned = setup.pinned_models(
                Path(__file__).resolve().parents[1] / "adapters/codex/profiles.toml"
            )
            for missing in sorted(setup.REQUIRED_MODELS - pinned):
                with self.subTest(version=version, missing=missing), tempfile.TemporaryDirectory() as temporary:
                    plugin, home, original, catalog, _ = self.fixture(Path(temporary))
                    fallback = {
                        "models": [
                            model for model in catalog["models"] if model["slug"] != missing
                        ]
                    }
                    with self.fake_command(FakeCodex(fallback)):
                        with self.assertRaisesRegex(
                            setup.SetupError,
                            f"Codex lacks required models: {missing}",
                        ):
                            setup.prepare(plugin, home, "codex", version=version)
                    self.assertEqual(original, (home / "config.toml").read_bytes())
                    self.assertFalse((home / f"models-casefile-{version}.json").exists())
                    self.assertFalse((home / "backups/casefile").exists())

    def test_complete_maintained_catalog_carries_pinned_model(self):
        profiles = Path(__file__).resolve().parents[1] / "adapters/codex/profiles.toml"
        pinned = setup.pinned_models(profiles)
        self.assertEqual({"gpt-5.3-codex-spark"}, pinned)
        for version in ("v1", "v2"):
            with self.subTest(version=version):
                written, catalog_models = setup.catalog_replacement(profiles, version)
                document = json.loads(written.decode("ascii"))
                slugs = {model["slug"] for model in document["models"]}
                self.assertEqual(setup.carried_models(profiles), slugs)
                self.assertEqual(slugs, set(catalog_models))
                entry = next(m for m in document["models"] if m["slug"] == "gpt-5.3-codex-spark")
                self.assertEqual("GPT-5.3-Codex-Spark", entry["display_name"])
                self.assertTrue(entry["base_instructions"])
                self.assertIsInstance(entry["model_messages"], dict)
                for model in document["models"]:
                    self.assertLessEqual(setup.REQUIRED_CATALOG_FIELDS, set(model))
                    self.assertTrue(
                        all(
                            level["description"]
                            for level in model["supported_reasoning_levels"]
                        )
                    )

    def test_unknown_projected_alias_does_not_change_replacement_catalog(self):
        with tempfile.TemporaryDirectory() as temporary:
            plugin, home, _, projection, _ = self.fixture(Path(temporary))
            projection["models"].append(
                {
                    "slug": "gpt-5.6-sol-wm",
                    "display_name": "GPT-5.6-Sol-WM",
                    "visibility": "hide",
                    "supported_reasoning_levels": [
                        {
                            "effort": "high",
                            "description": "Greater reasoning depth for complex problems",
                        }
                    ],
                }
            )
            with self.fake_command(FakeCodex(projection)):
                plan = setup.prepare(plugin, home, "codex", version="v2")
            slugs = {
                model["slug"] for model in json.loads(plan["catalog"])["models"]
            }
            self.assertNotIn("gpt-5.6-sol-wm", slugs)
            self.assertEqual(setup.carried_models(plugin / "config/profiles.toml"), slugs)


    def test_config_clobbers_owned_keys_and_preserves_unowned(self):
        prior = (
            b'model_catalog_json = "other.json"\n'
            b'keep = "me"\n'
            b'\n[mcp_servers.casefile]\ncommand = "/unowned"\n'
            b'\n[mcp_servers.unrelated]\ncommand = "/unrelated/server"\n'
            b'\n[features]\nmulti_agent = false\nother_feature = true\n'
            b'\n[agents]\nmax_threads = 1\n'
        )
        with tempfile.TemporaryDirectory() as temporary:
            plugin, home, _, catalog, _ = self.fixture(Path(temporary))
            (home / "config.toml").write_bytes(prior)
            with self.fake_command(FakeCodex(catalog)):
                plan = setup.prepare(plugin, home, "codex")
            document = tomllib.loads(plan["config"].decode("utf-8"))
            self.assertEqual(str(home / "models-casefile-v1.json"), document["model_catalog_json"])
            self.assertEqual(str(plan["binary"]), document["mcp_servers"]["casefile"]["command"])
            self.assertTrue(document["features"]["multi_agent"])
            self.assertFalse(document["features"]["multi_agent_v2"])
            self.assertEqual(2, document["agents"]["max_depth"])
            self.assertEqual(6, document["agents"]["max_threads"])
            self.assertEqual("me", document["keep"])
            self.assertTrue(document["features"]["other_feature"])
            self.assertEqual("/unrelated/server", document["mcp_servers"]["unrelated"]["command"])

    def test_writer_profile_and_runtime_override_drift_reject_before_model_export(self):
        for route in ("named", "runtime"):
            with self.subTest(route=route), tempfile.TemporaryDirectory() as temporary:
                plugin, home, _, catalog, _ = self.fixture(Path(temporary))
                profiles = tomllib.loads(
                    (plugin / "config/profiles.toml").read_text(encoding="ascii")
                )
                rows = (
                    profiles["writer_profiles"]
                    if route == "named"
                    else profiles["writer_runtime_overrides"]
                )
                agent = plugin / rows[0]["agent_file"]
                if route == "named":
                    agent.write_text(
                        agent.read_text(encoding="ascii").replace(
                            f'model = "{rows[0]["model"]}"', 'model = "wrong"'
                        ),
                        encoding="ascii",
                    )
                    diagnostic = "incoherent"
                else:
                    agent.write_text(
                        agent.read_text(encoding="ascii") + 'model = "gpt-5.6-sol"\n',
                        encoding="ascii",
                    )
                    diagnostic = "fixes a model"
                fake = FakeCodex(catalog)
                with self.fake_command(fake):
                    with self.assertRaisesRegex(setup.SetupError, diagnostic):
                        setup.prepare(plugin, home, "codex", version="v2")
                self.assertEqual(0, fake.model_acquisition_calls)

    def test_effective_v1_and_v2_catalogs_require_spark(self):
        for version in ("v1", "v2"):
            with self.subTest(version=version), tempfile.TemporaryDirectory() as temporary:
                plugin, home, _, catalog, _ = self.fixture(Path(temporary))
                catalog["models"] = [
                    model
                    for model in catalog["models"]
                    if model["slug"] != "gpt-5.3-codex-spark"
                ]
                if version == "v1":
                    for model in catalog["models"]:
                        if model["slug"] in setup.V1_SELECTOR_MODELS:
                            model["multi_agent_version"] = None
                else:
                    for model in catalog["models"]:
                        model["multi_agent_version"] = "v2"
                (home / f"models-casefile-{version}.json").write_bytes(setup.canonical(catalog))
                with self.fake_command(FakeCodex(catalog)):
                    with self.assertRaisesRegex(
                        setup.SetupError, "missing gpt-5.3-codex-spark"
                    ):
                        setup.verify_effective_catalog(
                            {
                                "home": home,
                                "executable": "codex",
                                "environment": {"CODEX_HOME": str(home)},
                                "multi_agent_version": version,
                                "root": plugin,
                            }
                        )

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
                    "catalog/models.json",
                    r"catalog\\models.json",
                ),
                encoding="ascii",
            )
            with self.fake_command(FakeCodex(catalog)):
                self.assertIn("gpt-5.6-sol", setup.prepare(plugin, home, "codex")["catalog_models"])

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
