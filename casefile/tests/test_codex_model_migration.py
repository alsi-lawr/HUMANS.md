from __future__ import annotations

import contextlib
import io
import json
import os
import re
import shutil
import sys
import tempfile
import tomllib
import unittest
from pathlib import Path
from unittest import mock

from _load import script
import test_codex_setup as lifecycle
from test_writer_binding import binding, model, projection, PROFILES_PATH


setup = lifecycle.setup
migration = script("casefile/adapters/codex/scripts/codex_model_migration.py")


def files(root: Path) -> dict[str, bytes]:
    return {str(path.relative_to(root)): path.read_bytes() for path in root.rglob("*") if path.is_file()}


def legacy_package(plugin: Path) -> Path:
    legacy = plugin.with_name("pre-astra-plugin")
    shutil.copytree(plugin, legacy)
    profiles = legacy / "config/profiles.toml"
    matrix, rest = profiles.read_text().split("[catalog]", 1)
    source = matrix.replace('model = "gpt-6-astra"', 'model = "gpt-5.6-sol"') + "[catalog]" + rest
    source = re.sub(
        r"(?ms)^\[\[(?:catalog.targets|writer_profiles)\]\].*?(?=^\[\[|\Z)",
        lambda match: "" if "gpt-6-astra" in match[0] else match[0], source,
    )
    profiles.write_text(source)
    fragment = legacy / "config/config-fragment.toml.in"
    fragment.write_text("".join(
        block for block in re.split(r"(?m)(?=^\[agents\.)", fragment.read_text())
        if not block.startswith("[agents.casefile-implementation-writer-gpt-6-astra-")
    ))
    for path in (legacy / "agents").glob("casefile-implement-*-implementation-writer.toml"):
        path.write_text(path.read_text().replace('model = "gpt-6-astra"', 'model = "gpt-5.6-sol"'))
    catalog = legacy / "config/catalog/models.json"
    document = json.loads(catalog.read_bytes())
    document["models"] = [m for m in document["models"] if m["slug"] != "gpt-6-astra"]
    catalog.write_bytes(setup.canonical(document))
    return legacy


