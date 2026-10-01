# HMD-061 mutation phases and changed-only rollback

Local candidate against reviewed parent `7c965348712432708ab0df709125d9a872ef7c10`. All **21**
assigned outcomes (original20 plus later source finding109) are proposed resolved for independent
exact-commit review. This report does not mutate the canonical ledger, reject a finding, start
HMD-062, or claim review acceptance.

## Ownership and failure contract

The human's superseding choice was: "None, it should just discard and allow the external edit to
win." Records, progress and governance now share private changed-only publication receipts and
in-place cleanup. Only successful actual changes get receipts: no-op and never-written targets are
not restored. Receipt identity comes from the temporary inode before publication; its installed
revision comes from the owned published handle. Expected-absent publication uses no-clobber and
reports an intervening creator as conflict, without overwriting it or claiming a successful write.

Cleanup retains Casefile locks and compares identity, revision, bytes and safe regular-file type.
Detected external change discards restoration for that path, leaves its canonical state intact and
continues cleanup of other safe own changes. There is **no portable atomic content CAS guarantee**
against a noncooperating edit between the final ownership observation and replacement/deletion. That
bounded final-syscall limitation is intentional under the selected contract, not a rescue, detach,
journal, retry or process-crash recovery promise.

Incomplete cleanup is `StoreError::IncompleteRollback`, transparently retained through Provider. Its
common serialized payload contains `code: incomplete_rollback`, concise operation/phase,
`io`/`invalid`/`stale` cause category and affected paths with restoration reason and tagged observed
`absent`/`regular` revision/`symlink`/`directory`/`other`/`unknown` state. It contains **no file
contents or arbitrary source/parser error excerpts**. Original Rust source remains separately
available. CLI returns nonzero with structured stderr; MCP returns `isError: true` and
schema-conforming structured details; HTTP returns409 with code/details. Existing success DTOs and
unrelated errors remain unchanged; there is no dated MCP protocol change.

## Phase-local work, not weaker freshness

- Request-only batch path/duplicate/render failures precede filesystem capture. Drafts render once
  per independent preview/apply phase, and byte-equal diffs avoid git/tempfiles entirely.
- Capture owns original bytes once; private projections borrow unchanged inputs and reuse validated
  draft/strategy/metadata facts. Changed bodies are independently classified. Projection metadata
  does not pretend to be a byte-complete public scan: genuine `Store::scan` stays unchanged.
- Each discovery pass reads/parses a shared progress log once. The locked pass independently
  reacquires it; its parsed facts feed capture without another read/parse. Record-only dependencies
  retain validated operation/ticket IDs rather than full owned note messages, through the **same**
  strict borrowed progress parser/validator as full logs. Progress and binding state/message
  consumers keep full facts. Append IDs are indexed incrementally; proposed logs/rendered bytes and
  selected typed matrices are handed forward within their trusted phase.
- Required global filename/board identities, reverse references, canonical-case checks, dependency
  revisions and lock-bound rediscovery remain. Foreign global board-ID metadata is still necessary;
  this is not a claim of zero foreign traversal. Unneeded evidence/review bodies outside target
  investigations are omitted; accepted progress targets and batch paths are indexed. Supersession
  closure uses an identity index/frontier rather than repeated all-candidate scans. Case-sensitivity
  probes are reused only within one acquisition, never across operations/mount changes.
- Board/transition header discovery parses complete TOML once, not growing prefixes. Needed native
  names are represented exactly or produce an ordinary unsupported-name error, never lossy aliases;
  type/needed-extension checks precede encoding for opaque/nonfollowed entries.
- Approval diff rewriting stops at the hunk boundary, preserving literal `---`/`+++` content. Only
  documented git diff exit0/1 statuses succeed; signal/error output cannot approve a preview.

## Individual proposed ledger evidence

