from __future__ import annotations
import json, subprocess, sys, tempfile, tomllib, unittest
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
class CasefileBoundaryTests(unittest.TestCase):
 def test_casefile_setup_is_separate_from_core_contract(self):
  script=ROOT/"casefile/adapters/codex/scripts/setup-codex.py"
  self.assertEqual(0,subprocess.run([sys.executable,"-m","py_compile",str(script)],capture_output=True,text=True).returncode)
  version=tomllib.loads((ROOT/"casefile/packaging/plugin.toml").read_text(encoding="ascii"))["version"]
  manifest=json.loads((ROOT/"casefile/adapters/codex/metadata/plugin.json.in").read_text(encoding="ascii").replace("${name_json}", '"casefile"').replace("${publisher_json}", '"alsi-lawr"').replace("${repository_url_json}", '"https://example.test"').replace("${description_json}", '"Casefile"').replace("${license_json}", '"MIT"').replace("${version_json}", json.dumps(version)))
  self.assertEqual("casefile",manifest["name"])
  self.assertFalse((ROOT/"casefile"/"templates/AGENTS.md").exists())
  self.assertTrue((ROOT/"casefile/adapters/codex/config-fragment.toml.in").is_file())
if __name__=="__main__": unittest.main()
