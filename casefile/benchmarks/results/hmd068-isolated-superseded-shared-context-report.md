# Superseded shared-context integration report

The following historical measurements are not the new isolated comparison. Its numeric samples and
failed attempts remain evidence; archive byte-preservation policy has been superseded.

# HMD-068: integrated local verification

Local-only candidate on `casefile-speedup-cleanup-20260930`, parent
`18b71c8bbd70db48b1f12832e41084c0e918a237`. No push, release, installation, restart, live-store
migration or hosted/platform acceptance. Root owns the canonical ledger, human decisions and final
acceptance. This report records actual checks, not self-acceptance.

## Required corrections

- Finding 112: SQLite staging previously wrote `user_version` and five schema tables before its
  existing transaction. The same transaction now covers schema, metadata, prepared inserts and
  indexes. `journal_mode=DELETE` remains outside; schema 1, default durability, atomic publication,
  stale/current/degraded behavior and ordinary errors are unchanged. Existing SQLite behavioral
  tests exercise replacement, failed replacement, freshness, scoped Unicode search, ambiguity and
  relationships. No new test asserting source/transaction registration was added. All eight existing
  [SQLite integration tests](../casefile-store-sqlite/tests/integration.rs) pass on the final
  source, including `replacement_index_is_revision_bound_repairable_and_queryable`,
  `failed_replacement_keeps_published_index_and_prior_schema_rebuilds_as_missing`,
  `exact_duplicate_identity_is_ambiguous_in_canonical_and_indexed_reads`,
  `pushed_scope_and_search_match_rust_unicode_substrings_in_path_order` and
  `relationship_endpoints_and_board_scopes_preserve_nullable_identity_and_order`.
- Finding 113 / human amendment: old caches update through the existing atomic Provider rebuild.
  `SqliteIndex::open` inspects recognized existing metadata/schema once through a read-only
  connection; ordinary probe failures propagate. A private pending-upgrade atomic routes legacy
  reads through Missing until successful persist clears it. Stale/failed prepare or publication
  preserves prior bytes and the pending upgrade. Subsequent checked reads retain source-revision
  freshness without schema/version SQL. No constructor Store acquisition, schema/API/dependency
  change, malformed-artifact repair, migration registry or new framework. Deliberate later external
  old-format substitution is not additional compatibility support. The existing legacy test now
  exercises first actual Provider indexed read, subsequent Current/stable publication, failed
  preparation/revision observation and stale publication; all eight SQLite tests pass.
- Two stale Provider 4 Python mock constants now match source Provider 5; their mock package version
  and behavior assertions are unchanged. The source Casefile skill was formatted with unchanged
  whitespace-normalized words. These are narrow root-gated integration corrections, not installed
  package updates.

The first all-59 distribution is preserved in
[pre-transaction results](results/hmd068-pre-transaction-after.json) and
[literal output](results/hmd068-initial-criterion.log). SQLite preparation was 390.668 ms versus
65.926 ms before: +492.59%, 95% change CI [460.33%, 541.38%]. All ten samples were slow, not one
isolated outlier. Boards was +14.39% [13.94%, 14.83%]. Neither is explained away as noise.
[Read-only investigation](results/hmd068-prepare-investigation.json) retains the actual samples,
original fixture identity, earlier 064 mixed results and local locked upstream API evidence.
Removing concrete redundant autocommits is not proof of the cause of the entire observed slowdown.
The second post-transaction all-59 run is separately preserved in
[post-transaction results](results/hmd068-post-transaction-after.json),
[output](results/hmd068-post-transaction-criterion.log) and
[exact source diff](results/hmd068-post-transaction-source-diff.txt). Its preparation arithmetic
mean was 14.814 ms (-77.53% versus 057), while boards remained +23.124% [22.616%, 23.640%]. This
prompted human-directed one-time cache upgrade, not another timing variant or a noise dismissal.

## Integrated verification and preservation

