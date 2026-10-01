# Priority Criterion baseline -- HMD-057

This is an unchanged-production-logic baseline, not an optimization result. Source starts at
`3b173d93195bffc722565fb8c5f13488f8585aaa`. The original 42 scenarios and
[2026-09-29 result table](BASELINE.md) are preserved. Seventeen additional controls measure the
accepted priority interactions. The only production source edit makes `App::handle` crate-visible
for the private benchmark executable; its implementation is unchanged.

## Reproduce

From `casefile/`, use the repository Nix development shell and an absolute disposable target:

```sh
export CARGO_TARGET_DIR="$PWD/../.agent-workspace/<session>/target"
nix develop .. --command cargo bench \
  -p casefile-server --bench http \
  -p casefile-store-sqlite --bench baselines --bench priority \
  -p casefile-tui --bench interactions \
  -- --noplot --save-baseline hmd057-before
python3 benchmarks/capture-results.py \
  "$CARGO_TARGET_DIR/criterion" hmd057-before benchmarks/results/hmd057-before.json
```

Use `-- --list` for discovery and `-- --test` for fixture/behavior smoke checks. Criterion CLI
filters still apply. A later matched run can use `--baseline hmd057-before` while its Cargo target
is retained. The checked-in JSON retains complete estimates (including 95% confidence intervals) and
the ten raw sample iteration/time pairs for every scenario, independent of disposable caches.
`time / iters` is each sample's observed operation time; the displayed median is Criterion's
bootstrap median estimator, not the command's total wall time.

## Fixture and timing contract

All roots are disposable synthetic Stores copied from the minimum test fixture. No live planning
Store is opened, mutated, migrated, or measured. Numbers such as 250/1,000 are **additional**
accepted tickets, not total fixture records. The original fixture builders and timing boundaries are
unchanged; they now share `benchmarks/support/mod.rs` with the new benchmark executables.

| New group                                          | Fixture                                                                         | Timed boundary                                                                                                                                          | Excluded setup                                                                                                         |
| -------------------------------------------------- | ------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| `priority_scope_500_notes` (6)                     | 250/1,000 tickets; 500 valid notes targeting seed ticket HMD-011                | Uncached Provider existing/missing detail, selected-scope record index                                                                                  | Fixture creation, Store opening/checking, response-validity assertions                                                 |
| `priority_single_record_250_records_500_notes` (2) | 250 tickets; 500 valid notes; isolated Git repository                           | Single-record Provider preview or apply; apply alternates two different valid titles                                                                    | Repository creation, draft parsing; fresh Provider per iteration; apply preview outside the apply timer                |
| `http_loopback_records` (6)                        | 250/1,000 tickets; 500 valid notes; external disposable SQLite index            | Full loopback request: connect, request write, production Host authority/routing, Workbench refresh, SQLite query, JSON serialization, response read    | Server/index creation, initial request/cache warm-up, response JSON assertions                                         |
| `tui_250_records_500_notes` (3)                    | 250 tickets; 500 valid notes; 80-paragraph seed-ticket body; 120x40 TestBackend | Real App key transition plus Terminal draw: project->investigation->ticket navigation (3 frames); list or rendered-detail down/up scrolling (20 frames) | Fixture scan/derivation, App/Terminal creation, assertions that navigation, selection, and detail scroll really change |

Fixtures are checked as valid before new measurements. The intentionally invalid missing-target
progress fixture in the original suite still has its expected diagnostics. Detail setup verifies 500
projected notes on the existing record and absence of the requested missing identity. HTTP setup
verifies 200 responses and current result rows for unchanged, matching, and nonmatching searches.
Mutation setup applies a real title change, verifies the persisted parsed draft, and restores it;
each measured apply has a nonempty diff. TUI setup verifies real state transitions and uses
sufficient rendered content to scroll rather than measuring a clamped no-op.

The HTTP control sends one HTTP/1.0 request per connection, with `Connection: close`; this avoids
implementing a chunked-response parser. It measures loopback server/wire behavior, **not** browser
execution, keep-alive, concurrent queueing, or the server's blocking `serve` lifecycle. Actual
`api.rs`, `workbench.rs`, and `assets.rs` modules are compiled into the benchmark executable; no
production behavior is copied or approximated.

The TUI has a dedicated `src/benchmark.rs` executable root that compiles the actual production
modules. It calls the real key handler and `App::render` through Ratatui's TestBackend. Its private
`PAGE_SIZE` constant is a compile-only paging shim; PageUp/PageDown are not in any measured loop.
Future TUI changes must reconcile that shim if they add page-key controls. No new public runtime
API, library/crate, watcher seam, or production optimization is introduced. Benchmark-only unused
imports/dead code allowances cover production modules and their unit-test helpers compiled without a
test harness; they do not change ordinary library compilation.

