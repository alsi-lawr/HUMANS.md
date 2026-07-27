from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from _load import ROOT, script


smoke = script("scripts/smoke-casefile-mcp.py")


class CasefileSmokeTests(unittest.TestCase):
    def test_failed_command_reports_command_exit_stdout_and_stderr_as_safe_json(self):
        command = [
            sys.executable,
            "-c",
            "import sys; print('partial'); print('bad\\x1b[31m', file=sys.stderr); raise SystemExit(7)",
        ]
        with self.assertRaises(smoke.SmokeError) as raised:
            smoke.run_checked(command)
        rendered = str(raised.exception)
        self.assertNotIn("\x1b", rendered)
        payload = json.loads(rendered)
        self.assertEqual(command, payload["command"])
        self.assertEqual(7, payload["exit_code"])
        self.assertEqual("partial\n", payload["stdout"])
        self.assertEqual("bad\x1b[31m\n", payload["stderr"])

    def test_cli_failure_prints_mcp_command_and_captured_streams_without_traceback(self):
        with tempfile.TemporaryDirectory() as temporary:
            executable = Path(temporary) / "casefile"
            executable.write_text(
                """#!/usr/bin/env python3
import json, sys
if sys.argv[1:] == ['--version']:
    print('casefile 0.4.0')
elif sys.argv[1:] == ['mcp-compatibility']:
    print(json.dumps({'identity':'casefile','provider_protocol_version':1}))
else:
    print('partial response')
    print('native error\\x1b[31m', file=sys.stderr)
    raise SystemExit(9)
""",
                encoding="ascii",
            )
            executable.chmod(0o755)
            result = subprocess.run(
                [
                    sys.executable,
                    ROOT / "scripts/smoke-casefile-mcp.py",
                    "--executable",
                    executable,
                    "--version",
                    "0.4.0",
                ],
                capture_output=True,
                text=True,
                encoding="utf-8",
                errors="strict",
            )
        self.assertEqual(1, result.returncode)
        self.assertEqual("", result.stdout)
        self.assertNotIn("Traceback", result.stderr)
        self.assertNotIn("\x1b", result.stderr)
        self.assertIn(r"\u001b", result.stderr)
        prefix = "Casefile smoke command failed: "
        self.assertTrue(result.stderr.startswith(prefix), result.stderr)
        payload = json.loads(result.stderr.removeprefix(prefix))
        self.assertEqual(9, payload["exit_code"])
        self.assertEqual("partial response\n", payload["stdout"])
        self.assertEqual("native error\x1b[31m\n", payload["stderr"])
        self.assertEqual(str(executable.resolve()), payload["command"][0])
        self.assertEqual("mcp-package", payload["command"][1])
        self.assertEqual("--planning-root", payload["command"][2])


if __name__ == "__main__":
    unittest.main()
