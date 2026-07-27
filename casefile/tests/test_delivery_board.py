from __future__ import annotations

import subprocess
import unittest
from pathlib import Path

from _load import ROOT


class DeliveryBoardProviderCutoverTests(unittest.TestCase):
    def test_provider_owns_default_board_preview_and_apply(self):
        source = (ROOT / "casefile/casefile-store/src/provider.rs").read_text(encoding="utf-8")
        self.assertIn("preview_default_delivery_board", source)
        self.assertIn("apply_default_delivery_board", source)
        mcp = (ROOT / "casefile/casefile-cli/src/mcp.rs").read_text(encoding="utf-8")
        self.assertIn('"casefile_preview_default_delivery_board"', mcp)
        self.assertIn('"casefile_apply_default_delivery_board"', mcp)

    def test_skills_require_complete_preview_and_external_approval(self):
        for relative in ("casefile/skills/casefile/SKILL.md", "casefile/skills/casefile-consolidate/SKILL.md"):
            text = (ROOT / relative).read_text(encoding="utf-8")
            self.assertIn("complete", text)
            self.assertIn("explicit human approval", text)
            self.assertIn("casefile_apply_default_delivery_board", text)

    def test_exact_retired_sources_are_absent(self):
        tracked = {
            path
            for path in subprocess.check_output(["git", "ls-files"], cwd=ROOT, text=True).splitlines()
            if (ROOT / path).exists()
        }
        scripts = "casefile/casefile-workflow/scripts"
        forbidden = {
            f"{scripts}/" + "provision-delivery-board" + ".py",
            f"{scripts}/" + "transition-ticket-progress" + ".py",
            f"{scripts}/" + "switch-strategy" + ".py",
        }
        self.assertTrue(forbidden.isdisjoint(tracked))


if __name__ == "__main__":
    unittest.main()
