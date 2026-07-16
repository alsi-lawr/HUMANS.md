---
name: contract-bootstrap
description: "Use when a human explicitly asks to preview or apply the repository behaviour contract to a target. Never run during package installation or merge ambiguous contract text."
---

# Contract Bootstrap

Require explicit source and destination files. A packaged canonical source is available at `<plugin-root>/templates/AGENTS.md`, but never select or apply it implicitly. Run the bundled script without `--apply` first and show the destination plus complete unified diff. If the destination differs, require explicit replacement authority; never synthesize or merge contracts.

On apply, preserve an existing target in a fresh collision-safe sibling backup before atomic replacement. Keep restrictive temporary-file permissions and verify the written bytes. Identical source and destination is a successful no-op that preserves mtime.

Installation never invokes bootstrap. Load `scripts/bootstrap-contract.py` only for an explicit bootstrap request.
