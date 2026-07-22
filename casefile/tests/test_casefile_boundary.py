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

 def test_codex_implementation_writers_use_sol_high_everywhere(self):
  codex=ROOT/"casefile/adapters/codex"
  profiles=tomllib.loads((codex/"profiles.toml").read_text(encoding="ascii"))
  writers=[row for row in profiles["matrix_profiles"] if row["role"]=="implementation-writer"]
  expected={"casefile-implement-ticket-batch","casefile-implement-pipeline"}
  self.assertEqual(expected,{row["strategy_id"] for row in writers})
  self.assertEqual(2,len(writers))
  for row in writers:
   self.assertEqual("gpt-5.6-sol",row["model"])
   self.assertEqual("high",row["reasoning"])
   agent=tomllib.loads((codex/row["agent_file"]).read_text(encoding="ascii"))
   self.assertEqual("gpt-5.6-sol",agent["model"])
   self.assertEqual("high",agent["model_reasoning_effort"])
   matrix=tomllib.loads((codex/"matrices"/(row["strategy_id"]+".toml")).read_text(encoding="ascii"))
   worker=next(worker for worker in matrix["workers"] if worker["role"]=="implementation-writer")
   self.assertEqual(row["profile"],worker["platform_profile"])
   self.assertEqual("gpt-5.6-sol",worker["model"])
   self.assertEqual("high",worker["reasoning"])
if __name__=="__main__": unittest.main()
