# HMD-068: integrated verification and isolated Criterion comparison

Local-only candidate, parent `18b71c8bbd70db48b1f12832e41084c0e918a237`. No push, release,
installation, restart, live planning Store migration or hosted/platform acceptance. Root owns
canonical records, ledger disposition and independent acceptance.

## Assigned outcomes

- **112:** SQLite staging's existing transaction now includes `user_version`, five schema tables,
  metadata, prepared inserts and indexes. `journal_mode=DELETE` remains outside; schema 1, default
  durability, atomic publication and ordinary errors remain. No new framework/API/dependency.
- **113:** `SqliteIndex::open` makes one supported read-only recognized metadata/version
  observation. A private atomic marks an old cache for the existing Provider atomic rebuild.
  Successful persist clears it; stale/failed publication preserves old bytes and pending upgrade.
  Hot reads retain source-revision validation but do not repeat schema SQL. Constructor SQL/I/O
  errors propagate; arbitrary malformed artifacts are not classified as repairable upgrades. The
  existing legacy integration test covers actual Provider first upgrade, subsequent Current/stable
  publication, stale publication and failed preparation/revision observation.
- **114:** All four real Criterion implementations construct only needed case-local inputs inside
  the supported callback before `iter`/`iter_batched`. Filtered cases skip unrelated fixture setup.
  Providers, sessions, HTTP servers and TUI Apps are independent. SQLite reads drop unused snapshot
  and Store before timing; preparation alone retains its required snapshot. Core ticket cases omit
  unconsumed progress. Batch1 parses one draft, batch10 ten; single preview retains one changed
  request. Timed operations/output drops, mutation batch semantics and all59 IDs/counts are
  retained. Real old/current build, discovery and smoke validate these paths, not private allocation
  counts.
- Two stale Python Provider4 mocks now match source Provider5 (mock package version unchanged).
  Source Casefile skill formatting changes whitespace only. These prior necessary coupled fixes
  remain visible, not silently included as performance changes.

## Genuinely matched isolated baseline

Historical runtime is immutable `8e97fc4e368361d75b0323913ed4e56e745fc259`; current runtime is
accepted057-067 plus112/113 and the isolated harness. The same case-local workload definitions run
on both. Baselines/TUI files are identical. Explicit old API adaptations use owned ProviderPreview
and its canonical diagnostics rather than current preview ID/review envelope. HTTP uses each
runtime's actual production transport: original tiny_http Host/Workbench wrapper versus current
Hyper/Tokio, same isolated callback/request/socket operation.
[Exact adapter patch](results/hmd068-isolated-api-adapters.txt),
[isolation patch](results/hmd068-isolated-benchmark-patch.txt).

All59 IDs discovered on each runtime and all16 actual list/smoke commands pass. All builds ended
before the finite run: old four executables once save **new** `hmd068-isolated-before`, then current
four once compare natively against it. Order: baselines, priority, HTTP, interactions. NEW owned
Criterion/TMPDIR, never the original `hmd057-before` directories. Pinned Nix rustc/cargo1.95.0;
Criterion0.8.2,10samples,1s warmup,2s target,95% confidence,1% noise. Criterion extends sampling
when needed. Warm local Linux filesystem; shared NEW owned TMPDIR on the home filesystem (historical
original shared-context runs used their separately recorded locations). This does not establish a
filesystem cause for differences between historical absolute means. No CPU pinning/frequency
control. Exact tool/command paths, literal exits and durations:
[environment](results/hmd068-isolated-environment.log),
[timing commands](results/hmd068-isolated-timing-commands.json). Eight timing commands exit0.

This is cumulative057-068 performance, not isolated causal attribution to individual findings. The
new baseline is genuinely sampled, not reconstructed from historical estimates. Necessary
runtime/API/transport/dependency changes remain actual comparison differences.

### All59 arithmetic means

Means are arithmetic means of actual per-sample time/iteration in milliseconds. Change and95% CI use
the matched native Criterion mean-change estimate. Complete raw samples/estimates/benchmark
metadata/change CIs: [comparison JSON](results/hmd068-isolated-comparison.json); all eight logs are
adjacent `hmd068-isolated-{historical,current}-{baselines,priority,http,interactions}.log`.

