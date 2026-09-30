# HMD-063 retained preview authority and current-cache work

Local candidate against reviewed parent `f3f2df036aaf8f677b5e7858d95804bfb0dc48df`.
Both assigned outcomes, **035 and037**, are proposed resolved for exact-commit review. Root owns
canonical progress, ledger and acceptance. This report does not start HMD-064 or complete HMD-066.

## Human-selected contract

- Retain the existing **256-preview count and oldest-first eviction**, with no additional byte cap,
  oversized-preview rejection, TTL or retention framework.
- Provider owns each original canonical preview **once**. Callers receive a compact review and apply
  its opaque ID, not a returned editable canonical payload. **No fingerprint receipts** or new
  serialization/hash dependency. Existing canonical file-revision checks remain mutation authority.
- Native filesystem events invalidate configured acceleration; they are **not write authority**.
  Every apply, including no-op/replay, still independently admits the retained operation against
  fresh Store facts and lock-bound dependencies.
- This intentionally changes Provider's public API/wire contract to **protocol5**. Dated MCP
  transport versions, canonical planning formats and successful apply-result DTOs are unchanged.
  Old caller-returned apply bodies are explicitly refused, not reinterpreted by ignoring extras.

## Implementation and necessary coupling

`provider/preview_vault.rs` retains one `Arc<StoredPreview>` per ID. Each variant contains only the
original Store preview, without another Provider/MCP canonical copy or duplicated rendered
request. An apply temporarily shares the same Arc, rather than deep-cloning it; an acquired
original remains alive if a concurrent newer preview evicts its lookup entry. IDs qualify the
existing sequence with native PID, creation nanoseconds and a process-local atomic Provider
instance. Unknown, expired, wrong-family and foreign/restarted-instance IDs fail. IDs are neither
authentication nor a cryptographic/durable cross-process identity guarantee.

`provider/review.rs` projects `{preview_id,kind,approval_required,no_op,operations,diagnostics,diff}`.
Display diff/diagnostic copies are transient caller review, **never a second retained authority**.
Governed change bytes needed by the original Store preview remain there once. Approval policy is
unchanged. Record, batch, progress, default-board, strategy and writer-binding applies retrieve the
appropriate original and use the common Store admission engine. Necessary borrowed Store seams
share normalization, render/validation, fresh capture, lock rediscovery and restoration; they do
not trust an ID as freshness or introduce an alternate validator. Record preflight borrows already
canonical drafts and expected-revision values, moving its phase overlay bytes instead of copying
those full values. Legacy/native-case normalization may still require a necessary owned request;
independent phase validation/rendering and result-display copies remain.

MCP's second vault and display SHA are removed. Its review envelope preserves field budgets,
operation counts, diff byte count, diagnostics and approval/no-op information. Real apply tools
accept strict `{preview_id}` arguments, with existing error channels and incomplete-rollback
schema union preserved. The HTTP handler likewise strictly accepts `{preview_id}`. The browser
uses a typed preview ID and compact review decoder; full editable record/render/search behavior
and layout are unchanged. The actual JS bundle was rebuilt; no CSS, HTML, package or routing edits.
CLI same-session record/progress workflows retain the Provider until ID apply. **Standalone CLI
progress-repair** remains its existing explicit owned Store-preview workflow; it is not a Provider
old-payload compatibility path. Source adapter/smoke/skill protocol requirements move together to5;
no live installed plugin or configuration was changed.

Future HMD-066 consumes this existing Provider-owned original/ID boundary; it must not introduce a
second server canonical-preview vault. Its remaining integrated resource/transport outcome is
not claimed by this ticket.

## Cache invalidation and supported limits

Configured Providers use the existing pinned workspace `notify`9.0.0-rc.4; only Store manifest
membership and lock bookkeeping changed, with no package upgrade. `provider/watching.rs` owns a
small native recursive CORE-event watcher with **follow_symlinks(false)**, a dirty epoch and an
ordinary error. Only exact direct-root `.git` and `.agent-workspace` exclusions are reused; nested
canonical paths are not excluded. SQLite still requires its disposable DB outside the Store.
Unconfigured Providers do not start a watcher or derive cache data merely to return NotConfigured.

An explicit full-cache refresh first observes the legitimate **global full-index metadata
revision**. Clean/current acceleration returns before body derivation. A dirty/rescan/error event
forces actual derive/prepare/publish, even if the metadata revision and existing index both appear
Current. A second metadata observation and epoch reconciliation refuse an observed intervening
edit/event as Current; a during-work event is not acknowledged clean. The DerivedIndex adapter no
longer short-circuits that forced rebuild merely because its existing revision is equal. Errors
attempt one ordinary rewatch at the next explicit boundary; degraded acceleration leaves canonical
success and supported replay intact. There is no background retry/recovery framework.