[All literal command exits](results/hmd068-commands.json) and
[all initial, supplementary, isolated and final output](results/hmd068-verification.txt) distinguish
failed attempts from successful reruns. On the final source in the unchanged repo-pinned temporary
Nix shell, Rust workspace **285 tests**, all-target Clippy with warnings denied and Rust formatting
pass. Browser frozen install, typecheck, **37 tests** and production build pass. The unchanged full
Python helper passes **113 tests** (22 shared, 17 humans-md, 70 Casefile, 4 coding) in a
hash-matched, owned current-source copy. Source package-root and coding skill validators pass. Real
supported `--root` builds/checks of humans-md and coding produce 70 inventoried files, not six
native Casefile release artifacts.

The first direct Python helper failed two stale protocol fixtures; the first formatter failed the
source skill plus archived evidence. Subsequent supplemental package tests unexpectedly wrote
primary ignored `build/marketplace` outputs and default `casefile/target`. No pre-suite byte
snapshot of those ignored directories exists. A later observation identified eight changed metadata
files; it is not a before-suite preservation proof. Their unknown prior contents were not deleted or
restored. Corrected verification ran only in owned source copies with the absolute retained target.
[Preservation inventory and limitation](results/hmd068-workspace-preservation.json) identifies what
was actually known: 84 tracked prior-evidence/bundle hashes were captured before Bun; 506 ignored
file hashes were observed later, during the supplemental run.

The first correction-2 attempt used changed host tools: rustup could not locate cargo, which also
failed five native browser launches and Python switch-test setup. Host Bun 1.4.2 briefly regenerated
JS differently. Actual pinned Bun 1.3.13 production build reproduced accepted JS/CSS byte-for-byte;
no manual asset restoration or asset delta is included. Pinned Cargo/Rust 1.95.0 (rustc commit
`59807616e1fa2540724bfbac14d7976d7e4a3860`, LLVM 21.1.8), Python 3.14.6 and unchanged `flake.lock`
were verified. No profile, package, flake or toolchain update was made.

The fresh source release CLI and actual `casefile/adapters/shared/casefile_runtime.py::probe` pass
on an absolute disposable synthetic Store: version 0.8.0, Provider 5, dated MCP 2025-11-25, two
responses and twelve distinct tools.
[Final native adapter proof](results/hmd068-correction3-adapter-smoke.json) preserves actual
commands, output and binary/adapter hashes. Installed 0.8.0 Provider 3 remains untouched.

**Archive policy superseded:** the human explicitly authorized meaning-preserving formatting and
ASCII normalization rather than literal byte preservation. Historical failed check exits remain
recorded. Existing checker policies and exclusions are unchanged; final normalized checks and the
new genuinely isolated historical/current comparison are required before immutable handoff.

## Coverage and original human leads

Root's read-only coverage preflight verifies its then-current contiguous 001-112 IDs, unique primary
owners and existing evidence links: 111 independently accepted outcomes plus assigned 112, not
semantic re-review or final acceptance. Root subsequently recorded human-directed finding 113; 112
and 113 are proposed outcomes awaiting this immutable review, not self-closed ledger rows. All
eleven prerequisite immutable batches and their individual finding evidence remain byte-preserved.
This integrated check does not replace those reviews.

| Original lead          | Concrete evidence and bounded interpretation                                                                                                                                                                                                                                                                                                                                                                                     |
| ---------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Raw bytes / strings    | 059 narrow fact acquisition and 060 consumer-required projections avoid unrelated body/message ownership; full genuine byte consumers retain their contract.                                                                                                                                                                                                                                                                     |
| Rayon opportunities    | 059 actual serial, bounded Rayon 2, sorted-vector and combined variants retain measured read-stage wins and mixed larger-query intervals. Zero/one candidates stay serial. Later batches remove redundant work and preserve ordered mutation/progress/UI state. No claim all alternatives were measured or scheduling is optimal; first-pool, CPU/contention and broad threading remain unmeasured. Root owns final disposition. |
| BTreeMap tradeoff      | 059 real sorted-vector plus bounded Rayon selection, with both mixed large-case comparisons preserved; no universal collection superiority.                                                                                                                                                                                                                                                                                      |
| Iterator-to-Vec waste  | 058 core parsed-value reuse, 059 scoped facts, 066 direct SQL decode consumption and 067 maintained viewport projections remove actual intermediate/repeated work.                                                                                                                                                                                                                                                               |
| Cardinality-one lookup | 059 exact candidates/ambiguity and 064 SQL `LIMIT 2` preserve zero/one/multiple semantics without full-result decoding.                                                                                                                                                                                                                                                                                                          |
| Recursive walking      | 059 iterative pruned non-following discovery and 062 native identity/incremental reuse retain explicit unsafe governed entries; rare deliberate ancestor swap requires full refresh by human choice, not an anchored/CAS guarantee.                                                                                                                                                                                              |

