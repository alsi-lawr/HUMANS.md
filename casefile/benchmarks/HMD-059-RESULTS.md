# HMD-059 scoped acquisition and validation evidence

Candidate against reviewed parent `53a0f17638d817a47b63a80cacb2dcfd1a7b2f2f`.
All 33 assigned outcomes are implemented below. These are proposed ledger entries, not canonical
planning mutations or human acceptance of a non-defect disposition. No non-defect rejection is
proposed. Provider protocol **4** is the deliberately selected source wire change; the installed
runtime and dated MCP transport negotiation were not changed.

## Read boundary and freshness contract

- Catalogue/snapshot uses tagged `CatalogueToken` and a global inventory revision. Complete-index,
  cache, presentation, and independent preview/apply freshness remain global. Full `Store::scan`
  still returns every regular file's actual bytes; it is not a summary benchmark disguised as a
  byte-complete scan.
- Each narrow result uses tagged `ScopeReadToken` with its exact query and project/investigation;
  detail additionally identifies the requested record. Tokens compare only for the same typed
  target. The digest also includes that domain, so extracting the inner stamp cannot accidentally
  equate different queries/scopes. No narrow operation computes a whole-store token.
- Index observes selected ticket/epic membership and descriptors; boards additionally observes
  selected boards; transition queries observe selected modern transition candidates. Index/boards
  currently also depend on the canonical parent progress path because their existing consumers
  use progress. That dependency is optional in the observation, not a permanent unconditional
  freshness policy; conditional board/progress work remains HMD-060's separate responsibility.
- Exact detail checks the six existing ticket/epic disposition paths. Absence is represented by
  their absence from the selected stamp; creation, deletion, duplicates, or nested activation
  exclusion changes affect the token. Existing malformed/unsafe records error explicitly rather
  than becoming missing. Only a single existing accepted ticket loads progress; an absent record
  does not read even a malformed progress log. Existing core filename-stem identity rules remain.
- Diagnostics observe selected governed inputs plus same-project work-item/decision support, not
  foreign boards/evidence bodies. Selected mapping semantics affect this token; unrelated foreign
  mapping values do not. TOML syntax must still parse; the global catalogue/root check still
  validates every mapping value. Source roots remain native absolute offline paths.
- Diagnostic attachment dependencies stamp exact referenced paths' existence, regular/nonregular
  type and required containment. They do **not** stamp attachment body bytes/mtime. Unreferenced
  opaque edits and referenced regular-file body-only edits leave diagnostic results/tokens stable;
  missing/present, type, symlink, and containment changes affect them.
- One validated activation and retained parsed facts serve acquisition/classification/validation.
  End observation rechecks metadata/membership and necessary semantic dependencies, without
  reclassifying selected bodies or rebuilding/hashing the global inventory. An unchanged activation
  descriptor shares its already validated value; a changed one is revalidated and projected.
- Every selected read retains safe-parent checks and an opened-descriptor identity/stamp anchor.
  Final observations reject affecting edits; ordinary unrelated body edits do not become a global
  read barrier. The stronger original reader is still used by independent mutation paths.
- Scope ownership is the deepest activated ancestor, using component boundaries. Nested activated
  subtrees are pruned before body work; unactivated descendants remain parent-owned. Native
  unrepresentable filenames error explicitly, never collapse through lossy conversion. Ungoverned
  nonregular objects remain opaque and are not followed; governed unsafe inputs are explicit.
  Existing non-directory/symlink fixed containers needed by the current query also error rather
  than looking empty. Missing containers remain absent; an unrequested strategy container does
  not become a tickets-only index dependency.
  Existing non-directory/symlink fixed containers needed by the current query also error rather
  than looking empty. An explicitly activated root itself is a governed directory container,
  including in global check/scan; it cannot become opaque through ancestor-only lookup. Missing containers remain absent; an unrequested strategy container does
  not become a tickets-only index dependency.

