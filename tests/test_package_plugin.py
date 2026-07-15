from __future__ import annotations

import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from _load import ROOT, script


package = script("scripts/package-plugin.py")


class PackagePluginTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        (self.root / "packaging/plugins").mkdir(parents=True)
        (self.root / "src").mkdir()
        (self.root / "src/file.txt").write_text("portable\n", encoding="ascii")
        (self.root / "metadata.json.in").write_text(
            '{"license": ${license_json}, "name": ${name_json}, "vendor": ${vendor_json}}\n',
            encoding="ascii",
        )

    def tearDown(self):
        self.temporary.cleanup()

    def write_manifest(self, name: str, vendor: str) -> Path:
        path = self.root / f"packaging/plugins/{name}.toml"
        path.write_text(
            f'''schema_version = 1
name = "{name}"
version = "1.2.3"
publisher = "example-org"
repository = "example-org/{name}"
repository_url = "https://example.test/example-org/{name}"
license = "Apache-2.0"
description = "{name} package"
[[shared]]
source = "src"
destination = "payload"
[vendors.{vendor}]
output = "out/{vendor}/{name}"
[[vendors.{vendor}.template]]
source = "metadata.json.in"
destination = "plugin.json"
format = "json"
''',
            encoding="ascii",
        )
        return path

    def command(self, operation: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [
                sys.executable,
                str(ROOT / "scripts/package-plugin.py"),
                operation,
                "--all",
                "--root",
                str(self.root),
            ],
            capture_output=True,
            text=True,
            check=False,
        )

    def test_cli_all_build_check_is_deterministic_for_a_real_second_plugin(self):
        self.write_manifest("alpha-tool", "codex")
        self.write_manifest("beta-notes", "gemini")
        cache = self.root / "src/__pycache__"
        cache.mkdir()
        (cache / "module.cpython-314.pyc").write_bytes(b"compiled")
        first = self.command("build")
        self.assertEqual(0, first.returncode, first.stdout + first.stderr)
        self.assertIn("built alpha-tool:codex", first.stdout)
        self.assertIn("built beta-notes:gemini", first.stdout)
        snapshot = {
            path.relative_to(self.root): path.read_bytes()
            for path in sorted((self.root / "out").rglob("*"))
            if path.is_file()
        }
        second = self.command("build")
        self.assertEqual(0, second.returncode, second.stdout + second.stderr)
        self.assertEqual(
            snapshot,
            {
                path.relative_to(self.root): path.read_bytes()
                for path in sorted((self.root / "out").rglob("*"))
                if path.is_file()
            },
        )
        checked = self.command("check")
        self.assertEqual(0, checked.returncode, checked.stdout + checked.stderr)
        self.assertFalse(any(path.suffix == ".pyc" for path in (self.root / "out").rglob("*")))

    def test_rejects_or_detects_high_risk_package_inputs(self):
        manifest = self.write_manifest("alpha-tool", "codex")
        document = package.read_manifest(manifest, self.root)
        with self.assertRaises(package.PackageError):
            package.safe_relative("../escape", "destination")
        document["shared"][0]["source"] = "missing"
        with self.assertRaises(package.PackageError):
            package.expected_files(self.root, document, "codex")
        (self.root / "empty").mkdir()
        document["shared"][0]["source"] = "empty"
        with self.assertRaises(package.PackageError):
            package.expected_files(self.root, document, "codex")
        document = package.read_manifest(manifest, self.root)
        try:
            os.symlink(self.root / "src/file.txt", self.root / "linked")
        except OSError:
            self.skipTest("symlinks unavailable")
        document["shared"][0]["source"] = "linked"
        with self.assertRaises(package.PackageError):
            package.expected_files(self.root, document, "codex")

        second = self.write_manifest("beta-notes", "codex")
        second.write_text(
            second.read_text().replace("out/codex/beta-notes", "out/codex/alpha-tool"),
            encoding="ascii",
        )
        with self.assertRaisesRegex(package.PackageError, "output collision"):
            package.package_specs(
                self.root,
                [self.root / "packaging/plugins/alpha-tool.toml", second],
            )

        document = package.read_manifest(manifest, self.root)
        expected = package.expected_files(self.root, document, "codex")
        output = self.root / document["vendors"]["codex"]["output"]
        package.build(output, expected)
        (output / "stale.txt").write_text("stale\n", encoding="ascii")
        self.assertTrue(
            any("stale generated file" in item for item in package.compare(expected, package.actual_files(output)))
        )


if __name__ == "__main__":
    unittest.main()
