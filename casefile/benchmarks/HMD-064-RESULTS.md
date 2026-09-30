# HMD-064 SQLite filtering and replacement

Local candidate against reviewed parent `9cde6d83915ae08c246a03622d08a6b57d6d74e0`.
All **four** assigned findings047/048/049/082 are proposed resolved for exact-commit review.
Root owns canonical progress/ledger/acceptance. No HMD-065 implementation is included.

## Implementation and contract

- `records()` constructs bound SQL predicates for exact project/nullable investigation and optional
  search **before document decoding**, retaining `ORDER BY path`. No supplied values enter SQL text.
  Preparation stores the existing central `DerivedRecord::search_text().to_lowercase()` once in a
  purposeful normalized column. Reads lower the needle once and use SQLite `instr`, not `LIKE`,
  SQLite `lower`, collation or Unicode normalization. Dotted-I expansion, Greek final-sigma context,
  combining sequences, literal percent/underscore and embedded NUL preserve the existing Rust
  lowercase-substring behavior. Search still scans candidate normalized strings; this is not FTS.
- Composite indexes match exact record identities, scope/path ordering, board scope/identity and
  **both** relationship endpoints. Nullable investigation comparisons remain `IS ?`; project-level
  NULL is not a wildcard for investigation or unscoped records. Existing deterministic ordering is
  unchanged. Query-plan capture is diagnostic evidence, not a planner-text test or portable promise.
- Exact record lookup orders by path and fetches at most two matches. Multiple matches return
  `SqliteIndexError::AmbiguousRecordIdentity` with the **first two sorted paths**, before JSON decode,
  instead of returning an arbitrary first record. This matches canonical ambiguity behavior without
  claiming the bounded SQLite error inventories every duplicate path.
- Replacement prepares one INSERT statement per records/relationships/boards/diagnostics table,
  reuses it within the existing transaction, then builds secondary indexes after row insertion.
  Metadata has a single existing INSERT. All serialization, commit, temporary-file ownership,
  revision-source validation and atomic publication remain central; no parallel builder/framework.
- The replacement carries private SQLite `user_version=1`. A single owned SELECT joins the
  metadata revision with table-valued `pragma_user_version`, rather than preparing two separate
  observation statements per checked read. `Indexed::Missing` now also means
  **no usable current-schema cache**, not necessarily an absent filesystem file. Prior version0
  acceleration returns Missing without rewriting it; the existing Provider missing-cache path
  prepares/publishes a disposable replacement atomically. This is not a canonical planning-format
  migration, in-place live DB repair, arbitrary corruption recovery or future-schema compatibility
  promise. Unexpected SQL/I/O failures still use their ordinary error boundary.

A genuine complete index retains its **global DerivedSnapshot source revision**. It does not accept
or attach a narrow `ScopeReadToken` as full-index or mutation authority. Canonical selective Provider
reads, independent lock-bound applies and HMD-063 native watcher/ID contracts are unchanged.
Only SQLite source and its existing integration test changed; no Store/export/index trait,
dependency/manifest, CLI/server/frontend, benchmark source or helper edit was necessary.

## Individual proposed ledger evidence

All test references are in [integration.rs](../casefile-store-sqlite/tests/integration.rs).

| Finding | Implemented boundary | Meaningful evidence |
| --- | --- | --- |
| 047 | Exact scope/search SQL before JSON decode; existing Rust normalization source and deterministic path ordering. | `pushed_scope_and_search_match_rust_unicode_substrings_in_path_order` differentially compares real derived records to the former Rust filter for scoped/unscoped/project-NULL/missing scopes, empty search, dotted-I, Greek sigma, combining accents, sharp-S, literal%/_ and embeddedNUL; reversed insertion protects path ordering. Existing compact source/search/requested-render round-trip passes. Native SQLite-backed HTTP additionally preserves Unicode/NUL search, scope isolation and full rendered record. |
| 048 | Predicate indexes for scoped identity, scope/path, boards and both relationship endpoints. | `relationship_endpoints_and_board_scopes_preserve_nullable_identity_and_order` compares canonical derived relationships for incoming project-NULL and outgoing investigation endpoints, rejects same identity in the wrong nullable scope, and checks multiple board identities whose identity ordering differs from path insertion order. Existing scoped misses/cards tests retained. Actual produced-DB EXPLAIN diagnostics show matched indexes, without assertions of planner wording. Narrow lookup latency/storage tradeoffs remain partly unmeasured. |
| 049 | Bounded exact-identity ambiguity before decode, not arbitrary first result. | `exact_duplicate_identity_is_ambiguous_in_canonical_and_indexed_reads` creates actual same-ID accepted/provisional ticket files: canonical Provider and SQLite both error with sorted duplicate paths; project-NULL identity does not falsely match that investigation. Existing unique/null project-decision exact lookup and missing cases pass. No exact-record performance control is claimed. |
| 082 | Once-per-table prepared INSERT reuse, transactional rows plus index creation, unchanged temporary atomic/stale-safe publish. | `failed_replacement_keeps_published_index_and_prior_schema_rebuilds_as_missing` submits a duplicate-path snapshot through public prepare, observes ordinary SQLite failure and byte-identical previously published DB/query result; valid prior schema remains unchanged on reads, then normal Provider refresh rebuilds it. Existing `replacement_index_is_revision_bound_repairable_and_queryable` preserves deterministic bytes, canonical files, stale-publish refusal and prior DB bytes. Prepared-statement reduction is structural evidence; rebuild timing below includes new normalized source/index work. |

