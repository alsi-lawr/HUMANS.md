mod support;

use casefile_core::{ChangeRequest, Kind, parse_draft, parse_progress_log, render_draft};
use casefile_store::{
    DerivedIndex, Indexed, InvestigationScope, InvestigationScopedIdentity, PresentationEvent,
    PresentationLoadRequest, PresentationSession, PresentationTarget, Provider, ProviderQuery,
    RecordScope, ScopedIdentity, Store,
};
use casefile_store_sqlite::SqliteIndex;
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use std::{fs, hint::black_box, time::Duration};
use support::{Fixture, INVESTIGATION};
use tempfile::TempDir;

fn scope() -> InvestigationScope {
    InvestigationScope {
        project: "demo".into(),
        investigation: "sample".into(),
    }
}

fn assert_valid_fixture(store: &Store) {
    let result = store.check(Some(INVESTIGATION)).expect("fixture check");
    assert_eq!(
        result.valid,
        Some(true),
        "fixture diagnostics: {:?}",
        result.diagnostics
    );
}

fn bench_inventory(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("inventory");
    for count in [2_000, 8_000] {
        group.throughput(Throughput::Elements(count as u64));
        for operation in ["metadata_summary", "scoped_check", "full_scan"] {
            group.bench_with_input(
                BenchmarkId::new(operation, count),
                &count,
                |bench, &count| {
                    let fixture = Fixture::new().opaque_files(count);
                    let store = Store::open(fixture.root.path()).expect("store");
                    assert_valid_fixture(&store);
                    match operation {
                        "metadata_summary" => {
                            bench.iter(|| black_box(store.scan_summary().expect("summary")))
                        }
                        "scoped_check" => bench
                            .iter(|| black_box(store.check(Some(INVESTIGATION)).expect("check"))),
                        "full_scan" => bench.iter(|| black_box(store.scan().expect("scan"))),
                        _ => unreachable!(),
                    }
                },
            );
        }
    }
    group.finish();
    let mut group = criterion.benchmark_group("activation_roots");
    for roots in [1, 100] {
        group.throughput(Throughput::Elements(2_000));
        group.bench_with_input(
            BenchmarkId::new("scoped_check", roots),
            &roots,
            |bench, &roots| {
                let fixture = Fixture::new().opaque_files(2_000).activation_roots(roots);
                let store = Store::open(fixture.root.path()).expect("store");
                assert_valid_fixture(&store);
                bench.iter(|| black_box(store.check(Some(INVESTIGATION)).expect("check")));
            },
        );
    }
    group.finish();
}

fn bench_records(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("records");
    for count in [250, 1_000] {
        group.throughput(Throughput::Elements(count as u64));
        for operation in ["scoped_check", "full_derived", "record_index", "boards"] {
            group.bench_with_input(
                BenchmarkId::new(operation, count),
                &count,
                |bench, &count| {
                    let fixture = Fixture::new().tickets(count, false);
                    let store = Store::open(fixture.root.path()).expect("store");
                    assert_valid_fixture(&store);
                    match operation {
                        "scoped_check" => bench
                            .iter(|| black_box(store.check(Some(INVESTIGATION)).expect("check"))),
                        "full_derived" => {
                            bench.iter(|| black_box(store.derived_snapshot().expect("derived")))
                        }
                        "record_index" | "boards" => {
                            let query = if operation == "record_index" {
                                ProviderQuery::RecordIndex { scope: scope() }
                            } else {
                                ProviderQuery::Boards { scope: scope() }
                            };
                            let provider = Provider::without_cache(store);
                            bench.iter(|| {
                                black_box(provider.query(query.clone()).expect("scoped query"))
                            });
                        }
                        _ => unreachable!(),
                    }
                },
            );
        }
    }
    group.finish();
    let mut group = criterion.benchmark_group("supersession");
    for count in [250, 500] {
        for chain in [false, true] {
            let label = if chain { "chain" } else { "independent" };
            group.throughput(Throughput::Elements(count as u64));
            group.bench_with_input(
                BenchmarkId::new(label, count),
                &(count, chain),
                |bench, &(count, chain)| {
                    let fixture = Fixture::new().tickets(count, chain);
                    let store = Store::open(fixture.root.path()).expect("store");
                    assert_valid_fixture(&store);
                    bench.iter(|| black_box(store.check(Some(INVESTIGATION)).expect("check")));
                },
            );
        }
    }
    group.finish();
}

