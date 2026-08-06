from __future__ import annotations

import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def load(path: Path, name: str):
    specification = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(specification)
    assert specification and specification.loader
    sys.modules[name] = module
    specification.loader.exec_module(module)
    return module


packager = load(ROOT / "scripts/package-plugin.py", "package_plugin_includes_test")

AGENTS = ROOT / "casefile/adapters/claude/agents"
ROLES = ROOT / "casefile/casefile-workflow/roles"


def expand(path: Path) -> str:
    return packager.expand_includes(ROOT, path, path.read_bytes()).decode("ascii")


class PackagingIncludeTests(unittest.TestCase):
    def test_no_claude_agent_defers_its_role_to_a_runtime_read(self):
        for path in sorted(AGENTS.glob("*.md")):
            with self.subTest(agent=path.name):
                self.assertNotIn("CLAUDE_PLUGIN_ROOT", path.read_text(encoding="ascii"))

    def test_every_agent_carries_its_role_text_after_packaging(self):
        for path in sorted(AGENTS.glob("*.md")):
            with self.subTest(agent=path.name):
                packaged = expand(path)
                self.assertNotIn("{{include:", packaged)
                role = path.read_text(encoding="ascii").split("roles/")[1].split(".md")[0]
                heading = (ROLES / f"{role}.md").read_text(encoding="ascii").splitlines()[0]
                self.assertIn(heading, packaged)

    def test_editing_a_role_changes_the_packaged_agent(self):
        source = ROLES / "detective.md"
        original = source.read_bytes()
        agent = AGENTS / "detective.md"
        before = expand(agent)
        try:
            source.write_text(
                original.decode("ascii") + "\nSENTINEL-ROLE-EDIT.\n", encoding="ascii"
            )
            after = expand(agent)
        finally:
            source.write_bytes(original)
        self.assertNotIn("SENTINEL-ROLE-EDIT", before)
        self.assertIn("SENTINEL-ROLE-EDIT", after)
        self.assertEqual(before, expand(agent))

    def test_codex_skill_overlay_tracks_the_shared_body(self):
        shared = ROOT / "coding/skills/git-contribution/SKILL.md"
        overlay = ROOT / "coding/adapters/codex/skills/git-contribution/SKILL.md"
        original = shared.read_bytes()
        self.assertNotIn("Codex GitHub CLI policy", original.decode("ascii"))
        try:
            shared.write_text(
                original.decode("ascii") + "- SENTINEL-SHARED-EDIT.\n", encoding="ascii"
            )
            packaged = expand(overlay)
        finally:
            shared.write_bytes(original)
        self.assertIn("SENTINEL-SHARED-EDIT", packaged)
        self.assertIn("Codex GitHub CLI policy", packaged)
        self.assertNotIn("SENTINEL-SHARED-EDIT", expand(overlay))

    def test_missing_and_nested_include_targets_are_refused(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            missing = root / "agent.md"
            missing.write_text("{{include:does/not/exist.md}}\n", encoding="ascii")
            with self.assertRaisesRegex(packager.PackageError, "missing or unsafe"):
                packager.expand_includes(root, missing, missing.read_bytes())

            (root / "inner.md").write_text("{{include:deeper.md}}\n", encoding="ascii")
            (root / "deeper.md").write_text("leaf\n", encoding="ascii")
            outer = root / "outer.md"
            outer.write_text("{{include:inner.md}}\n", encoding="ascii")
            with self.assertRaisesRegex(packager.PackageError, "nested include"):
                packager.expand_includes(root, outer, outer.read_bytes())

    def test_include_escaping_the_repository_is_refused(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            escape = root / "agent.md"
            escape.write_text("{{include:../outside.md}}\n", encoding="ascii")
            with self.assertRaisesRegex(packager.PackageError, "unsafe include"):
                packager.expand_includes(root, escape, escape.read_bytes())


if __name__ == "__main__":
    unittest.main()
