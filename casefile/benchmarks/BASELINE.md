# Criterion baseline -- 2026-09-29

This is the first source-controlled performance baseline for the Rust workspace. It measures
existing behavior at production source commit `7e910231b077fd529a2e8adcad2d329803562ffc`; the only
source changes in this run add the benchmark suite and its dependency. It is **not** an optimization
result or a performance budget.

## Run and workload contract

From `casefile/`, with the repository's Nix development shell available:

```sh
nix develop .. --command cargo bench -p casefile-store-sqlite --bench baselines -- --noplot
```

To keep generated binaries/results disposable, set an **absolute** `CARGO_TARGET_DIR` outside
tracked source before invoking the command. `cargo bench ... -- --list` enumerates scenarios;
`--save-baseline <name>` and `--baseline <name>` support same-host Criterion comparisons when that
local target directory is retained. The checked-in table below is the durable reference when build
caches are deleted.

The suite generates isolated temporary planning Stores from `casefile-store`'s minimum fixture.
Fixtures are prepared outside timed iterations, and expected-valid fixtures are checked before
measurement. The missing-target progress case is intentionally invalid. `inventory` has 2k/8k extra
opaque files; `records` has 250/1k extra valid accepted tickets; `supersession` compares independent
tickets with an acyclic chain; `progress` has 250/500 notes plus 1k opaque files; `scoped_progress`
has 250 extra tickets with 0/500 notes; SQLite has 500 extra tickets. Each fixture also contains the
seed records and governed files. Mutation preview uses a temporary Git repository. Presentation
measures a fresh session versus an unchanged session refresh. Nothing reads or writes the live
planning Store.

Host: Linux 6.18.53 x86_64, AMD Ryzen 7 5700X3D (8 cores/16 threads), rustc 1.95.0, Criterion 0.8.2,
optimized `bench` profile. Warm local filesystem caches; no CPU pinning or controlled frequency.
Criterion configuration: 10 samples, 1 s warm-up, 2 s target measurement (Criterion extended some
sampling periods). Values are median time per operation in **milliseconds**, with Criterion's 95%
bootstrap confidence interval for that median (rounding can collapse a very small interval in the
table). The main suite and the additional `scoped_progress` group were measured in consecutive runs
on the same host. Compare within matched fixtures and repeat on the target machine before committing
to a speedup claim.

## Results

| Scenario                                             | Median (ms) |     95% CI (ms) |
| ---------------------------------------------------- | ----------: | --------------: |
| `activation_roots/scoped_check/1`                    |      17.745 |   17.676-17.941 |
| `activation_roots/scoped_check/100`                  |      18.187 |   18.079-18.345 |
| `core_parsing/progress_parse_500_notes`              |       2.154 |     2.142-2.165 |
| `core_parsing/ticket_parse`                          |       0.033 |     0.031-0.033 |
| `core_parsing/ticket_render_roundtrip`               |       0.035 |     0.035-0.035 |
| `inventory/full_scan/2000`                           |      43.191 |   42.845-43.763 |
| `inventory/full_scan/8000`                           |     177.062 | 171.661-179.893 |
| `inventory/metadata_summary/2000`                    |       7.856 |     7.848-7.884 |
| `inventory/metadata_summary/8000`                    |      32.844 |   32.780-32.935 |
| `inventory/scoped_check/2000`                        |      17.141 |   17.103-17.227 |
| `inventory/scoped_check/8000`                        |      67.316 |   66.443-69.046 |
| `mutation_preview_250_records/replace_batch/1`       |      26.222 |   25.942-27.719 |
| `mutation_preview_250_records/replace_batch/10`      |      45.015 |   44.390-45.233 |
| `presentation_250_records/cold_session`              |      35.020 |   34.798-35.376 |
| `presentation_250_records/unchanged_refresh`         |       1.414 |     1.408-1.418 |
| `progress/accepted_target/250`                       |      11.296 |   11.249-11.342 |
| `progress/accepted_target/500`                       |      13.679 |   13.621-13.761 |
| `progress/missing_targets/250`                       |      30.114 |   29.753-30.638 |
| `progress/missing_targets/500`                       |      50.304 |   50.022-51.332 |
| `records/boards/1000`                                |      61.613 |   61.196-62.087 |
| `records/boards/250`                                 |      16.735 |   15.810-16.837 |
| `records/full_derived/1000`                          |     135.918 | 130.272-139.426 |
| `records/full_derived/250`                           |      34.384 |   34.326-34.554 |
| `records/record_index/1000`                          |      61.115 |   60.720-62.257 |
| `records/record_index/250`                           |      16.233 |   16.055-16.557 |
| `records/scoped_check/1000`                          |      96.405 |   96.259-96.728 |
| `records/scoped_check/250`                           |      24.677 |   24.574-25.635 |
| `scoped_progress_250_records/disposition_boards/0`   |      16.242 |   16.160-16.661 |
| `scoped_progress_250_records/disposition_boards/500` |      21.642 |   20.931-22.206 |
| `scoped_progress_250_records/missing_detail/0`       |       2.791 |     2.784-2.841 |
| `scoped_progress_250_records/missing_detail/500`     |       4.982 |     4.972-5.006 |
| `scoped_progress_250_records/record_index/0`         |      15.873 |   15.528-16.413 |
| `scoped_progress_250_records/record_index/500`       |      20.918 |   20.674-21.308 |
| `sqlite_500_records/boards`                          |       0.340 |     0.337-0.346 |
| `sqlite_500_records/prepare_replacement`             |      66.659 |   65.654-68.402 |
| `sqlite_500_records/records_all`                     |       3.280 |     3.265-3.288 |
| `sqlite_500_records/records_search_miss`             |       3.323 |     3.316-3.333 |
| `sqlite_500_records/relationships_miss`              |       0.207 |     0.200-0.224 |
| `supersession/chain/250`                             |      33.274 |   33.157-33.491 |
| `supersession/chain/500`                             |      84.538 |   83.670-85.855 |
| `supersession/independent/250`                       |      24.932 |   24.850-25.256 |
| `supersession/independent/500`                       |      49.676 |   49.541-50.062 |