## Measurement contract

Final measurements are cumulative 057-to-integrated results, not isolated 068 or per-finding causal
attribution. Original 59 controls use unchanged synthetic fixtures/counts/setup/timing. Labels
250/500/1000 denote added tickets; the minimum fixture contributes two work items, so actual totals
are 252/502/1002. Inventory labels denote added opaque files, and scoped progress controls retain
their stated 0/500 note counts. Original baselines/TUI source is preserved; 063 priority ID-only API
and 066 production HTTP transport require explicit narrow harness-source changes already reviewed,
so not all original benchmark source bytes are identical. HTTP retains original HTTP/1.0
Connection-close payload/timing. No new controls, variants, profiling, synthetic before, rehydrated
baseline or winner repeats.

Criterion 0.8.2, release, ten samples, one-second warmup, two-second measurement target, 95% change
interval, one-percent noise threshold, same host and warm filesystem. Slow controls may extend the
sample target automatically. No CPU affinity/frequency control. Builds/tests end before the single
final correction-3 all-59 run; root/reviewer do not compete. Absolute retained target:

```text
/home/alex/dev/HUMANS.md/.agent-workspace/20260930-speedup-implementation/writer/target
```

Each original executable is discovered (`--list`), smoked (`--test`) then measured once:
`--bench --baseline hmd057-before`. Complete samples, arithmetic-mean estimates, change intervals,
outlier fences, configuration and source/binary SHA256 are retained. All actual 236 named original
before JSON objects are archived as relative path, SHA256 and original UTF-8 text in
[byte-exact native-before archive](results/hmd068-original-native-before.json); decoding is verified
against original bytes/hashes. This is an archive, not a newly generated timing baseline. Root must
independently verify it before deleting retained caches.

Original TUI detail selects governed epic **HMD-E-001**. The fixture separately contains HMD-011's
80 paragraphs and 500 notes; those are not selected long-ticket/progress-detail timing. No measured
first-layout, heap, peak-memory, board/watch, cold-disk, browser end-to-end, Windows/macOS runtime,
real six-package release, authenticated service or hosted CI claim. `cold_session` means a new
presentation session on a warm filesystem. Structural resolution is not measured speedup.

## Final all-59 results

[Final complete results](results/hmd068-after.json) and
[literal final output](results/hmd068-criterion.log) are correction-3 source-identical. All four
discovery, four smoke and four measured executable commands succeed across the original 59 controls;
source and four executable hashes are unchanged across the run, and all 236 native-before objects
plus four original baseline artifacts retain their hashes. Criterion reports 57 improved controls,
one within its one-percent noise threshold, and one regressed control (SQLite boards). Every control
is included below, including adverse or statistically unclear results. Times are arithmetic means in
milliseconds; change is Criterion's mean change estimate and 95% interval. Full raw
samples/iterations, fences and literal outlier classifications are retained. No zero-regression gate
is invented.

The boards control directly calls `SqliteIndex::boards`, not Provider/publication/watch guards. Its
board query/body remains unchanged; the final source removes recurring schema SQL from `checked()`
but the interval remains adverse. The prior joined query therefore is not proven to explain the
entire regression. This is not noise or a gain; no additional timing repeat, profiler or causal
claim is made.

