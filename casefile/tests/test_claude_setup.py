from __future__ import annotations

import hashlib
import json
import tempfile
import unittest
from pathlib import Path

from _load import ROOT, script

setup = script("casefile/adapters/claude/scripts/setup-claude.py")
TARGETS = sorted(set(setup.casefile_runtime.TARGETS.values()))


class ClaudeSetupTests(unittest.TestCase):
    def test_host_target_normalizes_supported_vendor_spellings(self):
        self.assertEqual("x86_64-unknown-linux-musl", setup.casefile_runtime.host_target("Linux", "AMD64"))
        self.assertEqual("aarch64-unknown-linux-musl", setup.casefile_runtime.host_target("Linux", "arm64"))
        self.assertEqual("aarch64-apple-darwin", setup.casefile_runtime.host_target("Darwin", "aarch64"))
        self.assertEqual("x86_64-pc-windows-msvc", setup.casefile_runtime.host_target("Windows", "x86_64"))

    def fixture(self, root: Path):
        plugin = root / "plugin"
        (plugin / ".claude-plugin").mkdir(parents=True)
        (plugin / ".claude-plugin/plugin.json").write_text(json.dumps({"name":"casefile","version":"0.4.0"})+"\n", encoding="ascii")
        runtime_source = (
            "#!/usr/bin/env python3\nimport json,sys\n"
            "ops=" + repr(sorted(setup.casefile_runtime.REQUIRED_OPERATIONS)) + "\n"
            "if sys.argv[1:] == ['--version']: print('casefile 0.4.0')\n"
            "elif sys.argv[1:] == ['mcp-compatibility']: print(json.dumps({'identity':'casefile','provider_protocol_version':1,'required_provider_operations':ops}))\n"
            "elif sys.argv[1:2] == ['mcp-package']:\n"
            " for row in [json.loads(line) for line in sys.stdin if line.strip()]:\n"
            "  result={'serverInfo':{'name':'casefile'}} if row['method']=='initialize' else {'tools':[{} for _ in range(12)]}\n"
            "  print(json.dumps({'jsonrpc':'2.0','id':row['id'],'result':result}))\n"
            "else: raise SystemExit(2)\n"
        ).encode("ascii")
        rows=[]
        for target in TARGETS:
            name="casefile.exe" if target.endswith("windows-msvc") else "casefile"
            path=plugin/"runtime/bin"/target/name
            path.parent.mkdir(parents=True, exist_ok=True); path.write_bytes(runtime_source); path.chmod(0o755)
            rows.append({"path":path.relative_to(plugin/"runtime").as_posix(),"sha256":hashlib.sha256(runtime_source).hexdigest(),"size":len(runtime_source),"target":target})
        (plugin/"runtime/artifacts.json").write_text(json.dumps({"schema_version":1,"version":"0.4.0","source_commit":"1"*40,"artifacts":rows},indent=2,sort_keys=True)+"\n",encoding="ascii")
        planning=root/"planning"; planning.mkdir(); (planning/"casefile.toml").write_text("schema_version = 1\n",encoding="ascii"); (planning/"projects.toml").write_text("schema_version = 1\nprojects = []\n",encoding="ascii")
        home=root/"claude-home"; home.mkdir()
        claude=root/"claude"
        claude.write_text("""#!/usr/bin/env python3
import json,os,pathlib,sys
state=pathlib.Path(os.environ['CLAUDE_CONFIG_DIR'])/'fake-mcp.json'
args=sys.argv[1:]
if args[:3]==['mcp','add','--scope']:
 i=args.index('--'); value={'command':args[i+1],'args':args[i+2:]}
 if (state.parent/'corrupt-add').exists(): value={'command':'/wrong','args':[]}
 state.write_text(json.dumps(value)); print('added')
elif args[:2]==['mcp','get']:
 if not state.exists(): raise SystemExit(1)
 print(state.read_text())
elif args[:3]==['mcp','remove','--scope']:
 if (state.parent/'fail-remove').exists(): raise SystemExit(3)
 state.unlink(missing_ok=True); print('removed')
else: raise SystemExit(2)
""",encoding="ascii"); claude.chmod(0o755)
        return plugin, planning, home, claude

    def test_fresh_install_binds_exact_binary_and_uninstall_preserves_unrelated(self):
        with tempfile.TemporaryDirectory() as temporary:
            plugin, planning, home, claude = self.fixture(Path(temporary))
            unrelated=home/"unrelated.json"; unrelated.write_text("keep\n",encoding="ascii")
            plan=setup.prepare(plugin,home,str(claude),planning)
            self.assertFalse(setup.pointer(home).exists())
            result=setup.install(plan)
            receipt=json.loads(Path(result["receipt"]).read_text())
            binary=Path(receipt["binary"])
            self.assertTrue(binary.is_file())
            binding=json.loads((home/"fake-mcp.json").read_text())
            self.assertEqual(str(binary),binding["command"])
            self.assertEqual(["mcp-package","--planning-root",str(planning)],binding["args"])
            self.assertEqual("preview",setup.uninstall(home,str(claude),False)["status"])
            self.assertEqual("uninstalled",setup.uninstall(home,str(claude),True)["status"])
            self.assertFalse(binary.exists()); self.assertEqual("keep\n",unrelated.read_text())

    def test_unowned_binding_and_tampered_matrix_refuse_before_mutation(self):
        with tempfile.TemporaryDirectory() as temporary:
            plugin, planning, home, claude = self.fixture(Path(temporary))
            (home/"fake-mcp.json").write_text('{}',encoding="ascii")
            with self.assertRaisesRegex(setup.SetupError,"unowned"):
                setup.prepare(plugin,home,str(claude),planning)
            self.assertFalse((home/"casefile").exists())

    def test_registration_verification_failure_removes_binding_and_copied_binary(self):
        with tempfile.TemporaryDirectory() as temporary:
            plugin, planning, home, claude = self.fixture(Path(temporary))
            (home / "corrupt-add").touch()
            plan = setup.prepare(plugin, home, str(claude), planning)
            with self.assertRaisesRegex(setup.SetupError, "rollback attempted"):
                setup.install(plan)
            self.assertFalse((home / "fake-mcp.json").exists())
            self.assertFalse(plan["binary"].exists())
            self.assertFalse(setup.pointer(home).exists())

    def test_uninstall_failure_restores_binding_binary_and_pointer(self):
        with tempfile.TemporaryDirectory() as temporary:
            plugin, planning, home, claude = self.fixture(Path(temporary))
            result = setup.install(setup.prepare(plugin, home, str(claude), planning))
            receipt = json.loads(Path(result["receipt"]).read_text())
            binary = Path(receipt["binary"])
            (home / "fail-remove").touch()
            with self.assertRaisesRegex(setup.SetupError, "rollback verified"):
                setup.uninstall(home, str(claude), True)
            self.assertTrue(binary.is_file())
            self.assertTrue(setup.pointer(home).is_file())
            binding = json.loads((home / "fake-mcp.json").read_text())
            self.assertEqual(str(binary), binding["command"])

    def test_tampered_matrix_refuses_before_mutation(self):
        with tempfile.TemporaryDirectory() as temporary:
            plugin, planning, home, claude = self.fixture(Path(temporary))
            target=setup.casefile_runtime.host_target(); row=next(r for r in json.loads((plugin/"runtime/artifacts.json").read_text())["artifacts"] if r["target"]==target)
            (plugin/"runtime"/row["path"]).write_bytes(b"tampered")
            with self.assertRaisesRegex(setup.casefile_runtime.RuntimeError,"size|hash"):
                setup.prepare(plugin,home,str(claude),planning)
            self.assertFalse((home/"casefile").exists())


if __name__ == "__main__": unittest.main()
