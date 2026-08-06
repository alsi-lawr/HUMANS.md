from __future__ import annotations

import importlib.util
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


def load(path: Path, name: str):
    specification = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(specification)
    assert specification and specification.loader
    specification.loader.exec_module(module)
    return module


validator = load(
    ROOT / "casefile/scripts/validate-claude-profiles.py", "validate_claude_profiles"
)


class ClaudeProfileBindingTests(unittest.TestCase):
    def test_matrices_profiles_and_agent_files_resolve(self):
        failures = validator.check(ROOT / "casefile/adapters/claude")
        self.assertEqual([], failures)


if __name__ == "__main__":
    unittest.main()