| Original control                                       | Before ms | After ms | Change % | 95% change CI %  |
| ------------------------------------------------------ | --------: | -------: | -------: | ---------------- |
| `activation_roots/scoped_check/1`                      |   17.0867 |   0.9177 |   -94.63 | [-94.69, -94.55] |
| `activation_roots/scoped_check/100`                    |   17.4747 |   2.6120 |   -85.05 | [-85.21, -84.87] |
| `core_parsing/progress_parse_500_notes`                |    2.1329 |   1.0653 |   -50.06 | [-50.45, -49.65] |
| `core_parsing/ticket_parse`                            |    0.0315 |   0.0274 |   -13.09 | [-15.07, -11.30] |
| `core_parsing/ticket_render_roundtrip`                 |    0.0362 |   0.0296 |   -17.99 | [-18.68, -17.31] |
| `http_loopback_records/search_hit/1000`                |  142.4501 |  20.1397 |   -85.86 | [-86.04, -85.71] |
| `http_loopback_records/search_hit/250`                 |   42.5091 |   6.2658 |   -85.26 | [-85.50, -85.06] |
| `http_loopback_records/search_miss/1000`               |  143.1035 |  20.2363 |   -85.86 | [-86.09, -85.64] |
| `http_loopback_records/search_miss/250`                |   42.7427 |   5.9755 |   -86.02 | [-86.25, -85.86] |
| `http_loopback_records/unchanged/1000`                 |  147.0852 |  58.1511 |   -60.46 | [-60.71, -60.25] |
| `http_loopback_records/unchanged/250`                  |   44.6332 |  17.0222 |   -61.86 | [-62.32, -61.33] |
| `inventory/full_scan/2000`                             |   42.2502 |  38.7463 |    -8.29 | [-10.08, -6.16]  |
| `inventory/full_scan/8000`                             |  172.0206 | 148.0998 |   -13.91 | [-15.12, -12.69] |
| `inventory/metadata_summary/2000`                      |    8.0633 |   6.7119 |   -16.76 | [-17.18, -16.29] |
| `inventory/metadata_summary/8000`                      |   31.6830 |  27.3555 |   -13.66 | [-14.49, -12.85] |
| `inventory/scoped_check/2000`                          |   17.3262 |   0.9192 |   -94.70 | [-94.78, -94.57] |
| `inventory/scoped_check/8000`                          |   66.7175 |   0.9055 |   -98.64 | [-98.65, -98.64] |
| `mutation_preview_250_records/replace_batch/1`         |   25.5730 |  24.1700 |    -5.49 | [-7.00, -4.03]   |
| `mutation_preview_250_records/replace_batch/10`        |   44.9581 |  40.2055 |   -10.57 | [-11.68, -9.33]  |
| `presentation_250_records/cold_session`                |   34.8275 |  16.8157 |   -51.72 | [-52.21, -51.28] |
| `presentation_250_records/unchanged_refresh`           |    1.4228 |   1.4010 |    -1.53 | [-2.01, -0.98]   |
| `priority_scope_500_notes/existing_detail/1000`        |   13.9208 |   1.3579 |   -90.25 | [-90.42, -90.09] |
| `priority_scope_500_notes/existing_detail/250`         |    7.0712 |   1.3604 |   -80.76 | [-80.89, -80.61] |
| `priority_scope_500_notes/missing_detail/1000`         |   11.4949 |   0.1656 |   -98.56 | [-98.57, -98.55] |
| `priority_scope_500_notes/missing_detail/250`          |    4.8680 |   0.1705 |   -96.50 | [-96.55, -96.46] |
| `priority_scope_500_notes/selected_record_index/1000`  |   64.4724 |  35.1098 |   -45.54 | [-46.70, -44.29] |
| `priority_scope_500_notes/selected_record_index/250`   |   19.8094 |  10.0215 |   -49.41 | [-50.29, -48.63] |
| `priority_single_record_250_records_500_notes/apply`   |   30.5168 |  26.6782 |   -12.58 | [-14.09, -11.26] |
| `priority_single_record_250_records_500_notes/preview` |   30.8792 |  26.2157 |   -15.10 | [-16.49, -13.71] |
| `progress/accepted_target/250`                         |   11.5160 |   1.5343 |   -86.68 | [-86.77, -86.58] |
| `progress/accepted_target/500`                         |   13.5455 |   2.1161 |   -84.38 | [-84.46, -84.29] |
| `progress/missing_targets/250`                         |   28.9013 |   1.6705 |   -94.22 | [-94.25, -94.18] |
| `progress/missing_targets/500`                         |   48.3525 |   2.3552 |   -95.13 | [-95.15, -95.10] |
| `records/boards/1000`                                  |   61.1695 |  34.3108 |   -43.91 | [-45.06, -42.90] |
| `records/boards/250`                                   |   15.6486 |   9.1521 |   -41.52 | [-42.32, -40.72] |
| `records/full_derived/1000`                            |  131.8179 |  67.3727 |   -48.89 | [-49.81, -47.93] |
| `records/full_derived/250`                             |   34.8029 |  18.7789 |   -46.04 | [-47.42, -44.43] |
| `records/record_index/1000`                            |   61.8248 |  33.6938 |   -45.50 | [-46.85, -44.23] |
| `records/record_index/250`                             |   16.0193 |   8.7469 |   -45.40 | [-46.94, -43.90] |
| `records/scoped_check/1000`                            |   98.7873 |  66.9850 |   -32.19 | [-33.93, -30.55] |
| `records/scoped_check/250`                             |   24.5722 |  18.5317 |   -24.58 | [-25.75, -23.75] |
| `scoped_progress_250_records/disposition_boards/0`     |   15.7753 |   9.2042 |   -41.65 | [-42.00, -41.35] |
| `scoped_progress_250_records/disposition_boards/500`   |   19.7834 |   9.0870 |   -54.07 | [-54.56, -53.65] |
| `scoped_progress_250_records/missing_detail/0`         |    2.9408 |   0.1649 |   -94.39 | [-94.44, -94.34] |
| `scoped_progress_250_records/missing_detail/500`       |    4.9587 |   0.1611 |   -96.75 | [-96.76, -96.74] |
| `scoped_progress_250_records/record_index/0`           |   16.3085 |   9.1873 |   -43.67 | [-45.30, -41.75] |
| `scoped_progress_250_records/record_index/500`         |   20.0317 |   9.9134 |   -50.51 | [-50.95, -50.18] |
| `sqlite_500_records/boards`                            |    0.3314 |   0.4007 |   +20.94 | [+18.54, +23.17] |
| `sqlite_500_records/prepare_replacement`               |   65.9256 |  14.1128 |   -78.59 | [-78.97, -78.22] |
| `sqlite_500_records/records_all`                       |    3.3473 |   2.0826 |   -37.78 | [-38.89, -36.79] |
| `sqlite_500_records/records_search_miss`               |    3.3380 |   0.7949 |   -76.19 | [-76.35, -75.94] |
| `sqlite_500_records/relationships_miss`                |    0.2021 |   0.1162 |   -42.50 | [-42.78, -42.25] |
| `supersession/chain/250`                               |   33.4619 |  17.9690 |   -46.30 | [-47.13, -45.57] |
| `supersession/chain/500`                               |   84.9101 |  35.2671 |   -58.47 | [-59.21, -57.75] |
| `supersession/independent/250`                         |   25.0389 |  17.4101 |   -30.47 | [-31.17, -29.90] |
| `supersession/independent/500`                         |   49.1563 |  34.8830 |   -29.04 | [-30.41, -27.44] |
| `tui_250_records_500_notes/detail_scroll_20_frames`    |   10.7045 |   6.1043 |   -42.97 | [-43.40, -42.56] |
| `tui_250_records_500_notes/list_scroll_20_frames`      |   11.1142 |   6.5374 |   -41.18 | [-41.72, -40.78] |
| `tui_250_records_500_notes/selected_scope_navigation`  |    1.1011 |   0.8595 |   -21.95 | [-22.72, -21.17] |

## Retained scratch / review boundary

Owned `writer/hmd068/` contains command drivers/logs, hash-matched source snapshots, isolated
package outputs and synthetic adapter Stores; it is retained pending root's durable review capture
and independent native-before verification. Shared `writer/target` and reviewer targets remain under
root closeout ownership. Primary ignored `build/marketplace` and `casefile/target` are not
authorized cleanup targets. All staged candidate paths will be inspected before the eventual
immutable local commit; this report does not authorize source changes beyond the three explicit
correction paths.
