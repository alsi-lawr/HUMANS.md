from __future__ import annotations

import json
import os
import tempfile
import unittest
from pathlib import Path

from _load import script


cutover = script("adapters/codex/scripts/cutover-codex.py")


class CodexCutoverTests(unittest.TestCase):
    def fixture(self, root: Path):
        plugin = root / "plugin"
        (plugin / ".codex-plugin").mkdir(parents=True)
        (plugin / ".codex-plugin/plugin.json").write_text("{}\n", encoding="ascii")
        marketplace_source = root / "marketplace-source"
        (marketplace_source / ".agents/plugins").mkdir(parents=True)
        (marketplace_source / ".agents/plugins/marketplace.json").write_text(
            "{}\n", encoding="ascii"
        )
        codex_home = root / "codex-home"
        marketplace = codex_home / "plugins"
        marketplace.mkdir(parents=True)
        (marketplace / "state.json").write_text("old marketplace\n", encoding="ascii")
        active = codex_home / "config.toml"
        active.write_text("old config\n", encoding="ascii")
        active.chmod(0o640)
        fixed_mtime = 1_700_000_000_123_456_789
        os.utime(active, ns=(fixed_mtime, fixed_mtime))
        candidate = root / "candidate.toml"
        candidate.write_text("new config\n", encoding="ascii")
        direct_skills = root / "direct-skills"
        direct_agents = root / "direct-agents"
        workflow = root / "workflow"
        for path, value in (
            (direct_skills, "skill"),
            (direct_agents, "agent"),
            (workflow, "workflow"),
        ):
            path.mkdir()
            (path / "old.txt").write_text(value + "\n", encoding="ascii")
        gates = []
        for kind, expected in (
            ("strict_config", ["strict-ok"]),
            ("discovery", ["discovered"]),
            ("v1_runtime", ["v1"]),
            ("root_profile", ["gpt-5.6-sol", "xhigh"]),
            ("inspector_profile", ["gpt-5.6-terra", "xhigh"]),
        ):
            gate = {
                "kind": kind,
                "command": ["fake", kind],
                "expected": expected,
            }
            if kind in cutover.FRESH_GATES:
                gate["fresh_process"] = True
            gates.append(gate)
        document = {
            "schema_version": 1,
            "install_ref": "humans-md@humans-md",
            "codex_executable": "fake-codex",
            "marketplace_source": str(marketplace_source),
            "marketplace_name": "humans-md",
            "marketplace_action": "add",
            "plugin_action": "add",
            "remove_plugin_on_uninstall": True,
            "remove_marketplace_on_uninstall": True,
            "candidate_config": str(candidate),
            "active_config": str(active),
            "codex_home": str(codex_home),
            "managed_paths": [
                {
                    "kind": "active_config",
                    "path": str(active),
                    "remove_after_success": False,
                },
                {
                    "kind": "direct_skills",
                    "path": str(direct_skills),
                    "remove_after_success": True,
                },
                {
                    "kind": "direct_agents",
                    "path": str(direct_agents),
                    "remove_after_success": True,
                },
                {
                    "kind": "workflow_resources",
                    "path": str(workflow),
                    "remove_after_success": True,
                },
                {
                    "kind": "marketplace_state",
                    "path": str(marketplace),
                    "remove_after_success": False,
                },
            ],
            "gates": gates,
            "recovery_gates": [
                {
                    "kind": "strict_config",
                    "command": ["fake", "recovery_strict_config"],
                    "expected": ["recovery-strict-ok"],
                },
                {
                    "kind": "discovery",
                    "command": ["fake", "recovery_discovery"],
                    "expected": ["recovery-discovered"],
                },
            ],
        }
        paths = {
            "active": active,
            "marketplace": marketplace,
            "direct_skills": direct_skills,
            "direct_agents": direct_agents,
            "workflow": workflow,
            "marketplace_source": marketplace_source,
            "plugin": plugin,
        }
        return plugin, document, paths, fixed_mtime

    def runner(self, paths: dict[str, Path], fail: str | None = None):
        outputs = {
            "strict_config": "strict-ok",
            "discovery": "discovered",
            "v1_runtime": "v1",
            "root_profile": "gpt-5.6-sol xhigh",
            "inspector_profile": "gpt-5.6-terra xhigh",
            "recovery_strict_config": "recovery-strict-ok",
            "recovery_discovery": "recovery-discovered",
        }

        def run(command: list[str], _environment: dict[str, str]):
            if command[:3] == ["fake-codex", "plugin", "marketplace"]:
                if command[3] == "add":
                    self.assertEqual(str(paths["marketplace_source"]), command[4])
                    self.assertNotEqual(str(paths["plugin"]), command[4])
                if command[3] == "remove":
                    (paths["marketplace"] / "state.json").unlink(missing_ok=True)
                (paths["marketplace"] / "state.json").write_text(
                    "marketplace added\n" if command[3] == "add" else "marketplace removed\n",
                    encoding="ascii",
                )
                return cutover.CommandResult(0, "added", "")
            if command[:3] == ["fake-codex", "plugin", "add"]:
                (paths["marketplace"] / "installed.json").write_text(
                    "installed\n", encoding="ascii"
                )
                return cutover.CommandResult(0, "installed", "")
            if command[:3] == ["fake-codex", "plugin", "remove"]:
                (paths["marketplace"] / "installed.json").unlink(missing_ok=True)
                return cutover.CommandResult(0, "removed", "")
            kind = command[-1]
            for name in ("direct_skills", "direct_agents", "workflow"):
                self.assertTrue(paths[name].exists(), "old copies were removed before gates passed")
            if kind == fail:
                return cutover.CommandResult(7, "", "injected gate failure")
            return cutover.CommandResult(0, outputs[kind], "")

        return run

    def test_gate_failure_restores_every_managed_path_and_metadata(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            plugin, document, paths, fixed_mtime = self.fixture(root)
            original = cutover.inventory(document)
            record_path = root / "record.json"
            with self.assertRaisesRegex(cutover.CutoverError, "rollback verified"):
                cutover.run_cutover(
                    document,
                    b"plan",
                    plugin,
                    root / "backup",
                    record_path,
                    runner=self.runner(paths, fail="discovery"),
                )
            cutover.verify_inventory(original)
            self.assertEqual(b"old config\n", paths["active"].read_bytes())
            self.assertEqual(0o640, paths["active"].stat().st_mode & 0o777)
            self.assertEqual(fixed_mtime, paths["active"].stat().st_mtime_ns)
            self.assertEqual(
                b"old marketplace\n", (paths["marketplace"] / "state.json").read_bytes()
            )
            self.assertFalse((paths["marketplace"] / "installed.json").exists())
            record = json.loads(record_path.read_text(encoding="ascii"))
            self.assertEqual("failed", record["status"])
            self.assertTrue(record["rollback_verified"])

    def test_install_uses_marketplace_root_and_uninstall_restores_backup(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            plugin, document, paths, _fixed_mtime = self.fixture(root)
            self.assertEqual([], cutover.validate_plan(document, plugin))
            install_record = root / "install-record.json"
            install_backup = root / "install-backup"
            cutover.run_cutover(
                document,
                b"plan",
                plugin,
                install_backup,
                install_record,
                runner=self.runner(paths),
            )
            self.assertEqual(b"new config\n", paths["active"].read_bytes())
            self.assertFalse(paths["direct_skills"].exists())
            self.assertTrue((install_backup / "recovery.json").is_file())

            uninstall_record = root / "uninstall-record.json"
            cutover.run_uninstall(
                install_record,
                install_backup,
                root / "uninstall-rollback",
                uninstall_record,
                runner=self.runner(paths),
            )
            self.assertEqual(b"old config\n", paths["active"].read_bytes())
            self.assertTrue(paths["direct_skills"].is_dir())
            self.assertEqual(
                b"old marketplace\n", (paths["marketplace"] / "state.json").read_bytes()
            )
            self.assertFalse((paths["marketplace"] / "installed.json").exists())
            self.assertEqual(
                "success", json.loads(uninstall_record.read_text(encoding="ascii"))["status"]
            )

    def test_uninstall_failure_restores_installed_state(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            plugin, document, paths, _fixed_mtime = self.fixture(root)
            install_record = root / "install-record.json"
            install_backup = root / "install-backup"
            cutover.run_cutover(
                document,
                b"plan",
                plugin,
                install_backup,
                install_record,
                runner=self.runner(paths),
            )
            uninstall_record = root / "uninstall-record.json"
            with self.assertRaisesRegex(cutover.CutoverError, "rollback verified"):
                cutover.run_uninstall(
                    install_record,
                    install_backup,
                    root / "uninstall-rollback",
                    uninstall_record,
                    runner=self.runner(paths, fail="recovery_discovery"),
                )
            self.assertEqual(b"new config\n", paths["active"].read_bytes())
            self.assertFalse(paths["direct_skills"].exists())
            self.assertTrue((paths["marketplace"] / "installed.json").is_file())
            record = json.loads(uninstall_record.read_text(encoding="ascii"))
            self.assertEqual("failed", record["status"])
            self.assertTrue(record["rollback_verified"])


if __name__ == "__main__":
    unittest.main()