| Control                                                | Old mean ms | Current mean ms | Change % |    95% change CI % |
| ------------------------------------------------------ | ----------: | --------------: | -------: | -----------------: |
| `activation_roots/scoped_check/1`                      |   20.519510 |        1.106012 |  -94.610 | [-94.695, -94.507] |
| `activation_roots/scoped_check/100`                    |   21.451944 |        3.437607 |  -83.975 | [-84.248, -83.704] |
| `core_parsing/progress_parse_500_notes`                |    2.167013 |        1.087576 |  -49.812 | [-50.503, -48.822] |
| `core_parsing/ticket_parse`                            |    0.031439 |        0.026426 |  -15.944 | [-17.999, -13.912] |
| `core_parsing/ticket_render_roundtrip`                 |    0.037918 |        0.031679 |  -16.454 | [-18.910, -14.013] |
| `http_loopback_records/search_hit/1000`                |  150.252363 |       25.440152 |  -83.068 | [-83.495, -82.692] |
| `http_loopback_records/search_hit/250`                 |   44.912220 |        7.637017 |  -82.996 | [-83.319, -82.658] |
| `http_loopback_records/search_miss/1000`               |  154.917412 |       25.467792 |  -83.560 | [-84.008, -83.187] |
| `http_loopback_records/search_miss/250`                |   45.004109 |        7.638409 |  -83.027 | [-83.460, -82.650] |
| `http_loopback_records/unchanged/1000`                 |  153.492230 |       66.078356 |  -56.950 | [-57.953, -55.599] |
| `http_loopback_records/unchanged/250`                  |   47.916599 |       18.759146 |  -60.850 | [-62.810, -59.317] |
| `inventory/full_scan/2000`                             |   48.711070 |       47.003101 |   -3.506 |   [-4.794, -1.970] |
| `inventory/full_scan/8000`                             |  208.009246 |      191.365862 |   -8.001 |   [-9.566, -6.322] |
| `inventory/metadata_summary/2000`                      |    9.896931 |        8.871678 |  -10.359 |  [-12.380, -8.555] |
| `inventory/metadata_summary/8000`                      |   39.400413 |       37.582125 |   -4.615 |   [-7.044, -2.126] |
| `inventory/scoped_check/2000`                          |   20.957429 |        1.050360 |  -94.988 | [-95.087, -94.893] |
| `inventory/scoped_check/8000`                          |   80.583294 |        1.116541 |  -98.614 | [-98.667, -98.555] |
| `mutation_preview_250_records/replace_batch/1`         |   27.063634 |       25.584902 |   -5.464 |   [-7.563, -3.240] |
| `mutation_preview_250_records/replace_batch/10`        |   46.480824 |       42.056474 |   -9.519 |  [-10.909, -8.249] |
| `presentation_250_records/cold_session`                |   36.972880 |       18.553269 |  -49.819 | [-50.777, -48.701] |
| `presentation_250_records/unchanged_refresh`           |    1.613608 |        1.590169 |   -1.453 |   [-3.237, +0.363] |
| `priority_scope_500_notes/existing_detail/1000`        |   15.353552 |        1.501572 |  -90.220 | [-90.378, -90.065] |
| `priority_scope_500_notes/existing_detail/250`         |    7.823312 |        1.475518 |  -81.139 | [-81.483, -80.773] |
| `priority_scope_500_notes/missing_detail/1000`         |   12.983024 |        0.211995 |  -98.367 | [-98.396, -98.337] |
| `priority_scope_500_notes/missing_detail/250`          |    5.474934 |        0.213057 |  -96.108 | [-96.144, -96.063] |
| `priority_scope_500_notes/selected_record_index/1000`  |   66.428143 |       39.586359 |  -40.407 | [-41.261, -39.248] |
| `priority_scope_500_notes/selected_record_index/250`   |   21.481735 |       11.233094 |  -47.709 | [-49.217, -46.015] |
| `priority_single_record_250_records_500_notes/apply`   |   31.137378 |       27.778715 |  -10.787 |  [-12.777, -8.333] |
| `priority_single_record_250_records_500_notes/preview` |   30.643070 |       27.120741 |  -11.495 | [-13.079, -10.066] |
| `progress/accepted_target/250`                         |   13.188668 |        1.780499 |  -86.500 | [-86.779, -86.179] |
| `progress/accepted_target/500`                         |   16.270839 |        2.345124 |  -85.587 | [-86.127, -85.051] |
| `progress/missing_targets/250`                         |   33.732084 |        1.861883 |  -94.480 | [-94.794, -94.210] |
| `progress/missing_targets/500`                         |   50.385484 |        2.578951 |  -94.882 | [-94.949, -94.820] |
| `records/boards/1000`                                  |   63.876122 |       38.116043 |  -40.328 | [-41.634, -38.861] |
| `records/boards/250`                                   |   16.825870 |       10.235208 |  -39.170 | [-40.581, -37.394] |
| `records/full_derived/1000`                            |  138.123006 |       79.711507 |  -42.289 | [-43.433, -41.155] |
| `records/full_derived/250`                             |   34.535589 |       19.909418 |  -42.351 | [-43.041, -41.678] |
| `records/record_index/1000`                            |   66.340646 |       37.794208 |  -43.030 | [-44.488, -41.534] |
| `records/record_index/250`                             |   16.855544 |        9.909577 |  -41.209 | [-42.548, -39.745] |
| `records/scoped_check/1000`                            |  101.675018 |       74.620092 |  -26.609 | [-27.412, -25.623] |
| `records/scoped_check/250`                             |   26.004383 |       19.802678 |  -23.849 | [-24.890, -22.758] |
| `scoped_progress_250_records/disposition_boards/0`     |   17.745489 |       10.129558 |  -42.918 | [-43.823, -41.964] |
| `scoped_progress_250_records/disposition_boards/500`   |   21.847659 |        9.698029 |  -55.611 | [-56.275, -54.778] |
| `scoped_progress_250_records/missing_detail/0`         |    3.219374 |        0.215327 |  -93.312 | [-93.418, -93.192] |
| `scoped_progress_250_records/missing_detail/500`       |    5.420823 |        0.215544 |  -96.024 | [-96.078, -95.950] |
| `scoped_progress_250_records/record_index/0`           |   17.073197 |       10.414174 |  -39.003 | [-40.395, -37.535] |
| `scoped_progress_250_records/record_index/500`         |   21.362557 |       11.789602 |  -44.812 | [-46.822, -42.505] |
| `sqlite_500_records/boards`                            |    0.366020 |        0.379570 |   +3.702 |   [-0.076, +8.596] |
| `sqlite_500_records/prepare_replacement`               |  388.276416 |       65.790913 |  -83.056 | [-84.813, -81.259] |
| `sqlite_500_records/records_all`                       |    3.544371 |        2.088091 |  -41.087 | [-45.002, -37.340] |
| `sqlite_500_records/records_search_miss`               |    3.538545 |        0.863728 |  -75.591 | [-76.109, -75.014] |
| `sqlite_500_records/relationships_miss`                |    0.225412 |        0.143925 |  -36.150 | [-38.658, -33.293] |
| `supersession/chain/250`                               |   34.714762 |       20.206927 |  -41.792 | [-42.938, -40.751] |
| `supersession/chain/500`                               |   87.746728 |       39.104859 |  -55.434 | [-56.098, -54.841] |
| `supersession/independent/250`                         |   26.890430 |       20.395386 |  -24.154 | [-26.585, -21.780] |
| `supersession/independent/500`                         |   54.244210 |       38.149040 |  -29.672 | [-35.571, -25.555] |
| `tui_250_records_500_notes/detail_scroll_20_frames`    |   10.863433 |        6.116800 |  -43.694 | [-45.238, -42.231] |
| `tui_250_records_500_notes/list_scroll_20_frames`      |   11.086930 |        6.680915 |  -39.741 | [-41.178, -38.601] |
| `tui_250_records_500_notes/selected_scope_navigation`  |    1.085518 |        0.899233 |  -17.161 | [-19.561, -15.434] |

