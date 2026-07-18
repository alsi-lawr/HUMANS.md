from __future__ import annotations
import importlib.util, json, tempfile, unittest
from pathlib import Path
from unittest import mock
ROOT=Path(__file__).resolve().parents[2]
def load(path,name):
 spec=importlib.util.spec_from_file_location(name,path); module=importlib.util.module_from_spec(spec); assert spec and spec.loader; spec.loader.exec_module(module); return module
core=load(ROOT/"humans-md/adapters/codex/scripts/setup-codex.py","core_setup_test")
migrate=load(ROOT/"humans-md/adapters/codex/scripts/migrate-v0.1.5.py","codex_migration_test")
claude_setup=load(ROOT/"humans-md/adapters/claude/scripts/setup-claude.py","claude_setup_test")
claude=load(ROOT/"humans-md/adapters/claude/scripts/migrate-v0.1.5-claude.py","claude_migration_test")
class CodexMigrationTests(unittest.TestCase):
 def fixture(self,root):
  plugin=root/"plugin"; (plugin/".codex-plugin").mkdir(parents=True); (plugin/"templates").mkdir(); (plugin/".codex-plugin/plugin.json").write_text('{"name":"humans-md","version":"0.2.0"}\n',encoding="ascii"); (plugin/"templates/AGENTS.md").write_text("# core\n",encoding="ascii")
  home=root/"home"; home.mkdir(); (home/"config.toml").write_text('# >>> humans-md setup scalars >>>\nmodel_catalog_json = "legacy"\n# <<< humans-md setup scalars <<<\nbase = "keep"\n\n# >>> humans-md setup tables >>>\n[features]\nmulti_agent = true\n# <<< humans-md setup tables <<<\n',encoding="ascii"); (home/"AGENTS.md").write_text("# legacy\n",encoding="ascii"); (home/"models-humans-md-v1.json").write_text("{}\n",encoding="ascii")
  receipt_dir=home/"backups/humans-md/legacy"; (receipt_dir/"before").mkdir(parents=True); before=[]
  for relative in migrate.LEGACY_PATHS:
   existed=relative in {"config.toml","AGENTS.md"}; before.append({"path":relative,"existed":existed})
   if existed:
    target=receipt_dir/"before"/relative; target.parent.mkdir(parents=True,exist_ok=True); target.write_text('base = "original"\n' if relative=="config.toml" else "# original\n",encoding="ascii")
  receipt={"schema_version":2,"status":"installed","plugin_version":"0.1.5","before":before,"remove_plugin":True,"remove_marketplace":True}; (receipt_dir/"receipt.json").write_text(json.dumps(receipt),encoding="ascii"); (home/"state/humans-md").mkdir(parents=True); (home/"state/humans-md/current.json").write_text(json.dumps({"receipt":str(receipt_dir/"receipt.json")}),encoding="ascii")
  return plugin,home
 def checked(self,args,environment):
  if args[1:3]==["plugin","list"]: return {"installed":[{"pluginId":"humans-md@humans-md","installed":True,"enabled":True,"version":"0.2.0"}]}
  return {"marketplaces":[{"name":"humans-md"}]}
 def test_reseed_binds_approval_and_preserves_marketplace(self):
  with tempfile.TemporaryDirectory() as td:
   plugin,home=self.fixture(Path(td))
   with mock.patch.object(migrate.core,"checked_json",side_effect=self.checked):
    plan,*_=migrate.preview(home,plugin,"codex"); result=migrate.apply(home,plugin,"codex",plan["approval_fingerprint"])
   self.assertEqual("migrated",result["status"]); self.assertEqual("# core\n",(home/"AGENTS.md").read_text(encoding="ascii")); self.assertIn('base = "keep"',(home/"config.toml").read_text(encoding="ascii"))
 def test_changed_target_rejects_stale_approval_without_mutation(self):
  with tempfile.TemporaryDirectory() as td:
   plugin,home=self.fixture(Path(td))
   with mock.patch.object(migrate.core,"checked_json",side_effect=self.checked):
    plan,*_=migrate.preview(home,plugin,"codex"); (home/"AGENTS.md").write_text("# changed\n",encoding="ascii")
    with self.assertRaisesRegex(migrate.MigrationError,"stale approval"): migrate.apply(home,plugin,"codex",plan["approval_fingerprint"])
   self.assertEqual("# changed\n",(home/"AGENTS.md").read_text(encoding="ascii")); self.assertTrue(migrate.core.pointer(home).exists())
 def test_unsafe_legacy_inventory_is_rejected(self):
  with tempfile.TemporaryDirectory() as td:
   _,home=self.fixture(Path(td)); path=Path(json.loads(core.pointer(home).read_bytes())["receipt"]); receipt=json.loads(path.read_bytes()); receipt["before"][0]["path"]="../config.toml"; path.write_text(json.dumps(receipt),encoding="ascii")
   with self.assertRaisesRegex(migrate.core.SetupError,"unsafe receipt path"): migrate.legacy_receipt(home)
