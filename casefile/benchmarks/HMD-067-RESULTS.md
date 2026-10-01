# HMD-067: incremental TUI transitions and retained viewport layouts

Local-only candidate on `casefile-speedup-cleanup-20260930`, parent
`27ca531f7ab8fc19a9c6a17684269c2743f4f791`. These are proposed implementation/evidence outcomes, not
self-acceptance or canonical ledger mutations. No push, release, installation, restart, live-store
measurement, dependency, public DTO/API, wire change or visual redesign.

## Implemented boundaries

- Browser owns ordered path/scope membership, normalized filter facts, counts and selected-path
  indices. Relevant projection/filter/navigation changes maintain the visible projection; unchanged
  redraws reuse it. Rendering creates only viewport rows, including existing directory headers. File
  order, filtered hierarchy, removed/ambiguous selection and narrow/wide resize remain supported.
- App owns keyed record/entry indices, scoped ambiguity-aware identities, diagnostics, boards and
  relationships instead of repeatedly scanning/sorting flat global projections. Delta removal
  updates moved indices; only changed diagnostic paths and relationship source neighborhoods are
  replaced. Board cache invalidation considers relevant board/identity/diagnostic facts, not every
  status event.
- One current detail revision/tab/width, review diff/width and selected board projection/width
  retain layout cells/row offsets. Supported public Ratatui Paragraph/Buffer APIs perform wrapping;
  scrolling copies only visible cells. Review sanitization is done once; board selection markers are
  applied to visible cached rows. Width/tab/content/derived-only changes invalidate the relevant
  layout. This is not a parser cache followed by full Text cloning/layout on every redraw.
- List projections omit body bytes/content Strings and hold shared immutable PresentationEntry Arcs.
  Detail borrows the existing body, including eager strategy/source data. Owned Loaded events move
  into an Arc rather than cloning their body. Unchanged facts can retain an already fetched lazy
  body; changed non-body facts are not overwritten by old content. Canonical full Store.scan keeps
  its byte contract unchanged.
- Coordinator retains target ownership, cached catalogue facts/generation and keyed completion
  membership. Catalogue events move their owned data; content/status events do not rebuild roots.
  Scoped completion uses the requested path range and exact activated scope, excluding nested roots.
  Foreign catalogue-only entries are not admitted as target updates. Relationship identity and
  reverse-reference facts select the affected neighborhood, using the existing central derivation.
  Pending/Failure/cancellation still request redraw; unchanged payloads take the no-op merge path.
- Watcher classifies normalized native ancestors deepest-first. Dominated same-impact observations
  coalesce; per-impact generation floors survive suppression/partial clearing. Sibling investigation
  edits do not invalidate a requested refresh, but actual ancestor/nested overlaps do. The global
  handoff generation no longer emits a false target-specific completion warning; both report
  generations remain available to the existing scoped watcher freshness reducer. Observation
  compaction is not an arbitrary cap, dropped-event recovery policy or stronger filesystem
  guarantee.

The four new private modules are
[browsing/projection.rs](../casefile-tui/src/browsing/projection.rs),
[record_detail/layout.rs](../casefile-tui/src/record_detail/layout.rs),
[workbench/facts.rs](../casefile-tui/src/workbench/facts.rs) and
[workbench/boards.rs](../casefile-tui/src/workbench/boards.rs). There is no new crate/framework,
duplicated parser/relationship validator or benchmark implementation.

## Individual proposed finding evidence

Each row proposes resolved implementation evidence for root review; none is a non-defect rejection.
Test names below identify actual executed inline behavior checks, not a registration/count claim.

