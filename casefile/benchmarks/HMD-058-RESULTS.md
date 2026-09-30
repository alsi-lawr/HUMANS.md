# HMD-058 core parsing and validation evidence

Candidate implementation against accepted baseline `8e97fc4e368361d75b0323913ed4e56e745fc259`. All
eight assigned findings have proposed implementation evidence below; final acceptance and the
canonical finding ledger remain root-owned. No non-defect rejection is proposed.

## Finding-by-finding proposed ledger evidence

| Finding | Implemented outcome                                                                                                                                                                                                                                                                     | Focused evidence                                                                                                                                                                                                                                                                                                                   | Performance scope                                                                                |
| ------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| 017     | Checked worker-minimum accumulation; overflow and exceeding capacity both return `strategy_capacity`.                                                                                                                                                                                   | `strategy::tests::worker_minimum_sum_overflow_is_a_capacity_diagnostic`, passed in debug and release with three `i64::MAX` minima and an `i64::MAX` capacity.                                                                                                                                                                      | Correctness; unmeasured.                                                                         |
| 018     | One CommonMark heading/offset pass supplies H1 identity and H2 section boundaries. Section strings are moved into the draft, not cloned from a second representation. The public heading helper contract stays unchanged.                                                               | `work_item::tests::fenced_headings_and_inline_heading_markup_preserve_work_item_sections`; existing nested-heading extraction test; parse/render roundtrip retains body content.                                                                                                                                                   | Matched ticket parse/render Criterion controls below; no allocation-profile or end-to-end claim. |
| 019     | Extract complete `PREFIX-D-<digits>` from `PREFIX-D-<digits>[-slug].md`, then require an exact H1 token. No new uppercase-prefix or minimum-digit-count policy.                                                                                                                         | `decision::tests::filename_identity_cannot_be_satisfied_by_prefixes_or_longer_heading_tokens`; existing Store project-scoped decision-resolution check.                                                                                                                                                                            | Structural/correctness; unmeasured.                                                              |
| 020     | Typed project-map decoding preserves the mapping; each configured source root must satisfy native `Path::is_absolute`. Empty/relative roots return `invalid_project_map`; existence is not checked.                                                                                     | Public values/summary API probe for empty, relative, drive-relative, Unix/Windows-shaped, and unavailable absolute roots; existing Store extra-project-map-entry success fixture adapted to native absolute offline paths.                                                                                                         | Structural/correctness; unmeasured. Windows-native acceptance was not executed on Linux.         |
| 021     | Kind/status filters reject unsupported or status-source-mismatched tokens in the shared validator, including independently constructed mutation drafts. Filter arrays are converted once. Column-status policy is unchanged.                                                            | Public parse/render probes for valid disposition/progress filters, misspellings, context mismatch, and malformed independent drafts; two existing Store progress-board consumer tests.                                                                                                                                             | Structural/correctness; unmeasured.                                                              |
| 022     | Fixed byte predicate replaces per-record regex compilation while preserving `[a-z0-9][a-z0-9-]*`: nonempty, first lowercase ASCII letter/digit, remaining lowercase ASCII letters/digits/hyphens.                                                                                       | Existing strict transition parse/render roundtrip; structural equivalence of the predicate to the prior anchored expression.                                                                                                                                                                                                       | Structural; no transition-specific timing claim.                                                 |
| 023     | One typed metadata parse distinguishes absent metadata from malformed-present or unclosed frontmatter. Present errors cannot be bypassed by decision H2 sections. Valid heading-only and valid frontmatter forms remain supported. Obsolete error-swallowing `value` helper is removed. | `decision::tests::heading_shape_cannot_bypass_present_malformed_frontmatter`; existing CRLF metadata test.                                                                                                                                                                                                                         | Structural/correctness; unmeasured.                                                              |
| 087     | Component-lexicographic sort followed by an outer-covering-claim sweep replaces cross-owner all-pairs comparisons. Safe-path validation remains unchanged; no path text is rewritten.                                                                                                   | `strategy_transition::tests::same_owner_descendants_cannot_hide_cross_owner_ancestor_conflicts` and `ownership_distinguishes_exact_conflicts_from_same_owner_and_disjoint_subtrees`. Includes `A:a,A:a/x,B:a/y`, punctuation interleaving `A:a,A:a-x,B:a/y`, exact conflicts, repeated same-owner ancestry, and disjoint siblings. | Structural complexity improvement; no ownership-specific timing claim.                           |