57 controls have a change CI wholly below -1%. Two do not establish a direction: unchanged
presentation -1.453% CI[-3.237,+0.363], and direct SQLite boards **+3.702% CI[-0.076,+8.596]**.
Retain the positive board estimate; CI crossing zero is not proof of zero regression. No favorable
repeat/variant/profiler or hard numerical gate was used. Preparation388.276->65.791ms improves
83.056% here; do not substitute these absolute values into original shared-context reports.

## Historical adverse evidence and diagnosis

[Superseded shared-context report](results/hmd068-isolated-superseded-shared-context-report.md)
retains all original59 rows, failed attempts and three integrated distributions. The initial
390.668ms preparation/+492.59% adverse outcome preceded112; later14.814ms and14.113ms outcomes
remain historical, not the new isolated comparison. Direct boards' historical+20.940% CI[18.535,
23.174] is not erased or explained as noise.

The authorized45-case scratch diagnosis found a concrete retained-setup/lifetime contributor: same
executable/input, dropping only unused snapshot changed boards395.612->345.723us and
serde231.384->203.186us. Historical/current matched local setup: retained360.338->380.578us,
dropped352.460->356.032us (overlapping dropped CIs). Schema/index controls did not explain the full
historical difference. This supports proper case isolation, not a claim about a particular malloc
bin, exact attribution of all20.940%, pooling, or snapshot representation alone. Root's durable
planning diagnostic report/raw archive retain every finite outcome and failed attempt; scratch
`writer/hmd068/boards-diagnostic` remains available for review.