| Finding | Owned source outcome                                                                                      | Actual behavioral proof                                                                                                                                                                                                                                                                                                                                                                 |
| ------- | --------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 054     | Browser scope counts and normalized filter membership are maintained on relevant transitions.             | `section_header_counts_keep_their_scope_across_views_filters_and_scoped_refresh` preserves counts through all views/filtering and scoped file/board/strategy changes.                                                                                                                                                                                                                   |
| 055     | Revision/tab/width detail layout reuse; scrolling copies only viewport cells.                             | `rendered_and_source_tabs_keep_markdown_readable_and_exact`, `detail_scroll_resets_only_when_selected_content_revision_changes` and Unicode wrap/style/resize parity protect content and invalidation.                                                                                                                                                                                  |
| 056     | Freshness floors compare exact/ancestor/nested scopes, not all investigations in a project.               | `investigation_change_is_direct_without_invalidating_a_sibling` and `refresh_window_ignores_sibling_bursts_but_preserves_nested_floors_and_clears` exercise sibling bursts, requested clears and nested-window refusal.                                                                                                                                                                 |
| 057     | Facts owns ambiguity-aware scoped card identity lookup.                                                   | `board_keyboard_selection_marks_the_card_changes_detail_and_skips_unresolved_identities` and `board_card_selection_survives_complete_projection_with_deletion_and_ambiguity_controls` retain canonical path selection, missing/ambiguous refusal and deletion behavior.                                                                                                                 |
| 058     | Same-impact stale history coalesces; latest generation floors preserve refresh-window evidence.           | Repeated sibling/nested burst test checks observable warnings and clearing; `reducer_preserves_later_observation_and_failure_never_clears` and health restoration tests preserve failure/late observation semantics. No private history-size/cap assertion.                                                                                                                             |
| 059     | Ordered visible paths and row offsets reused; only viewport ListItems built.                              | `viewport_navigation_keeps_last_group_selected_across_resize_and_filter` traverses 120 extra grouped files, keeps the final row visible at three dimensions, then filters and navigates correctly. Existing directory/header/navigation tests pass.                                                                                                                                     |
| 060     | Sanitized review lines retained once and width layout reused.                                             | `review_scrolls_store_diff_and_colours_additions_and_removals` and `native_scroll_edge_keeps_visible_diff_lines_beyond_u16_row_count` preserve literal colored diff lines and the existing reachable trailing viewport at End/resize/Home.                                                                                                                                              |
| 063     | Arc body ownership plus empty list bytes/content; detail borrows the actual body.                         | `eager_and_lazy_detail_survive_promotion_refresh_and_external_edit` preserves eager ticket and strategy source, fetches lazy evidence, refreshes, then displays the actual external edit without replacing unrelated ticket bytes.                                                                                                                                                      |
| 069     | Keyed/path-range target completion and moved catalogue ownership.                                         | `parent_scope_refresh_keeps_nested_body_and_removes_only_parent_members` proves requested parent removal does not replace nested loaded facts/body; existing exact nested-target and unrelated-row scoped-refresh tests pass.                                                                                                                                                           |
| 070     | Maintained selected-entry/record indices on redraw and delta replacement.                                 | Selected deletion/semantic promotion/ambiguity tests, strategy binding-only refresh and diagnostic/source tests preserve selection and current record details. `catalogue_root_change_rescopes_cached_card_identities_without_body_updates` exercises a catalogue-only containment change, reveals the resulting card ambiguity, and preserves diagnostics.                             |
| 074     | Component ancestor lookup with deepest activated-root precedence.                                         | `classifier_is_lexical_longest_root_and_exact_root_exclusions`, root-namespace tests, first catalogue rebuild and actual Linux recursive watcher fixture pass. Windows normalization is lexical evidence only.                                                                                                                                                                          |
| 079     | Path diagnostic count/display and scoped board diagnostics maintained incrementally.                      | `diagnostics_and_editing_remain_governed_path_only`, terminal-control diagnostic tests, `boards_distinguish_no_definition_invalid_and_empty` and scoped refresh tests preserve explicit errors and counts.                                                                                                                                                                              |
| 088     | Keyed removal/record membership and moved-index maintenance, without global sorting.                      | Parent/nested completion and selected deletion/promotion tests protect removal scope; viewport/grouped ordering and relationship parity preserve deterministic outward order.                                                                                                                                                                                                           |
| 089     | One scoped board line/layout cache with row offsets and visible selection overlay.                        | Board keyboard/card tests, narrow rendering, scoped progress/board replacement and deletion preserve rank, markers, canonical detail and invalid/empty states. The real scoped-refresh fixture renames board identity/title and verifies the old board disappears, then makes progress malformed, observes the explicit invalid board state, repairs it and recovers the current board. |
| 090     | Identity/reverse-reference neighborhood selection, central relationship derivation over borrowed records. | `canonical_relationships_follow_cross_scope_targets_without_replacing_source_bodies` compares actual canonical results after target/reference edits; `relationship_availability_tracks_project_coverage_not_a_stale_store_complete_flag` preserves explicit unavailable coverage.                                                                                                       |
| 092     | Keyed diagnostics/relationships and empty-delta fast paths; status is independent.                        | `status_only_content_events_redraw_and_selection_cancellation_rejects_late_delivery` observes Pending, Failure and selection cancellation while payload remains unchanged; obsolete-generation/failure and scoped projection tests pass.                                                                                                                                                |
| 093     | Cached catalogue facts/generation; watch catalogue rebuild only on changed catalogue generation.          | Status-only content test and actual eager/lazy promotion/refresh tests preserve roots and detail, with no global catalogue projection attached to status-only deltas.                                                                                                                                                                                                                   |

