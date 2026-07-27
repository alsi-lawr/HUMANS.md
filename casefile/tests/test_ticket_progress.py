from __future__ import annotations

import unittest
from pathlib import Path

from _load import ROOT


class TicketProgressProviderCutoverTests(unittest.TestCase):
    def test_provider_and_mcp_own_governed_progress(self):
        provider = (ROOT / "casefile/casefile-store/src/provider.rs").read_text(encoding="utf-8")
        self.assertIn("pub fn preview_progress", provider)
        self.assertIn("pub fn apply_progress", provider)
        mcp = (ROOT / "casefile/casefile-cli/src/mcp.rs").read_text(encoding="utf-8")
        self.assertIn('"casefile_preview_progress"', mcp)
        self.assertIn('"casefile_apply_progress"', mcp)

    def test_malformed_replacement_is_bounded_to_cli_recovery(self):
        main = (ROOT / "casefile/casefile-cli/src/main.rs").read_text(encoding="utf-8")
        commands = (ROOT / "casefile/casefile-cli/src/commands.rs").read_text(encoding="utf-8")
        self.assertIn("ProgressRepairPreview", main)
        self.assertIn("ProgressRepairApply", main)
        self.assertIn("replacement_source.is_none()", commands)
        mcp = (ROOT / "casefile/casefile-cli/src/mcp.rs").read_text(encoding="utf-8")
        self.assertNotIn("progress_repair", mcp)

    def test_progress_and_board_remain_separate_operations(self):
        skill = (ROOT / "casefile/skills/casefile-consolidate/SKILL.md").read_text(encoding="utf-8")
        self.assertIn("sequential and independent", skill)
        self.assertIn("do not", skill.lower())


if __name__ == "__main__":
    unittest.main()
