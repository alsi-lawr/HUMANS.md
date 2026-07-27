from __future__ import annotations

import tempfile
import struct
import unittest
from pathlib import Path

from _load import script


artifacts = script("scripts/casefile_artifacts.py")


def executable(target: str) -> bytes:
    if target.endswith("linux-musl"):
        data = bytearray(64); data[:4] = b"\x7fELF"; data[4] = 2; data[5] = 1
        struct.pack_into("<H", data, 18, 183 if target.startswith("aarch64") else 62)
    elif target.endswith("darwin"):
        data = bytearray(64); data[:4] = b"\xcf\xfa\xed\xfe"
        struct.pack_into("<I", data, 4, 0x0100000C if target.startswith("aarch64") else 0x01000007)
    else:
        data = bytearray(128); data[:2] = b"MZ"; struct.pack_into("<I", data, 0x3C, 64); data[64:68] = b"PE\0\0"
        struct.pack_into("<H", data, 68, 0xAA64 if target.startswith("aarch64") else 0x8664)
    return bytes(data)


class CasefileArtifactTests(unittest.TestCase):
    def inputs(self, root: Path) -> Path:
        inputs = root / "inputs"
        for target in artifacts.TARGETS:
            path = inputs / target / artifacts.executable_name(target)
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(executable(target))
        return inputs

    def test_assemble_is_canonical_complete_source_bound_and_verifiable(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            output = root / "runtime"
            document = artifacts.assemble(output, self.inputs(root), "0.4.0", "1" * 40)
            self.assertEqual(list(artifacts.TARGETS), [row["target"] for row in document["artifacts"]])
            self.assertEqual(document, artifacts.load(output, "0.4.0", "1" * 40))
            self.assertEqual(artifacts.canonical(document), (output / "artifacts.json").read_bytes())

    def test_wrong_format_missing_extra_hash_and_source_refuse(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            inputs = self.inputs(root)
            (inputs / artifacts.TARGETS[0] / artifacts.executable_name(artifacts.TARGETS[0])).write_bytes(b"wrong")
            with self.assertRaisesRegex(artifacts.ArtifactError, "format"):
                artifacts.assemble(root / "runtime", inputs, "0.4.0", "1" * 40)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); output = root / "runtime"
            artifacts.assemble(output, self.inputs(root), "0.4.0", "1" * 40)
            (output / "extra").write_bytes(b"extra")
            with self.assertRaisesRegex(artifacts.ArtifactError, "inventory"):
                artifacts.load(output)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); output = root / "runtime"
            document = artifacts.assemble(output, self.inputs(root), "0.4.0", "1" * 40)
            row = document["artifacts"][0]
            path = output / row["path"]
            path.write_bytes(path.read_bytes() + b"tamper")
            with self.assertRaisesRegex(artifacts.ArtifactError, "size mismatch"):
                artifacts.load(output)
            with self.assertRaisesRegex(artifacts.ArtifactError, "source"):
                artifacts.load(output, "0.4.0", "2" * 40)


if __name__ == "__main__":
    unittest.main()
