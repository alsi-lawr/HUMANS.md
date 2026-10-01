# HMD-066 HTTP workbench and guarded workspace reuse

Local candidate against accepted parent `985f30e5731275a76660bf31cfcecff60a12df70`.
All **five** assigned findings046/053/077/103/104 are proposed resolved for independent
exact-commit review; root owns canonical progress, ledger and acceptance. No HMD-067 work.

## Implemented boundaries

- One Provider-owned index replaces Workbench's duplicate index/state barrier. Guarded full-index
  callbacks include SQL acquisition, JSON decoding and requested display materialization, followed
  by actual publication, canonical metadata-descriptor/membership and native watcher checks.
  A clean Current cache already avoided body derivation in063; this batch does **not** claim that
  every prior Current read reparsed bodies. It removes redundant server observation and shares
  the existing refresh phase; dirty acquisition skips the unused preliminary source inventory.
  Ordinary clean observation/publication errors surface without an automatic derive/repair.
  Missing/Stale/native-dirty can perform the normal single rebuild; no ending-guard retry.
- `IndexPublicationId::for_file` reuses existing target metadata revision/platform stamp, not file
  contents: Unix native identity/length/mtime/ctime and the existing Windows stamp implementation.
  `WorkspaceReadToken` has `token:"workspace_index"`, source_revision, publication_id and the
  **existing** Provider-vault instance qualifier. No random SQL publication field, schema change,
  durable generation, preview fingerprint, second vault or new filesystem framework.
  SQLite **schema1 and deterministic rebuilt bytes remain unchanged**.
- Conditional workspace wire is unchanged{freshness,matching_paths} or
  updated{freshness,records,diagnostics,matching_paths}. Only complete token equality permits reuse.
  Another Provider replacement, same-source publication or restarted/foreign Provider cannot reuse
  an old browser baseline. Path-only SQL shares exact nullable scope predicates, one Rust-lowered
  needle, `instr` and path ordering; it never decodes/renders unfiltered records. Changed baseline
  acquires complete records/diagnostics once and materializes full existing editable/source/HTML
  fields once. Record document rows decode directly from the iterator; no gather of all JSON
  Strings before decoding.
- ID-only `{preview_id,context?}` apply shares central retained-original admission and independent
  lock-bound canonical checks. Context only requests projection; it is never mutation authority.
  A successful commit refreshes/projects once. Any postcommit cache/projection/ending-guard failure
  remains **committed success + degraded cache + absent projection**, not a failed/repeated apply.
  Without context, apply performs no workspace projection. Existing result/cache fields,
  Provider protocol5, dated MCP protocol and strict refusal of obsolete preview bodies remain.
- Browser reducer owns one workspace baseline; no mirrored full ReadyWorkspace ref. Search sends
  one guarded workspace query, reuses matching paths and ignores obsolete replies. Supplied apply
  projection is adopted directly; absent projection deliberately triggers one fallback refresh.
  Search/publication changes while apply runs cannot replace a newer baseline/search with an old
  response. New source/publication/instance IDs are distinct opaque TS classes with private storage,
  validated factories and string JSON serialization, without unchecked branding casts.
  Typed incomplete-rollback409 details survive the browser boundary.
- tiny_http is removed; minimal Hyper HTTP1/Tokio handles connections independently. Supported
  TimeoutStream Pending-read/write idle timers are30seconds. No application body/concurrency cap,
  router/auth/exposure redesign, custom timer/parser or whole-operation deadline. Body acquisition
  precedes an owned `spawn_blocking` job capturing request and Arc<Host>; disconnecting its network
  future does not cancel admitted canonical work. Ordinary runtime/OS resource failure still applies.