## Proposed per-finding ledger evidence

| Finding | Implemented outcome | Focused evidence and measurement boundary |
| --- | --- | --- |
| 001 | Native `Path` exclusion precedes UTF conversion; one representable identity or explicit error, never lossy aliases. | `native_names_never_alias_and_opaque_symlinks_are_not_followed_in_deep_trees` creates two distinct Unix non-UTF8 names. Linux only; no Windows execution claim. |
| 002 | Existing invalid detail is an error containing its requested path/diagnostic; missing remains `None`. | `exact_detail_distinguishes_supported_records_malformed_duplicates_and_absence`: malformed, missing, duplicate, ticket/epic dispositions. |
| 003 | Narrow observation is selected dependencies, not two global inventories/hashes; global verification compares recorded file descriptors/directory membership without reconstructing an inventory/hash. | `scope_observation_rejects_affecting_changes_but_admits_unrelated_edits`; native inventory/scoped-check/detail controls. Global safety remains deliberately global. |
| 004 | Observational record reads use safe parent + opened descriptor stamp, then operation-level dependency verification, not redundant descriptor/path checks per read. Independent mutation reader remains stronger. | Descriptor/membership race test and parallel ordered failure/retry test. No isolated syscall-count claim. |
| 005 | Walker branches on directories before computing file revisions; no discarded directory revisions. | Actual inventory controls. Necessary final directory type/containment/membership checks are not removed. |
| 006 | Candidate discovery prunes unrelated groups and activated descendants before metadata/body classification. | Exact nested-scope regression; native selected-index/detail controls. |
| 007 | Removed unreachable binding diagnostics from index/board selected-kind paths; full/scoped diagnostic checks use real retained binding/strategy facts. | Existing binding/governance checks plus parent/nested/parent binding regression; no invented strategy data in index. |
| 008 | Iterate scoped candidate collection directly rather than materializing borrowed candidates solely to iterate. Parallel results retain actual output/error values, not a candidate-only vector. | Real Provider index controls and collection/parallel alternatives below. |
| 009 | Borrowed iterator checks one/ambiguous activated scope, cloning only the one accepted path. | Public nested/exact-detail scope tests; no clone-count or allocation profile claim. |
| 010 | Removed sorts that repeated global ordered inventory iteration. The chosen scoped Vec requires one candidate sort; combined selected/support output ordering is also necessary, not a redundant map sort. | Stable ordered parallel failure/retry and duplicate/cycle diagnostics; actual controls below. |
| 011 | Component-boundary `contains_path` avoids per-root concatenated prefix allocation. | Deepest activated scope/unactivated descendant regression. No isolated allocation timing. |
| 012 | Binding diagnostics use one canonical-scope implementation lookup and retained strategy projections instead of repeated scans/reparses. | `binding_diagnostics_use_canonical_scope_despite_parent_nested_parent_order`. |
| 013 | Full scan consumes already parsed activation state and emits its diagnostics once; superseded duplicate activation parser removed. | `project_decisions_are_flat_and_invalid_activation_diagnostics_are_not_duplicated`; root check/scan equivalence. |
| 014 | Ungoverned nonregular objects stay opaque; governed unsafe inputs are explicit instead of globally marking opaque attachments invalid. | Unix opaque/symlink/deep-tree, attachment type/containment, and requested-container missing/unsafe/non-following regressions. |
| 015 | Supported non-following iterative `walkdir` traversal replaces recursive filesystem walker. | 300-directory public scan fixture; inventory controls. No recursive thread-stack growth from directory traversal. |
| 016 | Summary/narrow check acquisitions open only needed bodies, retaining minimal facts; full byte-returning scan remains byte-complete. | Existing foreign-evidence/peak-body check tests; full scan versus scoped check controls. No unmeasured heap claim. |
| 031 | Iterative reverse-edge leaf elimination computes cycle-reachable nodes once rather than restarting recursive DFS per node. | `long_supersession_chains_and_cycle_reachable_ancestors_preserve_diagnostics`, 1200-node chain/cycle plus duplicate; supersession controls. |
| 032 | Preindex observed/accepted scoped ticket identities and filename basenames; progress misses use lookups rather than per-entry global scans. | Existing missing/invalid/nonaccepted progress tests; 50/250/500-note controls. |
| 033 | Project decisions are direct children only; nested archive files are ungoverned. | `project_decisions_are_flat_and_invalid_activation_diagnostics_are_not_duplicated`. |
| 034 | One capacity-reserved hex string receives digit pushes instead of allocating per-byte formatted strings. | Existing revision/stale mutation tests; overall controls only, no isolated hex timing. |
| 036 | Classification retains parsed drafts/metadata/progress/strategy facts and passes them into validation instead of reparsing the same records. | Existing v1/governance/public check equivalence and graph/progress tests; validation controls. |
| 038 | Binding facts group by canonical scope, not a contiguous lexicographic run. | Parent-binding / nested-strategy / parent-implementation regression catches the adapter mismatch in both check and scan. |
| 061 | Scoped acquisition avoids unrelated inventory paths; canonical path facts resolve selected/support scope/kind/project once per entry. | Activation-root/scoped-check controls and nested public regressions. No isolated root-lookup timing. |
| 065 | Prefix/reference checks reuse the resolved project and its configured prefix, not a second all-root walk per writable entry. | Existing project-scoped reference and prefix tests plus nested regression. |
| 066 | Kind grammar consumes a bounded component iterator rather than allocating a split-component vector; arbitrary nested review headings/path support preserved. | Existing layout/governance checks; structural, not isolated timing. |
| 067 | Narrow checks retain only selected records and required same-project support facts, not every unrelated entry. | Foreign-body observation regression; inventory/scoped-check controls. |
| 075 | Index/board workers drop ticket/epic original bytes and unused drafts before retaining outputs; detail moves its parsed draft without a second parse/body copy. | Real index/detail responses; structural ownership, no heap/peak-memory measurement claim. |
| 084 | Progress validation borrows the same resolved path-facts map as cross-validation. | Existing progress regressions; progress controls. |
| 085 | Deepest activated ownership excludes nested tickets/boards/transitions/progress from parent queries; canonical exact detail also respects exclusions. | `deepest_activated_scope_excludes_nested_records_boards_and_progress`, including a nested root at parent's accepted directory and progress path. |
| 105 | Exclude native `Path` before conversion; files build one string without temporary component vectors; directories do not allocate discarded relative strings. | Non-UTF8/deep/exclusion tests and inventory controls. |
| 106 | Legacy/current transition discrimination parses TOML once and passes the owned value through core's existing typed validator. | Existing legacy/current Store v1 and strict core transition checks. No copied validator or transition-specific timing claim. |
| 107 | Find/validate exact record before loading applicable progress; no progress for absence. | Missing detail with deliberately malformed log succeeds; native 0/500-note and priority missing-detail controls. |
| 108 | Foreign same-project board bodies are not diagnostic support. | Foreign-body check regression includes a malformed binary foreign board and verifies it was not opened; selected-board existing checks remain. |