Tests reside in [workbench tests](../casefile-tui/src/workbench/tests.rs),
[progressive workbench tests](../casefile-tui/src/workbench/progressive_tests.rs),
[relationship tests](../casefile-tui/src/workbench/relationship_tests.rs),
[flow tests](../casefile-tui/src/workbench/flow_tests.rs),
[progressive tests](../casefile-tui/src/progressive/tests.rs),
[watching.rs](../casefile-tui/src/watching.rs), [review.rs](../casefile-tui/src/review.rs),
[record_detail.rs](../casefile-tui/src/record_detail.rs) and the shared layout module.

## Focused verification and transparent failures

[Actual commands/output, discovery/smoke and earlier failed probes](results/hmd067-verification.txt).
Final-source chain exit0: **67 TUI library tests pass**, versus the original60; TUI all-target
Clippy with warnings denied, ordinary TUI build, CLI consumer check, workspace Rust fmt and release
benchmark build pass. The seven added tests cover Unicode wrap/resize/style parity, native trailing
diff viewport, sibling/nested bursts, status-only/cancel delivery, nested completion, viewport
filtering and catalogue-only identity rescoping. Existing eager/lazy detail proof was strengthened
to include strategy source and an actual external edit. Tests do not assert private call counts,
cache capacities or implementation source text.

```sh
nix develop --command sh -ec '
  export CARGO_TARGET_DIR=/home/alex/dev/HUMANS.md/.agent-workspace/20260930-speedup-implementation/writer/target
  cd casefile
  cargo fmt -p casefile-tui
  cargo test -p casefile-tui --lib
  cargo clippy -p casefile-tui --all-targets -- -D warnings
  cargo build -p casefile-tui
  cargo check -p casefile-cli
  cargo fmt --all --check
  cargo bench -p casefile-tui --bench interactions --no-run
'
```

All execution chains preserve the actual exit (`sh -ec` and wrapper return/subprocess status).
Original three-control discovery and smoke exit0. Source/report/JSON staged diff-check passes. Full
staged diff-check reports only preserved raw stdout/TestBackend-grid whitespace and unified patch
context-space lines; raw evidence is not trimmed to hide those warnings. No integrated068 ASCII/CI
result is claimed. No workspace-wide release/package/native TTY, Windows runtime or
first-layout/heap profiling check is claimed.

Earlier output is retained, not disguised as final PASS: &str/String delta types, restored help
helpers accidentally removed during the board move, Box-versus-Arc Loaded ownership, fixture
constructor edits, RefCell borrow scopes, borrowed layout call, private DTO argument/style Clippy
corrections and a shell wrapper's reserved `status` variable. Raw App fixture additions were moved
before construction instead of teaching tests private cache rebuilding. Earlier tests expecting the
old incorrect sibling invalidation/history count now assert actual scoped warning behavior. The
board cache generation bug discovered by scoped refresh was fixed before final checks. A new
strategy expectation was corrected because the existing minimum fixture uses `solo`, which
legitimately has no supported flow chart; exact source remains available. The nested-body probe was
an actual product bug: scoped Store loads include foreign catalogue-only entries, which previously
replaced unrelated loaded bodies. Exact target admission now prevents that, and the final public
session/App regression passes. A temporary probe tried accessing a private Coordinator field and
failed to compile; the subsequent probe used the existing projection. Both failures remain recorded.
The first timing run preceded the final catalogue-only scope-fact reindex correction and is
explicitly superseded. Its new regression initially used the wrong header label; the actual rendered
header retained both diagnostics. Correcting that fixture assertion did not change product behavior.
The blanket global-generation completion warning was also removed, preserving report generations and
scoped watcher warnings. The post-start-observation regression verifies completed status is clear;
sibling/nested floor tests verify actual scoped freshness. Final checks and original controls reran
after these bounded corrections. Staged keyed-transition inspection then found an old board key
surviving an identity edit. Changed boards now remove the superseded scoped key; derived-board
absence invalidates its display cache. The existing real scoped fixture covers identity/title
rename, invalid progress, repair and removal before final checks and original controls rerun.

## Original native Criterion controls

[Final source/binary hashes, estimates, raw samples and configuration](results/hmd067-after.json),
[full Criterion output](results/hmd067-criterion.log). All three original controls ran once after
final checks with **source-identical final production code**, no competing builds, release
optimization, warm filesystem,10 samples/1s warmup/2s target, 95% confidence and1% noise threshold.
No new timing variant/repeat or numerical gate.

The original fixture adds250 tickets:252 work items,266 canonical files and264 scoped records, 500
synthetic progress notes and80 long-ticket paragraphs; TestBackend120x40. Fixtures, key loops, setup
and timed operations are unchanged. Source inspection of the unchanged Tickets/Home/Right/Tab setup
shows that detail scrolling selects the lexically first governed work item,
`epics/accepted/HMD-E-001.md`, not the extended `HMD-011` ticket. Thus the fixture contains a long
80-paragraph/500-note ticket, but this original detail control does **not** measure that ticket's
long-progress detail layout. No fixture/key change or extra timing variant was made; this limits the
measured detail claim. The actual benchmark executable includes production TUI modules; no module
shim update was necessary. App/Store construction is outside measurements. Boundaries are real App
key transitions plus TestBackend rendering, not watcher/session/canonical I/O acquisition, terminal
transport or user-perceived end-to-end latency.

