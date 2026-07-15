from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from _load import ROOT, script


profiler = script("adapters/codex/scripts/profile-codex-catalog.py")


class CatalogProfileTests(unittest.TestCase):
    def fixture(self, root: Path):
        catalog = json.loads(
            (ROOT / "tests/fixtures/codex-models-current-shape.json").read_text(encoding="ascii")
        )
        resources = root / "catalog/gpt-5.6-sol"
        resources.mkdir(parents=True)
        base = resources / "base-instructions.md"
        base.write_text("authored instructions\n", encoding="ascii")
        messages = resources / "model-messages.json"
        messages.write_text(
            json.dumps(
                {
                    "approvals": None,
                    "instructions_template": "authored template",
                    "instructions_variables": {},
                },
                indent=2,
                sort_keys=True,
            )
            + "\n",
            encoding="ascii",
        )
        profile_path = root / "profiles.toml"
        profile_path.write_text(
            f'''schema_version = 1
adapter = "codex"
[catalog]
id_field = "slug"
instruction_field = "base_instructions"
model_messages_field = "model_messages"
selector_fields = ["multi_agent_version"]
[[catalog.targets]]
id = "gpt-5.6-sol"
required_reasoning = ["xhigh"]
base_instructions_file = "catalog/gpt-5.6-sol/base-instructions.md"
base_instructions_sha256 = "{profiler.sha256(base.read_bytes())}"
model_messages_file = "catalog/gpt-5.6-sol/model-messages.json"
model_messages_sha256 = "{profiler.sha256(messages.read_bytes())}"
null_selectors = ["multi_agent_version"]
[catalog.targets.expected]
display_name = "GPT-5.6-Sol"
''',
            encoding="ascii",
        )
        return catalog, profile_path, profiler.load_profile(profile_path)

    def test_allowlisted_resources_and_declared_selector_preserve_unrelated_fields(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            catalog, profile_path, profile = self.fixture(root)
            result, stale = profiler.build(catalog, profile, profile_path)
            self.assertEqual([], stale)
            model = result["models"][0]
            self.assertEqual("authored instructions\n", model["base_instructions"])
            self.assertEqual("authored template", model["model_messages"]["instructions_template"])
            self.assertIsNone(model["multi_agent_version"])
            self.assertEqual({"preserve": True}, model["fixture_unrelated"])

            fresh = root / "fresh.json"
            fresh.write_bytes(profiler.canonical(catalog))
            profiled = root / "profiled.json"
            profiled.write_bytes(profiler.canonical(result))
            report = root / "report.md"
            command = [
                sys.executable,
                str(ROOT / "scripts/check-codex-model-drift.py"),
                "--profiles",
                str(profile_path),
                "--output",
                str(report),
            ]
            self.assertEqual(
                1,
                subprocess.run(
                    command[:2] + ["--catalog", str(fresh)] + command[2:],
                    capture_output=True,
                    text=True,
                    check=False,
                ).returncode,
            )
            self.assertEqual(
                0,
                subprocess.run(
                    command[:2] + ["--catalog", str(profiled)] + command[2:],
                    capture_output=True,
                    text=True,
                    check=False,
                ).returncode,
            )

    def test_failed_install_restores_bytes_mode_mtime_and_last_installed_backup(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            catalog, profile_path, profile = self.fixture(root)
            profiled, _ = profiler.build(catalog, profile, profile_path)
            target = root / "target.json"
            previous = b'{"previous": true}\n'
            target.write_bytes(previous)
            target.chmod(0o640)
            fixed_mtime = 1_700_000_000_123_456_789
            os.utime(target, ns=(fixed_mtime, fixed_mtime))

            def fail(_path: Path, _data: bytes, _profiled: dict) -> None:
                raise RuntimeError("injected verification failure")

            with self.assertRaisesRegex(RuntimeError, "injected"):
                profiler.install_profiled_catalog(
                    target,
                    profiler.canonical(profiled),
                    profiled,
                    profiler.canonical(catalog),
                    root / "backups",
                    verifier=fail,
                )
            self.assertEqual(previous, target.read_bytes())
            self.assertEqual(0o640, target.stat().st_mode & 0o777)
            self.assertEqual(fixed_mtime, target.stat().st_mtime_ns)
            self.assertEqual(
                previous,
                next((root / "backups").glob("last-installed-*.json")).read_bytes(),
            )


if __name__ == "__main__":
    unittest.main()
