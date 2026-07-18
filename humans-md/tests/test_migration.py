from __future__ import annotations
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
def load(path: Path, name: str):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec); assert spec and spec.loader
    spec.loader.exec_module(module); return module
core = load(ROOT / "humans-md/adapters/codex/scripts/setup-codex.py", "core_setup_test")
migrate = load(ROOT / "humans-md/adapters/codex/scripts/migrate-v0.1.5.py", "migration_test")

class MigrationTests(unittest.TestCase):
    def fixture(self, root: Path):
        plugin = root / "plugin"; (plugin / ".codex-plugin").mkdir(parents=True); (plugin / "templates").mkdir()
        (plugin / ".codex-plugin/plugin.json").write_text('{"name":"humans-md","version":"0.2.0"}\n', encoding="ascii")
        (plugin / "templates/AGENTS.md").write_text("# core\n", encoding="ascii")
        home = root / "home"; home.mkdir(); (home / "config.toml").write_text('# >>> humans-md setup scalars >>>\nmodel_catalog_json = "legacy"\n# <<< humans-md setup scalars <<<\nbase = "keep"\n\n# >>> humans-md setup tables >>>\n[features]\nmulti_agent = true\n# <<< humans-md setup tables <<<\n', encoding="ascii")
        (home / "AGENTS.md").write_text("# legacy\n", encoding="ascii"); (home / "models-humans-md-v1.json").write_text("{}\n", encoding="ascii")
        receipt_dir = home / "backups/humans-md/legacy"; (receipt_dir / "before").mkdir(parents=True)
        before=[]
        for relative in migrate.LEGACY_PATHS:
            existed = relative in {"config.toml", "AGENTS.md"}; before.append({"path":relative,"existed":existed})
            if existed:
                target=receipt_dir / "before" / relative; target.parent.mkdir(parents=True,exist_ok=True)
                target.write_text('base = "original"\n' if relative=="config.toml" else "# original\n",encoding="ascii")
        receipt={"schema_version":2,"status":"installed","plugin_version":"0.1.5","before":before,"remove_plugin":True,"remove_marketplace":True}
        (receipt_dir / "receipt.json").write_text(json.dumps(receipt),encoding="ascii")
        (home / "state/humans-md").mkdir(parents=True); (home / "state/humans-md/current.json").write_text(json.dumps({"receipt":str(receipt_dir / "receipt.json")}),encoding="ascii")
        return plugin, home
    def checked(self, args, environment):
        if args[1:3] == ["plugin", "list"]: return {"installed":[{"pluginId":"humans-md@humans-md","installed":True,"enabled":True,"version":"0.2.0"}]}
        return {"marketplaces":[{"name":"humans-md"}]}
    def test_supported_migration_restores_then_reseeds_without_marketplace_removal(self):
        with tempfile.TemporaryDirectory() as td:
            plugin,home=self.fixture(Path(td))
            with mock.patch.object(migrate.core,"checked_json",side_effect=self.checked):
                result=migrate.apply(home,plugin,"codex")
            self.assertEqual("migrated",result["status"]); self.assertTrue(result["marketplace_preserved"])
            self.assertIn('base = "keep"', (home/"config.toml").read_text(encoding="ascii"))
            self.assertEqual("# core\n",(home/"AGENTS.md").read_text(encoding="ascii"))
            receipt=json.loads((Path(json.loads(core.pointer(home).read_bytes())["receipt"])).read_bytes())
            self.assertEqual(4,receipt["schema_version"]); self.assertFalse(receipt["remove_marketplace"])
    def test_failed_reseed_restores_active_legacy_receipt_exactly(self):
        with tempfile.TemporaryDirectory() as td:
            plugin,home=self.fixture(Path(td)); pointer=core.pointer(home).read_bytes(); config=(home/"config.toml").read_bytes()
            with mock.patch.object(migrate.core,"checked_json",side_effect=self.checked), mock.patch.object(migrate.core,"install",side_effect=core.SetupError("injected")):
                with self.assertRaisesRegex(migrate.MigrationError,"rollback verified"): migrate.apply(home,plugin,"codex")
            self.assertEqual(pointer,core.pointer(home).read_bytes()); self.assertEqual(config,(home/"config.toml").read_bytes())
if __name__ == "__main__": unittest.main()

