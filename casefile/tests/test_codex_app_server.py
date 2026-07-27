from __future__ import annotations

import os
import tempfile
import textwrap
import unittest
from pathlib import Path

from _load import script


app_server = script("casefile/adapters/codex/scripts/codex_app_server.py")


FAKE_SERVER = r'''#!/usr/bin/env python3
import json, os, sys, time

mode = os.environ.get("FAKE_MODE", "ok")
for line in sys.stdin:
    message = json.loads(line)
    if message.get("method") == "initialize":
        if mode == "invalid-json":
            print("not-json", flush=True)
        elif mode == "exit":
            print("server stopped", file=sys.stderr, flush=True)
            raise SystemExit(7)
        else:
            print(json.dumps({"id": message["id"], "result": {"codexHome": "/tmp"}}), flush=True)
    elif message.get("method") == "initialized":
        print(json.dumps({"method": "configWarning", "params": {"summary": "ignored"}}), flush=True)
    elif message.get("method") == "model/list":
        if mode == "timeout":
            time.sleep(2)
        elif mode == "error":
            print(json.dumps({"id": message["id"], "error": {"code": -1, "message": "denied"}}), flush=True)
        else:
            cursor = message["params"].get("cursor")
            identifier = "gpt-5.6-sol" if cursor is None else "gpt-5.3-codex-spark"
            result = {
                "data": [{
                    "id": identifier,
                    "model": identifier,
                    "displayName": identifier,
                    "hidden": cursor is not None,
                    "supportedReasoningEfforts": [{"reasoningEffort": "high"}],
                }],
                "nextCursor": "next" if cursor is None else None,
            }
            print(json.dumps({"id": message["id"], "result": result}), flush=True)
else:
    marker = os.environ.get("FAKE_CLOSED")
    if marker:
        open(marker, "w", encoding="ascii").write("closed\n")
'''


class CodexAppServerTests(unittest.TestCase):
    def executable(self, root: Path) -> Path:
        path = root / "fake-codex"
        path.write_text(textwrap.dedent(FAKE_SERVER), encoding="ascii")
        path.chmod(0o755)
        return path

    def test_initializes_pages_tolerates_notifications_and_closes_child(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            marker = root / "closed"
            environment = {**os.environ, "FAKE_CLOSED": str(marker)}
            projection = app_server.model_projection(
                str(self.executable(root)), environment, timeout=1
            )
            self.assertEqual(
                ["gpt-5.6-sol", "gpt-5.3-codex-spark"],
                [model["slug"] for model in projection["models"]],
            )
            self.assertEqual("list", projection["models"][0]["visibility"])
            self.assertEqual("hide", projection["models"][1]["visibility"])
            self.assertEqual(
                [{"effort": "high"}],
                projection["models"][0]["supported_reasoning_levels"],
            )
            self.assertEqual("closed\n", marker.read_text(encoding="ascii"))

    def test_surfaces_protocol_process_and_timeout_failures(self):
        for mode, expected in (
            ("error", "model/list failed"),
            ("invalid-json", "invalid JSON"),
            ("exit", "exited with 7"),
            ("timeout", "timed out"),
        ):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                environment = {**os.environ, "FAKE_MODE": mode}
                with self.assertRaisesRegex(app_server.AppServerError, expected):
                    app_server.model_projection(
                        str(self.executable(root)), environment, timeout=0.1
                    )

    def test_rejects_selector_or_identifier_drift(self):
        base = {
            "id": "gpt-5.6-sol",
            "model": "different",
            "displayName": "Sol",
            "hidden": False,
            "supportedReasoningEfforts": [{"reasoningEffort": "high"}],
        }
        with self.assertRaisesRegex(app_server.AppServerError, "selector differs"):
            app_server.normalize([base])
        base["model"] = base["id"]
        with self.assertRaisesRegex(app_server.AppServerError, "duplicated model ID"):
            app_server.normalize([base, base])


if __name__ == "__main__":
    unittest.main()
