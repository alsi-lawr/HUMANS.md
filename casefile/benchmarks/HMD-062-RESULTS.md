# HMD-062 presentation freshness and incremental loading

Local candidate against reviewed parent `80d7bb79026585db655fbef7bc00f8d7ea3b741e`. All **nine**
assigned outcomes are proposed resolved for exact-commit review. Root owns the canonical
ledger/dispositions; this report neither accepts its own review nor starts HMD-063.

## Contract and implementation

- Same-session acceptance uses a private internal order, independent of caller generation. New
  overlapping targets cancel older work; disjoint targets remain independent. Cache publication,
  received-handle registration and manual cancellation serialize through the existing state mutex.
  Late queued older events cannot replace current handles. Earlier scopes may already have emitted
  legitimate partial events **before** cancellation; these are not retroactively retracted.
- Completed scopes stage locally and commit together only for the current, noncancelled request.
  Fresh file reads classify/parse once. Metadata-only snapshots, compact local projections,
  validated operation/ticket pairs and folded progress are retained privately; public entry bodies
  remain unchanged and are shared through existing `Arc<PresentationEntry>` values. Unchanged
  bodies/messages are not reparsed or deep-cloned merely because a neighbor changed.
- Necessary progress membership/log, implementation/binding and board card dependencies drive
  projection. Note-only edits retain unchanged board entries; status transitions update cards. Root
  project-map facts also depend on activated project keys, without invalidating unrelated
  investigation facts/handles. Diagnostic and board outputs are indexed by path/identity; unchanged
  dependent values keep their public immutable entry.
- Descriptor ownership uses the existing indexed deepest-scope resolver. Unchanged lazy paths keep
  their handle; changed/deleted descriptors invalidate their old handles across received views.
  Native names encode exactly or produce an ordinary explicit UTF-8 representation error, never a
  lossy alias. The public string path/entry/content-handle DTOs remain unchanged.
- Existing platform readers/no-follow/type checks remain. The human accepted deliberate ancestor
  swaps as a rare **full-refresh edge case on both Linux and Windows**, not an absent defect.
  Refresh reacquires current descriptors and disposable state. There is **no anchored traversal or
  race-proof containment promise**, new platform API, watcher, repair, retry or durable framework.
- Reads check cancellation between fixed-size chunks, before/after each read and before subsequent
  classification/projection/materialization. Discovery checks between entries; scope loops and
  publication check cancellation. Bounded-channel cancellation wakes parked producers. A blocking OS
  call or an in-progress parser/validator call is not forcibly interrupted; these checkpoints are
  cooperative, not a wall-clock deadline or an allocation-profile result.

Necessary same-crate reuse remains central: record/binding/progress projection and board engine in
`derived`, cached-operation progress diagnostics in `validation`. Superseded presentation
whole-scope byte reconstruction/reparse helpers and zero-consumer `classify`/`binding_diagnostics`
wrappers were removed, not retained as parallel implementations. Genuine canonical scans still own
all included bytes; narrow Provider tokens and independent mutation authority are unchanged. No
dependency, public DTO, wire schema, persisted format, CLI/server/frontend or benchmark-source
change.

## Individual proposed ledger evidence

Tests are in [presentation/tests.rs](../casefile-store/src/presentation/tests.rs).

