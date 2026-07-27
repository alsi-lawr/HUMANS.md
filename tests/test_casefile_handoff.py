from __future__ import annotations

import json
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
        self.assertIn("--manifest-sha256 \"$EXPECTED_MANIFEST_SHA256\"", publish)
        self.assertLess(publish.index(validator), publish.index(packaging))
        self.assertNotIn("cargo build", publish)
        build = (ROOT / ".github/workflows/build-casefile-binaries.yml").read_text(encoding="ascii")
        for retained in (
            "native-smoke",
            "codex-casefile-inventory.sha256",
            "claude-casefile-inventory.sha256",
            "casefile-runtime-manifest.sha256",
        ):
            self.assertIn(retained, build)

    def fixture(self, root: Path) -> tuple[Path, Path, str]:
        inputs = root / "inputs"
        for target in artifacts.TARGETS:
            path = inputs / target / artifacts.executable_name(target)
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(native_stub(target))
        runtime = root / "handoff/casefile-runtime"
        manifest = artifacts.assemble(runtime, inputs, "0.4.0", SOURCE)
        manifest_hash = artifacts.digest(runtime / "artifacts.json")
        handoff_root = root / "handoff"
        (handoff_root / "casefile-runtime-manifest.sha256").write_text(
            f"{manifest_hash}  casefile-runtime/artifacts.json\n", encoding="ascii"
        )
        smoke = handoff_root / "native-smoke"
        smoke.mkdir()
        for target in artifacts.TARGETS:
            (smoke / f"{target}.json").write_text(
                json.dumps({"identity": "casefile", "tools": 12, "version": "0.4.0"}) + "\n",
                encoding="ascii",
            )
        runtime_inventory = {"runtime/artifacts.json": manifest_hash}
        runtime_inventory.update(
            {f"runtime/{row['path']}": row["sha256"] for row in manifest["artifacts"]}
        )
        for vendor in ("codex", "claude"):
            lines = [
                f"{checksum}  build/marketplace/plugins/{vendor}/casefile/{relative}"
                for relative, checksum in sorted(runtime_inventory.items())
            ]
            lines.append(
                f"{'2' * 64}  build/marketplace/plugins/{vendor}/casefile/skills/casefile/SKILL.md"
            )
            (handoff_root / f"{vendor}-casefile-inventory.sha256").write_text(
                "\n".join(lines) + "\n", encoding="ascii"
            )
        run = root / "run.json"
        run.write_text(
            json.dumps({
                "id": int(RUN_ID),
                "name": handoff.WORKFLOW_NAME,
                "path": handoff.WORKFLOW_PATH,
                "event": "workflow_dispatch",
                "status": "completed",
                "conclusion": "success",
                "head_sha": SOURCE,
            }),
            encoding="ascii",
        )
        return handoff_root, run, manifest_hash

    def test_accepts_only_complete_successful_exact_source_build_handoff(self):
        with tempfile.TemporaryDirectory() as temporary:
            root, run, manifest_hash = self.fixture(Path(temporary))
            handoff.validate_handoff(root, run, RUN_ID, "0.4.0", SOURCE, manifest_hash)

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
                root, run, manifest_hash = self.fixture(Path(temporary))
                metadata = json.loads(run.read_text(encoding="ascii"))
                metadata[key] = value
                run.write_text(json.dumps(metadata), encoding="ascii")
                with self.assertRaisesRegex(handoff.HandoffError, "reviewed binary build"):
                    handoff.validate_handoff(root, run, RUN_ID, "0.4.0", SOURCE, manifest_hash)

    def test_rejects_missing_smoke_and_mismatched_package_inventory(self):
        with tempfile.TemporaryDirectory() as temporary:
            root, run, manifest_hash = self.fixture(Path(temporary))
            next((root / "native-smoke").iterdir()).unlink()
            with self.assertRaisesRegex(handoff.HandoffError, "native smoke inventory"):
                handoff.validate_handoff(root, run, RUN_ID, "0.4.0", SOURCE, manifest_hash)
        with tempfile.TemporaryDirectory() as temporary:
            root, run, manifest_hash = self.fixture(Path(temporary))
            inventory = root / "claude-casefile-inventory.sha256"
            inventory.write_text(
                inventory.read_text(encoding="ascii").replace(manifest_hash, "3" * 64, 1),
                encoding="ascii",
            )
            with self.assertRaisesRegex(handoff.HandoffError, "reviewed runtime bytes"):
                handoff.validate_handoff(root, run, RUN_ID, "0.4.0", SOURCE, manifest_hash)


if __name__ == "__main__":
    unittest.main()
