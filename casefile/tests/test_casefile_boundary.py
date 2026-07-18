from __future__ import annotations
import json, subprocess, sys, tempfile, unittest
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
class CasefileBoundaryTests(unittest.TestCase):
 def test_casefile_setup_is_separate_from_core_contract(self):
  script=ROOT/"casefile/adapters/codex/scripts/setup-codex.py"
  self.assertEqual(0,subprocess.run([sys.executable,"-m","py_compile",str(script)],capture_output=True,text=True).returncode)
  manifest=json.loads((ROOT/"build/marketplace/plugins/codex/casefile/.codex-plugin/plugin.json").read_text(encoding="ascii"))
  self.assertEqual("casefile",manifest["name"])
  package=ROOT/"build/marketplace/plugins/codex/casefile"
  self.assertFalse((package/"templates/AGENTS.md").exists())
  self.assertTrue((package/"config/profiles.toml").is_file())
if __name__=="__main__": unittest.main()