## Run conditions and verification

Final measurement: 2026-09-30, same host as the retained original baseline: Linux 6.18.53 x86_64,
AMD Ryzen 7 5700X3D (8 cores / 16 threads), rustc 1.95.0 in the repository Nix shell, Criterion
0.8.2, optimized bench profile. Ten samples, one-second warm-up, two-second target measurement,
default Criterion bootstrap/resampling settings. Sampling can extend beyond the target for slow
operations. Filesystem caches are warm; CPU frequency is not controlled, CPU affinity is not pinned,
and normal host activity is not excluded. All final benchmarks run sequentially **after**
compilation and checks; no other writer build runs concurrently. An earlier exploratory
original-suite run overlapped TUI compilation and is deliberately excluded from the durable
acceptance results. Criterion's automatic `change` notices in the measurement stdout compare against
that preliminary same-code run, not against a production fix, and are not accepted before/after
evidence.

Actual target directory:
`/home/alex/dev/HUMANS.md/.agent-workspace/20260930-speedup-implementation/writer/target`. The root
retains it as active pipeline scratch for native `--baseline hmd057-before` comparisons and owns
final cleanup. No generated binaries, build caches, or HTML plots are committed.

Focused verification:

- Four benchmark executables compile; discovery enumerates all 59 scenarios (42 preserved + 17 new).
- All 59 Criterion `--test` scenarios pass their fixture/behavior checks.
- Existing TUI library tests: 60 passed; no benchmark is run by `cargo test --lib`.
- Ordinary TUI library build passes, without the benchmark entry point.
- `cargo fmt --all --check` passes.
- Focused `cargo clippy` for the four benchmark targets with `-D warnings` passes.
- Original baseline source/table hashes are unchanged; shared fixture methods retain their behavior.

[Focused checks, discovery, environment, and preservation hashes](results/hmd057-verification.txt)
are recorded alongside the results.

[Machine-readable estimates and samples](results/hmd057-before.json) and
[measurement stdout](results/hmd057-before.log) are the durable baseline artifacts.

## Results

Times are milliseconds per operation. TUI scroll operations each include **20 frames**, not one.