- Exact gated direct versions/features: Hyper1.11.1 defaults-off http1/server; hyper-util0.1.21
  defaults-off tokio; Tokio1.53.1 defaults-off net/rt-multi-thread/time; tokio-io-timeout1.2.1;
  http-body-util0.1.5; bytes1.12.1 defaults-off std. Cargo resolved the lock, not hand edits.
  No frontend package/style/layout change. Production JS and CSS regenerate reproducibly.
  Root approved generated CSS23070→21889 bytes: obsolete lexical `filter` utility/support removed,
  lexical `static` utility added. Authored CSS and JSX classes are unchanged.

## Individual proposed finding evidence

| Finding | Implemented outcome | Actual behavioral evidence |
| --- | --- | --- |
| 046 | Guarded central Current/index reuse, no duplicate Workbench barrier or unused dirty-branch inventory; true publication+instance baseline. | Server `workspace_tokens_distinguish_same_source_replacement_and_provider_restart` rebuilds identical DB bytes/source under another publication, observes it via another request, then proves old context gets updated data; foreign instance also gets updated. `callback_replacement_and_canonical_edit_refuse_partial_current_data` refuses both races without exposing partial Current. Provider `ordinary_cache_observation_error_preserves_the_previous_publication_without_repair` exercises injected ordinary cache-observation failure; existing actual native watch create/edit/rename/delete, rescan/same-metadata and observation-window regressions pass. |
| 053 | Independent connections, Pending-I/O30s timers, uncapped application input and separately owned admitted canonical job. | Actual production-loopback server tests `stalled_headers_and_body_expire_while_unrelated_and_large_requests_succeed`, `progressing_body_may_take_longer_than_the_idle_deadline`, `non_reader_write_timeout_does_not_hold_unrelated_requests` and `admitted_apply_survives_disconnected_client_beyond_io_deadline`. They accept >8MiB valid input, expire stalled header/body/non-reader peers, complete a progressing >30s body, serve unrelated queries and commit an admitted apply after client disconnect and a32s test pause. Pause is before canonical dispatch, not a fabricated mid-write cancellation proof. Stale reuse is409, not a duplicate write. |
| 077 | One conditional search acquisition, SQL paths-only reuse, one reducer-owned baseline and obsolete-response rejection. | Actual native fixture probe confirms unchanged responses contain only state/freshness/matching_paths, accurate hit/miss paths and preserved full fields in updated/legacy replies. SQL `pushed_scope_and_search_match_rust_unicode_substrings_in_path_order` now compares path-only and full-row results across Unicode/NUL/nullable scopes. Browser `a delayed obsolete search reply cannot replace the newer guarded workspace` holds a real older HTTP response and delivers it after a newer search; newer visible records remain. |
| 103 | Consume063 retained-original ID authority once; preserve compact review, accurate diff/capability and independent apply checks. | New production `real_http_apply_returns_the_committed_workspace_without_another_refresh` observes compact preview, ID-context apply and old rendered-byte refusal. Existing native CLI `serve_preserves_preview_and_gates_apply_with_capability` supplies obsolete request/diff fields with a valid live ID, proves400/no write, then proper ID commit and stale409. Public Provider17 regressions retain unknown/foreign/wrong-family, eviction, original authority, safe no-op/progress replay and external-edit checks. No new canonical vault or fingerprint. |
| 104 | Single committed projection; success survives projection failure; direct adoption or one intentional fallback. | Server `committed_apply_projection_failure_stays_success_and_old_payload_is_refused` commits real bytes despite injected projection error and returns degraded+None. Actual HTTP context apply returns updated committed records/matching paths. Browser governed-work fixture verifies supplied projection causes no additional workspace query; for fallback it changes only the projection wire to absent/degraded **after the real host commit**, and observes exactly one query and committed board bytes. This is a browser wire-failure injection, not a claimed real filesystem cache-failure fixture. |

