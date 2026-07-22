from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from _load import script


validate = script("coding/scripts/validate-skill.py")


class ValidateSkillTests(unittest.TestCase):
    def test_frontmatter_accepts_prettier_wrapped_description(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "SKILL.md"
            path.write_text(
                '''---
name: sample
description:
  "A description that
  continues."
---

# Sample

Enough instructions to make this a useful skill.
''',
                encoding="ascii",
            )

            metadata, body = validate.frontmatter(path)

            self.assertEqual("sample", metadata["name"])
            self.assertEqual("A description that continues.", metadata["description"])
            self.assertTrue(body.startswith("# Sample"))


if __name__ == "__main__":
    unittest.main()