```sh
CARGO_TARGET_DIR=/home/alex/dev/HUMANS.md/.agent-workspace/20260930-speedup-implementation/writer/target \
  /home/alex/dev/HUMANS.md/.agent-workspace/20260930-speedup-implementation/writer/target/release/deps/interactions-cee69abf78b7fcec \
  '^tui_250_records_500_notes/' --bench --baseline hmd057-before
```

| Control                   | Before mean ms | After mean ms | Mean change (95% CI)       |
| ------------------------- | -------------: | ------------: | -------------------------- |
| detail_scroll_20_frames   |         10.705 |         5.956 | -44.36% [-44.62%, -44.09%] |
| list_scroll_20_frames     |         11.114 |         6.453 | -41.94% [-42.46%, -41.54%] |
| selected_scope_navigation |          1.101 |         0.853 | -22.51% [-23.26%, -21.75%] |

All three improve against057; comparisons are cumulative accepted058-067, not an isolated067
multiplier. Table uses arithmetic means; Criterion headlines can use regression slope. The
superseded first run had one high severe detail-scroll outlier, retained in its raw evidence; the
final run reports one high mild navigation and one high mild list outlier, retained unchanged. No
allocation-count, peak-memory, first-layout, cold-start, board/watch/progressive latency, sustained
throughput or Windows performance claim.

[Superseded first distributions/provenance](results/hmd067-pre-rescope-after.json),
[stdout](results/hmd067-pre-rescope-criterion.log) and
[verified intermediate-to-first reverse source patch](results/hmd067-pre-rescope-source.patch)
remain intact. The second, post-rescope/pre-board-identity run is also preserved:
[distributions/provenance](results/hmd067-pre-board-identity-after.json),
[stdout](results/hmd067-pre-board-identity-criterion.log) and
[verified final-to-intermediate reverse patch](results/hmd067-pre-board-identity-source.patch).
These two historical patch steps reconstruct the superseded source hashes exactly; they are
provenance, not patches to apply to production. First means were navigation0.851ms/list6.364ms/
detail5.868ms; second means were navigation0.853ms/list6.359ms/detail5.962ms. Final means are above.
All distributions are retained without selecting an earlier winner, causal attribution to unmeasured
corrections, or a universal/no-regression claim. No extra performance variants.

Four original before artifact/source hashes and all236 named native before objects pass SHA256.
All68 previously tracked benchmark/evidence paths equal parent bytes. Original TUI harness hashes
match parent and057. No prior report, baseline table, sample, fixture/helper or benchmark source was
modified. No build cache, binary, HTML report or archive is committed.

## Ownership, scratch and supported limits

Only the assigned TUI sources/inline tests, four approved private modules and this report/results
change. No Store/core/export/manifest/dependency/other transport/frontend/style/planning mutation.
The previous flat projections and obsolete per-redraw helpers were replaced, not left as parallel
active implementations. Root owns all actual finding dispositions and exact-commit acceptance.

Layout retention trades first width-layout work and retained cells/row offsets for warm redraw
reuse. Cell storage scales with viewport width and wrapped rows, and can exceed the source Text
memory substantially. Review also retains its sanitized immutable lines; one current revision/tab/
width or scoped board cache is held, not every visited record/width. This tradeoff was approved, but
is **not heap-profiled or first-layout benchmarked**. All logical lines and previously reachable
native-u16 scroll/viewports are preserved, including visible rows beyond65535. The pre-existing u16
scroll range remains; no promise of arbitrary all-depth navigation or stronger wrapping API.
Dominance compaction retains scope generation floors, not an arbitrary resource bound; distinct
observed scopes can still retain distinct freshness facts.

Native observations and metadata remain fallible/non-atomic; no ancestor anchoring, portable CAS,
watch recovery framework or mutation-freshness weakening. Existing062 cancellation/scoped freshness
and065 caller/edit boundaries are unchanged. Tests run on Linux; lexical Windows cases are not
Windows runtime proof. Read/projection errors remain explicit and preserve prior complete data.

Owned067 scratch contains exploratory/failure/final build logs, discovery/smoke, temporary nested
probe, measurement provenance and raw stdout. Useful reviewable evidence is retained in the linked
text/JSON/log; owned067 scratch is removed at handoff. Root-owned writer/target and before-hash
manifests remain active until integrated068 closeout. No subsequent ticket is started by this
writer.