| Finding | Implemented boundary                                                                                                             | Meaningful evidence                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| ------- | -------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 040     | Internal accepted order and coherent current-request cache/handle publication.                                                   | `newer_nonmonotonic_load_wins_and_late_old_events_cannot_replace_handles`: paused99 loses to2, then0; current body/cache/handle survive delayed old events. `disjoint_target_loads_do_not_cancel_each_other_or_invalidate_received_handles` preserves separate views.                                                                                                                                                                                                                                             |
| 041     | Human-selected rare ancestor-swap/full-refresh limitation; unchanged platform checks.                                            | Unix `full_refresh_after_ancestor_replacement_reacquires_descriptors_without_an_anchoring_promise` replaces parent, rejects its observed symlink, refreshes current contained bytes/new descriptor, rejects old handle and completes a fresh session. Existing root/parent-symlink checks retained. No Windows runtime or after-check race-proof claim.                                                                                                                                                           |
| 042     | One existing `ScopeIndex` per walk replaces repeated linear activation searches.                                                 | `nested_activation_uses_deepest_scope_and_membership_change_reclassifies_cached_paths`: deepest nested activation owns record, removal reclassifies the same unchanged path as lazy raw. Structural indexed-resolution evidence, not isolated root-count timing.                                                                                                                                                                                                                                                  |
| 043     | Exact native-name conversion or explicit error; no replacement-character reconstruction.                                         | Unix `unrepresentable_native_names_fail_explicitly_instead_of_aliasing_a_replacement_name`: distinct FF/FE names plus genuine replacement-character name produce explicit Failure, never Complete/alias. Linux evidence only.                                                                                                                                                                                                                                                                                     |
| 044     | Per-file immutable facts reuse; central conditional dependent projections.                                                       | `one_ticket_membership_edit_updates_progress_diagnostics_and_shares_unrelated_entries`: disposition edit invalidates log target but preserves unrelated review Arc/evidence handle. `note_only_edit_shares_board_entry_but_transition_updates_cards` preserves note content while selectively changing board cards. `activation_project_membership_invalidates_map_facts_not_unrelated_entries_or_handles` exercises necessary root-map dependency. Existing canonical-fact equivalence and live-save tests pass. |
| 045     | Chunked cancel-aware reads, coalescing and bounded producer wakeup.                                                              | `interrupted_reads_retry_but_cancellation_never_returns_partial_content` handles Interrupted then cancellation during a read without returning partial bytes. Existing `batches_and_channels_are_bounded_and_cancellation_stops_backpressure` plus paused final-read cancellation. No forced syscall interruption claim.                                                                                                                                                                                          |
| 068     | Local diagnostics retained once; cross diagnostics indexed by path; borrowed equality avoids unnecessary unchanged-entry copies. | Membership test updates the progress-path diagnostic and shares unrelated public entry; existing per-entry read failure/retry and canonical diagnostic parity remain. Structural elimination of per-entry whole-diagnostic scanning, not an allocation measurement.                                                                                                                                                                                                                                               |
| 078     | Stable equal-descriptor lazy handles; precise changed/deleted invalidation across views.                                         | `lazy_handles_survive_unrelated_edits_but_changed_and_deleted_paths_are_invalidated`; unchanged evidence/raw handles survive neighbor edit, changed raw/deleted evidence old handles fail. Existing untouched-target handle test and disjoint-target regression pass.                                                                                                                                                                                                                                             |
| 099     | Post-read and pre-projection/materialization/commit cancellation barriers.                                                       | `cancellation_after_the_final_read_does_not_publish_or_cache_that_scope`: cancel after captured final bytes, no scope Entries/Complete; subsequent same-version read failure proves cancelled facts were not cached. Late-old-load regression protects cache and received handles.                                                                                                                                                                                                                                |

## Actual verification

[Complete focused command/output evidence](results/hmd062-verification.txt): final sequential shell
used `set -e`; actual final exit0 for:

```sh
export CARGO_TARGET_DIR=/home/alex/dev/HUMANS.md/.agent-workspace/20260930-speedup-implementation/writer/target
nix develop . --command cargo test --manifest-path casefile/Cargo.toml -p casefile-store -p casefile-store-sqlite -p casefile-tui -p casefile-server
nix develop . --command cargo clippy --manifest-path casefile/Cargo.toml --workspace --all-targets -- -D warnings
nix develop . --command cargo fmt --manifest-path casefile/Cargo.toml --all --check
nix develop . --command cargo bench --manifest-path casefile/Cargo.toml -p casefile-store-sqlite --bench baselines --no-run
```