Server tests: [api.rs](../casefile-server/src/api.rs).
Provider phase tests: [outcome_tests.rs](../casefile-store/src/provider/outcome_tests.rs);
public identity/freshness consumers: [provider.rs](../casefile-store/tests/provider.rs).
SQLite parity/determinism: [integration.rs](../casefile-store-sqlite/tests/integration.rs).
Browser real-host behavior: [app.browser.test.tsx](../web/src/app/app.browser.test.tsx).
Typed decode/error behavior: [api-contract.test.ts](../web/src/api-contract.test.ts).

## Focused verification and transparent failures

[Actual outputs and failures](results/hmd066-verification.txt).
**46 distinct Rust tests and16 TS tests pass**: Server10, SQLite8, Provider phase8/public17,
CLI native HTTP3; TS boundary11 and real-host React5. Streaming correction reran affected
Server10/SQLite8/CLI3 and TS16; unchanged Store source's25 Provider tests passed immediately before.
Final relevant all-target Clippy, Rust fmt, TypeScript typecheck and exact-path Prettier passed.
Production `bun run build` reproduced both embedded JS/CSS byte-for-byte. Six-control discovery,
release benchmark build, executed measurements and final source/binary hash checks passed.
No workspace-wide release/package/Windows/peak-memory check is claimed.

All chains propagate actual exit status (`sh -ec`, subprocess check/explicit return).
Commands used the absolute retained target:
`/home/alex/dev/HUMANS.md/.agent-workspace/20260930-speedup-implementation/writer/target`.

```sh
nix develop --command sh -ec '
  export CARGO_TARGET_DIR=/home/alex/dev/HUMANS.md/.agent-workspace/20260930-speedup-implementation/writer/target
  cd casefile
  cargo test -p casefile-server --lib
  cargo test -p casefile-store-sqlite --test integration
  cargo test -p casefile-store --lib provider::outcome_tests
  cargo test -p casefile-store --test provider
  cargo test -p casefile-cli --test serve
  cargo clippy -p casefile-server -p casefile-store-sqlite -p casefile-store -p casefile-cli --all-targets -- -D warnings
  cargo fmt --all --check
  cd web
  bun run typecheck
  bun test src/api-contract.test.ts src/app/app.browser.test.tsx
  bun run build
'
```

The streaming-final chain in the evidence reran the affected packages and benchmark build.
Earlier failures are retained, not final PASS evidence: child helper visibility/TimeoutStream
Unpin compile corrections; API fixture missing workspace fields; wrong preview method/type fixture;
Serde internally-tagged struct failed token deserialization (fixed required finite token field);
Clippy return-type complexity (domain alias); wide TS fixture literal (explicit domain type);
test hook paused replay and hung (owned test binary SIGTERM, one-shot hook corrected); changed-ID
replay incorrectly expected200 (fixed to supported409, canonical freshness not relaxed); one5s
browser status-wait timeout (native context roundtrip, isolated browser and two final full runs
passed without a product workaround); direct Criterion command omitted runtime target and
failed **before sample collection** (corrected absolute target, no invented before baseline).
No live/application process was terminated by that test cleanup.

[Exact executed fixture probe](results/hmd066-fixture-probe.py) /
[actual results](results/hmd066-fixture-probe.json) use disposable copies of the unchanged fixture,
not a live Store or an extra timing control. Launch write capabilities are not retained.

## Six original native Criterion controls

[Final estimates/raw samples/source and binary provenance](results/hmd066-after.json),
[full Criterion output](results/hmd066-criterion.log).
Original tiny_http harness was deliberately replaced with the **actual production Hyper/Tokio
transport**; hashes for057/original and parent harness plus final source are retained. No obsolete
backend timing or byte-identical harness claim. Identical fixture/data, scope/search payloads,
HTTP1.0 Connection:close, setup outside measurement,10 samples/1s warmup/2s target/95% CI/1% noise.
Criterion expanded collection duration when necessary (unchanged1000 warning retained).