| Finding | Implemented boundary                                                                                                             | Meaningful verification / evidence                                                                                                                                                                                                                                                  |
| ------- | -------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 024     | Progress publication/result/validation failures use shared cleanup.                                                              | `progress_post_write_failure_restores_owned_bytes_but_discards_external_restoration`: ordinary failure restores bytes; detected external edit wins with typed state and no contents.                                                                                                |
| 025     | `canonical_target` streams/short-circuits exact paths; retained native case/identity resolution.                                 | Existing `reverse_evidence_uses_canonical_metadata_and_aliases_cannot_bypass_it` and independent-process coordination; source proof removes sibling collection. Linux runtime only; no Windows/macOS timing claim.                                                                  |
| 026     | Shared expected-absent `persist_noclobber`; AlreadyExists becomes conflict before receipt.                                       | `progress_absent_publication_does_not_clobber_an_intervening_creator` preserves creator bytes and returns stale conflict.                                                                                                                                                           |
| 027     | `canonical_diff` rewrites real pre-hunk headers only.                                                                            | Real git diff `literal_hunk_lines_are_not_rewritten_as_canonical_file_headers`, plus existing `diff_paths` integration.                                                                                                                                                             |
| 028     | Shared receipts exist only for changed successful writes.                                                                        | `contested_batch_cleanup_continues_and_never_rewrites_noop_or_unwritten_targets`, `governed_write_failure_does_not_republish_an_unchanged_matrix`, `governed_post_write_cleanup_leaves_external_matrix_and_removes_own_transition`; bytes/revisions and continued cleanup asserted. |
| 029     | `require_diff_success` rejects signal and non0/1 statuses.                                                                       | Unix `terminated_and_failed_diff_processes_cannot_publish_partial_approval_output` executes terminated/exit2 processes with partial stdout.                                                                                                                                         |
| 030     | Progress `capture`/`prepare` hand forward parsed existing/proposed facts and one rendered proposal; result remains fresh.        | Existing bootstrap/append/replay/binding/governance tests, plus actual post-write cleanup tests; no dedicated progress-append Criterion/allocation instrumentation.                                                                                                                 |
| 071     | `git_diff` returns immediately for byte-equal inputs.                                                                            | Existing `record_delete_no_op_and_default_board_receipts_survive_disjoint_result_changes`; original no-op batch1/10 controls below. No test pins internal subprocess counts.                                                                                                        |
| 073     | Discovery caches one log input per phase, independent confirmed pass, full-or-operation projection handed into capture.          | Shared-parser valid/escaped/declaration-order parity and malformed-unselected error parity in core's existing two projection tests; existing reverse-progress membership/atomic continuation tests.                                                                                 |
| 076     | Append IDs maintained with a set; replay uses indexed entries.                                                                   | Existing progress bootstrap/append/replay and `atomic_multi_ticket_continuations_capture_the_complete_proposal` preserve operation conflicts/validation; source removes growing-log duplicate searches.                                                                             |
| 080     | Scope-selected evidence/review work plus required global ID and reverse facts.                                                   | `growth_and_nontransitive_neighbors_do_not_expand_body_reads_or_conflicts`, `global_identity_phantoms_and_multiline_board_metadata_remain_conflicts`, `shared_reference_changes_and_reverse_progress_membership_cannot_write_skew` and real coordinated processes.                  |
| 081     | Board/transition header metadata parses once after complete read.                                                                | Existing global identity/multiline late-ID fixture and governance collision/invalid-schema tests. No isolated header-parser benchmark.                                                                                                                                              |
| 091     | Acquisition-local native case probe map.                                                                                         | Existing alias/collision and independent-process tests; source proof shows no across-operation cache. Probe counts and non-Linux platforms unmeasured.                                                                                                                              |
| 094     | Original bytes owned once, borrowed receipts/projections and parse facts; no repeated deep-copy captured body maps.              | Existing disjoint result windows plus progress/batch/governance failure interleavings. Byte-copy/allocation counts not instrumented.                                                                                                                                                |
| 095     | Pure `preflight` before canonical/filesystem capture.                                                                            | Source ordering is straightforward; existing duplicate/malformed/tampered batch tests remain. No guard-restatement test added.                                                                                                                                                      |
| 096     | Core `parse_selected_strategy_matrix` centralizes original strict matrix validation into typed owned facts reused by governance. | Existing authoritative transition tamper, strict matrix, replay, binding and phase-validation tests; result independent verification retained.                                                                                                                                      |
| 098     | Identity-indexed supersession frontier retains duplicate resolution/cycle reachability.                                          | `reverse_ordered_supersession_closure_rejects_a_new_cycle_without_writing` plus existing global identity/phantom and reference-cycle checks.                                                                                                                                        |
| 100     | Rendered overlay handed through canonicalization/prepare/apply once each trusted phase.                                          | Existing complete proposal/tamper and actual changed-record preview/apply controls; no weakened independent apply validation.                                                                                                                                                       |
| 101     | `(scope, accepted ID)` lookup once per discovery instead of all candidates per log.                                              | Existing precise same-scope progress rejections, nested scope/atomic continuation tests and shared reverse-progress tests.                                                                                                                                                          |
| 102     | Context/result path maps replace repeated linear batch target searches.                                                          | Existing atomic batch/no-op/stale/rollback behavior and batch1/10 controls; structural lookup improvement, no universal scaling claim.                                                                                                                                              |
| 109     | `mutation_metadata::list` inspects needed type/extension first, then exact UTF-8 representation or explicit ordinary error.      | Source evidence removes authoritative `to_string_lossy` construction, retaining opaque/nonfollowed entries. No public exploit/Windows reproduction or performance claim invented.                                                                                                   |

The shared error boundary is tested additionally by CLI
`rollback_failure_channel_retains_details_and_excludes_private_source_text`, actual initialized MCP
ToolService `mcp_apply_rollback_failure_keeps_error_flag_and_declared_structured_details`, and
server `rollback_error_crosses_the_http_failure_channel_as_409_without_private_source_contents`.
Store tests induce real filesystem cleanup interleavings. Adapter tests inject that typed error: the
HTTP test uses real loopback bytes and the same production responder, but does not claim a
cross-crate injected filesystem rollback through Host. MCP's captured advertised apply schema and
actual tools/call response passed external jsonschema4.26.0 validation using temporary repo-pinned
Nix tooling, not a handwritten validator or product dependency.
[Actual selected tool declaration and error response](results/hmd061-mcp-error.json).

