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
        let fixture = Fixture::new().opaque_files(count);
        let store = Store::open(fixture.root.path()).expect("store");
        assert_valid_fixture(&store);
        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(
            BenchmarkId::new("metadata_summary", count),
            &store,
            |bench, store| {
                bench.iter(|| black_box(store.scan_summary().expect("summary")));
            },
        );
        group.bench_with_input(
            BenchmarkId::new("scoped_check", count),
            &store,
            |bench, store| {
                bench.iter(|| black_box(store.check(Some(INVESTIGATION)).expect("check")));
            },
        );
        group.bench_with_input(
            BenchmarkId::new("full_scan", count),
            &store,
            |bench, store| {
                bench.iter(|| black_box(store.scan().expect("scan")));
            },
        );
    }
    group.finish();

    let mut group = criterion.benchmark_group("activation_roots");
    for roots in [1, 100] {
        let fixture = Fixture::new().opaque_files(2_000).activation_roots(roots);
        let store = Store::open(fixture.root.path()).expect("store");
        assert_valid_fixture(&store);
        group.throughput(Throughput::Elements(2_000));
        group.bench_with_input(
            BenchmarkId::new("scoped_check", roots),
            &store,
            |bench, store| {
                bench.iter(|| black_box(store.check(Some(INVESTIGATION)).expect("check")));
            },
        );
    }
    group.finish();
}

fn bench_records(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("records");
    for count in [250, 1_000] {
        let fixture = Fixture::new().tickets(count, false);
        let store = Store::open(fixture.root.path()).expect("store");
        assert_valid_fixture(&store);
        let provider = Provider::without_cache(store.clone());
        let index_query = ProviderQuery::RecordIndex { scope: scope() };
        let boards_query = ProviderQuery::Boards { scope: scope() };
        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(
            BenchmarkId::new("scoped_check", count),
            &store,
            |bench, store| {
                bench.iter(|| black_box(store.check(Some(INVESTIGATION)).expect("check")));
            },
        );
        group.bench_with_input(
            BenchmarkId::new("full_derived", count),
            &store,
            |bench, store| {
                bench.iter(|| black_box(store.derived_snapshot().expect("derived")));
            },
        );
        group.bench_with_input(
            BenchmarkId::new("record_index", count),
            &provider,
            |bench, provider| {
                bench
                    .iter(|| black_box(provider.query(index_query.clone()).expect("record index")));
            },
        );
        group.bench_with_input(
            BenchmarkId::new("boards", count),
            &provider,
            |bench, provider| {
                bench.iter(|| black_box(provider.query(boards_query.clone()).expect("boards")));
            },
        );
    }
    group.finish();

    let mut group = criterion.benchmark_group("supersession");
    for count in [250, 500] {
        for chain in [false, true] {
            let fixture = Fixture::new().tickets(count, chain);
            let store = Store::open(fixture.root.path()).expect("store");
            assert_valid_fixture(&store);
            let label = if chain { "chain" } else { "independent" };
            group.throughput(Throughput::Elements(count as u64));
            group.bench_with_input(BenchmarkId::new(label, count), &store, |bench, store| {
                bench.iter(|| black_box(store.check(Some(INVESTIGATION)).expect("check")));
            });
        }
    }
    group.finish();
}

fn bench_progress(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("progress");
    for count in [250, 500] {
        for missing in [false, true] {
            let fixture = Fixture::new()
                .opaque_files(1_000)
                .progress_notes(count, missing);
            let store = Store::open(fixture.root.path()).expect("store");
            let result = store.check(Some(INVESTIGATION)).expect("fixture check");
            assert_eq!(result.valid, Some(!missing));
            let label = if missing {
                "missing_targets"
            } else {
                "accepted_target"
            };
            group.throughput(Throughput::Elements(count as u64));
            group.bench_with_input(BenchmarkId::new(label, count), &store, |bench, store| {
                bench.iter(|| black_box(store.check(Some(INVESTIGATION)).expect("check")));
            });
        }
    }
    group.finish();
}

fn bench_scoped_progress(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("scoped_progress_250_records");
    for notes in [0, 500] {
        let fixture = Fixture::new()
            .tickets(250, false)
            .progress_notes(notes, false);
        let store = Store::open(fixture.root.path()).expect("store");
        assert_valid_fixture(&store);
        let provider = Provider::without_cache(store);
        let index = ProviderQuery::RecordIndex { scope: scope() };
        let boards = ProviderQuery::Boards { scope: scope() };
        let missing_detail = ProviderQuery::RecordDetail {
            identity: InvestigationScopedIdentity {
                scope: scope(),
                identity: "HMD-999999".into(),
            },
        };
        for (operation, query) in [
            ("record_index", index),
            ("disposition_boards", boards),
            ("missing_detail", missing_detail),
        ] {
            group.bench_with_input(
                BenchmarkId::new(operation, notes),
                &provider,
                |bench, provider| {
                    bench.iter(|| black_box(provider.query(query.clone()).expect("scoped query")));
                },
            );
        }
    }
    group.finish();
}

