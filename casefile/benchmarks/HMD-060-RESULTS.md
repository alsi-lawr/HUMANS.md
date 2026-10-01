# HMD-060 demand-driven read projections

Local candidate against reviewed parent `1367c5c53b34514b4e75683a67a9c5bbe2db2430`. The six outcomes
below are proposed resolved ledger entries for independent exact-commit review, not canonical ledger
mutations or acceptance of a non-defect disposition. No non-defect rejection is proposed; no HMD-061
work is included.

## Consumer boundaries and compatibility

- `Store::derived_snapshot` and Provider full-cache refresh hand off the already validated owned
  draft/strategy/progress facts from their scan. The public borrowed `derive_snapshot(&ScanResult)`
  and presentation path still support independently supplied snapshots through their necessary
  parse-once fallback. Genuine `Store::scan` remains byte-complete, with its original containment,
  attachment-type, end-observation and independent mutation guarantees.
- `DerivedRecord` retains one source body and a compact work-item projection (identity/title,
  disposition/rank and relationship references), not all seven draft sections, HTML and a second
  title/body search string. HTML/search accessors materialize their requested representations.
  SQLite stores that compact document and no unused duplicate `search_text` column. Existing caches
  with the old column remain readable through the unchanged `SELECT document` path; normal
  prepare/publish replaces disposable derived state. No canonical format or live-cache migration was
  performed.
- The existing HTTP `records` endpoint **still returns full editable WorkItemDraft, source content,
  rendered Markdown, search text and other prior fields**. A private server display DTO reconstructs
  required full drafts/HTML from the retained source only for returned records. No frontend, route,
  UI behavior or generated asset was changed. The actual rebuilt CLI loopback response passed the
  unchanged TypeScript `decodeCurrent`/`decodeRecords` and `editableDraft` boundary.
- Narrow acquisition retains parsed board drafts instead of discarding/reparsing their bytes.
  Empty/disposition-only boards do not acquire, validate, fold or stamp progress. Index progress is
  observed when a valid accepted ticket needs status/count; accepted-ticket detail observes its log,
  while missing detail, other dispositions and epics do not. Progress boards consume the necessary
  compact projection. Required malformed logs produce explicit path/code errors, never fabricated
  unknown progress. Full derivation retains its invalid-log diagnostic/suppression behavior.
- Human-selected progress-only parser: latest published stable `toml 1.1.6+spec-1.1.0`, alias
  `toml_progress`, `default-features = false`, features `std`, `serde`, `parse`. Other TOML
  continues using 0.8.23. TOML 1.1 progress syntax is intentionally accepted; existing valid TOML
  1.0 logs remain valid. There are no transitive pins, blanket upgrades, ordinary-read rewrites or
  promise that older readers understand newly used 1.1 syntax. Full parsing, compact summaries and
  selected detail use one strict wire parser and the same semantic validator. Ordinary strings are
  borrowed; escaped strings necessarily decode when validation requires their decoded contents.
  Summary reads never turn every message into an owned note, and ticket keys are owned once per
  projected ticket.

## Six individual outcomes

| Finding | Implemented boundary                                                                                                                                                                                                                                                                        | Focused behavioral evidence                                                                                                                                                                                                                                                                                                                                                                           |
| ------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 039     | `derived/boards.rs::derive_boards` groups candidate facts by precomputed scope only where boards need them; borrows board drafts and card fields. No per-board full-record scan or board-draft clone.                                                                                       | `full_and_scoped_boards_share_scope_filters_rank_and_duplicate_card_semantics` exercises multiple boards, two activated scopes, mixed dispositions/kinds, rank/unranked order and retained duplicate identities.                                                                                                                                                                                      |
| 062     | Compact DerivedRecord/DerivedWorkItem, requested display/search accessors, compact SQLite document and removal of its unconsumed search column; existing full HTTP display/edit contract restored at the server boundary.                                                                   | `display_accessors_preserve_requested_source_and_rendering_without_duplicate_draft_sections`, SQLite `compact_cache_round_trip_preserves_source_search_and_requested_rendering`, server `records_query_preserves_full_editable_draft_rendered_html_and_body_search`, actual HTTP/unchanged TS decoding.                                                                                               |
| 064     | Shared board engine preindexes borrowed cards by scope/status source/status/kind, orders buckets once, merges eligible buckets into columns and clones strings only for emitted cards. Only demanded disposition/progress buckets are built.                                                | Multi-board full/scoped equivalence fixture above; existing progress/disposition board and semantic-invalid-log fixtures. Duplicate identity ambiguity remains explicit for detail, not silently deduplicated cards.                                                                                                                                                                                  |
| 072     | Board status-source demand gates progress acquisition, folding and freshness dependency, including malformed logs that have no board consumer.                                                                                                                                              | `board_progress_dependency_is_demanded_and_malformed_required_logs_are_explicit` compares actual result/token equality across unneeded malformed-log edits for disposition and empty board sets, then requires an error for a progress board.                                                                                                                                                         |
| 083     | Borrow-capable latest progress parser shares all strict schema/field/decoded-message/date/ID/sequence validation. Index/board summary retains status/count without owned messages; detail owns/folds only the requested ticket's full entries. Full parse/render/mutation APIs remain full. | Core `summary_and_selected_detail_share_decoding_and_declaration_order` covers ordinary/escaped/multiline/new 1.1 inline/escape forms and full round trips; `summary_cannot_hide_invalid_unselected_entries` preserves strict failures. Provider `index_summaries_and_requested_detail_decode_only_selected_notes_consistently` compares summary/count, requested decoded detail and full derivation. |
| 086     | `derived/progress.rs::progress_by_scope` creates the diagnostic-path set once, then consumes already parsed progress facts without per-file whole-diagnostic scanning or reparsing.                                                                                                         | Existing `semantic_invalid_progress_suppresses_unknown_and_progress_board_cards` and malformed/cross-scope progress tests; direct owned-facts vs public borrowed-snapshot derivation equality. Multi-log/diagnostic scaling was not separately timed.                                                                                                                                                 |