## Actual verification

**172 focused tests passed** on final source: core17, Store lib54, derived2, Provider16, scanning9,
governance11, v1 29, validation1, diff_paths1, server2, CLI bin4/check8/mcp6/serve3/tui9. Workspace
all-target Clippy `-D warnings`, Rust fmt check, release benchmark compilation and external schema
validation all exited0. Original successful171/53-test checkpoints were superseded after the final
reverse-chain fixture; earlier Clippy errors were corrected and are recorded, not masked.
[Exact commands and final/exploratory verification output](results/hmd061-verification.txt).

Root explicitly approved the two private Store siblings, necessary typed Core matrix/parser seams,
shared error exports and coupled CLI/MCP/HTTP paths before hardening. The single existing
`core/src/progress.rs` re-export gateway was added before its **exact path** gate, reported
immediately, and explicitly approved before final verification; prior ownership is not retroactively
claimed. No dependency, manifest, lockfile, benchmark helper/source, frontend or planning record
changed. The necessary existing MCP success-schema fixture now checks its unchanged success branch
in the new success/error union rather than assuming a success-only root schema.

## Matched native Criterion controls

Final source-identical run: **2026-09-30 19:34:07 UTC-19:34:29 UTC**. Release Criterion0.8.2, 10
samples, 1s warm-up, 2s measurement target (actual sampling extends when needed), 95% bootstrap
confidence interval and default1% noise threshold. Warm disposable synthetic fixtures only; no
live-store data, competing builds, variants, repeats, new profiling or numerical acceptance gate.

Existing `250` controls add250 tickets to the minimum ticket+epic fixture (**252 work items**).
Batch1/10 controls are byte-equal replacements; they are not changed-batch apply measurements.
Priority controls edit one ticket with a 500-note log in the scope. Preview measures actual Provider
preview; apply alternates two drafts, with preview/setup outside its timed operation. Fixture
creation/git setup/validation and Criterion per-iteration setup remain outside timing.

Native named baseline `hmd057-before` remains in root-owned retained absolute target:
`/home/alex/dev/HUMANS.md/.agent-workspace/20260930-speedup-implementation/writer/target`.

```sh
export CARGO_TARGET_DIR=/home/alex/dev/HUMANS.md/.agent-workspace/20260930-speedup-implementation/writer/target
"$CARGO_TARGET_DIR/release/deps/baselines-79c3fc1f4572a921" '^mutation_preview_250_records/replace_batch/(1|10)$' --bench --baseline hmd057-before
"$CARGO_TARGET_DIR/release/deps/priority-9c72257aadc5988c" '^priority_single_record_250_records_500_notes/(preview|apply)$' --bench --baseline hmd057-before
```

Absolute estimates below are **means** (Criterion stdout may display regression-slope estimates).
All four controls, including small/unclear results, are retained:

| Existing control        | Before mean | After mean | Mean change, 95% CI        | Criterion interpretation                                            |
| ----------------------- | ----------: | ---------: | -------------------------- | ------------------------------------------------------------------- |
| Batch preview1          |    25.573ms |   24.960ms | -2.40% [-4.52%, -0.28%]    | Within configured noise threshold; not a demonstrated material win. |
| Batch preview10         |    44.958ms |   44.499ms | -1.02% [-3.89%, +2.51%]    | No detected change; possible regression included in CI.             |
| Provider single preview |    30.879ms |   26.870ms | -12.99% [-14.56%, -11.33%] | Improvement detected.                                               |
| Provider single apply   |    30.517ms |   27.328ms | -10.45% [-12.39%, -7.76%]  | Improvement detected.                                               |

[All samples, estimates, comparisons and final-source/executable hashes](results/hmd061-after.json),
[actual Criterion stdout](results/hmd061-criterion.log). Comparisons include
cumulative058/059/060/061 against057, **not isolated061 attribution**. There is no claim of
universal speedup, allocation reduction measured in bytes, progress/governance latency, cold
initialization/resource profiling or Windows/macOS runtime proof. Structural removals are reviewable
code evidence; these four end-to-end controls are the measured evidence.

## Preservation and scratch closeout

All236 native named-baseline JSON hashes verified both before and after measurement; original42/59
suite sources/results/tables remain unchanged. No build outputs/binaries/HTML/archive are committed.
Writer-owned `writer/hmd061/` contains intermediate test/build logs, final samples capture tooling
and raw MCP tools/list capture; useful compact final and failed/exploratory evidence is copied to
the linked durable artifacts before that disposable directory is removed. Retained native target and
older pipeline scratch remain under **root's explicit cleanup ownership**, needed for later genuine
before/after comparisons. Temporary Nix verification packages did not modify profiles, manifests,
flakes, locks or live installation.
