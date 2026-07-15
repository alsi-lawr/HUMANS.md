from __future__ import annotations

import os
import tempfile
import unittest
from pathlib import Path

from _load import script


package = script("scripts/package-plugin.py")


class PackagePluginTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        (self.root / "src").mkdir()
        (self.root / "src/file.txt").write_text("portable\n", encoding="ascii")

    def tearDown(self):
        self.temporary.cleanup()

    def manifest(self, vendor: str) -> dict:
        return {
            "schema_version": 1,
            "plugin_id": f"{vendor}-humans-md",
            "name": "humans-md",
            "vendor": vendor,
            "version": "0.1.0",
            "publisher": "alsi-lawr",
            "repository": "alsi-lawr/HUMANS.md",
            "license": "MIT",
            "description": "test package",
            "output": f"plugins/{vendor}/humans-md",
            "copy": [{"source": "src", "destination": "payload"}],
        }

    def test_deterministic_multi_plugin_build(self):
        snapshots = {}
        for vendor in ("codex", "claude"):
            document = self.manifest(vendor)
            expected = package.expected_files(self.root, document)
            output = self.root / document["output"]
            package.build(output, expected)
            self.assertEqual([], package.compare(expected, package.actual_files(output)))
            snapshots[vendor] = package.actual_files(output)
            package.build(output, expected)
            self.assertEqual(snapshots[vendor], package.actual_files(output))

    def test_ignores_transient_python_cache(self):
        cache = self.root / "src/__pycache__"
        cache.mkdir()
        (cache / "module.cpython-314.pyc").write_bytes(b"compiled")
        (self.root / "src/legacy.pyc").write_bytes(b"compiled")

        expected = package.expected_files(self.root, self.manifest("codex"))

        self.assertFalse(any("__pycache__" in path.parts for path in expected))
        self.assertFalse(any(path.suffix == ".pyc" for path in expected))

    def test_rejects_traversal_missing_empty_and_symlink(self):
        with self.assertRaises(package.PackageError):
            package.safe_relative("../escape", "test")
        document = self.manifest("codex")
        document["copy"][0]["source"] = "missing"
        with self.assertRaises(package.PackageError):
            package.expected_files(self.root, document)
        (self.root / "empty").mkdir()
        document["copy"][0]["source"] = "empty"
        with self.assertRaises(package.PackageError):
            package.expected_files(self.root, document)
        try:
            os.symlink(self.root / "src/file.txt", self.root / "linked")
        except OSError:
            self.skipTest("symlinks unavailable")
        document["copy"][0]["source"] = "linked"
        with self.assertRaises(package.PackageError):
            package.expected_files(self.root, document)

    def test_detects_stale_and_mode_drift(self):
        document = self.manifest("claude")
        expected = package.expected_files(self.root, document)
        output = self.root / document["output"]
        package.build(output, expected)
        (output / "stale.txt").write_text("stale\n", encoding="ascii")
        errors = package.compare(expected, package.actual_files(output))
        self.assertTrue(any("stale generated file" in item for item in errors))
        (output / "stale.txt").unlink()
        target = output / "payload/file.txt"
        target.chmod(0o755)
        errors = package.compare(expected, package.actual_files(output))
        self.assertTrue(any("mode mismatch" in item for item in errors))


if __name__ == "__main__":
    unittest.main()