Catalogue/global-full-index freshness remains distinct from narrow query dependency tokens. No
new token database, scoped invalidation architecture or persisted format. Native events are not
infallible or synchronous, and metadata reconciliation is not an atomic filesystem snapshot: an
unobserved last-syscall edit/rare deliberate ancestor swap has the previously selected full-refresh
limitation. No new anchored traversal, automatic detector, watcher-latency deadline, portable CAS,
Windows runtime or universal resource claim follows from these checks.

## Individual proposed ledger evidence

| Finding | Implemented boundary | Meaningful evidence |
| --- | --- | --- |
| 035 | Human-selected one-original ID authority; compact review; retained256 oldest eviction; borrowed central apply admission; no fingerprints/second MCP vault. | Store `record_ids_bind_original_authority_and_foreign_or_unknown_ids_preserve_the_store` mutates caller review yet only original is written; unknown/foreign IDs mutate nothing. `preview_count_eviction_is_oldest_first_and_newest_id_still_applies_original` protects oldest expiry/newest original. Existing batch/progress/default-board/governance tests preserve fresh conflict, correct family and supported replay. MCP `mcp_rejects_old_body_and_foreign_tool_ids_before_applying_retained_original` plus native restarted-process test refuse obsolete bodies/IDs. Actual HTTP and typed browser proof reject old body400, then apply ID and preserve edit/search/display. |
| 037 | Clean/current global cache observe before body derivation; native dirty/error/rescan forces actual rebuild; end-observation/epoch protects observed edits; cache failures cannot undo canonical success/no-op receipt. | `current_cache_survives_publication_failure_but_required_external_changes_do_not` proves clean/current avoids rebuilding while a real external edit requires it. `native_watch_create_edit_rename_delete_reconciles_canonical_index_content` checks actual native delivery and resulting bodies/membership. `events_and_rescan_require_rebuild_even_when_metadata_revision_is_unchanged` tests rescan/error and forced rebuild despite Current. `observation_window_external_edit_cannot_claim_the_old_index_is_current` protects an edit during observe. Existing commit/replay failure fixtures preserve receipts/bytes. Unix opaque-descendant fixture protects non-following native integration without an ancestor-race claim. |

Relevant sources: [Provider tests](../casefile-store/tests/provider.rs),
[cache/native fixtures](../casefile-store/src/provider/outcome_tests.rs),
[MCP session tests](../casefile-cli/src/mcp_tests.rs),
[native MCP tests](../casefile-cli/tests/mcp.rs),
[HTTP tests](../casefile-cli/tests/serve.rs),
[typed browser contract tests](../web/src/api-contract.test.ts).

## Actual verification

[Commands and final outputs, plus failed/exploratory attempts](results/hmd063-verification.txt).
All final commands propagated their actual exit status under `set -e`; **145 Rust tests passed**:
Store128 (70 inline,17 Provider,11 governance,29 v1,1 diff-path), Server2, CLI15 (5 inline,7 MCP,3 HTTP).
The Web typecheck,9 contract tests, production build and four authored TS-file Prettier checks
passed. Relevant Store/CLI/Server/SQLite all-target Clippy `-D warnings`, workspace fmt and release
priority benchmark discovery/build passed. Other crate consumers compiled as necessary; this is
not a claim that every workspace test/platform was run.

Final native binary additionally passed:
- Actual CLI `record-session` on an external payload/disposable Store: compact create review and
  ID apply, wrong delete approval nonzero with bytes preserved, exact ID approval deletes target.
- Existing MCP smoke: identity/version, protocol5 requirements and12 tools.
- Actual MCP `tools/list` + `tools/call` compact preview/ID apply against external JSON Schema
  validation from temporary repo-pinned Nix Python/jsonschema tooling; obsolete body refused and
  dated `2025-06-18` transport unchanged. [Captured declarations and responses](results/hmd063-mcp-id-schema.json).
- Actual loopback native HTTP plus **the real authored TS API/decoder/edit model**, using a
  disposable synthetic Store: full editable ticket -> compact review -> old body400 -> ID-only
  request -> applied full record and title search. No write capability is logged or retained.

