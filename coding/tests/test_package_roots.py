from __future__ import annotations
import subprocess, sys, unittest
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
class PackageRootTests(unittest.TestCase):
 def test_manifests_are_synchronized_and_generated_boundaries_are_disjoint(self):
  run=lambda *args: subprocess.run([sys.executable,*args],cwd=ROOT,text=True,capture_output=True,check=False)
  self.assertEqual(0,run("scripts/package-plugin.py","build","--all").returncode)
  self.assertEqual(0,run("scripts/package-plugin.py","check","--all").returncode)
  expected={"humans-md","casefile","coding"}
  for vendor in ("codex","claude"):
   roots={item.name for item in (ROOT/"build/marketplace/plugins"/vendor).iterdir() if item.is_dir()}
   self.assertEqual(expected,roots)
  core=ROOT/"build/marketplace/plugins/codex/humans-md"
  self.assertFalse((core/"casefile-workflow").exists()); self.assertFalse((core/"skills/git-contribution").exists())
if __name__ == "__main__": unittest.main()
