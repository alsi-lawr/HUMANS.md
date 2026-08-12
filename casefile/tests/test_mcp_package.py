from __future__ import annotations

import json
import subprocess
import struct
import sys
import tempfile
import tomllib
import unittest
from pathlib import Path

from _load import ROOT

TARGETS = (
    "aarch64-apple-darwin", "aarch64-pc-windows-msvc", "aarch64-unknown-linux-musl",
    "x86_64-apple-darwin", "x86_64-pc-windows-msvc", "x86_64-unknown-linux-musl",
)
SOURCE = "1" * 40
VERSION = tomllib.loads(
    (ROOT / "casefile/packaging/plugin.toml").read_text(encoding="ascii")
)["version"]


def executable(target: str) -> bytes:
    if target.endswith("linux-musl"):
        data=bytearray(64); data[:4]=b"\x7fELF"; data[4]=2; data[5]=1; struct.pack_into("<H",data,18,183 if target.startswith("aarch64") else 62)
    elif target.endswith("darwin"):
        data=bytearray(64); data[:4]=b"\xcf\xfa\xed\xfe"; struct.pack_into("<I",data,4,0x0100000C if target.startswith("aarch64") else 0x01000007)
    else:
        data=bytearray(128); data[:2]=b"MZ"; struct.pack_into("<I",data,0x3C,64); data[64:68]=b"PE\0\0"; struct.pack_into("<H",data,68,0xAA64 if target.startswith("aarch64") else 0x8664)
    return bytes(data)


class McpPackageTests(unittest.TestCase):
    def artifacts(self, root: Path) -> Path:
        artifact = root / "artifacts"
        rows = []
        for target in TARGETS:
            name = "casefile.exe" if target.endswith("windows-msvc") else "casefile"
            path = artifact / "bin" / target / name
            path.parent.mkdir(parents=True, exist_ok=True)
            data = executable(target)
            path.write_bytes(data)
            rows.append({
                "path": path.relative_to(artifact).as_posix().replace("/", "\\") + "///",
                "sha256": "not checked for landing",
                "size": -1,
                "target": target,
            })
        manifest = {
            "schema_version": 1,
            "version": VERSION,
            "source_commit": SOURCE,
            "artifacts": rows,
            "ignored_metadata": "caf\u00e9",
        }
        (artifact / "artifacts.json").write_bytes(
            (json.dumps(manifest, ensure_ascii=False, separators=(", ", ": ")) + "\r\n").encode(
                "utf-8"
            )
        )
        return artifact

    def build(self, artifact: Path) -> subprocess.CompletedProcess[str]:
        return subprocess.run([
            sys.executable, str(ROOT / "scripts/package-plugin.py"), "build", "--manifest",
            "casefile/packaging/plugin.toml", "--casefile-artifact-root", str(artifact),
            "--casefile-source-commit", SOURCE,
        ], cwd=ROOT, capture_output=True, text=True)

    def test_packages_land_complete_matrix_without_automatic_launcher(self):
        with tempfile.TemporaryDirectory() as temporary:
            artifact = self.artifacts(Path(temporary))
            result = self.build(artifact)
            self.assertEqual(0, result.returncode, result.stdout + result.stderr)
            codex = ROOT / "build/marketplace/plugins/codex/casefile"
            claude = ROOT / "build/marketplace/plugins/claude/casefile"
            for package in (codex, claude):
                self.assertFalse((package / ".mcp.json").exists())
                self.assertFalse((package / "Cargo.toml").exists())
                self.assertFalse((package / "scripts/casefile-mcp-launcher.py").exists())
                self.assertTrue((package / "runtime/artifacts.json").is_file())
                self.assertEqual(6, len(list((package / "runtime/bin").glob("*/*"))))
                plugin = next(
                    path
                    for path in (
                        package / ".codex-plugin/plugin.json",
                        package / ".claude-plugin/plugin.json",
                    )
                    if path.is_file()
                )
                metadata = json.loads(plugin.read_text(encoding="utf-8"))
                metadata["ignored_metadata"] = "na\u00efve"
                plugin.write_bytes(
                    (
                        json.dumps(metadata, ensure_ascii=False, separators=(", ", ": "))
                        + "\r\n"
                    ).encode("utf-8")
                )
                validated = subprocess.run(
                    [
                        sys.executable,
                        str(ROOT / "casefile/scripts/validate-casefile.py"),
                        "--source",
                        str(package),
                    ],
                    cwd=ROOT,
                    capture_output=True,
                    text=True,
                )
                self.assertEqual(0, validated.returncode, validated.stdout + validated.stderr)
            for package in (codex, claude):
                self.assertTrue(all(path.stat().st_size > 0 for path in (package / "runtime").rglob("*") if path.is_file()))
            metadata = json.loads((codex / ".codex-plugin/plugin.json").read_text(encoding="utf-8"))
            self.assertNotIn("mcpServers", metadata)
            self.assertTrue((codex / "scripts/list-codex-models.py").is_file())
            for root in (ROOT / "casefile/adapters/codex", codex):
                scripts = "\n".join(
                    path.read_text(encoding="ascii") for path in root.rglob("*.py")
                )
                self.assertNotIn('[executable, "debug", "models"]', scripts)
                self.assertNotIn("codex debug models", scripts)

    def test_missing_empty_unsafe_wrong_source_and_absent_matrix_fail_before_build(self):
        with tempfile.TemporaryDirectory() as temporary:
            artifact = self.artifacts(Path(temporary))
            missing = next((artifact / "bin").glob("*/*"))
            missing.unlink()
            self.assertNotEqual(0, self.build(artifact).returncode)
        with tempfile.TemporaryDirectory() as temporary:
            artifact = self.artifacts(Path(temporary))
            next((artifact / "bin").glob("*/*")).write_bytes(b"")
            self.assertNotEqual(0, self.build(artifact).returncode)
        with tempfile.TemporaryDirectory() as temporary:
            artifact = self.artifacts(Path(temporary))
            (artifact / "extra").write_bytes(b"extra")
            self.assertEqual(0, self.build(artifact).returncode)
        with tempfile.TemporaryDirectory() as temporary:
            artifact = self.artifacts(Path(temporary))
            manifest_path = artifact / "artifacts.json"
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
            manifest["artifacts"][0]["path"] = r"C:bin\casefile"
            manifest_path.write_text(json.dumps(manifest), encoding="ascii")
            self.assertNotEqual(0, self.build(artifact).returncode)
        with tempfile.TemporaryDirectory() as temporary:
            artifact = self.artifacts(Path(temporary))
            result = subprocess.run([
                sys.executable, str(ROOT / "scripts/package-plugin.py"), "build", "--manifest",
                "casefile/packaging/plugin.toml", "--casefile-artifact-root", str(artifact),
                "--casefile-source-commit", "2" * 40,
            ], cwd=ROOT, capture_output=True, text=True)
            self.assertNotEqual(0, result.returncode)
        result = subprocess.run([
            sys.executable, str(ROOT / "scripts/package-plugin.py"), "build", "--manifest",
            "casefile/packaging/plugin.toml",
        ], cwd=ROOT, capture_output=True, text=True)
        self.assertNotEqual(0, result.returncode)
        self.assertIn("requires --casefile-artifact-root", result.stdout)

    @staticmethod
    def inventory(root: Path) -> list[tuple[str, bytes]]:
        return sorted((path.relative_to(root).as_posix(), path.read_bytes()) for path in root.rglob("*") if path.is_file())


if __name__ == "__main__":
    unittest.main()