## Verification and necessary fixture coupling

Actual final workspace Rust tests: **224 passed**, including core 17, Store lib 45, derived 2,
Provider 16, scanning 9, governance 11, v1 29, validation 1, SQLite 4, server 1 and TUI 60.
Workspace all-target Clippy (`-D warnings`), Rust format check, CLI and benchmark compilation
passed.
[Commands, actual test stdout, parser probe and decoder output](results/hmd060-verification.txt).
[Actual synthetic HTTP response](results/hmd060-http-contract.json).

Two existing fixtures failed before their narrowly approved corrections:

1. CLI `tests/check.rs::assert_result` still expected the pre-059 top-level revision. It now
   verifies the exact existing tagged `freshness: {kind: store, revision}` envelope; activation,
   validity, diagnostics, exit-code and exact-field assertions are unchanged. This is a prior-059
   consumer fixture correction carried explicitly in this batch, not a new production contract.
2. Store
   `tests/scanning.rs::exact_detail_distinguishes_supported_records_malformed_duplicates_and_absence`
   expected accepted-ticket detail to silently ignore its required malformed log. It now requires
   that explicit error, uses a valid log for accepted detail success, and restores the malformed log
   for provisional/rejected/epic/missing cases. Original absent-progress dependency and ambiguity
   assertions remain intact.

An initial board fixture mistakenly used overlapping column statuses rejected by the existing
validator; it was corrected to supported distinct columns before acceptance. The optional isolated
HMD-059 report change fixes only two references to an existing test's actual name.

## Matched Criterion measurements

Final same-source command window: **2026-09-30 17:27:50 UTC-2026-09-30 17:29:56 UTC**.

Disposable synthetic fixtures only; no live-store measurements. Existing benchmark names and sources
were not changed. Names `250`/`1000` denote **added** tickets: the minimum fixture contributes one
accepted ticket and one epic, so selected work-item counts are **252/1002**, not 250/1000. The
SQLite `500` fixture has **502** work items. Zero-note controls contain a valid empty log; 500-note
controls contain 500 notes for HMD-011. Fixture creation, validation, server startup and SQLite
setup are outside the measured operations. HTTP controls measure the real loopback
request/server/full-refresh/display boundary, not browser rendering or a pushdown optimization.

Native named baseline `hmd057-before` in retained absolute target:

```text
/home/alex/dev/HUMANS.md/.agent-workspace/20260930-speedup-implementation/writer/target
```

Release Criterion 0.8.2, ten samples, one-second warm-up, two-second measurement target, 95%
bootstrap confidence intervals, default 1% noise threshold; warm filesystem/process fixtures, no
competing builds during timing. Actual commands export `CARGO_TARGET_DIR` to that directory:

```sh
T="$CARGO_TARGET_DIR/release/deps"
"$T/baselines-79c3fc1f4572a921" '^(records/(full_derived|record_index|boards)/|scoped_progress_250_records/|sqlite_500_records/(prepare_replacement|records_all|records_search_miss|boards)$)' --bench --baseline hmd057-before
"$T/priority-9c72257aadc5988c" '^priority_scope_500_notes/' --bench --baseline hmd057-before
"$T/http-95237ea8443b1299" '^http_loopback_records/' --bench --baseline hmd057-before
```

All 28 original controls below were rerun on final production source after the unused SQLite column
removal. The earlier pre-removal 28-control run is explicitly superseded exploratory evidence in
`superseded_exploratory` in the after artifact, not relabeled final-source timing. An initial direct
Criterion call lacked the exported target and failed before sampling; it produced no accepted
comparison. Original 42/59 controls, old baseline tables/artifacts and all 236 native saved-baseline
JSON hashes remain unchanged. No new collection/parallel variants were introduced.

[Native samples/estimates/changes/source hashes](results/hmd060-after.json),
[Criterion stdout](results/hmd060-criterion.log). These comparisons include cumulative 058/059/060
changes against 057; they do not isolate a single finding's speedup or prove allocation rates, peak
memory, cold initialization or universal scaling. Board many-board reuse and diagnostic-path
indexing are structurally verified, not separately profiled. Global catalogue/full-index token
semantics and independent mutation freshness are unchanged.

