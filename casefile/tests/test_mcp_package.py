from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from _load import ROOT


PACKAGES = ROOT / "build/marketplace/plugins"


class McpPackageTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        result = subprocess.run(
            [sys.executable, str(ROOT / "scripts/package-plugin.py"), "build", "--manifest", "casefile/packaging/plugin.toml"],
            cwd=ROOT,
            capture_output=True,
            text=True,
        )
        if result.returncode:
            raise AssertionError(result.stderr or result.stdout)

    def test_vendor_declarations_share_one_fixed_root_launcher_contract(self):
        declarations = {}
        for vendor, variable, marker in (
            ("codex", "${CODEX_PLUGIN_ROOT}", ".codex-plugin/plugin.json"),
            ("claude", "${CLAUDE_PLUGIN_ROOT}", ".claude-plugin/plugin.json"),
        ):
            package = PACKAGES / vendor / "casefile"
            value = json.loads((package / ".mcp.json").read_text(encoding="ascii"))
            self.assertEqual(["casefile"], list(value["mcpServers"]))
            declaration = value["mcpServers"]["casefile"]
            self.assertEqual(f"{variable}/scripts/casefile-mcp-launcher.py", declaration["command"])
            self.assertEqual(["--planning-root", "${CASEFILE_PLANNING_ROOT}"], declaration["args"])
            self.assertTrue((package / "scripts/casefile-mcp-launcher.py").is_file())
            self.assertTrue((package / "Cargo.toml").is_file())
            self.assertTrue((package / "Cargo.lock").is_file())
            self.assertTrue((package / marker).is_file())
            declarations[vendor] = declaration["args"]
        self.assertEqual(declarations["codex"], declarations["claude"])
        codex = json.loads((PACKAGES / "codex/casefile/.codex-plugin/plugin.json").read_text(encoding="ascii"))
        self.assertEqual("./.mcp.json", codex["mcpServers"])

    def test_launcher_refuses_implicit_invalid_and_unavailable_prerequisites_without_root_artifacts(self):
        package = PACKAGES / "codex/casefile"
        launcher = package / "scripts/casefile-mcp-launcher.py"
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            planning = root / "planning"
            planning.mkdir()
            (planning / "casefile.toml").write_text("schema_version = 1\n", encoding="ascii")
            home = root / "home"
            home.mkdir()
            before_package = self.inventory(package)
            before_planning = self.inventory(planning)

            missing = subprocess.run([sys.executable, str(launcher)], cwd=planning, capture_output=True, text=True)
            self.assertNotEqual(0, missing.returncode)
            self.assertIn("--planning-root", missing.stderr)

            relative = subprocess.run(
                [sys.executable, str(launcher), "--planning-root", "."],
                cwd=planning,
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(0, relative.returncode)
            self.assertIn("absolute path", relative.stderr)

            environment = {**os.environ, "HOME": str(home), "PATH": ""}
            unavailable = subprocess.run(
                [sys.executable, str(launcher), "--planning-root", str(planning)],
                cwd=root,
                env=environment,
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(0, unavailable.returncode)
            self.assertIn("Rust tool `cargo` is unavailable", unavailable.stderr)
            self.assertEqual(before_package, self.inventory(package))
            self.assertEqual(before_planning, self.inventory(planning))
            self.assertFalse(any(path.name == "target" for path in package.rglob("target")))
            self.assertFalse(any(path.name == "target" for path in planning.rglob("target")))

    def test_external_override_is_explicit_verified_and_receives_one_canonical_root(self):
        package = PACKAGES / "claude/casefile"
        launcher = package / "scripts/casefile-mcp-launcher.py"
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            planning = root / "planning"
            planning.mkdir()
            (planning / "casefile.toml").write_text("schema_version = 1\n", encoding="ascii")
            incompatible = root / "incompatible"
            incompatible.write_text("#!/bin/sh\nprintf '{\"identity\":\"wrong\"}\\n'\n", encoding="ascii")
            incompatible.chmod(0o755)
            refused = subprocess.run(
                [sys.executable, str(launcher), "--planning-root", str(planning), "--external-executable", str(incompatible)],
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(0, refused.returncode)
            self.assertIn("incompatible", refused.stderr)

            compatible = root / "compatible.py"
            compatible.write_text(self.compatible_override_source(), encoding="ascii")
            compatible.chmod(0o755)
            launched = subprocess.run(
                [sys.executable, str(launcher), "--planning-root", str(planning), "--external-executable", str(compatible)],
                capture_output=True,
                text=True,
            )
            self.assertEqual(0, launched.returncode, launched.stderr)
            value = json.loads(launched.stdout)
            self.assertEqual("mcp-stdio", value[0])
            self.assertEqual(1, value.count("--planning-root"))
            self.assertEqual(str(planning.resolve()), value[value.index("--planning-root") + 1])
            self.assertEqual(str(planning.resolve()), value[value.index("--expected-root") + 1])

    def test_launcher_refuses_broken_toolchain_failed_cargo_and_invalid_package_source(self):
        package = PACKAGES / "codex/casefile"
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            planning = root / "planning"
            planning.mkdir()
            (planning / "casefile.toml").write_text("schema_version = 1\n", encoding="ascii")
            home = root / "home"
            home.mkdir()
            tools = root / "tools"
            tools.mkdir()
            launcher = package / "scripts/casefile-mcp-launcher.py"

            self.write_tool(tools / "cargo", "#!/bin/sh\necho 'broken cargo' >&2\nexit 9\n")
            broken = self.launch(launcher, planning, home, tools)
            self.assertNotEqual(0, broken.returncode)
            self.assertIn("Rust prerequisite `cargo` failed", broken.stderr)
            self.assertEqual("", broken.stdout)

            self.write_tool(tools / "cargo", "#!/bin/sh\necho 'cargo 1.95.0'\n")
            (tools / "rustc").unlink(missing_ok=True)
            missing_rust = self.launch(launcher, planning, home, tools)
            self.assertNotEqual(0, missing_rust.returncode)
            self.assertIn("Rust tool `rustc` is unavailable", missing_rust.stderr)

            self.write_tool(
                tools / "cargo",
                "#!/bin/sh\nif [ \"$1\" = '--version' ]; then echo 'cargo 1.95.0'; exit 0; fi\necho 'registry cache and network unavailable' >&2\nexit 101\n",
            )
            self.write_tool(tools / "rustc", "#!/bin/sh\necho 'rustc 1.95.0'\n")
            failed = self.launch(launcher, planning, home, tools)
            self.assertNotEqual(0, failed.returncode)
            self.assertIn("registry or Git cache/network", failed.stderr)
            self.assertEqual("", failed.stdout)
            self.assertFalse(any(path.name == "target" for path in package.rglob("target")))
            self.assertFalse(any(path.name == "target" for path in planning.rglob("target")))

            copied = root / "copied-package"
            shutil.copytree(package, copied)
            (copied / "Cargo.lock").unlink()
            invalid = self.launch(copied / "scripts/casefile-mcp-launcher.py", planning, home, tools)
            self.assertNotEqual(0, invalid.returncode)
            self.assertIn("Cargo.lock is missing", invalid.stderr)

    def test_launcher_refuses_unwritable_external_output_and_absent_override(self):
        package = PACKAGES / "claude/casefile"
        launcher = package / "scripts/casefile-mcp-launcher.py"
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            planning = root / "planning"
            planning.mkdir()
            (planning / "casefile.toml").write_text("schema_version = 1\n", encoding="ascii")
            home_file = root / "not-a-home-directory"
            home_file.write_text("file", encoding="ascii")
            environment = {**os.environ, "HOME": str(home_file)}
            output = subprocess.run(
                [sys.executable, str(launcher), "--planning-root", str(planning)],
                env=environment,
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(0, output.returncode)
            self.assertIn("controlled Cargo output is not writable", output.stderr)
            self.assertEqual("", output.stdout)

            absent = subprocess.run(
                [sys.executable, str(launcher), "--planning-root", str(planning), "--external-executable", str(root / "absent")],
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(0, absent.returncode)
            self.assertIn("cannot be resolved", absent.stderr)

    def test_casefile_package_removal_does_not_touch_sibling_packages(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            shutil.copytree(PACKAGES / "codex", root / "plugins")
            siblings = {name: self.inventory(root / "plugins" / name) for name in ("humans-md", "coding")}
            shutil.rmtree(root / "plugins/casefile")
            for name, inventory in siblings.items():
                self.assertEqual(inventory, self.inventory(root / "plugins" / name))

    @staticmethod
    def inventory(root: Path) -> list[tuple[str, bytes]]:
        return sorted(
            (path.relative_to(root).as_posix(), path.read_bytes())
            for path in root.rglob("*")
            if path.is_file()
        )

    @staticmethod
    def write_tool(path: Path, source: str) -> None:
        path.write_text(source, encoding="ascii")
        path.chmod(0o755)

    @staticmethod
    def launch(launcher: Path, planning: Path, home: Path, tools: Path) -> subprocess.CompletedProcess[str]:
        environment = {**os.environ, "HOME": str(home), "PATH": str(tools)}
        return subprocess.run(
            [sys.executable, str(launcher), "--planning-root", str(planning)],
            env=environment,
            capture_output=True,
            text=True,
        )

    @staticmethod
    def compatible_override_source() -> str:
        operations = [
            "snapshot", "query_tickets", "query_epics", "query_boards", "query_progress",
            "query_strategy_transitions", "preview_record_draft", "apply_record_draft",
            "bootstrap_progress", "preview_progress", "apply_progress",
            "preview_default_delivery_board", "apply_default_delivery_board",
            "preview_strategy_transition", "apply_strategy_transition", "preview_writer_binding",
            "apply_writer_binding",
        ]
        contract = {
            "identity": "casefile",
            "adapter_protocol_version": 1,
            "provider_protocol_version": 1,
            "required_provider_operations": operations,
            "mcp_protocol_versions": ["2025-06-18", "2025-11-25"],
        }
        return (
            "#!/usr/bin/env python3\nimport json,sys\n"
            f"contract={contract!r}\n"
            "if sys.argv[1:] == ['mcp-compatibility']: print(json.dumps(contract))\n"
            "else: print(json.dumps(sys.argv[1:]))\n"
        )


if __name__ == "__main__":
    unittest.main()