The ownership sweep relies on component sorting, not raw string sorting: each subtree is contiguous.
A retained outer claim subsumes intervening same-owner descendants, so they cannot hide a later
cross-owner conflict. Once the sweep leaves that subtree, no later claim can re-enter it. Sorting
cost is comparison-sort complexity plus path comparison; the sweep visits each claim once. No trie,
framework, parallel execution, or public API was added.

## Focused verification

- `cargo test -p casefile-core --lib`: **15 passed**.
- `cargo test -p casefile-core --release --lib`: **15 passed**, including the overflow case.
- `cargo clippy -p casefile-core --all-targets -- -D warnings`: passed.
- `cargo clippy -p casefile-store --test v1 -- -D warnings`: passed.
- `cargo fmt --all --check`: passed.
- Existing Store `v1` focused consumers: board/progress **2 passed**, structural fault table **1
  passed**, project-scoped decision resolution **1 passed**.
- One-off public-operation probes: native source-root policy and board parse/independent-render
  boundaries passed. Their source, command, and output are retained in
  [verification evidence](results/hmd058-verification.txt), not added as guard-restatement unit
  tests.
- Original HMD-057 source/table/checked-in before artifacts and all **236** native saved-baseline
  JSON files were hash-verified unchanged.

The Store structural-fault test initially failed because its success tail used relative roots
`demo='x'` and `legacy='keep'`. Those violate the expressly accepted root policy. The approved
fixture-only correction uses absolute offline paths beneath its disposable root and preserves the
extra-project-entry success assertion and all fault assertions. The final rerun passed; it
supersedes an earlier premature PASS checkpoint. No Store production code was changed.

No planning file was rewritten or migrated. The tightened malformed-input rules are the accepted
policy changes, not an implicit valid-data-format migration. Decisions retain optional filename
slugs, lowercase prefixes, and short numeric IDs. Source roots do not need to exist. Column
statuses, safe relative ownership-path semantics, and same-owner ownership claims retain their prior
policy.

## Matched Criterion comparison

Same host/toolchain and fixture/timing definitions as [HMD-057](HMD-057-BASELINE.md): Linux 6.18.53
x86_64, Ryzen 7 5700X3D, repository Nix shell rustc 1.95.0, Criterion 0.8.2, optimized bench
profile, ten samples, one-second warm-up, two-second measurement target, default
bootstrap/confidence settings. Warm filesystem caches; no CPU pinning or controlled frequency. No
competing writer/root build ran during measurement. Run: 2026-09-30 12:54:47–12:54:59 UTC.

```sh
cd casefile
export CARGO_TARGET_DIR=/home/alex/dev/HUMANS.md/.agent-workspace/20260930-speedup-implementation/writer/target
nix develop .. --command cargo bench -p casefile-store-sqlite --bench baselines -- \
  core_parsing --noplot --baseline hmd057-before
```

The controls and generated synthetic inputs are unchanged. The after run selects the existing
`core_parsing` group; the before group came from the complete HMD-057 run. Criterion compares
natively against the saved `hmd057-before` samples, without reconstructing that baseline. Named
before files remain unchanged; after results are in Criterion's `new` directories. The root retains
the target as active pipeline scratch and owns its eventual cleanup.

Times below are **microseconds per operation**. Confidence intervals for the median and native
Criterion's relative mean-time change are both 95%; mean changes need not equal median ratios.

| Scenario                                | Before median (µs) | After median (µs) | After median 95% CI (µs) | Native mean-time change (95% CI) |
| --------------------------------------- | -----------------: | ----------------: | -----------------------: | -------------------------------: |
| `core_parsing/progress_parse_500_notes` |           2133.158 |          2143.347 |        2122.049–2156.684 |           +0.50% (-0.26%–+1.30%) |
| `core_parsing/ticket_parse`             |             31.240 |            27.253 |            26.824–27.663 |        -13.44% (-15.14%–-11.96%) |
| `core_parsing/ticket_render_roundtrip`  |             36.095 |            29.261 |            28.884–29.358 |        -19.32% (-20.08%–-18.65%) |

Criterion reports ticket parse and render-roundtrip improvements, and no detected performance change
in the unmodified progress-parser control (`p=0.27`). No suspicious regression required repetition.
These results support lower time only for the measured parsing/rendering operations, not Store
refresh, TUI/HTTP latency, decision/transition validation, heap allocations, or peak memory.

[After estimates, raw sample vectors, and native change estimates](results/hmd058-after.json),
[Criterion stdout](results/hmd058-criterion.log), and
[focused verification/probe evidence](results/hmd058-verification.txt) retain the reviewable
results. No binaries, caches, HTML plots, or archives are committed.