## Baseline interpretation and gaps

- On 8k opaque files, metadata summary is ~32.8 ms, scoped check ~67.3 ms, and full body scan ~177
  ms. These operations have different contracts; their difference is not all waste.
- The **incremental supersession cost** (chain minus independent) grows from ~8.3 ms at 250 tickets
  to ~34.9 ms at 500, about 4.2x for 2x length. This is consistent with the previously identified
  quadratic validator; the benchmark does not isolate that function alone.
- With 500 notes, even disposition-only boards rise from ~16.6 to ~21.5 ms; a missing detail lookup
  rises from ~2.8 to ~5.0 ms. These quantify full-log work on queries that cannot use the resulting
  note projection.
- SQLite `records_search_miss` (~3.32 ms) is no cheaper than `records_all` (~3.28 ms) on this
  500-ticket fixture, consistent with decode-before-filter. A single SQL relationship miss is much
  cheaper, but server HTTP refresh cost is excluded from these SQLite-only numbers.
- The suite covers public core, Store, Provider, presentation, writer-preview, and SQLite paths. It
  does **not** benchmark TUI frame rendering, filesystem watcher bursts, HTTP queueing/wire
  payloads, apply/rollback side effects, peak allocations, cold disks, or concurrent mutations.
  Those need separate scenario-specific instrumentation before making quantitative claims.

The audit's source-derived findings, caveats, and earlier loopback measurements are in the separate
Casefile investigation. This baseline should be rerun after any implementation change using
identical fixture shape and host conditions; lower times alone do not prove behavioral correctness.

## HMD-068 case-isolation amendment

The original table above remains the historical shared-context baseline. The current harness
constructs each fixture inside its Criterion callback, before the timed loop, so filtering a case
skips unrelated setup. Providers, sessions, servers and TUI Apps are independent per case; SQLite
cached reads do not retain the unused derived snapshot. The preparation case retains its required
snapshot. Timed operations, workload counts and output-drop semantics remain unchanged. SQLite's 500
label means 500 added tickets plus the seeded accepted ticket (501 cards); the excluded epic is not
a ticket card. The TUI warm-detail control selects epic HMD-E-001, not the long-ticket/progress
fixture. A separately sampled isolated 8e97/runtime-current comparison is reported in
[HMD-068 results](HMD-068-RESULTS.md), not substituted into this original table.