## Six original human leads, separately accounted for

1. **`Vec<u8>`/`str`:** filesystem consumers retain owned bytes only when actually returning bodies;
   governed parsers borrow UTF8 text, opaque binary input is not decoded, parsed facts are reused.
   No extra byte-to-string copy is introduced; there is no allocation-profile claim.
2. **Rayon:** bounded two-worker actual read/classification stage measured below and retained only
   for its demonstrated Provider-query benefit. Zero/one selected candidate stays serial without
   initializing the pool. Pool initialization failure is an explicit error, not fallback/config.
3. **BTreeMap:** genuine sorted-Vec replacement with binary-search membership measured below.
   The combined Vec/Rayon choice retains scoped owned candidates directly, one required sort and
   deterministic last-write support deduplication. Global inventory remains BTreeMap. No universal
   collection superiority or non-defect rejection is asserted; both mixed large-run outcomes remain.
4. **Temporary candidate vectors:** removed; ordered parallel output/error values are necessary
   products, not borrowed candidates collected merely to iterate them.
5. **Cardinality clone vectors:** removed; borrowed one/ambiguous resolution and one accepted clone.
6. **walkdir:** adopted supported iterative non-following traversal, preserving exclusions and
   containment; final existing inventory controls measured below. No isolated dependency comparison
   or Windows benchmark is claimed.