class LegacyReceiptShapeTests(MigrationTests):
    def test_schema_three_and_missing_original_are_reseeded_without_adoption(self):
        with tempfile.TemporaryDirectory() as td:
            plugin, home = self.fixture(Path(td))
            receipt_path = Path(json.loads(core.pointer(home).read_bytes())["receipt"])
            receipt = json.loads(receipt_path.read_bytes())
            receipt["schema_version"] = 3
            agent = next(entry for entry in receipt["before"] if entry["path"] == "AGENTS.md")
            agent["existed"] = False
            (receipt_path.parent / "before/AGENTS.md").unlink()
            receipt_path.write_text(json.dumps(receipt), encoding="ascii")
            with mock.patch.object(migrate.core, "checked_json", side_effect=self.checked):
                migrate.apply(home, plugin, "codex")
            fresh = Path(json.loads(core.pointer(home).read_bytes())["receipt"])
            current = json.loads(fresh.read_bytes())
            self.assertFalse(next(item for item in current["before"] if item["path"] == "AGENTS.md")["existed"])
    def test_unsafe_legacy_inventory_is_rejected_before_mutation(self):
        with tempfile.TemporaryDirectory() as td:
            plugin, home = self.fixture(Path(td))
            receipt_path = Path(json.loads(core.pointer(home).read_bytes())["receipt"])
            receipt = json.loads(receipt_path.read_bytes()); receipt["before"][0]["path"] = "../config.toml"
            receipt_path.write_text(json.dumps(receipt), encoding="ascii")
            before = (home / "config.toml").read_bytes()
            with self.assertRaisesRegex(migrate.core.SetupError, "unsafe receipt path"):
                migrate.legacy_receipt(home)
            self.assertEqual(before, (home / "config.toml").read_bytes())

claude = load(ROOT / "humans-md/adapters/claude/scripts/migrate-v0.1.5-claude.py", "claude_migration_test")
class ClaudeMigrationTests(unittest.TestCase):
    def fixture(self, root: Path, existed: bool = True):
        plugin=root/"plugin"; (plugin/"templates").mkdir(parents=True); (plugin/"templates/AGENTS.md").write_text("# core\n",encoding="ascii")
        config=root/"claude"; (config/"backups/humans-md/claude").mkdir(parents=True)
        if existed:
            (config/"CLAUDE.md").write_text("# modified\n",encoding="ascii")
            (config/"backups/humans-md/claude/CLAUDE.md.before").write_text("# original\n",encoding="ascii")
        else:
            (config/"backups/humans-md/claude/CLAUDE.md.was-missing").write_text("\n",encoding="ascii")
        return plugin,config
    def test_backup_state_is_retired_and_core_receipt_reseeded(self):
        with tempfile.TemporaryDirectory() as td:
            plugin,config=self.fixture(Path(td)); result=claude.apply(config,plugin)
            self.assertEqual("migrated",result["status"]); self.assertEqual("# core\n",(config/"CLAUDE.md").read_text(encoding="ascii"))
            self.assertTrue((config/"backups/humans-md/claude/CLAUDE.md.before").is_file())
            self.assertTrue(Path(result["retired_legacy_receipt"]).is_dir())
    def test_missing_original_reseeds_missing_marker(self):
        with tempfile.TemporaryDirectory() as td:
            plugin,config=self.fixture(Path(td),existed=False); claude.apply(config,plugin)
            root=config/"backups/humans-md/claude"
            self.assertTrue((root/"CLAUDE.md.was-missing").is_file()); self.assertFalse((root/"CLAUDE.md.before").exists())
    def test_ambiguous_backup_is_rejected_without_changes(self):
        with tempfile.TemporaryDirectory() as td:
            plugin,config=self.fixture(Path(td)); root=config/"backups/humans-md/claude"; (root/"CLAUDE.md.was-missing").write_text("\n",encoding="ascii")
            with self.assertRaisesRegex(claude.MigrationError,"no supported"):
                claude.preview(config,plugin)
            self.assertEqual("# modified\n",(config/"CLAUDE.md").read_text(encoding="ascii"))
    def test_apply_failure_restores_claude_file_and_active_legacy_receipt(self):
        with tempfile.TemporaryDirectory() as td:
            plugin,config=self.fixture(Path(td)); original=(config/"CLAUDE.md").read_bytes(); legacy=config/"backups/humans-md/claude/CLAUDE.md.before"
            original_write=claude.atomic_write
            def fail_core(path, data):
                if path == config/"CLAUDE.md" and data == b"# core\n": raise OSError("injected")
                return original_write(path,data)
            with mock.patch.object(claude,"atomic_write",side_effect=fail_core):
                with self.assertRaisesRegex(claude.MigrationError,"rollback verified"): claude.apply(config,plugin)
            self.assertEqual(original,(config/"CLAUDE.md").read_bytes()); self.assertEqual(b"# original\n",legacy.read_bytes())