**200 tests passed**, including23 presentation tests (12 retained,11 new), canonical/derived/scoped
Store and mutation-safety consumers, SQLite4, TUI60 and HTTP2. No ignored/failing tests. Earlier
compiler/fixture/clippy/runtime-environment failures are transparently described in the evidence;
none is reported as PASS or hidden by a log-tail exit code.

## Native Criterion comparison

Final source-identical executable/source hashes, native samples/estimates/changes and baseline
samples/estimates: [hmd062-after.json](results/hmd062-after.json). Final output:
[hmd062-criterion.log](results/hmd062-criterion.log).

```sh
export CARGO_TARGET_DIR=/home/alex/dev/HUMANS.md/.agent-workspace/20260930-speedup-implementation/writer/target
$CARGO_TARGET_DIR/release/deps/baselines-79c3fc1f4572a921 '^presentation_250_records/(cold_session|unchanged_refresh)$' --bench --baseline hmd057-before
```

Final UTC window:2026-09-30T20:41:54.628558Z-20:42:05.080899Z. Criterion0.8.2; 10 samples,1s
warmup,2s measurement target,95% confidence,1% noise threshold. Release mode, warm filesystem,
disposable synthetic Store, no competing builds. Original names retain "250"; fixture has250 added
tickets + original ticket/epic = **252 work items,265 files,0 progress notes**. Fixture
creation/validation are outside the timed operation. Cold-session timing includes creating and
draining a new real presentation session (not cold filesystem cache); unchanged refresh drains a
previously loaded real session and still performs filesystem catalogue/metadata work.

| Original control                             | Before slope |     Final slope [95% CI] | Criterion mean-relative change [95% CI] |
| -------------------------------------------- | -----------: | -----------------------: | --------------------------------------: |
| `presentation_250_records/cold_session`      |     35.121ms | 17.717ms [17.555,17.895] |                 -49.47% [-50.27,-48.83] |
| `presentation_250_records/unchanged_refresh` |      1.420ms |    1.376ms [1.371,1.382] |                    -2.92% [-3.60,-2.18] |

Both measured comparisons improved; unchanged refresh has one high-mild outlier. These are
**cumulative changes since HMD-057**, including058/059/060/061, not isolated062 attribution or
universal performance claims. No timing/peak-allocation claim for one-entry edits, many nested
roots, large/cancelled logs, Windows, TUI rendering or HTTP end-to-end requests follows from these
two controls. Sharing/dependency/cancellation outcomes additionally have structural/behavioral
rather than profiler evidence. No parallelism was added.

An earlier successful two-control run preceded final manual-cancel mutex serialization;
[its distributions/configuration/source and executable provenance](results/hmd062-exploratory.json)
are retained as **superseded**, not final timing. A separate failed direct invocation omitted the
runtime target environment and failed before sampling; it did not recreate the saved baseline.
Original42/59 benchmark source/table/artifacts and all**236** native `hmd057-before` JSON files
passed immutable hash checks before/after the final run. No build cache/binary/archive/HTML report
is committed. Owned062 scratch is closed out; the absolute shared writer target remains active
pipeline scratch under root's final-cleanup ownership for genuine later baseline comparisons.

## Exact source ownership

Store `src/presentation.rs` and its `catalogue.rs`, `content.rs`, `entries.rs`, new private
`facts.rs`, `loading.rs`, `reader.rs`, `scope.rs`, `session.rs`, `tests.rs`; necessary root-approved
`src/derived.rs`, `src/derived/records.rs`, `src/derived/boards.rs`, `src/validation.rs`; exact
obsolete wrapper cleanup in `src/scanning.rs`, `src/scanning/classification.rs`. Durable evidence
consists only of this report and
`results/hmd062-{after.json,criterion.log,exploratory.json,verification.txt}`. No remaining
implementation contention;041 remains an explicit human-accepted limitation.