## Focused verification

Commands ran through the repository Nix shell, with the retained absolute target below and `set -e`;
no tail/log command masked a failed status. Final production code checks (after attachment policy,
parallel stage, and race guards): core **15**, Store lib **44**, governance **11**, Provider **14**,
scanning **8**, v1 **29**, validation **1**, CLI MCP **6**, Python smoke unit **3**, all passed.
All-target core/Store/CLI Clippy with `-D warnings`, `cargo fmt --all --check`, source CLI build, and
an actual built-CLI MCP compatibility/smoke check on a disposable Store also passed. Mutation
preview/apply, stale revisions and governance rollback are covered by existing v1/governance/MCP
checks. The smoke exposed protocol4 and twelve tools; it did not install/run against a live store.

[Command/output evidence](results/hmd059-verification.txt) includes final test outputs and smoke.
Tests target scope isolation, requested-data race rejection, ordered parallel failure with complete
retry, diagnostic equivalence, native identity and graph behavior—not source/copy/tunable assertions.
Temporary Vec variants required borrowed iterator adapters to compile; checks were rerun
successfully before measuring. The container fixture initially needed an `Option<Kind>` comparison
correction; Clippy caught two helpers placed after test modules and they were moved, not suppressed.
Final checks passed after those corrections. Earlier serial/preliminary checks/numbers are not acceptance
claims for the final parallel implementation.

## Criterion configuration and measured boundaries

Same host/toolchain and unchanged fixtures/timing definitions as [HMD-057](HMD-057-BASELINE.md):
Linux 6.18.53 x86_64, Ryzen 7 5700X3D, Nix rustc 1.95.0, Criterion 0.8.2, optimized bench profile,
ten samples, one-second warm-up, two-second measurement target (Criterion extends where needed),
default bootstrap/confidence settings, warm filesystem caches. No CPU pinning/frequency control,
allocation instrumentation, or concurrent writer/root builds. All Stores are disposable synthetic
fixtures. Fixtures, validation, first-query/pool setup are outside the measured operation; query
controls measure actual synchronous Provider query acquisition/derivation/return, not helper loops.
Full scan/check/index controls retain their original operation boundaries. No TUI, HTTP, cold-init,
zero/one-candidate latency, thread-resource, or general end-to-end/allocation benefit is inferred.

```sh
export CARGO_TARGET_DIR=/home/alex/dev/HUMANS.md/.agent-workspace/20260930-speedup-implementation/writer/target
nix develop . --command cargo bench --manifest-path casefile/Cargo.toml \
  -p casefile-store-sqlite --bench baselines -- \
  '^(inventory|activation_roots|records|supersession|progress|scoped_progress_250_records)/' \
  --noplot --baseline hmd057-before
nix develop . --command cargo bench --manifest-path casefile/Cargo.toml \
  -p casefile-store-sqlite --bench priority -- priority_scope_500_notes \
  --noplot --baseline hmd057-before
```

The final production run compares **36 existing controls** natively with saved `hmd057-before`;
there is no baseline reconstruction. Original 42/59 control sources, table, checked-in before
artifacts and all 236 native saved-before JSON files remain immutable and hash-verified. The retained
target is active pipeline scratch owned by root for eventual cleanup, not a committed build cache.

