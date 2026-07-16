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


bootstrap = script("skills/contract-bootstrap/scripts/bootstrap-contract.py")


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

    def test_cli_preview_is_complete_and_non_mutating(self):
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
        self.assertEqual([], self.temporary_paths())

    def test_cli_refuses_replacement_without_authority(self):
        mtime = self.destination.stat().st_mtime_ns

        with self.assertRaisesRegex(ValueError, "--replace"):
            self.run_cli(
                "--source", str(self.source), "--destination", str(self.destination), "--apply"
            )

        self.assertEqual(self.old_bytes, self.destination.read_bytes())
        self.assertEqual(mtime, self.destination.stat().st_mtime_ns)
        self.assertEqual([], self.backups())
        self.assertEqual([], self.temporary_paths())

    def test_cli_apply_is_a_no_op_for_identical_bytes(self):
        self.destination.write_bytes(self.source_bytes)
        mtime = self.destination.stat().st_mtime_ns

        result, output = self.run_cli(
            "--source",
            str(self.source),
            "--destination",
            str(self.destination),
            "--apply",
            "--replace",
        )

        self.assertEqual(0, result)
        self.assertIn("no changes", output)
        self.assertIn("unchanged", output)
        self.assertEqual(mtime, self.destination.stat().st_mtime_ns)
        self.assertEqual([], self.backups())

    def test_cli_apply_works_without_fchmod(self):
        with mock.patch.object(bootstrap.os, "fchmod", None):
            result, output = self.run_cli(
                "--source",
                str(self.source),
                "--destination",
                str(self.destination),
                "--apply",
                "--replace",
            )

        self.assertEqual(0, result)
        self.assertIn("installed", output)
        self.assertEqual(self.source_bytes, self.destination.read_bytes())
        self.assertEqual([self.old_bytes], [backup.read_bytes() for backup in self.backups()])
        self.assertEqual([], self.temporary_paths())

    def test_each_replacement_creates_a_new_backup_and_preserves_existing_backups(self):
        unrelated = self.root / "AGENTS.md.backup-pre-existing"
        unrelated.write_bytes(b"unrelated backup\n")
        source_old = self.root / "old.md"
        source_old.write_bytes(self.old_bytes)

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

    def test_backup_failure_cleans_temporary_state_and_closes_descriptor(self):
        self.assert_stage_failures(stage="backup")

    def test_destination_failure_retains_committed_backup_and_cleans_temporary_state(self):
        self.assert_stage_failures(stage="destination")

    def assert_stage_failures(self, stage: str):
        for operation in ("write", "flush", "fsync", "mode", "replace"):
            with self.subTest(stage=stage, operation=operation):
                self.destination.write_bytes(self.old_bytes)
                failure, streams = self.failure_injection(operation, stage)
                with failure, self.assertRaises(OSError):
                    bootstrap.install(self.source, self.destination, replace=True)

                self.assertEqual(self.old_bytes, self.destination.read_bytes())
                self.assertTrue(streams)
                self.assertTrue(all(stream.closed for stream in streams))
                self.assertEqual([], self.temporary_paths())
                if stage == "backup":
                    self.assertEqual([], self.backups())
                else:
                    backups = self.backups()
                    self.assertEqual(1, len(backups))
                    self.assertEqual(self.old_bytes, backups[0].read_bytes())
                for backup in self.backups():
                    backup.unlink()

    def failure_injection(self, operation: str, stage: str):
        occurrence = 1 if stage == "backup" else 2
        streams: list[object] = []
        original_fdopen = bootstrap.os.fdopen
        calls = 0

        class Stream:
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

            def write(self, data):
                nonlocal calls
                if operation == "write":
                    calls += 1
                    if calls == occurrence:
                        raise OSError("injected write failure")
                return self.stream.write(data)

            def flush(self):
                nonlocal calls
                if operation == "flush":
                    calls += 1
                    if calls == occurrence:
                        raise OSError("injected flush failure")
                return self.stream.flush()

        def fdopen(*arguments, **keywords):
            stream = Stream(original_fdopen(*arguments, **keywords))
            streams.append(stream)
            return stream

        failures = contextlib.ExitStack()
        failures.enter_context(mock.patch.object(bootstrap.os, "fdopen", fdopen))

        if operation in {"write", "flush"}:
            return failures, streams

        if operation == "fsync":
            original_fsync = bootstrap.os.fsync
            calls = 0

            def fsync(descriptor):
                nonlocal calls
                calls += 1
                if calls == occurrence:
                    raise OSError("injected fsync failure")
                return original_fsync(descriptor)

            failures.enter_context(mock.patch.object(bootstrap.os, "fsync", fsync))
            return failures, streams

        if operation == "mode":
            original_chmod = bootstrap.os.chmod
            calls = 0

            def chmod(path, mode):
                nonlocal calls
                calls += 1
                if calls == occurrence:
                    raise OSError("injected mode failure")
                return original_chmod(path, mode)

            failures.enter_context(mock.patch.object(bootstrap.os, "chmod", chmod))
            return failures, streams

        original_replace = bootstrap.os.replace
        calls = 0

        def replace(source, destination):
            nonlocal calls
            calls += 1
            if calls == occurrence:
                raise OSError("injected replace failure")
            return original_replace(source, destination)

        failures.enter_context(mock.patch.object(bootstrap.os, "replace", replace))
        return failures, streams

    def test_available_fchmod_error_propagates_and_cleans_up(self):
        with mock.patch.object(bootstrap.os, "fchmod", side_effect=PermissionError("denied")):
            with self.assertRaises(PermissionError):
                bootstrap.install(self.source, self.destination, replace=True)

        self.assertEqual(self.old_bytes, self.destination.read_bytes())
        self.assertEqual([], self.backups())
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
