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

from _load import ROOT, script
import test_codex_setup as lifecycle


setup = lifecycle.setup
migration = script("casefile/adapters/codex/scripts/codex_model_migration.py")
PROFILES_PATH = ROOT / "casefile/adapters/codex/profiles.toml"
WRITER = "casefile-implement-ticket-batch-implementation-writer"
LEGACY_ALIAS = "casefile-implementation-writer-gpt-5-6-sol-high"


def files(root: Path) -> dict[str, bytes]:
    return {str(path.relative_to(root)): path.read_bytes() for path in root.rglob("*") if path.is_file()}


def legacy_package(plugin: Path) -> Path:
    legacy = plugin.with_name("pre-astra-plugin")
    shutil.copytree(plugin, legacy)
    profiles = legacy / "config/profiles.toml"
    profiles.write_text(re.sub(
        r'(?ms)^\[\[catalog.targets\]\]\nid = "gpt-6-astra"\n.*?(?=^\[\[|\Z)',
        "", profiles.read_text(),
    ))
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
                writer = old_plugin / "agents" / f"{WRITER}.toml"
                pinned = writer.read_text() + 'model = "gpt-5.6-sol"\nmodel_reasoning_effort = "high"\n'
                writer.write_text(pinned)
                alias = writer.with_name(f"{LEGACY_ALIAS}.toml")
                alias.write_text(pinned)
                config = home / "config.toml"
                registration = f"\n[agents.{LEGACY_ALIAS}]\nconfig_file = {json.dumps(alias.as_posix())}\n"
                config.write_bytes(config.read_bytes().replace(
                    setup.TABLE_END, registration.encode("ascii") + setup.TABLE_END, 1,
                ))
                yield plugin, home, fake

    def test_upgrade_preserves_runtime_config_and_recovery_and_repeats_without_changes(self):
        for version in ("v1", "v2"):
            with self.subTest(runtime=version), self.installed_legacy(version) as (plugin, home, fake):
                config, catalog = setup.managed(home, version)
                config.write_bytes(config.read_bytes().replace(b"max_threads = 12", b"max_threads = 17"))
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
            preview = migration.preview(setup, plan)
            self.assertEqual([str(config)], preview["changed_files"])
            self.assertIn(LEGACY_ALIAS, preview["profile_changes"]["removed"])
            migration.apply(setup, plan, plan["approval_digest"])
            self.assertEqual(original_catalog, catalog.read_bytes())
            roles = tomllib.loads(config.read_text())["agents"]
            self.assertNotIn(LEGACY_ALIAS, roles)
            definition = tomllib.loads(Path(roles[WRITER]["config_file"]).read_text())
            self.assertTrue(definition["developer_instructions"])
            self.assertFalse({"model", "model_reasoning_effort"} & definition.keys())

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
            # Native Windows checkouts use CRLF; setup and migration must agree on output.
            fragment = plugin / "config/config-fragment.toml.in"
            fragment.write_bytes(fragment.read_text(encoding="ascii").replace("\n", "\r\n").encode("ascii"))
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
            with mock.patch.dict(sys.modules, {setup.__name__: setup}), contextlib.redirect_stdout(io.StringIO()) as output:
                with mock.patch.object(sys, "argv", args):
                    self.assertEqual(0, setup.main())
                digest = json.JSONDecoder().raw_decode(output.getvalue())[0]["approval_digest"]
                with mock.patch.object(sys, "argv", [*args, "--apply"]):
                    self.assertEqual(1, setup.main())
                self.assertEqual(original, files(home))
                with mock.patch.object(sys, "argv", [*args, "--apply", "--expect-digest", digest]):
                    self.assertEqual(0, setup.main())

    def test_catalog_alias_resolves_to_owned_file_without_rewriting_the_selected_path(self):
        with self.installed_legacy() as (plugin, home, fake):
            alias = home.with_name("codex-home-alias")
            try:
                alias.symlink_to(home, target_is_directory=True)
            except OSError:
                self.skipTest("directory symlinks are unavailable")
            config, catalog = setup.managed(home)
            selected = str(alias / catalog.name)
            current = config.read_text().replace(json.dumps(str(catalog)), json.dumps(selected))
            config.write_text(current)
            plan = migration.prepare(setup, plugin, home, "codex")
            migration.apply(setup, plan, plan["approval_digest"])
            self.assertEqual(selected, tomllib.loads(config.read_text())["model_catalog_json"])

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

if __name__ == "__main__":
    unittest.main()