## Actual focused verification

[Printed commands, outputs and provenance](results/hmd064-verification.txt). Final commands ran
sequentially under `set -e`/`sh -ec`; actual final exit0, not a log-tail status:

```sh
export CARGO_TARGET_DIR=/home/alex/dev/HUMANS.md/.agent-workspace/20260930-speedup-implementation/writer/target
nix develop . --command cargo test --manifest-path casefile/Cargo.toml -p casefile-store-sqlite -p casefile-server --lib --test integration
nix develop . --command cargo test --manifest-path casefile/Cargo.toml -p casefile-cli --test serve
nix develop . --command cargo clippy --manifest-path casefile/Cargo.toml -p casefile-store-sqlite -p casefile-server -p casefile-cli --all-targets -- -D warnings
nix develop . --command cargo fmt --manifest-path casefile/Cargo.toml --all --check
nix develop . --command cargo bench --manifest-path casefile/Cargo.toml -p casefile-store-sqlite --bench baselines --no-run
```

**13 tests passed**: SQLite8 (4 retained,4 new), Server2, native CLI HTTP3. Relevant consumer builds,
all-target Clippy and fmt passed. This is not an entire workspace/package/platform release check.
The first SQLite8 run also passed before extending the multi-board ordering fixture; the final
13-test run supersedes it. No compiler/test/clippy failure. One necessary combined-observation source correction superseded
its earlier five-control run; one explicitly approved unchanged prepare-only diagnostic followed
the final high-variance result. All three distributions are retained below.

Actual final native server plus synthetic Store/index HTTP proof passed: dotted-I/NUL substring,
sharp-S non-equivalence, nullable scope isolation, complete source/rendered-record response.
[Exact executed Python proof](results/hmd064-query-probe.py) and
[query-plan diagnostics](results/hmd064-query-plan.json) are retained. The diagnostic EXPLAIN driver
is Python SQLite3.53.3 against the DB produced by the actual native server, not a claim about every
runtime SQLite version's planner. It used only a disposable Store; no write capability was logged.

## Original five Criterion controls

[Final samples/estimates/changes and71 source/binary hashes](results/hmd064-after.json),
[complete output](results/hmd064-criterion.log).

```sh
export CARGO_TARGET_DIR=/home/alex/dev/HUMANS.md/.agent-workspace/20260930-speedup-implementation/writer/target
$CARGO_TARGET_DIR/release/deps/baselines-a1b7be9b5e9f5b39 '^sqlite_500_records/(prepare_replacement|records_all|records_search_miss|relationships_miss|boards)$' --bench --baseline hmd057-before
```

Final-five UTC window:2026-09-30T22:41:10.046432Z–22:41:37.290612Z. Criterion0.8.2,10 samples,1s warmup,
2s measurement target,95% CI,1% noise threshold. Prepare's estimated collection extended to3.79s (actual samples include two much slower later
iterations).
Release, warm filesystem, no competing builds, same original fixture/helper/timing boundaries:
**502 work items** (500 added plus original ticket/epic),0 progress notes. Store derivation and
initial index publication are setup. Prepare measures a full disposable index rebuild including
serialization/transaction/index creation, not publication or canonical Store acquisition. Read
controls measure actual revision-bound SQLite queries/open/metadata/decode; scoped search misses
and relationship misses are not hit/HTTP end-to-end controls. Boards include the original cards.