Final source-identical matched run: **2026-09-30 15:18:35 UTC–2026-09-30 15:20:58 UTC**. It supersedes earlier serial and
pre-container-guard/exact-root-correction production runs. The comparison includes retained HMD-058 core improvements
against the saved pre-fix baseline; changes are not individually attributable to HMD-059.
Times below are microseconds per operation; median and native relative mean-change intervals
are 95%. Mean changes need not equal median ratios. Named ticket counts are generated additions to
the minimum fixture's existing two work-items, not literal total work-item counts.

| Existing scenario | Before median (µs) | After median (µs) | After median 95% CI (µs) | Native mean change 95% CI |
| --- | ---: | ---: | ---: | ---: |
| `activation_roots/scoped_check/1` | 17088.633 | 923.390 | 908.703–933.756 | -94.59% (-94.65%–-94.52%) |
| `activation_roots/scoped_check/100` | 17442.853 | 2590.154 | 2585.504–2594.967 | -85.17% (-85.21%–-85.12%) |
| `inventory/full_scan/2000` | 42176.970 | 36796.086 | 36746.637–37124.339 | -12.39% (-12.98%–-11.40%) |
| `inventory/full_scan/8000` | 170714.350 | 148061.217 | 146700.892–154619.151 | -12.47% (-14.28%–-10.20%) |
| `inventory/metadata_summary/2000` | 8056.767 | 6608.020 | 6591.039–6699.406 | -17.59% (-18.15%–-16.85%) |
| `inventory/metadata_summary/8000` | 31672.630 | 27235.047 | 26846.268–27480.577 | -14.07% (-14.88%–-13.24%) |
| `inventory/scoped_check/2000` | 17315.951 | 946.363 | 910.648–978.537 | -94.54% (-94.67%–-94.43%) |
| `inventory/scoped_check/8000` | 66613.746 | 947.705 | 926.643–970.015 | -98.58% (-98.60%–-98.56%) |
| `priority_scope_500_notes/existing_detail/1000` | 13651.321 | 4757.848 | 4741.137–4773.451 | -65.83% (-66.43%–-65.27%) |
| `priority_scope_500_notes/existing_detail/250` | 7071.768 | 4654.655 | 4638.403–4709.351 | -33.87% (-34.34%–-33.38%) |
| `priority_scope_500_notes/missing_detail/1000` | 11492.667 | 166.346 | 165.132–169.128 | -98.55% (-98.56%–-98.53%) |
| `priority_scope_500_notes/missing_detail/250` | 4869.075 | 162.741 | 161.752–163.095 | -96.66% (-96.68%–-96.65%) |
| `priority_scope_500_notes/selected_record_index/1000` | 64284.270 | 38657.199 | 38175.194–39311.598 | -39.85% (-40.56%–-39.05%) |
| `priority_scope_500_notes/selected_record_index/250` | 19561.704 | 13877.344 | 13560.694–14114.647 | -29.96% (-31.57%–-28.49%) |
| `progress/accepted_target/250` | 11506.414 | 2124.781 | 2118.795–2181.690 | -81.28% (-81.58%–-80.92%) |
| `progress/accepted_target/500` | 13535.071 | 3241.118 | 3219.561–3267.542 | -76.02% (-76.19%–-75.82%) |
| `progress/missing_targets/250` | 28870.040 | 2268.470 | 2252.026–2274.716 | -92.16% (-92.20%–-92.12%) |
| `progress/missing_targets/500` | 48300.840 | 3620.395 | 3605.408–3630.348 | -92.50% (-92.55%–-92.44%) |
| `records/boards/1000` | 60855.161 | 34478.064 | 34116.178–35078.868 | -43.58% (-44.49%–-42.81%) |
| `records/boards/250` | 15645.146 | 9081.764 | 8884.807–9168.244 | -41.87% (-42.96%–-40.60%) |
| `records/full_derived/1000` | 131583.458 | 98380.462 | 97392.001–99329.433 | -25.27% (-26.72%–-23.71%) |
| `records/full_derived/250` | 35256.579 | 25342.700 | 25288.280–25411.707 | -27.14% (-28.49%–-25.53%) |
| `records/record_index/1000` | 60985.557 | 34143.810 | 33618.537–34652.517 | -44.74% (-46.01%–-43.60%) |
| `records/record_index/250` | 15544.024 | 9081.845 | 8905.094–9461.982 | -42.19% (-44.83%–-39.50%) |
| `records/scoped_check/1000` | 96737.548 | 67092.736 | 66925.129–67734.853 | -31.72% (-33.27%–-30.21%) |
| `records/scoped_check/250` | 24589.100 | 17659.656 | 17503.946–17756.499 | -27.93% (-28.50%–-27.03%) |
| `scoped_progress_250_records/disposition_boards/0` | 15772.085 | 9391.630 | 9252.986–9848.350 | -38.38% (-41.18%–-34.43%) |
| `scoped_progress_250_records/disposition_boards/500` | 19780.136 | 13711.031 | 13537.467–13833.911 | -30.66% (-31.32%–-30.01%) |
| `scoped_progress_250_records/missing_detail/0` | 2937.132 | 165.086 | 163.934–165.430 | -94.40% (-94.44%–-94.36%) |
| `scoped_progress_250_records/missing_detail/500` | 4950.618 | 163.888 | 163.462–164.189 | -96.69% (-96.71%–-96.68%) |
| `scoped_progress_250_records/record_index/0` | 16441.160 | 8999.314 | 8948.862–9349.657 | -44.21% (-45.45%–-42.85%) |
| `scoped_progress_250_records/record_index/500` | 20005.878 | 13367.558 | 13161.138–13822.027 | -32.74% (-33.94%–-31.11%) |
| `supersession/chain/250` | 33107.183 | 17941.108 | 17792.197–19018.578 | -45.48% (-46.68%–-44.10%) |
| `supersession/chain/500` | 84201.301 | 34900.401 | 34796.609–35905.372 | -58.44% (-59.23%–-57.58%) |
| `supersession/independent/250` | 24897.621 | 17952.817 | 17738.012–18835.802 | -27.50% (-28.87%–-25.99%) |
| `supersession/independent/500` | 49084.745 | 33569.888 | 33201.438–35314.594 | -30.35% (-31.77%–-28.71%) |

