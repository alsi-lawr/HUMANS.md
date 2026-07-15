from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from _load import script


validator = script("scripts/validate-skill.py")


class ValidateSkillTests(unittest.TestCase):
    def test_valid_skill_and_identity(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "sample-skill"
            root.mkdir()
            path = root / "SKILL.md"
            path.write_text(
                '---\nname: sample-skill\ndescription: "Use for a sample task."\n---\n\n# Sample\n\nApply the bounded sample contract and report evidence.\n',
                encoding="ascii",
            )
            self.assertEqual([], validator.validate_skill(path))
            path.write_text(path.read_text().replace("name: sample-skill", "name: other"), encoding="ascii")
            self.assertTrue(any("name does not match directory" in item for item in validator.validate_skill(path)))

    def test_rejects_broken_link_and_non_executable_script(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "sample-skill"
            (root / "scripts").mkdir(parents=True)
            path = root / "SKILL.md"
            path.write_text(
                '---\nname: sample-skill\ndescription: "Use for a sample task."\n---\n\n# Sample\n\nLoad [missing](references/nope.md) and run the bundled script with care.\n',
                encoding="ascii",
            )
            (root / "scripts/check.py").write_text("print('ok')\n", encoding="ascii")
            errors = validator.validate_skill(path)
            self.assertTrue(any("broken local link" in item for item in errors))
            self.assertTrue(any("not executable" in item for item in errors))


if __name__ == "__main__":
    unittest.main()