class CodexModelMigrationTests(unittest.TestCase):
    @contextlib.contextmanager
    def installed_legacy(self, version="v1"):
        with tempfile.TemporaryDirectory() as temporary:
            fixture = lifecycle.CodexSetupTests()
            plugin, home, original, catalog, _ = fixture.fixture(Path(temporary))
            old_plugin = legacy_package(plugin)
            fake = lifecycle.FakeCodex(catalog)
            with fixture.fake_command(fake):
                with mock.patch.object(setup, "REQUIRED_MODELS", setup.REQUIRED_MODELS - {"gpt-6-astra"}), mock.patch.object(
                    setup, "V1_SELECTOR_MODELS", setup.V1_SELECTOR_MODELS - {"gpt-6-astra"}
                ):
                    setup.install(setup.prepare(old_plugin, home, "codex", version=version))
                yield plugin, home, fake

    def test_upgrade_preserves_runtime_config_and_recovery_and_repeats_without_changes(self):
        for version in ("v1", "v2"):
            with self.subTest(runtime=version), self.installed_legacy(version) as (plugin, home, fake):
                config, catalog = setup.managed(home, version)
                config.write_bytes(config.read_bytes().replace(b"max_threads = 6", b"max_threads = 17"))
                before_config = config.read_bytes()
                old_config = tomllib.loads(before_config.decode())
                native = home / "models_cache.json"
                native.write_bytes(b"Codex owns this cache, not the migration\n")
                other = home / f"models-casefile-{'v2' if version == 'v1' else 'v1'}.json"
                other.write_bytes(b"other runtime catalog\n")
                previous_path, previous = setup.receipt(home, None)
                recovery = files(previous_path.parent / "before")
                original_state = files(home)
                self.assertNotIn("gpt-6-astra", setup.catalog_ids(json.loads(catalog.read_bytes()), "old catalog"))

                plan = migration.prepare(setup, plugin, home, "codex")
                preview = migration.preview(setup, plan)
                self.assertIn("gpt-6-astra", preview["catalog_changes"]["added"])
                self.assertEqual(original_state, files(home))
                result = migration.apply(setup, plan, preview["approval_digest"])
                self.assertEqual("migrated", result["status"])
                new_config = tomllib.loads(config.read_text())
                self.assertEqual(old_config["features"], new_config["features"])
                self.assertEqual(old_config["mcp_servers"], new_config["mcp_servers"])
                self.assertEqual(old_config["model"], new_config["model"])
                self.assertEqual(17, new_config["agents"]["max_threads"])
                self.assertEqual(setup.unowned_config(before_config), setup.unowned_config(config.read_bytes()))
                self.assertEqual(original_state["models_cache.json"], native.read_bytes())
                self.assertEqual(original_state[other.name], other.read_bytes())
                models = json.loads(catalog.read_bytes())["models"]
                astra = next(m for m in models if m["slug"] == "gpt-6-astra")
                self.assertEqual(None if version == "v1" else "v2", astra["multi_agent_version"])
                self.assertIn("gpt-5.6-sol", {m["slug"] for m in models})
                current_path, current = setup.receipt(home, None)
                self.assertEqual(previous["before"], current["before"])
                self.assertEqual(previous["casefile_binary"], current["casefile_binary"])
                self.assertEqual(recovery, files(current_path.parent / "before"))

                after = files(home)
                repeat = migration.prepare(setup, plugin, home, "codex")
                self.assertEqual([], migration.preview(setup, repeat)["changed_files"])
                self.assertEqual("unchanged", migration.apply(setup, repeat, repeat["approval_digest"])["status"])
                self.assertEqual(after, files(home))
                setup.uninstall(home, "codex", current_path, current)
                self.assertEqual(setup.unowned_config(before_config), config.read_bytes())
                self.assertFalse(catalog.exists())
                self.assertEqual(original_state[other.name], other.read_bytes())

    def test_failed_verification_restores_catalog_config_and_active_receipt(self):
        with self.installed_legacy() as (plugin, home, fake):
            plan = migration.prepare(setup, plugin, home, "codex")
            original = {path: path.read_bytes() for path in plan["observed"]}
            fake.doctor_ok = False
            with self.assertRaisesRegex(setup.SetupError, "rollback verified"):
                migration.apply(setup, plan, plan["approval_digest"])
            self.assertEqual(original, {path: path.read_bytes() for path in original})
            fake.doctor_ok = True
            retry = migration.prepare(setup, plugin, home, "codex")
            self.assertEqual("migrated", migration.apply(setup, retry, retry["approval_digest"])["status"])

    def test_existing_astra_catalog_still_upgrades_legacy_profile_registrations(self):
        with self.installed_legacy("v2") as (plugin, home, fake):
            config, catalog = setup.managed(home, "v2")
            replacement, _ = setup.catalog_replacement(plugin / "config/profiles.toml", "v2")
            catalog.write_bytes(replacement)
            original_catalog = catalog.read_bytes()
            plan = migration.prepare(setup, plugin, home, "codex")
            self.assertEqual([str(config)], migration.preview(setup, plan)["changed_files"])
            migration.apply(setup, plan, plan["approval_digest"])
            self.assertEqual(original_catalog, catalog.read_bytes())
            astra_role = tomllib.loads(config.read_text())["agents"][
                "casefile-implementation-writer-gpt-6-astra-high"
            ]
            definition = tomllib.loads(Path(astra_role["config_file"]).read_text())
            self.assertEqual("gpt-6-astra", definition["model"])

    def test_partial_write_failure_restores_the_previous_integration(self):
        with self.installed_legacy() as (plugin, home, fake):
            plan = migration.prepare(setup, plugin, home, "codex")
            original = {path: path.read_bytes() for path in plan["observed"]}
            write = setup.atomic_write
            catalog = setup.managed(home)[1]

            def fail_catalog(path, data, mode=0o600):
                if path == catalog:
                    raise OSError("catalog write failed")
                return write(path, data, mode)

            with mock.patch.object(setup, "atomic_write", side_effect=fail_catalog):
                with self.assertRaisesRegex(setup.SetupError, "rollback verified"):
                    migration.apply(setup, plan, plan["approval_digest"])
            self.assertEqual(original, {path: path.read_bytes() for path in original})

    def test_preview_digest_rejects_changed_inputs_without_overwriting_them(self):
        with self.installed_legacy() as (plugin, home, fake):
            approved = migration.prepare(setup, plugin, home, "codex")
            config = home / "config.toml"
            config.write_bytes(config.read_bytes().replace(b"pragmatic", b"friendly"))
            edited = files(home)
            with self.assertRaisesRegex(setup.SetupError, "changed after preview"):
                migration.apply(setup, approved, approved["approval_digest"])
            refreshed = migration.prepare(setup, plugin, home, "codex")
            with self.assertRaisesRegex(setup.SetupError, "approved current preview"):
                migration.apply(setup, refreshed, approved["approval_digest"])
            self.assertEqual(edited, files(home))

    def test_missing_astra_does_not_replace_the_legacy_installation(self):
        with self.installed_legacy() as (plugin, home, fake):
            fake.catalog["models"] = [m for m in fake.catalog["models"] if m["slug"] != "gpt-6-astra"]
            original = files(home)
            with self.assertRaisesRegex(setup.SetupError, "lacks required models: gpt-6-astra"):
                migration.prepare(setup, plugin, home, "codex")
            self.assertEqual(original, files(home))

    def test_runtime_receipt_disagreement_does_not_migrate_or_take_over_another_catalog(self):
        with self.installed_legacy() as (plugin, home, fake):
            config = home / "config.toml"
            config.write_bytes(config.read_bytes().replace(
                b"multi_agent = true\nmulti_agent_v2 = false",
                b"multi_agent = false\nmulti_agent_v2 = true",
            ))
            original = files(home)
            with self.assertRaisesRegex(setup.SetupError, "runtime and setup receipt disagree"):
                migration.prepare(setup, plugin, home, "codex")
            self.assertEqual(original, files(home))

    def test_explicit_runtime_reconciliation_preserves_both_catalogs_through_recovery(self):
        with self.installed_legacy() as (plugin, home, fake):
            v1 = home / "models-casefile-v1.json"
            v2 = home / "models-casefile-v2.json"
            original_v2 = b'{"custom_v2_catalog": true}\n'
            v2.write_bytes(original_v2)
            setup.install(setup.prepare(plugin, home, "codex", version="v2"))
            plan = migration.prepare(setup, plugin, home, "codex")
            self.assertEqual("unchanged", migration.apply(setup, plan, plan["approval_digest"])["status"])
            receipt_path, receipt = setup.receipt(home, None)
            owned = [home / entry["path"] for entry in receipt["before"]]
            before_uninstall = {path: path.read_bytes() for path in [*owned, setup.pointer(home)]}
            with mock.patch.object(setup, "checked", side_effect=setup.SetupError("plugin removal failed")):
                with self.assertRaisesRegex(setup.SetupError, "rollback verified"):
                    setup.uninstall(home, "codex", receipt_path, receipt)
            self.assertEqual(before_uninstall, {path: path.read_bytes() for path in before_uninstall})
            setup.uninstall(home, "codex", receipt_path, receipt)
            self.assertFalse(v1.exists())
            self.assertEqual(original_v2, v2.read_bytes())

    def test_cli_requires_approved_preview_digest_before_applying(self):
        with self.installed_legacy() as (plugin, home, fake):
            args = ["setup-codex.py", "migrate-models", "--plugin-root", str(plugin),
                    "--codex-home", str(home), "--codex-executable", "codex"]
            original = files(home)
            with mock.patch.dict(sys.modules, {setup.__name__: setup}), contextlib.redirect_stdout(io.StringIO()):
                with mock.patch.object(sys, "argv", args):
                    self.assertEqual(0, setup.main())
                with mock.patch.object(sys, "argv", [*args, "--apply"]):
                    self.assertEqual(1, setup.main())
                self.assertEqual(original, files(home))
                digest = migration.prepare(setup, plugin, home, "codex")["approval_digest"]
                with mock.patch.object(sys, "argv", [*args, "--apply", "--expect-digest", digest]):
                    self.assertEqual(0, setup.main())

    def test_setup_discovery_ignores_old_selected_catalog_without_copying_credentials(self):
        homes = []

        def project(executable, timeout, environment):
            home = Path(environment["CODEX_HOME"])
            homes.append(home)
            self.assertTrue(home.is_dir())
            self.assertEqual([], list(home.iterdir()))
            return [{"id": "gpt-6-astra", "displayName": "GPT-6-Astra", "hidden": True,
                     "supportedReasoningEfforts": [{"reasoningEffort": "high"}]}]

        with mock.patch.dict(os.environ, {"CODEX_HOME": "/old-selected-home"}), mock.patch.object(
            setup.list_codex_models, "project", side_effect=project
        ):
            result = setup.acquire_models("codex", PROFILES_PATH)
        self.assertIn("gpt-6-astra", {m["slug"] for m in result["models"]})
        self.assertTrue(all(not home.exists() for home in homes))

    def test_astra_binding_resolves_both_runtime_routes_without_changing_historical_sol(self):
        for version in ("v1", "v2"):
            with self.subTest(runtime=version), mock.patch.object(binding, "active_runtime", return_value=version), mock.patch.object(
                binding, "active_catalog", return_value={"models": [
                    model("gpt-6-astra", ("high",), selector=None if version == "v1" else "v2"),
                    model("gpt-5.6-sol", ("high",), selector=None if version == "v1" else "v2"),
                ]}
            ), mock.patch.object(binding, "require_writer_progress"):
                offer = binding.offer("codex", Path("/home"), PROFILES_PATH)
                self.assertEqual({"model": "gpt-6-astra", "reasoning_effort": "high", "available": True}, offer["recommendation"])
                for selected in ("gpt-6-astra", "gpt-5.6-sol"):
                    with mock.patch.object(binding, "binding_projection", return_value=projection("resolved", selected, "high", "binding")):
                        result = binding.resolve_spawn("codex", Path("/home"), PROFILES_PATH, "casefile",
                            Path("/planning"), "projects/demo/investigations/sample",
                            "casefile-implement-ticket-batch", "HMD-011")
                    self.assertEqual(selected, result["model"])
                    if version == "v1":
                        self.assertEqual(f"casefile-implementation-writer-{selected.replace('.', '-')}-high", result["spawn"]["agent_type"])
                    else:
                        self.assertEqual(selected, result["spawn"]["model"])
                        self.assertEqual("high", result["spawn"]["reasoning_effort"])


if __name__ == "__main__":
    unittest.main()