[Final estimates/raw sample vectors/native change estimates](results/hmd059-after.json) and
[final Criterion stdout](results/hmd059-criterion.log) retain all 36 controls. All36 final controls
showed lower mean time against the saved pre-fix baseline. Changes are operation-specific, not
proof of allocation reduction, other file sizes, cold filesystems, or unmeasured front ends.

## Collection and bounded parallel alternatives

These experiments measure actual Provider record-index query acquisition/derivation/return. The
first stage used the complete HMD-059 serial/BTree candidate before the later fixed-container
correction, saved natively as `hmd059-serial`; no HMD-057 samples were invented. The sorted-Vec
variant **replaces** scoped BTree collection, sorts once, uses binary-search membership, and does
not materialize a final BTree. The two-worker variant uses a dedicated bounded Rayon pool for
actual guarded reads/classification, gathers ordered output/error values and then propagates the
first path-ordered failure before the same final dependency verification. Original mutation paths
are not parallelized. Worker initialization is an explicit error; no fallback/new configuration.

The first temporary `count=2` label adds two tickets to the base ticket/epic, so actually represents
**four** work-items plus 500 notes. It is not evidence for a minimum two-record Store; the supplement
uses the unextended **two**-work-item base plus an empty progress log. It has its own native saved
baseline `hmd059-minimum-serial`. The 250/1000 labels represent 252/1002 work-items respectively.
The post-container combined comparison has another native baseline, `hmd059-parallel-btree`,
measuring the exact same container safety and other semantics as the combined Vec/Rayon variant.
Zero/one selected candidate remains serial without pool initialization; no latency/cold-init claim
is made for those cases. First query/pool/fixture validation setup is outside measurements.