fn bench_progress(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("progress");
    for count in [250, 500] {
        for missing in [false, true] {
            let label = if missing {
                "missing_targets"
            } else {
                "accepted_target"
            };
            group.throughput(Throughput::Elements(count as u64));
            group.bench_with_input(
                BenchmarkId::new(label, count),
                &(count, missing),
                |bench, &(count, missing)| {
                    let fixture = Fixture::new()
                        .opaque_files(1_000)
                        .progress_notes(count, missing);
                    let store = Store::open(fixture.root.path()).expect("store");
                    assert_eq!(
                        store
                            .check(Some(INVESTIGATION))
                            .expect("fixture check")
                            .valid,
                        Some(!missing)
                    );
                    bench.iter(|| black_box(store.check(Some(INVESTIGATION)).expect("check")));
                },
            );
        }
    }
    group.finish();
}

fn bench_scoped_progress(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("scoped_progress_250_records");
    for notes in [0, 500] {
        for operation in ["record_index", "disposition_boards", "missing_detail"] {
            group.bench_with_input(
                BenchmarkId::new(operation, notes),
                &notes,
                |bench, &notes| {
                    let fixture = Fixture::new()
                        .tickets(250, false)
                        .progress_notes(notes, false);
                    let store = Store::open(fixture.root.path()).expect("store");
                    assert_valid_fixture(&store);
                    let query = match operation {
                        "record_index" => ProviderQuery::RecordIndex { scope: scope() },
                        "disposition_boards" => ProviderQuery::Boards { scope: scope() },
                        "missing_detail" => ProviderQuery::RecordDetail {
                            identity: InvestigationScopedIdentity {
                                scope: scope(),
                                identity: "HMD-999999".into(),
                            },
                        },
                        _ => unreachable!(),
                    };
                    let provider = Provider::without_cache(store);
                    bench.iter(|| black_box(provider.query(query.clone()).expect("scoped query")));
                },
            );
        }
    }
    group.finish();
}

fn bench_core(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("core_parsing");
    group.bench_function("ticket_parse", |bench| {
        let ticket_path = format!("{INVESTIGATION}/tickets/accepted/HMD-011.md");
        let ticket_text = {
            let fixture = Fixture::new();
            fs::read_to_string(fixture.root.path().join(&ticket_path)).expect("ticket")
        };
        bench.iter(|| {
            black_box(
                parse_draft(&ticket_path, Kind::Ticket, black_box(&ticket_text)).expect("parse"),
            )
        });
    });
    group.bench_function("ticket_render_roundtrip", |bench| {
        let ticket_path = format!("{INVESTIGATION}/tickets/accepted/HMD-011.md");
        let ticket_draft = {
            let fixture = Fixture::new();
            let text = fs::read_to_string(fixture.root.path().join(&ticket_path)).expect("ticket");
            parse_draft(&ticket_path, Kind::Ticket, &text).expect("ticket draft")
        };
        bench.iter(|| {
            black_box(render_draft(&ticket_path, black_box(&ticket_draft)).expect("render"))
        });
    });
    group.throughput(Throughput::Elements(500));
    group.bench_function("progress_parse_500_notes", |bench| {
        let progress_path = format!("{INVESTIGATION}/progress/log.toml");
        let progress_text = {
            let fixture = Fixture::new().progress_notes(500, false);
            fs::read_to_string(fixture.root.path().join(&progress_path)).expect("progress")
        };
        bench.iter(|| {
            black_box(parse_progress_log(&progress_path, black_box(&progress_text)).expect("parse"))
        });
    });
    group.finish();
}

fn drain_presentation(session: &PresentationSession, generation: u64) {
    let stream = session
        .load(PresentationLoadRequest {
            generation,
            target: PresentationTarget::Investigation {
                project: "demo".into(),
                path: INVESTIGATION.into(),
            },
        })
        .expect("presentation load");
    loop {
        match stream.recv().expect("presentation event") {
            PresentationEvent::Complete { .. } => break,
            PresentationEvent::Failure { message, .. } => panic!("presentation failure: {message}"),
            event => {
                black_box(event);
            }
        }
    }
}