Exact executed proof sources are retained as
[hmd063-schema-probe.py](results/hmd063-schema-probe.py),
[hmd063-browser-probe.py](results/hmd063-browser-probe.py) and
[hmd063-browser-probe.ts](results/hmd063-browser-probe.ts).
They originally execute from the disposable scratch paths described in verification evidence;
replay by copying them back with their original basenames. No profile/project dependency install.

## Native Criterion: original canonical workload, changed ID API seam

[Final-source samples/estimates/changes and hashes](results/hmd063-after.json),
[complete Criterion output](results/hmd063-criterion.log). No extra variant or repeat was run.

```sh
export CARGO_TARGET_DIR=/home/alex/dev/HUMANS.md/.agent-workspace/20260930-speedup-implementation/writer/target
$CARGO_TARGET_DIR/release/deps/priority-93bc18bb50e3d15c '^priority_single_record_250_records_500_notes/(preview|apply)$' --bench --baseline hmd057-before
```

UTC window:2026-09-30T21:51:37.961475Z–21:51:48.121869Z. Criterion0.8.2,10 samples,
1s warmup,2s measurement target,95% CI,1% noise threshold; actual collections extended to about
3.84s/3.12s. Release, warm filesystem, disposable synthetic Store, no competing builds.
Actual fixture: **252 work items** (250 added plus original ticket/epic), **500 progress notes**,
Provider **without configured cache**. Setup creates the Store/Provider outside timed work;
apply's original preview remains setup. Preview measures actual Provider preview/review retention;
apply measures actual fresh canonical apply through the new ID boundary.

| Original control | Before arithmetic mean | Final arithmetic mean | Criterion mean-relative change [95% CI] | Before slope | Final slope [95% CI] |
| --- | ---: | ---: | ---: | ---: | ---: |
| `priority_single_record_250_records_500_notes/preview` |30.879ms |36.588ms |**+18.49% [−8.97,+60.25]** |31.081ms |28.832ms [28.061,30.146] |
| `priority_single_record_250_records_500_notes/apply` |30.517ms |29.748ms |−2.52% [−8.71,+5.30] |30.689ms |32.232ms [28.417,35.267] |

**Neither comparison detects a statistically significant change** (p0.52/p0.53).
Preview's adverse mean and wide CI are retained, not labelled a speedup or zero-regression proof;
it has two high-severe outliers. Apply has one high-mild outlier. Slopes and arithmetic means differ;
both estimates are reported rather than selecting the favorable statistic.

The baseline-to-final comparison is cumulative since HMD-057, including058–062; it cannot isolate
063 attribution. These controls use no cache, so **no configured current-cache/watcher timing or
allocation/resource reduction is claimed**. Removing duplicate authority/clones and early-current
body derivation is structural plus behavioral evidence, not a profiler result. No universal latency
win, peak-memory, cold-process, large-vault, Windows or browser end-to-end speedup claim.

The original `priority.rs` needed **API-only** diagnostic access and apply-ID changes: setup,
canonical operation, fixture, per-iteration batching and measured boundary remain equivalent.
Its source hash therefore changes intentionally; do not claim all original benchmark-source hashes
are unchanged. No benchmark helper/tuning/workload variant changed. The original table/results/log,
`baselines.rs`, HMD-057 artifacts and **all236 native saved before JSON files** match their retained
SHA manifests. Final timing87 source hashes and executable match current code. Target/baselines
remain root-owned active pipeline scratch for later native comparisons; no rehydration or mutation.

## Exact ownership and scratch handoff

Source changes are31 explicitly approved paths: Provider+three focused private siblings, Store
borrowed apply wrappers/engines and necessary tests; CLI/MCP tests/callers; HTTP handler/workbench;
four authored web model/API/contract files plus actual JS bundle; adapter/smoke/skill; Store notify
manifest/lock membership; priority API consumer. No core/index/derived/scanning/validation/
presentation/TUI/SQLite query, CSS, frontend package, routing or live installed-state edit.
The immutable commit's exact path list is the authoritative inventory.

Only `.agent-workspace/20260930-speedup-implementation/writer/hmd063/` bulky ticket scratch is
closed after copying compact useful evidence. Other pipeline scratch and the retained absolute
Criterion target remain untouched under root cleanup ownership. Failed compiler, fixture, shell,
format and transport attempts are preserved transparently in verification evidence; earlier
successful pre-final tests/proofs were superseded, not claimed as final-source measurements.

Local-only commit; no push, release, install, live planning mutation or activation. No outstanding
contract blocker, new dependency choice, self-accepted disposition or autonomous next-ticket work.