| Scenario                                               | Original median (ms) | Fresh median (ms) | Fresh 95% CI (ms) |
| ------------------------------------------------------ | -------------------: | ----------------: | ----------------: |
| `activation_roots/scoped_check/1`                      |               17.745 |            17.089 |     17.055-17.110 |
| `activation_roots/scoped_check/100`                    |               18.187 |            17.443 |     17.422-17.532 |
| `core_parsing/progress_parse_500_notes`                |                2.154 |             2.133 |       2.128-2.140 |
| `core_parsing/ticket_parse`                            |                0.033 |             0.031 |       0.031-0.032 |
| `core_parsing/ticket_render_roundtrip`                 |                0.035 |             0.036 |       0.036-0.036 |
| `http_loopback_records/search_hit/1000`                |                   -- |           142.485 |   142.057-142.902 |
| `http_loopback_records/search_hit/250`                 |                   -- |            42.502 |     42.426-42.596 |
| `http_loopback_records/search_miss/1000`               |                   -- |           143.155 |   142.786-143.432 |
| `http_loopback_records/search_miss/250`                |                   -- |            42.459 |     42.210-42.715 |
| `http_loopback_records/unchanged/1000`                 |                   -- |           146.836 |   146.684-147.698 |
| `http_loopback_records/unchanged/250`                  |                   -- |            44.636 |     44.298-44.944 |
| `inventory/full_scan/2000`                             |               43.191 |            42.177 |     42.133-42.429 |
| `inventory/full_scan/8000`                             |              177.062 |           170.714 |   169.638-175.162 |
| `inventory/metadata_summary/2000`                      |                7.856 |             8.057 |       8.043-8.082 |
| `inventory/metadata_summary/8000`                      |               32.844 |            31.673 |     31.616-31.733 |
| `inventory/scoped_check/2000`                          |               17.141 |            17.316 |     17.150-17.439 |
| `inventory/scoped_check/8000`                          |               67.316 |            66.614 |     66.456-66.917 |
| `mutation_preview_250_records/replace_batch/1`         |               26.222 |            25.420 |     25.255-25.931 |
| `mutation_preview_250_records/replace_batch/10`        |               45.015 |            44.957 |     44.700-45.214 |
| `presentation_250_records/cold_session`                |               35.020 |            34.698 |     34.580-35.114 |
| `presentation_250_records/unchanged_refresh`           |                1.414 |             1.423 |       1.417-1.427 |
| `priority_scope_500_notes/existing_detail/1000`        |                   -- |            13.651 |     13.621-14.371 |
| `priority_scope_500_notes/existing_detail/250`         |                   -- |             7.072 |       7.045-7.097 |
| `priority_scope_500_notes/missing_detail/1000`         |                   -- |            11.493 |     11.468-11.519 |
| `priority_scope_500_notes/missing_detail/250`          |                   -- |             4.869 |       4.846-4.889 |
| `priority_scope_500_notes/selected_record_index/1000`  |                   -- |            64.284 |     64.258-64.664 |
| `priority_scope_500_notes/selected_record_index/250`   |                   -- |            19.562 |     19.500-20.075 |
| `priority_single_record_250_records_500_notes/apply`   |                   -- |            30.519 |     30.230-30.844 |
| `priority_single_record_250_records_500_notes/preview` |                   -- |            31.064 |     30.163-31.416 |
| `progress/accepted_target/250`                         |               11.296 |            11.506 |     11.426-11.599 |
| `progress/accepted_target/500`                         |               13.679 |            13.535 |     13.511-13.548 |
| `progress/missing_targets/250`                         |               30.114 |            28.870 |     28.777-29.029 |
| `progress/missing_targets/500`                         |               50.304 |            48.301 |     48.023-48.699 |
| `records/boards/1000`                                  |               61.613 |            60.855 |     60.703-61.113 |
| `records/boards/250`                                   |               16.735 |            15.645 |     15.615-15.700 |
| `records/full_derived/1000`                            |              135.918 |           131.583 |   127.727-135.812 |
| `records/full_derived/250`                             |               34.384 |            35.257 |     33.935-35.654 |
| `records/record_index/1000`                            |               61.115 |            60.986 |     60.386-63.315 |
| `records/record_index/250`                             |               16.233 |            15.544 |     15.494-16.921 |
| `records/scoped_check/1000`                            |               96.405 |            96.738 |    96.308-102.974 |
| `records/scoped_check/250`                             |               24.677 |            24.589 |     24.477-24.623 |
| `scoped_progress_250_records/disposition_boards/0`     |               16.242 |            15.772 |     15.730-15.820 |
| `scoped_progress_250_records/disposition_boards/500`   |               21.642 |            19.780 |     19.764-19.806 |
| `scoped_progress_250_records/missing_detail/0`         |                2.791 |             2.937 |       2.914-2.971 |
| `scoped_progress_250_records/missing_detail/500`       |                4.982 |             4.951 |       4.936-4.986 |
| `scoped_progress_250_records/record_index/0`           |               15.873 |            16.441 |     15.862-16.676 |
| `scoped_progress_250_records/record_index/500`         |               20.918 |            20.006 |     19.971-20.094 |
| `sqlite_500_records/boards`                            |                0.340 |             0.331 |       0.330-0.333 |
| `sqlite_500_records/prepare_replacement`               |               66.659 |            65.679 |     65.015-66.378 |
| `sqlite_500_records/records_all`                       |                3.280 |             3.331 |       3.326-3.350 |
| `sqlite_500_records/records_search_miss`               |                3.323 |             3.340 |       3.326-3.346 |
| `sqlite_500_records/relationships_miss`                |                0.207 |             0.202 |       0.201-0.203 |
| `supersession/chain/250`                               |               33.274 |            33.107 |     32.904-34.201 |
| `supersession/chain/500`                               |               84.538 |            84.201 |     83.133-87.001 |
| `supersession/independent/250`                         |               24.932 |            24.898 |     24.787-25.268 |
| `supersession/independent/500`                         |               49.676 |            49.085 |     49.007-49.237 |
| `tui_250_records_500_notes/detail_scroll_20_frames`    |                   -- |            10.713 |     10.634-10.767 |
| `tui_250_records_500_notes/list_scroll_20_frames`      |                   -- |            11.086 |     11.025-11.133 |
| `tui_250_records_500_notes/selected_scope_navigation`  |                   -- |             1.099 |       1.088-1.119 |

## Evidence limits

No before/after speedup is claimed in this ticket. Comparing the original September 29 medians with
this rerun measures host/run variation as well as sampling; production algorithms have not changed.
Later tickets must use identical fixture sizes, query/mutation semantics, timed boundaries,
host/toolchain, and Criterion configuration, and repeat suspicious regressions.

Unmeasured: filesystem watcher bursts, progressive coordinator load/interleaving, real terminal
escape/output costs, browser interactions, page keys, cold disks, allocations/peak memory,
concurrent mutations, and rollback. The added apply control proves a successful synthetic write, not
rollback or cross-process race safety. These gaps must not be described as end-to-end or
resource-cost improvements. Existing boards/SQL controls remain useful but are not additional
priority interactions.