fn bench_core(criterion: &mut Criterion) {
    let fixture = Fixture::new().progress_notes(500, false);
    let ticket_path = format!("{INVESTIGATION}/tickets/accepted/HMD-011.md");
    let ticket_text = fs::read_to_string(fixture.root.path().join(&ticket_path)).expect("ticket");
    let ticket_draft = parse_draft(&ticket_path, Kind::Ticket, &ticket_text).expect("ticket draft");
    let progress_path = format!("{INVESTIGATION}/progress/log.toml");
    let progress_text =
        fs::read_to_string(fixture.root.path().join(&progress_path)).expect("progress");
    let mut group = criterion.benchmark_group("core_parsing");
    group.bench_function("ticket_parse", |bench| {
        bench.iter(|| {
            black_box(
                parse_draft(&ticket_path, Kind::Ticket, black_box(&ticket_text)).expect("parse"),
            )
        });
    });
    group.bench_function("ticket_render_roundtrip", |bench| {
        bench.iter(|| {
            black_box(render_draft(&ticket_path, black_box(&ticket_draft)).expect("render"))
        });
    });
    group.throughput(Throughput::Elements(500));
    group.bench_function("progress_parse_500_notes", |bench| {
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
    let fixture = Fixture::new().tickets(250, false);
    let store = Store::open(fixture.root.path()).expect("store");
    assert_valid_fixture(&store);
    let warm_session = store.presentation_session();
    drain_presentation(&warm_session, 0);
    let mut generation = 1;
    let mut group = criterion.benchmark_group("presentation_250_records");
    group.throughput(Throughput::Elements(250));
    group.bench_function("cold_session", |bench| {
        bench.iter(|| {
            let session = store.presentation_session();
            drain_presentation(&session, 1);
        });
    });
    group.bench_function("unchanged_refresh", |bench| {
        bench.iter(|| {
            drain_presentation(&warm_session, generation);
            generation += 1;
        });
    });
    group.finish();
}

fn bench_mutation(criterion: &mut Criterion) {
    let fixture = Fixture::new().tickets(250, false).git_repository();
    let store = Store::open(fixture.root.path()).expect("store");
    assert_valid_fixture(&store);
    let requests = (0..10)
        .map(|number| {
            let path = format!(
                "{INVESTIGATION}/tickets/accepted/HMD-{:06}.md",
                number + 100_000
            );
            let text = fs::read_to_string(fixture.root.path().join(&path)).expect("ticket");
            let draft = parse_draft(&path, Kind::Ticket, &text).expect("draft");
            ChangeRequest::Replace { path, draft }
        })
        .collect::<Vec<_>>();
    assert!(
        store
            .preview_batch(requests[..1].to_vec())
            .expect("preview fixture")
            .diagnostics
            .is_empty()
    );
    let mut group = criterion.benchmark_group("mutation_preview_250_records");
    for count in [1, 10] {
        group.throughput(Throughput::Elements(count));
        let batch = requests[..count as usize].to_vec();
        group.bench_with_input(
            BenchmarkId::new("replace_batch", count),
            &batch,
            |bench, batch| {
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
    let revision = snapshot.source_revision.clone();
    let record_scope = RecordScope {
        project: "demo".into(),
        investigation: Some("sample".into()),
    };
    let missing_identity = ScopedIdentity {
        scope: record_scope.clone(),
        identity: "HMD-999999".into(),
    };

    let mut group = criterion.benchmark_group("sqlite_500_records");
    group.bench_function("prepare_replacement", |bench| {
        bench.iter(|| black_box(index.prepare(black_box(&snapshot)).expect("prepare")));
    });
    group.bench_function("records_all", |bench| {
        bench.iter(|| black_box(index.records(&revision, None, None).expect("records")));
    });
    group.bench_function("records_search_miss", |bench| {
        bench.iter(|| {
            black_box(
                index
                    .records(&revision, Some(&record_scope), Some("no-such-record"))
                    .expect("search"),
            )
        });
    });
    group.bench_function("relationships_miss", |bench| {
        bench.iter(|| {
            black_box(
                index
                    .relationships(&revision, &missing_identity)
                    .expect("relationships"),
            )
        });
    });
    group.bench_function("boards", |bench| {
        bench.iter(|| black_box(index.boards(&revision, &record_scope).expect("boards")));
    });
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