fn bench_presentation(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("presentation_250_records");
    group.throughput(Throughput::Elements(250));
    group.bench_function("cold_session", |bench| {
        let fixture = Fixture::new().tickets(250, false);
        let store = Store::open(fixture.root.path()).expect("store");
        assert_valid_fixture(&store);
        bench.iter(|| {
            let session = store.presentation_session();
            drain_presentation(&session, 1);
        });
    });
    group.bench_function("unchanged_refresh", |bench| {
        let fixture = Fixture::new().tickets(250, false);
        let store = Store::open(fixture.root.path()).expect("store");
        assert_valid_fixture(&store);
        let session = store.presentation_session();
        drain_presentation(&session, 0);
        let mut generation = 1;
        bench.iter(|| {
            drain_presentation(&session, generation);
            generation += 1;
        });
    });
    group.finish();
}

fn bench_mutation(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("mutation_preview_250_records");
    for count in [1, 10] {
        group.throughput(Throughput::Elements(count));
        group.bench_with_input(
            BenchmarkId::new("replace_batch", count),
            &count,
            |bench, &count| {
                let fixture = Fixture::new().tickets(250, false).git_repository();
                let store = Store::open(fixture.root.path()).expect("store");
                assert_valid_fixture(&store);
                let batch = (0..count)
                    .map(|number| {
                        let path = format!(
                            "{INVESTIGATION}/tickets/accepted/HMD-{:06}.md",
                            number + 100_000
                        );
                        let text =
                            fs::read_to_string(fixture.root.path().join(&path)).expect("ticket");
                        let draft = parse_draft(&path, Kind::Ticket, &text).expect("draft");
                        ChangeRequest::Replace { path, draft }
                    })
                    .collect::<Vec<_>>();
                assert!(
                    store
                        .preview_batch(batch[..1].to_vec())
                        .expect("preview fixture")
                        .diagnostics
                        .is_empty()
                );
                bench.iter(|| {
                    black_box(
                        store
                            .preview_batch(black_box(batch.clone()))
                            .expect("preview"),
                    )
                });
            },
        );
    }
    group.finish();
}

fn bench_sqlite(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("sqlite_500_records");
    for operation in [
        "prepare_replacement",
        "records_all",
        "records_search_miss",
        "relationships_miss",
        "boards",
    ] {
        group.bench_function(operation, |bench| {
            let fixture = Fixture::new().tickets(500, false);
            let store = Store::open(fixture.root.path()).expect("store");
            assert_valid_fixture(&store);
            let snapshot = store.derived_snapshot().expect("derived snapshot");
            let index_directory = TempDir::new().expect("index directory");
            let index = SqliteIndex::open(
                index_directory.path().join("casefile.sqlite"),
                fixture.root.path(),
            )
            .expect("external index");
            let prepared = index.prepare(&snapshot).expect("initial prepare");
            assert!(matches!(
                index.publish(prepared, &store).expect("publish"),
                Indexed::Current { .. }
            ));
            drop(store);
            if operation == "prepare_replacement" {
                bench.iter(|| black_box(index.prepare(black_box(&snapshot)).expect("prepare")));
            } else {
                let revision = snapshot.source_revision.clone();
                drop(snapshot);
                match operation {
                    "records_all" => bench
                        .iter(|| black_box(index.records(&revision, None, None).expect("records"))),
                    "records_search_miss" => {
                        let scope = RecordScope {
                            project: "demo".into(),
                            investigation: Some("sample".into()),
                        };
                        bench.iter(|| {
                            black_box(
                                index
                                    .records(&revision, Some(&scope), Some("no-such-record"))
                                    .expect("search"),
                            )
                        });
                    }
                    "relationships_miss" => {
                        let identity = ScopedIdentity {
                            scope: RecordScope {
                                project: "demo".into(),
                                investigation: Some("sample".into()),
                            },
                            identity: "HMD-999999".into(),
                        };
                        bench.iter(|| {
                            black_box(
                                index
                                    .relationships(&revision, &identity)
                                    .expect("relationships"),
                            )
                        });
                    }
                    "boards" => {
                        let scope = RecordScope {
                            project: "demo".into(),
                            investigation: Some("sample".into()),
                        };
                        bench.iter(|| black_box(index.boards(&revision, &scope).expect("boards")));
                    }
                    _ => unreachable!(),
                }
            }
        });
    }
    group.finish();
}

criterion_group! {
    name = baselines;
    config = Criterion::default()
        .sample_size(10)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(2));
    targets = bench_inventory, bench_records, bench_progress, bench_scoped_progress, bench_core, bench_presentation, bench_mutation, bench_sqlite
}
criterion_main!(baselines);