class ClaudeMigrationTests(unittest.TestCase):
 def fixture(self,root,existed=True):
  plugin=root/"plugin"; (plugin/"templates").mkdir(parents=True); (plugin/"templates/AGENTS.md").write_text("# core\n",encoding="ascii"); config=root/"claude"; legacy=config/"backups/humans-md/claude"; legacy.mkdir(parents=True)
  if existed: (config/"CLAUDE.md").write_text("# modified\n",encoding="ascii"); (legacy/"CLAUDE.md.before").write_text("# original\n",encoding="ascii")
  else: (legacy/"CLAUDE.md.was-missing").write_text("\n",encoding="ascii")
  return plugin,config
 def test_migration_retires_legacy_and_creates_distinct_core_state(self):
  with tempfile.TemporaryDirectory() as td:
   plugin,config=self.fixture(Path(td)); plan,*_=claude.preview(config,plugin); result=claude.apply(config,plugin,plan["approval_fingerprint"])
   self.assertEqual("# core\n",(config/"CLAUDE.md").read_text(encoding="ascii")); self.assertTrue(Path(result["retired_legacy_receipt"]).is_dir()); self.assertTrue(claude_setup.pointer(config).is_file()); self.assertTrue(Path(result["fresh_receipt"]).is_file())
   with self.assertRaisesRegex(claude.MigrationError,"v0.2.0|no supported"): claude.preview(config,plugin)
 def test_changed_target_rejects_stale_approval(self):
  with tempfile.TemporaryDirectory() as td:
   plugin,config=self.fixture(Path(td)); plan,*_=claude.preview(config,plugin); (config/"CLAUDE.md").write_text("# changed\n",encoding="ascii")
   with self.assertRaisesRegex(claude.MigrationError,"stale approval"): claude.apply(config,plugin,plan["approval_fingerprint"])
   self.assertTrue((config/"backups/humans-md/claude").is_dir())
 def test_failure_after_retirement_restores_exact_legacy_state_without_orphan(self):
  with tempfile.TemporaryDirectory() as td:
   plugin,config=self.fixture(Path(td)); plan,*_=claude.preview(config,plugin); original=(config/"CLAUDE.md").read_bytes()
   with mock.patch.object(claude.core,"install",side_effect=claude.core.SetupError("injected")):
    with self.assertRaisesRegex(claude.MigrationError,"rollback verified"): claude.apply(config,plugin,plan["approval_fingerprint"])
   self.assertEqual(original,(config/"CLAUDE.md").read_bytes()); self.assertTrue((config/"backups/humans-md/claude/CLAUDE.md.before").is_file()); self.assertFalse((config/"backups/humans-md/claude-v0.1.5-retired").exists()); self.assertFalse(claude_setup.pointer(config).exists())
 def test_missing_original_reseeds_missing_record(self):
  with tempfile.TemporaryDirectory() as td:
   plugin,config=self.fixture(Path(td),False); plan,*_=claude.preview(config,plugin); result=claude.apply(config,plugin,plan["approval_fingerprint"]); receipt=json.loads(Path(result["fresh_receipt"]).read_bytes()); self.assertEqual("missing",receipt["before"])
if __name__=="__main__": unittest.main()

class ClaudeReceiptSafetyTests(unittest.TestCase):
    def plugin(self, root: Path) -> Path:
        plugin = root / "plugin"
        (plugin / "templates").mkdir(parents=True)
        (plugin / "templates/AGENTS.md").write_text("# core\n", encoding="ascii")
        return plugin

    def test_core_setup_rejects_symlink_target_without_receipt_mutation(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            plugin = self.plugin(root)
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

    def test_legacy_missing_marker_must_be_regular_non_symlink_file(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            plugin = self.plugin(root)
            config = root / "claude"
            legacy = config / "backups/humans-md/claude"
            legacy.mkdir(parents=True)
            marker = legacy / "CLAUDE.md.was-missing"
            target = root / "marker-target"
            target.write_text("\n", encoding="ascii")
            marker.symlink_to(target)
            with self.assertRaisesRegex(claude.MigrationError, "unsafe or ambiguous"):
                claude.preview(config, plugin)
            marker.unlink()
            marker.mkdir()
            with self.assertRaisesRegex(claude.MigrationError, "unsafe or ambiguous"):
                claude.preview(config, plugin)

    def test_legacy_migration_rejects_non_regular_live_target_without_mutation(self):
        for kind in ("symlink", "directory"):
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                plugin = self.plugin(root)
                config = root / "claude"
                legacy = config / "backups/humans-md/claude"
                legacy.mkdir(parents=True)
                (legacy / "CLAUDE.md.before").write_text("# original\n", encoding="ascii")
                target = config / "CLAUDE.md"
                if kind == "symlink":
                    referent = root / "outside.md"
                    referent.write_text("# live\n", encoding="ascii")
                    target.symlink_to(referent)
                else:
                    target.mkdir()
                before_receipt = (legacy / "CLAUDE.md.before").read_bytes()
                with self.assertRaisesRegex(claude.MigrationError, "unsafe live Claude target"):
                    claude.preview(config, plugin)
                with self.assertRaisesRegex(claude.MigrationError, "unsafe live Claude target"):
                    claude.apply(config, plugin, "any-fingerprint")
                self.assertEqual(before_receipt, (legacy / "CLAUDE.md.before").read_bytes())
                self.assertFalse((config / "backups/humans-md/claude-v0.1.5-retired").exists())
                self.assertFalse(claude_setup.pointer(config).exists())
                if kind == "symlink":
                    self.assertTrue(target.is_symlink())
                    self.assertEqual("# live\n", referent.read_text(encoding="ascii"))
                else:
                    self.assertTrue(target.is_dir())