Labels250/1000 add that many tickets to the original fixture: **252/1002 work items**,
**264/1014 returned scoped records**, **266/1016 complete workspace/canonical files**,
**500 progress notes**. Hit selects one HMD-100000 path; miss selects zero; diagnostics zero.
The semantic fixture probe confirms these actual counts and conditional response shape.

Final command (no competing builds; warm filesystem, release; named native before retained):

```sh
CARGO_TARGET_DIR=/home/alex/dev/HUMANS.md/.agent-workspace/20260930-speedup-implementation/writer/target \
  /home/alex/dev/HUMANS.md/.agent-workspace/20260930-speedup-implementation/writer/target/release/deps/http-7be4e3cdc56e296b \
  '^http_loopback_records/' --bench --baseline hmd057-before
```

| Control / added tickets | Before mean ms | After mean ms | Mean change (95% CI) |
| --- | ---: | ---: | --- |
| search_hit/1000 | 142.450 | 18.901 | -86.73% [-86.83%, -86.60%] |
| search_hit/250 | 42.509 | 5.841 | -86.26% [-86.37%, -86.12%] |
| search_miss/1000 | 143.104 | 19.583 | -86.32% [-86.53%, -86.09%] |
| search_miss/250 | 42.743 | 5.830 | -86.36% [-86.61%, -86.15%] |
| unchanged/1000 | 147.085 | 59.460 | -59.57% [-59.75%, -59.38%] |
| unchanged/250 | 44.633 | 15.826 | -64.54% [-65.21%, -63.42%] |

All six comparisons improve against057; these are **cumulative058–066 changes**, not an isolated066
multiplier. Reported table uses arithmetic mean, while Criterion's headline time may use slope.
No end-to-end browser/apply latency, allocation count, peak memory, cold initialization, sustained
throughput or Windows performance claim. Matching-token workspace paths-only and HTTP small apply
have no original dedicated timing control; their evidence is structural/behavioral, not these timings.

[Superseded pre-streaming six distributions/provenance](results/hmd066-pre-streaming-after.json) and
[stdout](results/hmd066-pre-streaming-criterion.log) remain intact. Only SQLite document gather
changed afterward, then relevant checks and all six final controls reran. Superseded unchanged1000
mean55.574ms versus final59.460ms is adverse, despite cumulative baseline improvement; do not infer
a universal latency win from streaming removal, choose the earlier number, or claim zero regression.
No extra variants/profiling/repeat matrix or hard numerical gate.

Four original before artifact/source hashes plus **all236 named native before objects** pass SHA256
again after final measurement. Earlier reports/results and fixture/helper/original SQLite benchmark
source remain untouched. No cache/binary/archive/HTML report is committed.

## Ownership and scratch handoff

Changes are exactly the root-gated Store Provider/index/export/private-instance paths, SQLite
query/helper/tests, minimal transport/manifests/lock, server/workspace/API, typed browser callers/tests,
reproduced embedded JS/CSS, necessary HTTP benchmark harness and this report/results.
No planning/progress/ledger mutation, future ticket, dependency outside the gate, authored styling,
frontend package, TUI/CLI production source, core/parser/validation/presentation or canonical format change.

Owned066 scratch consisted of fetch/build/failure/final logs, token/debug native probes, CSS candidate
and diff, asset hashes, measurement provenance/discovery, semantic probe, result table and loaded TS
skill text. Useful evidence is retained in the linked compact text/JSON/source artifacts; owned066
scratch is removed before handoff. Root-owned retained writer target and before-hash manifests remain
active for later native comparisons; root owns their final pipeline cleanup.

Supported limits: native watcher delivery and existing metadata stamps are not infallible/atomic
filesystem authority; no stronger ancestor-swap/CAS guarantee. Native publication tests ran on Linux,
not Windows. Network timeout is Pending-I/O idle time, not absolute connection/body/operation time.
No application resource cap or crash-recovery promise. Projection unavailability can correctly
degrade a committed result; mutation freshness remains independently checked. Root review owns
acceptance and any newly demonstrated remaining issue.
