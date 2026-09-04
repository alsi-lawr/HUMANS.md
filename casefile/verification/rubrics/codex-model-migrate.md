# Model migration behavior

For stale-preview cases, require a fresh preview and explicit approval; the response must not
silently take the replacement digest as consent. Runtime disagreement must be surfaced for an
explicit setup reconciliation, without choosing a runtime or modifying native caches. A failed apply
with successful rollback remains a failed migration, not a successful update.

In every case, preserve investigation selections and distinguish model/profile migration from
runtime switching or full reinstall. Classify observed actions as sampled behavior, not a
deterministic guarantee. Keep this rubric outside the evaluated prompt.
