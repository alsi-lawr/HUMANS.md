from __future__ import annotations

import hashlib
import tempfile
import unittest
from pathlib import Path

from _load import script


bootstrap = script("skills/contract-bootstrap/scripts/bootstrap-contract.py")


class ContractBootstrapTests(unittest.TestCase):
    def test_preview_replace_backup_and_idempotence(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "source.md"
            destination = root / "AGENTS.md"
            source.write_text("# New contract\n", encoding="ascii")
            old = b"# Old contract\n"
            destination.write_bytes(old)
            _, _, diff = bootstrap.preview(source, destination)
            self.assertIn("-# Old contract", diff)
            with self.assertRaises(ValueError):
                bootstrap.install(source, destination, replace=False)
            self.assertEqual("installed", bootstrap.install(source, destination, replace=True))
            backup = destination.with_name(f"AGENTS.md.backup-{hashlib.sha256(old).hexdigest()}")
            self.assertEqual(old, backup.read_bytes())
            mtime = destination.stat().st_mtime_ns
            self.assertEqual("unchanged", bootstrap.install(source, destination, replace=True))
            self.assertEqual(mtime, destination.stat().st_mtime_ns)


if __name__ == "__main__":
    unittest.main()
