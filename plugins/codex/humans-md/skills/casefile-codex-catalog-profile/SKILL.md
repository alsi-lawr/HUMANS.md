---
name: casefile-codex-catalog-profile
description: "Use when a human explicitly asks to profile a caller-supplied fresh Codex bundled-model export for the humans-md Casefile profiles. Rejects cache paths and writes only a guarded candidate target."
---

# Casefile Codex Catalog Profile

Export the current bundled catalog into a new explicit file, then invoke `${CODEX_PLUGIN_ROOT}/scripts/profile-codex-catalog.py` with that caller-attested file, the packaged canonical profile, a separate target, and a backup directory. Preview first; inspect stale-model reporting, authored resource hashes, declared selector changes, and the warning that a renamed file's freshness cannot be mechanically attested.

Apply only after approval. The tool records hash-addressed pristine and last-installed backups, sets only declared selectors to JSON null, writes atomically with restrictive permissions, verifies strictly, and restores prior bytes, mode, and mtime on failure. It rejects missing, duplicate, or unsupported models, resource hash drift, symlink inputs, aliased input/target paths, and any path named `models_cache.json`.