## Verification and limits

Final owned-source-copy checks pass: **285 Rust workspace tests, 113 Python tests, 37 browser
tests**, workspace all-target Clippy `-D warnings`, Rust/source formatting, ASCII, frozen Bun
install, typecheck/production build, package-root/Casefile/coding validators, and supported
humans-md/coding build/check (70 local files, not six native packages). A freshly built release CLI
and the actual source adapter probe pass:0.8.0/Provider5, dated2025-11-25 initialization, two
responses and12 distinct tools. JS/CSS before/after hashes are identical.
[Literal commands](results/hmd068-isolated-final-check-commands.json),
[complete final verification](results/hmd068-isolated-final-verification.txt),
[adapter proof](results/hmd068-isolated-adapter-smoke.json),
[source/binary/config provenance](results/hmd068-isolated-provenance.json).

The initial copy driver used a wrong asset path and failed before checks. Its corrected copy then
lost executable file modes: formatter returned126; all other17 checks passed. Source modes were
copied accurately and formatter plus four package build/check commands passed. Both failed attempts
are retained, not rewritten as success. The linked command receipt also records actual successful
set-e aggregate reruns (no invented individual durations), with complete rerun logs in verification.
No production change resulted. Both sets of four timed executables still match their captured hashes
after final checks; all236 original before JSON objects and four retained original executables also
verify unchanged. Shared writer target stays under root cleanup ownership; diagnostic/isolated
targets and plain proof remain pending review capture, with no deletion of unknown primary ignored
outputs.

Human explicitly authorized meaning-preserving archive normalization. Existing formatter/ASCII/
validator rules and exclusions are unchanged. Typography/unit notation, raw terminal codepoint
escapes, JSON escapes (decoded equality), Python Unicode escapes (AST equality), and Prettier
preserve measured numbers and meaning. New normalized outer-file hashes are not old byte-identity
claims. [Normalization mapping](results/hmd068-isolated-normalization.json) records before/after
ASCII hashes; subsequent Prettier whitespace is separate. Original236 native-before JSON objects
remain archived with original decoded UTF8/hash, not a rehydrated timing baseline.

Earlier literal failures stay in historical logs: stale Protocol4 mocks, skill/evidence formatter/
ASCII failures, unpinned rustup/tool failures and derivative failures, transient generated-JS drift,
and final-check driver wrong asset path/copy-mode failures. Corrected actual asset paths and source
modes are verified. Python helper initially wrote unknown primary ignored build/
marketplace/defaulttarget outputs without a prior byte snapshot. We do not claim preservation or
restore/delete those unknown outputs. Corrected final checks use an owned source copy and only owned
target/default-target symlink; installed0.8.0Provider3 remains untouched.

Structural resolution is not automatically measured speedup. Original broad Rayon lead has
bounded059 two-worker/collection evidence, not proof every parallel alternative is optimal; other
batches remove repeated work, push down predicates or reuse facts while preserving ordered
mutation/progress/UI state. No new threading framework or universal parallelism claim.

Fixture labels count added tickets: SQLite500 has501accepted-ticket cards and an excluded epic. The
unchanged TUI detail selects epic HMD-E-001, not long-ticket HMD-011 with80paragraphs/500notes. No
measured long-progress-detail, first-layout/retained-cell heap, cold-cache, first-pool-init,
resource-contention/CPU/peak-memory, board-TUI/watch, browser end-to-end or Windows/macOS claim.
Package checks are local humans-md/coding/source contracts, not real six-native-artifact or hosted
release/install acceptance. Required independent mutation freshness stays authoritative.

Root alone reconciles114individual ledger outcomes and original human leads after independent
exact-commit review. This candidate does not self-close ledger rows or the task.
