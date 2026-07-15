---
name: casefile-codex-catalog-profile
description: "Use when a human explicitly asks to profile a fresh Codex bundled-model export for the humans-md Casefile profiles. Rejects cache files and writes only a guarded candidate target."
---

# Casefile Codex Catalog Profile

Export the current bundled catalog into a fresh explicit file, then invoke `${CODEX_PLUGIN_ROOT}/scripts/profile-codex-catalog.py` with that file, the packaged canonical profile, a separate target, and a backup directory. Preview first; inspect stale-model reporting and every changed allowlisted field.

Apply only after approval. The tool records hash-addressed pristine and last-installed backups, sets declared selectors to JSON null, writes atomically with restrictive permissions, verifies strictly, and restores the prior target on failure. It rejects missing, duplicate, or unsupported models and any path named `models_cache.json`.