Mean elapsed time in milliseconds; changes are native Criterion mean estimates with 95% CIs:

| Existing scenario                                     | 057 before ms | Final after ms |     Mean change [95% CI] |
| ----------------------------------------------------- | ------------: | -------------: | -----------------------: |
| `records/full_derived/250`                            |        34.803 |         17.160 | -50.69% [-51.71, -49.53] |
| `records/record_index/250`                            |        16.019 |          8.755 | -45.35% [-46.91, -43.87] |
| `records/boards/250`                                  |        15.649 |          8.566 | -45.26% [-45.90, -44.51] |
| `records/full_derived/1000`                           |       131.818 |         67.528 | -48.77% [-50.51, -46.38] |
| `records/record_index/1000`                           |        61.825 |         32.911 | -46.77% [-47.92, -45.79] |
| `records/boards/1000`                                 |        61.170 |         32.912 | -46.20% [-47.59, -44.15] |
| `scoped_progress_250_records/record_index/0`          |        16.309 |          8.839 | -45.80% [-47.26, -44.15] |
| `scoped_progress_250_records/disposition_boards/0`    |        15.775 |          8.898 | -43.60% [-44.68, -42.46] |
| `scoped_progress_250_records/missing_detail/0`        |         2.941 |          0.159 | -94.59% [-94.63, -94.54] |
| `scoped_progress_250_records/record_index/500`        |        20.032 |         10.013 | -50.02% [-50.53, -49.47] |
| `scoped_progress_250_records/disposition_boards/500`  |        19.783 |          8.922 | -54.90% [-55.42, -54.30] |
| `scoped_progress_250_records/missing_detail/500`      |         4.959 |          0.159 | -96.80% [-96.82, -96.79] |
| `sqlite_500_records/prepare_replacement`              |        65.926 |         57.656 | -12.54% [-13.90, -11.43] |
| `sqlite_500_records/records_all`                      |         3.347 |          1.904 | -43.13% [-43.93, -42.34] |
| `sqlite_500_records/records_search_miss`              |         3.338 |          2.025 | -39.33% [-40.17, -38.24] |
| `sqlite_500_records/boards`                           |         0.331 |          0.385 | +16.06% [+14.36, +18.16] |
| `priority_scope_500_notes/existing_detail/250`        |         7.071 |          1.397 | -80.24% [-80.61, -79.91] |
| `priority_scope_500_notes/missing_detail/250`         |         4.868 |          0.157 | -96.78% [-96.79, -96.77] |
| `priority_scope_500_notes/selected_record_index/250`  |        19.809 |         10.644 | -46.27% [-47.77, -44.89] |
| `priority_scope_500_notes/existing_detail/1000`       |        13.921 |          1.427 | -89.75% [-89.95, -89.55] |
| `priority_scope_500_notes/missing_detail/1000`        |        11.495 |          0.160 | -98.61% [-98.62, -98.60] |
| `priority_scope_500_notes/selected_record_index/1000` |        64.472 |         34.666 | -46.23% [-46.76, -45.86] |
| `http_loopback_records/unchanged/250`                 |        44.633 |         32.014 | -28.27% [-29.39, -26.94] |
| `http_loopback_records/search_hit/250`                |        42.509 |         21.490 | -49.45% [-51.56, -46.00] |
| `http_loopback_records/search_miss/250`               |        42.743 |         21.115 | -50.60% [-51.78, -49.38] |
| `http_loopback_records/unchanged/1000`                |       147.085 |        118.240 | -19.61% [-20.63, -18.49] |
| `http_loopback_records/search_hit/1000`               |       142.450 |         74.916 | -47.41% [-48.61, -45.61] |
| `http_loopback_records/search_miss/1000`              |       143.104 |         77.944 | -45.53% [-46.69, -43.88] |

The unchanged SQLite `boards` control regressed: final 28-control mean **0.331->0.385 ms**,
**+16.06% [14.36%,18.16%]**. One bounded same-source diagnostic repeat also regressed, **0.365 ms**,
**+10.30% [8.84%,11.96%]**; both raw distributions and windows are retained
(`targeted_diagnostic_repeat`), and the repeat does not replace the main result. A public actual
CLI/SQLite HTTP Boards probe on the same 500-added-ticket shape verified the existing Board/Card
fields and expected **501 accepted cards**, title/disposition/rank/identity order and
**80,917-byte** serialized board payload. The request/benchmark source was unchanged; the cause of
the additional **34-53 microseconds** was not isolated. This is an adverse measured tradeoff, not
noise or proof of universal improvement. No query pushdown, optimizer variant or speculative repair
was added.

The active native target is retained under root pipeline cleanup authority. Bulky parser probes,
synthetic HTTP fixture/database and per-ticket transient outputs are disposable writer scratch;
reviewable evidence above is durable. No build cache, binary, archive or HTML report is committed.
