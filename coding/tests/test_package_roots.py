from __future__ import annotations

import hashlib
import json
import subprocess
import struct
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TARGETS = (
    "aarch64-apple-darwin", "aarch64-pc-windows-msvc", "aarch64-unknown-linux-musl",
    "x86_64-apple-darwin", "x86_64-pc-windows-msvc", "x86_64-unknown-linux-musl",
)


def executable(target: str) -> bytes:
    if target.endswith("linux-musl"):
        data=bytearray(64); data[:4]=b"\x7fELF"; data[4]=2; data[5]=1; struct.pack_into("<H",data,18,183 if target.startswith("aarch64") else 62)
    elif target.endswith("darwin"):
        data=bytearray(64); data[:4]=b"\xcf\xfa\xed\xfe"; struct.pack_into("<I",data,4,0x0100000C if target.startswith("aarch64") else 0x01000007)
    else:
        data=bytearray(128); data[:2]=b"MZ"; struct.pack_into("<I",data,0x3C,64); data[64:68]=b"PE\0\0"; struct.pack_into("<H",data,68,0xAA64 if target.startswith("aarch64") else 0x8664)
    return bytes(data)


class PackageRootTests(unittest.TestCase):
    def test_manifests_are_synchronized_and_generated_boundaries_are_disjoint(self):
        run = lambda *args: subprocess.run([sys.executable, *args], cwd=ROOT, text=True, capture_output=True, check=False)
        self.assertNotEqual(0, run("scripts/package-plugin.py", "build", "--all").returncode)
        with tempfile.TemporaryDirectory() as temporary:
            artifact = Path(temporary)
            rows = []
            for target in TARGETS:
                name = "casefile.exe" if target.endswith("windows-msvc") else "casefile"
                path = artifact / "bin" / target / name
                path.parent.mkdir(parents=True, exist_ok=True)
                data = executable(target); path.write_bytes(data)
                rows.append({"path":path.relative_to(artifact).as_posix(),"sha256":hashlib.sha256(data).hexdigest(),"size":len(data),"target":target})
            (artifact/"artifacts.json").write_text(json.dumps({"schema_version":1,"version":"0.4.0","source_commit":"1"*40,"artifacts":rows},indent=2,sort_keys=True)+"\n",encoding="ascii")
            extra = ("--casefile-artifact-root", str(artifact), "--casefile-source-commit", "1" * 40)
            self.assertEqual(0, run("scripts/package-plugin.py", "build", "--all", *extra).returncode)
            self.assertEqual(0, run("scripts/package-plugin.py", "check", "--all", *extra).returncode)
        expected = {"humans-md", "casefile", "coding"}
        for vendor in ("codex", "claude"):
            roots = {item.name for item in (ROOT / "build/marketplace/plugins" / vendor).iterdir() if item.is_dir()}
            self.assertEqual(expected, roots)
        core = ROOT / "build/marketplace/plugins/codex/humans-md"
        self.assertFalse((core / "casefile-workflow").exists())
        self.assertFalse((core / "skills/git-contribution").exists())
        metadata = (core / ".codex-plugin/plugin.json").read_text(encoding="ascii")
        self.assertNotIn("Casefile workflow", metadata)
        self.assertNotIn("Create a skill", metadata)
        claude = ROOT / "build/marketplace/plugins/claude/humans-md"
        self.assertEqual(["bootstrap-contract.py"], sorted(path.name for path in claude.rglob("bootstrap-contract.py")))


if __name__ == "__main__":
    unittest.main()
