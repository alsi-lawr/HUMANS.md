from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from _load import script


artifacts = script("scripts/casefile_artifacts.py")


class CasefileArtifactTests(unittest.TestCase):
    def inputs(self, root: Path) -> Path:
        inputs = root / "inputs"
        for target in artifacts.TARGETS:
            path = inputs / target / artifacts.executable_name(target)
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(f"portable artifact for {target}\n".encode("ascii"))
        return inputs

    def test_assemble_writes_complete_deterministic_metadata(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            output = root / "runtime"
            document = artifacts.assemble(output, self.inputs(root), "0.4.0", "1" * 40)
            self.assertEqual(list(artifacts.TARGETS), [row["target"] for row in document["artifacts"]])
            self.assertEqual(document, artifacts.load(output, "0.4.0", "1" * 40))
            self.assertEqual(artifacts.canonical(document), (output / "artifacts.json").read_bytes())

    def test_complete_matrix_lands_despite_representation_and_unrelated_output(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            output = root / "runtime"
            document = artifacts.assemble(output, self.inputs(root), "0.4.0", "1" * 40)
            for index, row in enumerate(document["artifacts"]):
                row["path"] = row["path"].replace("/", "\\" if index % 2 else "//") + "///"
                row["sha256"] = "not used for landing"
                row["size"] = -1
            document["artifacts"].reverse()
            document["unrelated_metadata"] = True
            (output / "artifacts.json").write_bytes(
                json.dumps(document, separators=(", ", ": ")).replace("}", "}\r\n", 1).encode("utf-8")
            )
            (output / "unrelated.txt").write_text("keep\n", encoding="ascii")
            loaded = artifacts.load(output, "0.4.0", "1" * 40)
            self.assertEqual(set(artifacts.TARGETS), {row["target"] for row in loaded["artifacts"]})

    def test_missing_empty_nonregular_unsafe_and_wrong_destinations_refuse(self):
        unsafe = (
            "",
            "/bin/aarch64-apple-darwin/casefile",
            r"\bin\aarch64-apple-darwin\casefile",
            r"C:\bin\aarch64-apple-darwin\casefile",
            r"C:bin\aarch64-apple-darwin\casefile",
            r"\\server\share\casefile",
            r"\\?\C:\casefile",
            "bin/./aarch64-apple-darwin/casefile",
            "bin/../aarch64-apple-darwin/casefile",
            "bin/aarch64-apple-darwin/other",
        )
        for value in unsafe:
            with self.subTest(path=value), self.assertRaises(artifacts.ArtifactError):
                artifacts.normalized_artifact_path(value, "aarch64-apple-darwin")

        for state in ("missing", "empty", "directory"):
            with self.subTest(state=state), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                output = root / "runtime"
                document = artifacts.assemble(output, self.inputs(root), "0.4.0", "1" * 40)
                path = output / artifacts.normalized_artifact_path(
                    document["artifacts"][0]["path"], document["artifacts"][0]["target"]
                )
                path.unlink()
                if state == "empty":
                    path.touch()
                elif state == "directory":
                    path.mkdir()
                with self.assertRaisesRegex(artifacts.ArtifactError, "missing|empty|unsafe"):
                    artifacts.load(output)

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            output = root / "runtime"
            document = artifacts.assemble(output, self.inputs(root), "0.4.0", "1" * 40)
            row = document["artifacts"][0]
            relative = artifacts.normalized_artifact_path(row["path"], row["target"])
            outside = root / "outside"
            outside.mkdir()
            (outside / relative.name).write_text("escaped\n", encoding="ascii")
            (output / relative).unlink()
            (output / relative.parent).rmdir()
            try:
                (output / relative.parent).symlink_to(outside, target_is_directory=True)
            except OSError:
                pass
            else:
                with self.assertRaisesRegex(artifacts.ArtifactError, "missing|unsafe"):
                    artifacts.load(output)

    def test_source_admission_and_missing_build_input_still_refuse(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            inputs = self.inputs(root)
            missing = inputs / artifacts.TARGETS[0] / artifacts.executable_name(artifacts.TARGETS[0])
            missing.unlink()
            with self.assertRaisesRegex(artifacts.ArtifactError, "missing"):
                artifacts.assemble(root / "runtime", inputs, "0.4.0", "1" * 40)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            output = root / "runtime"
            artifacts.assemble(output, self.inputs(root), "0.4.0", "1" * 40)
            with self.assertRaisesRegex(artifacts.ArtifactError, "source"):
                artifacts.load(output, "0.4.0", "2" * 40)


if __name__ == "__main__":
    unittest.main()
