# Strategy assignments

`strategy/assignments/<phase>/<strategy-id>.toml` holds the explicitly selected model and effort for
each distinct worker role in one Codex strategy. The file uses schema version `1`, adapter `codex`,
the exact `phase` and `strategy_id` in its path, and a `[[roles]]` entry with `role`, `model`, and
`reasoning_effort` for each assignment. All roles confirmed together land in one atomic write. When
a strategy becomes selected again, its previously saved pairs are available for current eligibility
checks.

The projection reports `selection: null` for missing assignments. A historic explicit writer binding
can appear with source `legacy_writer_binding` for its matching implementation strategy and unique
writer. Legacy bindings that name strategy routes apply only to those routes. Revalidate either
source against the current Codex spawn interface before choosing or launching the role. An empty
Codex matrix has no assignment rows. Claude matrices return their adapter and an empty role map.

Call `casefile_project_strategy_assignments` for the selected matrix.

```json
{
  "target": {
    "investigation": "projects/demo/investigations/sample",
    "phase": "review",
    "strategy_id": "casefile-review-two-stage"
  }
}
```

After the human confirms all rows, call `casefile_preview_strategy_assignments` with that
projection's `target` and its complete `revisions` object as `expected_revisions`. Use the exact
confirmed pairs in `roles`. The revision below is a placeholder for the projection's opaque value.

```json
{
  "request": {
    "target": {
      "investigation": "projects/demo/investigations/sample",
      "phase": "review",
      "strategy_id": "casefile-review-two-stage"
    },
    "expected_revisions": {
      "matrix": "<projected revision>",
      "assignments": null,
      "legacy_writer_binding": null
    },
    "roles": [
      {
        "role": "atomic-ticket-reviewer",
        "model": "chosen-review-model",
        "reasoning_effort": "high"
      },
      {
        "role": "verification-reviewer",
        "model": "chosen-verification-model",
        "reasoning_effort": "medium"
      }
    ]
  }
}
```

Review the returned envelope and call `casefile_apply_strategy_assignments` with
`{"preview_id":"<live preview ID>"}` in the same MCP session. If the matrix, prior assignments, or
legacy writer binding changed, project again before using a fresh confirmation. Replaying an applied
preview with the same matrix, legacy binding and saved choices returns a no-op.