| Original control | Before slope | Final slope [95% CI] | Criterion mean-relative change [95% CI] |
| --- | ---: | ---: | ---: |
| `sqlite_500_records/prepare_replacement` |65.972ms |177.414ms [66.734,257.996] |+72.22% [+0.63,+170.71] |
| `sqlite_500_records/records_all` |3.372ms |2.119ms [2.084,2.156] |-36.13% [-37.01,-35.39] |
| `sqlite_500_records/records_search_miss` |3.340ms |0.851ms [0.839,0.867] |-74.28% [-74.67,-73.90] |
| `sqlite_500_records/relationships_miss` |0.202ms |0.126ms [0.125,0.127] |-37.58% [-38.05,-37.13] |
| `sqlite_500_records/boards` |0.332ms |0.401ms [0.397,0.405] |+20.44% [+19.44,+21.48] |

All adverse results are retained. Criterion calls final prepare **no detected change** (p0.09),
despite mean-change+72.22% and positive/wide bootstrap CI. Final prepare has two high-severe
outliers: eight samples average about65–70ms/iteration, then about287/314ms for the last two;
its slope CI is66.734–257.996ms. This is not a zero-regression proof. Boards show significant
**+20.44% regression** (p<0.05), with no classified outliers. Three read comparisons improve.
No hard numerical gate was selected; no further timing repeats/variants were run.

### Preserved prior source and one unchanged diagnostic

The initial source used a separate schema pragma and metadata query. Its successful five-control
run is retained in [pre-combined samples](results/hmd064-pre-combined-after.json) and
[output](results/hmd064-pre-combined-criterion.log), with exact source hashes and UTC window
22:33:38.354107Z–22:34:01.958355Z. Changes vs057: prepare+3.70% [+0.10,+7.46], all−36.25%
[−38.02,−34.61], search miss−74.75% [−74.95,−74.56], relationships miss−37.85% [−38.75,−36.95],
boards+20.03% [+17.93,+22.14]. It is **superseded**, not final-code timing. The necessary
[combined-query source diff](results/hmd064-combined-query.diff) reconstructs that prior file;
its reconstructed SHA was verified against the prior artifact. No durable memo/cache was added.

The final-five high-variance anomaly occurs in prepare, whose implementation is unchanged by that
read-only observation correction. Root approved **exactly one unchanged prepare-only diagnostic**,
not a source/fixture/configuration alternative. [Samples/provenance](results/hmd064-prepare-diagnostic.json),
[output](results/hmd064-prepare-diagnostic.log). UTC22:43:55.618886Z–22:44:03.375493Z, same final
source/executable/configuration and original native before:

```sh
$CARGO_TARGET_DIR/release/deps/baselines-a1b7be9b5e9f5b39 '^sqlite_500_records/prepare_replacement$' --bench --baseline hmd057-before
```

Diagnostic prepare slope **67.316ms [66.600,67.836]**, mean change**+0.87% [−1.18,+2.71]**;
p0.43, no detected change. It does **not erase or silently replace** the adverse final-five
177.414ms distribution. No external host/I/O cause was proven; variance and source separation are
evidence, not a causal diagnosis. All three distributions remain available; no cherry-picking.

These are **cumulative since HMD-057**, including058–063, not isolated064 effects. Prior060 already
reported a board regression. Schema usability observation, normalization and secondary indexes
add structural work; exact latency attribution is unmeasured. Interactive search/endpoint controls
improve while board/rebuild results are mixed/adverse; no universal fastest/zero-regression claim.
Search hits/large full-index storage, allocation/peak memory, cold disk, native watcher timing,
exact-record latency, HTTP latency and Windows runtime are unmeasured. No parallelism was added.

## Preservation and handoff

Exact source changes: `casefile/casefile-store-sqlite/src/lib.rs` and
`casefile/casefile-store-sqlite/tests/integration.rs`. Durable evidence is this report and
`casefile/benchmarks/results/hmd064-*`. All original benchmark sources/helpers, prior durable
artifacts, the4 original table/results/log/baselines source hashes and **236 saved native before
JSON** remain unchanged. Final timing source/executable hashes were verified against the candidate.

Only owned `.agent-workspace/20260930-speedup-implementation/writer/hmd064/` ticket scratch closes
at handoff after compact evidence capture. The retained absolute target and prior pipeline scratch
remain root-owned for native comparisons/cleanup. No live DB/Store migration, planning mutation,
push, release, install, activation, new public DTO/dependency, autonomous next ticket or self-accepted
ledger disposition. No outstanding contract blocker.
