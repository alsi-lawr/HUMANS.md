from __future__ import annotations

import json
import os
import sys
import struct
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from _load import ROOT, script

setup = script("casefile/adapters/claude/scripts/setup-claude.py")
TARGETS = sorted(set(setup.casefile_runtime.TARGETS.values()))


def native_stub(target: str) -> bytes:
    if target.endswith("linux-musl"):
        data = bytearray(64)
        data[:4] = b"\x7fELF"
        data[4:6] = b"\x02\x01"
        struct.pack_into("<H", data, 18, 183 if target.startswith("aarch64") else 62)
    elif target.endswith("darwin"):
        data = bytearray(64)
        data[:4] = b"\xcf\xfa\xed\xfe"
        struct.pack_into("<I", data, 4, 0x0100000C if target.startswith("aarch64") else 0x01000007)
    else:
        data = bytearray(128)
        data[:2] = b"MZ"
        struct.pack_into("<I", data, 0x3C, 64)
        data[64:68] = b"PE\0\0"
        struct.pack_into("<H", data, 68, 0xAA64 if target.startswith("aarch64") else 0x8664)
    return bytes(data)


class ClaudeSetupTests(unittest.TestCase):
    def setUp(self):
        self.probe = mock.patch.object(setup.casefile_runtime, "probe", return_value=None)
        self.probe.start()

    def tearDown(self):
        self.probe.stop()

    def test_host_target_normalizes_supported_vendor_spellings(self):
        self.assertEqual("x86_64-unknown-linux-musl", setup.casefile_runtime.host_target("Linux", "AMD64"))
        self.assertEqual("aarch64-unknown-linux-musl", setup.casefile_runtime.host_target("Linux", "arm64"))
        self.assertEqual("aarch64-apple-darwin", setup.casefile_runtime.host_target("Darwin", "aarch64"))
        self.assertEqual("x86_64-pc-windows-msvc", setup.casefile_runtime.host_target("Windows", "x86_64"))

    def test_binding_verification_is_windows_portable_and_semantically_exact(self):
        binary = r"C:\Users\Case File\.claude\casefile\runtime\casefile.exe"
        planning = r"C:\Users\Case File\planning root"
        human = "\n".join([
            "casefile:",
            "  Type: stdio",
            "  Command: c:/Users/Case File/.claude/casefile/runtime/casefile.exe",
            "  Args: mcp-package --planning-root c:/Users/Case File/planning root",
            "  Environment:",
        ])
        self.assertTrue(setup.binding_matches(human, binary, planning))
        encoded = json.dumps({
            "command": r"c:\Users\Case File\.claude\casefile\runtime\casefile.exe",
            "args": ["mcp-package", "--planning-root", r"c:\Users\Case File\planning root"],
        })
        self.assertTrue(setup.binding_matches(encoded, binary, planning))
        for unsafe in (
            human.replace("casefile.exe", "other.exe"),
            human.replace("mcp-package", "serve"),
            human.replace("  Environment:", "  Command: c:/duplicate.exe\n  Environment:"),
        ):
            self.assertFalse(setup.binding_matches(unsafe, binary, planning))
        plan = {
            "binary": Path(binary),
            "planning_root": Path(planning),
            "executable": "claude",
            "environment": {},
        }
        with (
            mock.patch.object(setup, "current_binding", return_value=human),
            mock.patch.object(setup, "checked") as checked,
        ):
            setup.register(plan)
            checked.assert_not_called()

    def fixture(self, root: Path):
        plugin = root / "plugin"
        (plugin / ".claude-plugin").mkdir(parents=True)
        (plugin / ".claude-plugin/plugin.json").write_text(json.dumps({"name":"casefile","version":"0.4.0"})+"\n", encoding="ascii")
        (plugin / "matrices").mkdir(parents=True)
        (plugin / "matrices/casefile-review-dialogue.toml").write_text(
            'schema_version = 1\nstrategy_id = "casefile-review-dialogue"\n'
            '[limits]\nmax_depth = 2\n', encoding="ascii")
        (plugin / "matrices/casefile-investigate-solo.toml").write_text(
            'schema_version = 1\nstrategy_id = "casefile-investigate-solo"\n'
            '[limits]\nmax_depth = 0\n', encoding="ascii")
        rows=[]
        for target in TARGETS:
            name="casefile.exe" if target.endswith("windows-msvc") else "casefile"
            path=plugin/"runtime/bin"/target/name
            runtime_source = native_stub(target)
            path.parent.mkdir(parents=True, exist_ok=True); path.write_bytes(runtime_source); path.chmod(0o755)
            rows.append({"path": path.relative_to(plugin / "runtime").as_posix().replace("/", "\\") + "///", "sha256": "representation-only", "size": -1, "target": target})
        (plugin / "runtime/artifacts.json").write_bytes((json.dumps({"artifacts": rows, "version": "0.4.0", "source_commit": "1" * 40, "schema_version": 1}, separators=(", ", ": ")) + "\r\n").encode("ascii"))
        planning=root/"planning"; planning.mkdir(); (planning/"casefile.toml").write_text("schema_version = 1\n",encoding="ascii"); (planning/"projects.toml").write_text("schema_version = 1\nprojects = []\n",encoding="ascii")
        home=root/"claude-home"; home.mkdir()
        script = root / "claude.py"
        script.write_text("""#!/usr/bin/env python3
import json,os,pathlib,sys
state=pathlib.Path(os.environ['CLAUDE_CONFIG_DIR'])/'.claude.json'
args=sys.argv[1:]
if args[:3]==['mcp','add','--scope']:
 if state.exists():
  print('MCP server casefile already exists in user config',file=sys.stderr); raise SystemExit(1)
 i=args.index('--'); value={'command':args[i+1],'args':args[i+2:]}
 if (state.parent/'corrupt-add').exists(): value={'command':'/wrong','args':[]}
 state.write_text(json.dumps(value,indent=2)+'\\r\\n'); print('added')
elif args[:2]==['mcp','get']:
 if not state.exists(): raise SystemExit(1)
 print(state.read_text())
elif args[:3]==['mcp','remove','--scope']:
 if (state.parent/'fail-remove').exists(): raise SystemExit(3)
 state.unlink(missing_ok=True); print('removed')
else: raise SystemExit(2)
""", encoding="ascii")
        script.chmod(0o755)
        if os.name == "nt":
            claude = root / "claude.cmd"
            claude.write_text(f'@"{sys.executable}" "{script}" %*\r\n', encoding="ascii")
        else:
            claude = script
        return plugin, planning, home, claude

    def test_fresh_install_binds_exact_binary_and_uninstall_preserves_unrelated(self):
        with tempfile.TemporaryDirectory() as temporary:
            plugin, planning, home, claude = self.fixture(Path(temporary))
            unrelated=home/"unrelated.json"; unrelated.write_text("keep\n",encoding="ascii")
            (home/"settings.json").write_text(
                json.dumps({"theme":"dark","env":{"OTHER":"keep"}})+"\n", encoding="ascii")
            plan=setup.prepare(plugin,home,str(claude),planning)
            self.assertFalse(setup.pointer(home).exists())
            result=setup.install(plan)
            receipt=json.loads(Path(result["receipt"]).read_text())
            binary=Path(receipt["binary"])
            self.assertTrue(binary.is_file())
            binding=json.loads((home/".claude.json").read_text())
            self.assertEqual(str(binary),binding["command"])
            self.assertEqual(str(plan["planning_root"]),receipt["planning_root"])
            self.assertEqual(
                ["mcp-package","--planning-root",str(plan["planning_root"])],
                binding["args"],
            )
            # Depth ceiling comes from the deepest matrix the plugin ships, not a constant.
            self.assertEqual(2,receipt["subagent_spawn_depth"])
            self.assertIsNone(receipt["subagent_spawn_depth_before"])
            settings=json.loads((home/"settings.json").read_text())
            self.assertEqual("2",settings["env"]["CLAUDE_CODE_MAX_SUBAGENT_SPAWN_DEPTH"])
            self.assertEqual("keep",settings["env"]["OTHER"])
            self.assertEqual("dark",settings["theme"])
            self.assertEqual("preview",setup.uninstall(home,str(claude),False)["status"])
            self.assertEqual("uninstalled",setup.uninstall(home,str(claude),True)["status"])
            self.assertFalse(binary.exists()); self.assertEqual("keep\n",unrelated.read_text())
            # Uninstall removes the key it added and leaves unrelated settings untouched.
            settings=json.loads((home/"settings.json").read_text())
            self.assertNotIn("CLAUDE_CODE_MAX_SUBAGENT_SPAWN_DEPTH",settings.get("env",{}))
            self.assertEqual("keep",settings["env"]["OTHER"])
            self.assertEqual("dark",settings["theme"])

    def test_overwrite_reinstalls_and_keeps_the_pre_casefile_depth(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary); plugin,planning,home,claude=self.fixture(root)
            (home/"settings.json").write_text(
                json.dumps({"env":{"CLAUDE_CODE_MAX_SUBAGENT_SPAWN_DEPTH":"7"}})+"\n",
                encoding="ascii")
            first=setup.install(setup.prepare(plugin,home,str(claude),planning))
            self.assertEqual("7",json.loads(Path(first["receipt"]).read_text())["subagent_spawn_depth_before"])

            # A plain reinstall is refused; the occupied binary path is the first gate.
            with self.assertRaisesRegex(setup.SetupError,"already occupied"):
                setup.prepare(plugin,home,str(claude),planning)

            setup.pointer(home).unlink()
            setup.pointer(home).parent.mkdir(parents=True,exist_ok=True)
            atomic=json.dumps({"receipt":first["receipt"]},indent=2,sort_keys=True)+"\n"
            setup.pointer(home).write_text(atomic,encoding="ascii")
            second=setup.install(setup.prepare(plugin,home,str(claude),planning,True))
            value=json.loads(Path(second["receipt"]).read_text())
            # The second receipt carries the host's original depth, not Casefile's own.
            self.assertEqual("7",value["subagent_spawn_depth_before"])
            self.assertEqual(2,value["subagent_spawn_depth"])

    def test_unowned_binding_refuses_before_mutation(self):
        with tempfile.TemporaryDirectory() as temporary:
            plugin, planning, home, claude = self.fixture(Path(temporary))
            (home/".claude.json").write_text('{}',encoding="ascii")
            with self.assertRaisesRegex(setup.SetupError,"unowned"):
                setup.prepare(plugin,home,str(claude),planning)
            self.assertFalse((home/"casefile").exists())

    def test_corrupt_registration_and_remove_failure_restore_absent_fresh_state(self):
        with tempfile.TemporaryDirectory() as temporary:
            plugin, planning, home, claude = self.fixture(Path(temporary))
            (home / "corrupt-add").touch()
            (home / "fail-remove").touch()
            plan = setup.prepare(plugin, home, str(claude), planning)
            with self.assertRaisesRegex(setup.SetupError, "rollback verified"):
                setup.install(plan)
            self.assertFalse((home / ".claude.json").exists())
            self.assertFalse(plan["binary"].exists())
            self.assertFalse(setup.pointer(home).exists())
            failure = next((home / "casefile/receipts").glob("*/failure.json"))
            state = json.loads(failure.read_text(encoding="ascii"))
            self.assertTrue(state["rollback_verified"])
            self.assertFalse(state["binding_present"])
            self.assertFalse(state["binary_present"])
            self.assertFalse(state["pointer_present"])

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
            binding = json.loads((home / ".claude.json").read_text())
            self.assertEqual(str(binary), binding["command"])

    def test_representation_differences_and_non_native_bytes_land(self):
        with tempfile.TemporaryDirectory() as temporary:
            plugin, planning, home, claude = self.fixture(Path(temporary))
            manifest_path = plugin / "runtime/artifacts.json"
            manifest = json.loads(manifest_path.read_text(encoding="ascii"))
            target = setup.casefile_runtime.host_target()
            row = next(item for item in manifest["artifacts"] if item["target"] == target)
            relative = setup.casefile_runtime.normalized_artifact_path(row["path"], target)
            (plugin / "runtime" / relative).write_bytes(b"present non-native artifact\n")
            plan = setup.prepare(plugin, home, str(claude), planning)
            self.assertEqual(target, plan["selected"]["target"])
            self.assertFalse(plan["binary"].exists())
            result = setup.install(plan)
            receipt = json.loads(Path(result["receipt"]).read_text(encoding="ascii"))
            self.assertNotIn("artifact_sha256", receipt)
            self.assertEqual(b"present non-native artifact\n", plan["binary"].read_bytes())

    def test_self_consistent_malformed_matrix_layout_refuses_before_mutation(self):
        with tempfile.TemporaryDirectory() as temporary:
            plugin, planning, home, claude = self.fixture(Path(temporary))
            manifest_path = plugin / "runtime/artifacts.json"
            manifest = json.loads(manifest_path.read_text(encoding="ascii"))
            row = manifest["artifacts"][0]
            source = plugin / "runtime" / setup.casefile_runtime.normalized_artifact_path(
                row["path"], row["target"]
            )
            replacement = plugin / "runtime/bin/unexpected/casefile"
            replacement.parent.mkdir(parents=True)
            source.replace(replacement)
            row["path"] = replacement.relative_to(plugin / "runtime").as_posix()
            manifest_path.write_bytes(setup.casefile_runtime.canonical(manifest))
            with self.assertRaisesRegex(setup.casefile_runtime.RuntimeError, "path is invalid"):
                setup.prepare(plugin, home, str(claude), planning)
            self.assertFalse((home / ".claude.json").exists())
            self.assertFalse((home / "casefile").exists())
            self.assertFalse(setup.pointer(home).exists())


if __name__ == "__main__": unittest.main()