All comparisons use the same ten-sample/one-second/two-second settings above. The explicitly
unchanged Criterion 0.8.2 default noise threshold is **1%**. A confidence interval crossing that
classification boundary can be called “within noise threshold” even if wholly positive or negative;
it does **not** prove equal performance or absence of a slowdown. The first combined 1002 comparison
was genuinely slower in that comparison. It triggered one bounded repeat, preserved below—not
cherry-picking or an endless search for a favorable result.

Times below are microseconds. Each native change compares with its named baseline, not necessarily
the first table row. All confidence intervals are 95%.

| Variant / native baseline | Actual work-items / notes | Median (µs) | Median 95% CI (µs) | Native mean change 95% CI |
| --- | ---: | ---: | ---: | ---: |
| Serial BTree / saved059-serial | 1002/500 | 61458.475 | 60553.398–62009.384 | Saved baseline |
| Serial BTree / saved059-serial | 4/500 | 4816.978 | 4809.332–4821.757 | Saved baseline |
| Serial BTree / saved059-serial | 252/500 | 19941.577 | 19359.022–20026.624 | Saved baseline |
| Serial sortedVec /059-serial | 1002/500 | 61669.809 | 60202.552–64282.092 | +1.30% (-0.74%–+3.41%) |
| Serial sortedVec /059-serial | 4/500 | 4861.626 | 4717.571–4885.480 | +0.04% (-1.21%–+1.04%) |
| Serial sortedVec /059-serial | 252/500 | 19455.562 | 19370.346–19524.242 | -1.02% (-2.46%–+0.94%) |
| Rayon2+BTree /059-serial | 1002/500 | 39413.631 | 38657.628–40074.855 | -35.89% (-36.91%–-34.82%) |
| Rayon2+BTree /059-serial | 4/500 | 4760.956 | 4735.539–4795.216 | -1.03% (-1.53%–-0.57%) |
| Rayon2+BTree /059-serial | 252/500 | 13671.582 | 13555.275–13841.751 | -30.69% (-31.51%–-29.71%) |
| Minimum serial BTree / saved059-minimum-serial | 2/0 | 361.343 | 358.726–385.320 | Saved baseline |
| Minimum serial sortedVec /059-minimum-serial | 2/0 | 355.304 | 352.502–356.337 | -4.34% (-6.57%–-2.12%) |
| Minimum Rayon2+BTree /059-minimum-serial | 2/0 | 338.627 | 326.676–349.063 | -8.77% (-11.59%–-6.08%) |
| Postguard Rayon2+BTree / saved059-parallel-btree | 2/0 | 358.433 | 348.668–364.400 | Saved baseline |
| Postguard Rayon2+BTree / saved059-parallel-btree | 1002/500 | 37808.105 | 37488.893–38611.436 | Saved baseline |
| Postguard Rayon2+BTree / saved059-parallel-btree | 252/500 | 13664.753 | 13469.671–13853.133 | Saved baseline |
| Postguard Rayon2+Vec, first /059-parallel-btree | 2/0 | 345.663 | 332.787–354.493 | -3.64% (-6.05%–-1.19%) |
| Postguard Rayon2+Vec, first /059-parallel-btree | 1002/500 | 38929.152 | 38618.732–39173.609 | +2.21% (+0.84%–+3.44%) |
| Postguard Rayon2+Vec, first /059-parallel-btree | 252/500 | 13621.809 | 13465.645–13715.476 | -0.20% (-1.61%–+1.27%) |
| Postguard Rayon2+Vec, repeat /059-parallel-btree | 2/0 | 345.422 | 337.373–351.852 | -3.41% (-5.61%–-1.17%) |
| Postguard Rayon2+Vec, repeat /059-parallel-btree | 1002/500 | 36257.226 | 36199.140–36413.376 | -4.66% (-5.89%–-3.60%) |
| Postguard Rayon2+Vec, repeat /059-parallel-btree | 252/500 | 13408.414 | 13083.503–13456.615 | -2.33% (-3.84%–-0.73%) |

