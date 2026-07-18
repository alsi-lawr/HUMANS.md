from __future__ import annotations

import contextlib
import io
import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from _load import script


bootstrap = script("humans-md/skills/contract-bootstrap/scripts/bootstrap-contract.py")


class ContractBootstrapTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.source = self.root / "source.md"
        self.destination = self.root / "AGENTS.md"
        self.source_bytes = b"# New contract\n"
        self.old_bytes = b"# Old contract\n"
        self.source.write_bytes(self.source_bytes)
        self.destination.write_bytes(self.old_bytes)

    def tearDown(self):
        self.temporary.cleanup()

    def backups(self) -> list[Path]:
        return sorted(self.root.glob("AGENTS.md.backup-*"))

    def temporary_paths(self) -> list[Path]:
        return sorted(self.root.glob(".AGENTS.md.*"))

    def run_cli(self, *arguments: str) -> tuple[int, str]:
        output = io.StringIO()
        with (
            mock.patch.object(sys, "argv", ["bootstrap-contract.py", *arguments]),
            contextlib.redirect_stdout(output),
        ):
            result = bootstrap.main()
        return result, output.getvalue()

    def test_preview_refusal_and_no_op(self):
        mtime = self.destination.stat().st_mtime_ns
        result, output = self.run_cli(
            "--source", str(self.source), "--destination", str(self.destination)
        )

        self.assertEqual(0, result)
        self.assertIn(f"destination: {self.destination.absolute()}", output)
        self.assertIn("-# Old contract", output)
        self.assertIn("+# New contract", output)
        self.assertNotIn("source_", output)
        self.assertNotIn("destination_", output)
        self.assertEqual(self.old_bytes, self.destination.read_bytes())
        self.assertEqual(mtime, self.destination.stat().st_mtime_ns)
        self.assertEqual([], self.backups())

        with self.assertRaisesRegex(ValueError, "--replace"):
            bootstrap.install(self.source, self.destination, replace=False)

        self.destination.write_bytes(self.source_bytes)
        mtime = self.destination.stat().st_mtime_ns
        self.assertEqual("unchanged", bootstrap.install(self.source, self.destination, replace=True))
        self.assertEqual(mtime, self.destination.stat().st_mtime_ns)
        self.assertEqual([], self.backups())

    def test_missing_fchmod_creates_distinct_backups_without_touching_existing_ones(self):
        unrelated = self.root / "AGENTS.md.backup-pre-existing"
        unrelated.write_bytes(b"unrelated backup\n")
        source_old = self.root / "old.md"
        source_old.write_bytes(self.old_bytes)

        with mock.patch.object(bootstrap.os, "fchmod", None):
            self.assertEqual("installed", bootstrap.install(self.source, self.destination, replace=True))
            self.assertEqual("installed", bootstrap.install(source_old, self.destination, replace=True))
            self.assertEqual("installed", bootstrap.install(self.source, self.destination, replace=True))

        backups = self.backups()
        self.assertEqual(4, len(backups))
        self.assertEqual(b"unrelated backup\n", unrelated.read_bytes())
        old_backups = [backup for backup in backups if backup.read_bytes() == self.old_bytes]
        self.assertEqual(2, len(old_backups))
        self.assertNotEqual(old_backups[0], old_backups[1])
        self.assertEqual(self.source_bytes, self.destination.read_bytes())

    def test_fdopen_failure_closes_raw_descriptor_and_removes_temporary_path(self):
        primary_error = OSError("injected fdopen failure")
        descriptors: list[int] = []
        original_close = bootstrap.os.close

        def fdopen(descriptor, *arguments, **keywords):
            descriptors.append(descriptor)
            raise primary_error

        with (
            mock.patch.object(bootstrap.os, "fdopen", fdopen),
            mock.patch.object(bootstrap.os, "close", wraps=original_close) as close,
        ):
            with self.assertRaises(OSError) as raised:
                bootstrap.atomic_write(self.destination, self.source_bytes)

        self.assertIs(primary_error, raised.exception)
        self.assertIn(mock.call(descriptors[0]), close.call_args_list)
        self.assertEqual(self.old_bytes, self.destination.read_bytes())
        self.assertEqual([], self.temporary_paths())

    @unittest.skipUnless(hasattr(os, "fchmod"), "fchmod is unavailable")
    def test_supported_fchmod_failure_closes_stream_and_preserves_error(self):
        primary_error = PermissionError("injected fchmod failure")
        original_fdopen = bootstrap.os.fdopen
        streams = []

        class TrackingStream:
            def __init__(self, stream):
                self.stream = stream

            @property
            def closed(self):
                return self.stream.closed

            def __enter__(self):
                self.stream.__enter__()
                return self

            def __exit__(self, *arguments):
                return self.stream.__exit__(*arguments)

            def fileno(self):
                return self.stream.fileno()

        def fdopen(*arguments, **keywords):
            stream = TrackingStream(original_fdopen(*arguments, **keywords))
            streams.append(stream)
            return stream

        with (
            mock.patch.object(bootstrap.os, "fdopen", fdopen),
            mock.patch.object(bootstrap.os, "fchmod", side_effect=primary_error),
        ):
            with self.assertRaises(PermissionError) as raised:
                bootstrap.atomic_write(self.destination, self.source_bytes)

        self.assertIs(primary_error, raised.exception)
        self.assertTrue(streams[0].closed)
        self.assertEqual(self.old_bytes, self.destination.read_bytes())
        self.assertEqual([], self.temporary_paths())

    def test_destination_replace_failure_retains_backup_and_destination(self):
        primary_error = OSError("injected destination replace failure")
        original_replace = bootstrap.os.replace
        calls = 0

        def replace(source, destination):
            nonlocal calls
            calls += 1
            if calls == 2:
                raise primary_error
            return original_replace(source, destination)

        with mock.patch.object(bootstrap.os, "replace", replace):
            with self.assertRaises(OSError) as raised:
                bootstrap.install(self.source, self.destination, replace=True)

        self.assertIs(primary_error, raised.exception)
        self.assertEqual(self.old_bytes, self.destination.read_bytes())
        self.assertEqual([self.old_bytes], [backup.read_bytes() for backup in self.backups()])
        self.assertEqual([], self.temporary_paths())

    def test_post_write_byte_mismatch_raises(self):
        original_read_bytes = Path.read_bytes

        def read_bytes(path: Path):
            data = original_read_bytes(path)
            if path == self.destination and data == self.source_bytes:
                return b"unexpected bytes\n"
            return data

        with mock.patch.object(Path, "read_bytes", read_bytes):
            with self.assertRaisesRegex(RuntimeError, "post-write verification failed"):
                bootstrap.install(self.source, self.destination, replace=True)

        self.assertEqual(self.source_bytes, self.destination.read_bytes())
        self.assertEqual([self.old_bytes], [backup.read_bytes() for backup in self.backups()])

    @unittest.skipUnless(os.name == "posix", "POSIX permission bits are unavailable")
    def test_successful_replacement_applies_posix_modes(self):
        bootstrap.install(self.source, self.destination, replace=True)

        self.assertEqual(0o644, self.destination.stat().st_mode & 0o777)
        self.assertEqual(0o600, self.backups()[0].stat().st_mode & 0o777)


if __name__ == "__main__":
    unittest.main()
