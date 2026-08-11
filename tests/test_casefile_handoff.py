from __future__ import annotations

import json
import re
import struct
import tempfile
import unittest
from pathlib import Path

from _load import ROOT, script


artifacts = script("scripts/casefile_artifacts.py")
handoff = script("scripts/validate-casefile-handoff.py")
SOURCE = "1" * 40
RUN_ID = "12345"


def native_stub(target: str) -> bytes:
    if target.endswith("linux-musl"):
        data = bytearray(64)
        data[:4] = b"\x7fELF"
        data[4:6] = b"\x02\x01"
        struct.pack_into("<H", data, 18, 183 if target.startswith("aarch64") else 62)
    elif target.endswith("darwin"):
        data = bytearray(64)
        data[:4] = b"\xcf\xfa\xed\xfe"
        struct.pack_into("<I", data, 4, 0x0100000C if target.startswith("aarch64") else 0x01000007)
    else:
        data = bytearray(128)
        data[:2] = b"MZ"
        struct.pack_into("<I", data, 0x3C, 64)
        data[64:68] = b"PE\0\0"
        struct.pack_into("<H", data, 68, 0xAA64 if target.startswith("aarch64") else 0x8664)
    return bytes(data)


class CasefileHandoffTests(unittest.TestCase):
    def test_publication_wires_run_provenance_and_handoff_validation_before_packaging(self):
        publish = (ROOT / ".github/workflows/publish-marketplace.yml").read_text(encoding="ascii")
        validator = "python scripts/validate-casefile-handoff.py"
        packaging = "python scripts/package-plugin.py build --all"
        self.assertIn('gh api "repos/$GITHUB_REPOSITORY/actions/runs/$BINARY_RUN_ID"', publish)
        self.assertIn("--run-id \"$BINARY_RUN_ID\"", publish)
        self.assertIn("--source-commit \"$SOURCE_COMMIT\"", publish)
        self.assertLess(publish.index(validator), publish.index(packaging))
        self.assertNotIn("cargo build", publish)
        build = (ROOT / ".github/workflows/build-casefile-binaries.yml").read_text(encoding="ascii")
        for retained in (
            "casefile-build-provenance.json",
            "native-smoke",
        ):
            self.assertIn(retained, build)

    def test_build_workflow_has_only_scoped_push_and_explicit_dispatch_sources(self):
        build = (ROOT / ".github/workflows/build-casefile-binaries.yml").read_text(
            encoding="ascii"
        )
        trigger = re.search(r"(?ms)^on:\n.*?(?=^permissions:)", build)
        self.assertIsNotNone(trigger)
        self.assertEqual(
            """on:
  push:
    branches:
      - "casefile/build-*"
  workflow_dispatch:
    inputs:
      source_commit:
        description: Exact 40-character source commit to build
        required: true
        type: string

""",
            trigger.group(0),
        )
        for forbidden in ("pull_request:", "schedule:", "release:", "tags:"):
            self.assertNotIn(forbidden, trigger.group(0))
        self.assertIn('case "$EVENT_NAME" in', build)
        self.assertIn("PUSH_SOURCE: ${{ github.sha }}", build)
        self.assertIn("DISPATCH_SOURCE: ${{ inputs.source_commit }}", build)
        self.assertIn('SOURCE_COMMIT="$PUSH_SOURCE"', build)
        self.assertIn('SOURCE_COMMIT="$DISPATCH_SOURCE"', build)
        self.assertIn('test "$PUSH_DELETED" = "false"', build)
        self.assertIn('[[ "$REF_NAME" =~ ^casefile/build-[^/]*$ ]]', build)
        self.assertIn('[[ "$SOURCE_COMMIT" =~ ^[0-9a-f]{40}$ ]]', build)
        self.assertEqual(1, build.count("${{ inputs.source_commit }}"))
        self.assertNotIn("|| github.sha", build)
        self.assertNotIn("|| inputs.source_commit", build)
        self.assertGreaterEqual(build.count("${{ needs.source.outputs.source_commit }}"), 8)
        self.assertEqual(2, build.count("- name: Verify exact source identity"))
        self.assertIn("casefile-runtime-${{ needs.source.outputs.source_commit }}", build)

    def fixture(
        self,
        root: Path,
        event: str = "workflow_dispatch",
        head_branch: str = "main",
    ) -> tuple[Path, Path]:
        inputs = root / "inputs"
        for target in artifacts.TARGETS:
            path = inputs / target / artifacts.executable_name(target)
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(native_stub(target))
        runtime = root / "handoff/casefile-runtime"
        manifest = artifacts.assemble(runtime, inputs, "0.4.0", SOURCE)
        for index, row in enumerate(manifest["artifacts"]):
            row["path"] = row["path"].replace("/", "\\" if index % 2 else "//") + "///"
            row["sha256"] = "not checked"
            row["size"] = -1
        (runtime / "artifacts.json").write_bytes(
            (json.dumps(manifest, separators=(", ", ": ")) + "\r\n").encode("ascii")
        )
        (runtime / "unrelated").write_text("accepted\n", encoding="ascii")
        handoff_root = root / "handoff"
        smoke = handoff_root / "native-smoke"
        smoke.mkdir()
        for target in artifacts.TARGETS:
            (smoke / f"{target}.json").write_text(
                json.dumps({"identity": "casefile", "tools": 12, "version": "0.4.0"}) + "\n",
                encoding="ascii",
            )
        provenance = {
            "event": event,
            "head_branch": head_branch if event == "push" else None,
            "run_id": int(RUN_ID),
            "schema_version": 1,
            "source_commit": SOURCE,
            "workflow_name": handoff.WORKFLOW_NAME,
            "workflow_path": handoff.WORKFLOW_PATH,
        }
        (handoff_root / "casefile-build-provenance.json").write_text(
            json.dumps(provenance, indent=2, sort_keys=True) + "\n", encoding="ascii"
        )
        run = root / "run.json"
        run.write_text(
            json.dumps({
                "id": int(RUN_ID),
                "name": handoff.WORKFLOW_NAME,
                "path": handoff.WORKFLOW_PATH,
                "event": event,
                "head_branch": head_branch,
                "status": "completed",
                "conclusion": "success",
                "head_sha": SOURCE,
            }),
            encoding="ascii",
        )
        return handoff_root, run

    def test_accepts_exact_dispatch_and_scoped_push_build_handoffs(self):
        for event, branch in (
            ("workflow_dispatch", "main"),
            ("push", "casefile/build-reviewed-fda6eea"),
        ):
            with self.subTest(event=event), tempfile.TemporaryDirectory() as temporary:
                root, run = self.fixture(Path(temporary), event, branch)
                handoff.validate_handoff(root, run, RUN_ID, "0.4.0", SOURCE)

    def test_rejects_wrong_workflow_status_run_or_source(self):
        for key, value in (
            ("name", "Another workflow"),
            ("path", ".github/workflows/other.yml"),
            ("status", "in_progress"),
            ("conclusion", "failure"),
            ("head_sha", "2" * 40),
            ("id", 999),
        ):
            with self.subTest(key=key), tempfile.TemporaryDirectory() as temporary:
                root, run = self.fixture(Path(temporary))
                metadata = json.loads(run.read_text(encoding="ascii"))
                metadata[key] = value
                run.write_text(json.dumps(metadata), encoding="ascii")
                with self.assertRaisesRegex(handoff.HandoffError, "reviewed binary build"):
                    handoff.validate_handoff(root, run, RUN_ID, "0.4.0", SOURCE)

    def test_rejects_unapproved_events_and_push_branches(self):
        for event, branch in (
            ("pull_request", "casefile/build-reviewed"),
            ("push", "main"),
            ("push", "feature/casefile-build"),
            ("push", "casefile/build-reviewed/nested"),
            ("push", ""),
        ):
            with self.subTest(event=event, branch=branch), tempfile.TemporaryDirectory() as temporary:
                root, run = self.fixture(Path(temporary), event, branch)
                with self.assertRaisesRegex(handoff.HandoffError, "allowed Casefile binary event"):
                    handoff.validate_handoff(
                        root, run, RUN_ID, "0.4.0", SOURCE
                    )

    def test_rejects_retained_provenance_that_differs_from_run(self):
        with tempfile.TemporaryDirectory() as temporary:
            root, run = self.fixture(
                Path(temporary), "push", "casefile/build-reviewed"
            )
            provenance = root / "casefile-build-provenance.json"
            value = json.loads(provenance.read_text(encoding="ascii"))
            value["source_commit"] = "2" * 40
            provenance.write_text(json.dumps(value), encoding="ascii")
            with self.assertRaisesRegex(handoff.HandoffError, "retained build provenance"):
                handoff.validate_handoff(root, run, RUN_ID, "0.4.0", SOURCE)

    def test_rejects_missing_smoke_and_missing_runtime_destination(self):
        with tempfile.TemporaryDirectory() as temporary:
            root, run = self.fixture(Path(temporary))
            next((root / "native-smoke").iterdir()).unlink()
            with self.assertRaisesRegex(handoff.HandoffError, "native smoke inventory"):
                handoff.validate_handoff(root, run, RUN_ID, "0.4.0", SOURCE)
        with tempfile.TemporaryDirectory() as temporary:
            root, run = self.fixture(Path(temporary))
            next((root / "casefile-runtime/bin").glob("*/*")).unlink()
            with self.assertRaisesRegex(artifacts.ArtifactError, "missing"):
                handoff.validate_handoff(root, run, RUN_ID, "0.4.0", SOURCE)


if __name__ == "__main__":
    unittest.main()