The original large serial collection comparison detected no latency difference, but the genuine
minimum showed a sorted-Vec benefit. The combined variant consistently improved the minimum versus
postguard Rayon/BTree (first −3.64%, repeat −3.41% mean changes). Its252-work-item results were near
equality/within threshold;1002 was first +2.21% slower and repeat −4.66% faster, with the full CIs and
absolute medians above. Root accepted the bounded interactive tradeoff: retain **sortedVec+Rayon2**,
not a universal winner claim, adaptive collection framework, or proof of zero larger regression.
The code-identical final 36-control run separately reports the chosen implementation versus057.
There is no allocation, cold initialization, thread-resource, other-core-count or Windows profile.

[All nine experiment distributions](results/hmd059-alternatives.json),
[all experiment stdout including both combined runs](results/hmd059-alternatives.log), and
[timed acquisition/processing source-variant patches and SHA256 provenance](results/hmd059-variants.patch)
retain reviewable evidence. Patch sections name their source basis and direction; apply only the
selected section sequence to a disposable copy, never the whole concatenated file. Hunks are
context-free (`git apply --unidiff-zero`) so patch context does not introduce whitespace artifacts. The foundation
patch starts from this final implementation and covers the timed acquisition/processing files,
canonical activation/layout and dependency manifest/lock; following sections reconstruct each measured alternative. Common classification/core/Provider logic remained unchanged through the collection experiments;
the patch also records activation/layout before the later exact-root containment correction. Temporary benchmark sections describe the initial added-two and actual minimum-two
controls; the benchmark source was restored byte-for-byte before final checks/run/commit.
No build caches, binaries, HTML, or archives are committed.

## Actual changed paths

Local-only immutable batch; no planning-store mutation, push, release, install or live migration.

```text
casefile/Cargo.lock
casefile/adapters/shared/casefile_runtime.py
casefile/benchmarks/HMD-059-RESULTS.md
casefile/benchmarks/results/hmd059-after.json
casefile/benchmarks/results/hmd059-alternatives.json
casefile/benchmarks/results/hmd059-alternatives.log
casefile/benchmarks/results/hmd059-criterion.log
casefile/benchmarks/results/hmd059-variants.patch
casefile/benchmarks/results/hmd059-verification.txt
casefile/casefile-cli/src/mcp.rs
casefile/casefile-cli/tests/mcp.rs
casefile/casefile-core/src/lib.rs
casefile/casefile-core/src/strategy.rs
casefile/casefile-core/src/strategy_transition.rs
casefile/casefile-store/Cargo.toml
casefile/casefile-store/src/activation.rs
casefile/casefile-store/src/checking.rs
casefile/casefile-store/src/layout.rs
casefile/casefile-store/src/lib.rs
casefile/casefile-store/src/provider.rs
casefile/casefile-store/src/read_context.rs
casefile/casefile-store/src/read_context/tokens.rs
casefile/casefile-store/src/revision.rs
casefile/casefile-store/src/scanning.rs
casefile/casefile-store/src/scanning/classification.rs
casefile/casefile-store/src/scanning/inventory.rs
casefile/casefile-store/src/scanning/scoped.rs
casefile/casefile-store/src/validation.rs
casefile/casefile-store/tests/provider.rs
casefile/casefile-store/tests/scanning.rs
casefile/casefile-store/tests/validation.rs
casefile/skills/casefile/SKILL.md
scripts/smoke-casefile-mcp.py
tests/test_smoke_casefile_mcp.py
```
