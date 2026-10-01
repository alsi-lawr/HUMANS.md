#[allow(dead_code)]
mod support;

use casefile_core::{ChangeRequest, Kind, RecordDraft, parse_draft};
use casefile_store::{
    InvestigationScope, InvestigationScopedIdentity, Provider, ProviderQuery, ProviderQueryResult,
    Store,
};
use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use std::{fs, hint::black_box, time::Duration};
use support::{Fixture, INVESTIGATION};

fn scope() -> InvestigationScope {
    InvestigationScope {
        project: "demo".into(),
        investigation: "sample".into(),
    }
}

fn bench_queries(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("priority_scope_500_notes");
    for count in [250, 1_000] {
        for name in ["existing_detail", "missing_detail", "selected_record_index"] {
            group.bench_with_input(BenchmarkId::new(name, count), &count, |bench, &count| {
                let fixture = Fixture::new()
                    .tickets(count, false)
                    .progress_notes(500, false);
                let store = Store::open(fixture.root.path()).unwrap();
                assert_eq!(store.check(Some(INVESTIGATION)).unwrap().valid, Some(true));
                let provider = Provider::without_cache(store);
                let query = if name == "selected_record_index" {
                    ProviderQuery::RecordIndex { scope: scope() }
                } else {
                    ProviderQuery::RecordDetail {
                        identity: InvestigationScopedIdentity {
                            scope: scope(),
                            identity: if name == "existing_detail" {
                                "HMD-011"
                            } else {
                                "HMD-999999"
                            }
                            .into(),
                        },
                    }
                };
                match provider.query(query.clone()).unwrap() {
                    ProviderQueryResult::RecordDetail { record, .. } => {
                        assert_eq!(record.is_some(), name == "existing_detail");
                        if let Some(record) = record {
                            assert_eq!(record.progress.unwrap().notes.len(), 500);
                        }
                    }
                    ProviderQueryResult::RecordIndex { records, .. } => {
                        assert!(records.len() >= count)
                    }
                    _ => panic!("query response"),
                }
                bench.iter(|| black_box(provider.query(black_box(query.clone())).unwrap()));
            });
        }
    }
    group.finish();
}

fn bench_record_mutation(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("priority_single_record_250_records_500_notes");
    group.bench_function("preview", |bench| {
        let fixture = Fixture::new()
            .tickets(250, false)
            .progress_notes(500, false)
            .git_repository();
        let store = Store::open(fixture.root.path()).unwrap();
        assert_eq!(store.check(Some(INVESTIGATION)).unwrap().valid, Some(true));
        let path = format!("{INVESTIGATION}/tickets/accepted/HMD-100000.md");
        let draft = {
            let text = fs::read_to_string(fixture.root.path().join(&path)).unwrap();
            let mut draft = parse_draft(&path, Kind::Ticket, &text).unwrap();
            let RecordDraft::Ticket(ticket) = &mut draft else {
                panic!("ticket draft")
            };
            ticket.title = "Changed benchmark ticket".into();
            draft
        };
        let request = ChangeRequest::Replace { path, draft };
        {
            let provider = Provider::without_cache(store.clone());
            let preview = provider.preview_record(request.clone()).unwrap();
            assert!(!preview.no_op);
            assert!(preview.diagnostics.is_empty());
        }
        bench.iter_batched(
            || Provider::without_cache(store.clone()),
            |provider| black_box(provider.preview_record(black_box(request.clone())).unwrap()),
            BatchSize::PerIteration,
        );
    });
    group.bench_function("apply", |bench| {
        let fixture = Fixture::new()
            .tickets(250, false)
            .progress_notes(500, false)
            .git_repository();
        let store = Store::open(fixture.root.path()).unwrap();
        assert_eq!(store.check(Some(INVESTIGATION)).unwrap().valid, Some(true));
        let path = format!("{INVESTIGATION}/tickets/accepted/HMD-100000.md");
        let original = {
            let text = fs::read_to_string(fixture.root.path().join(&path)).unwrap();
            parse_draft(&path, Kind::Ticket, &text).unwrap()
        };
        let mut changed = original.clone();
        let RecordDraft::Ticket(ticket) = &mut changed else {
            panic!("ticket draft")
        };
        ticket.title = "Changed benchmark ticket".into();
        {
            let provider = Provider::without_cache(store.clone());
            let preview = provider
                .preview_record(ChangeRequest::Replace {
                    path: path.clone(),
                    draft: changed.clone(),
                })
                .unwrap();
            assert!(!preview.no_op);
            assert!(preview.diagnostics.is_empty());
            assert!(
                !provider
                    .apply_record(&preview.preview_id)
                    .unwrap()
                    .result
                    .no_op
            );
            assert_eq!(
                parse_draft(
                    &path,
                    Kind::Ticket,
                    &fs::read_to_string(fixture.root.path().join(&path)).unwrap()
                )
                .unwrap(),
                changed
            );
            provider
                .apply_record(
                    &provider
                        .preview_record(ChangeRequest::Replace {
                            path: path.clone(),
                            draft: original.clone(),
                        })
                        .unwrap()
                        .preview_id,
                )
                .unwrap();
        }
        let mut changed_next = true;
        bench.iter_batched(
            || {
                let provider = Provider::without_cache(store.clone());
                let draft = if changed_next {
                    changed.clone()
                } else {
                    original.clone()
                };
                changed_next = !changed_next;
                let preview = provider
                    .preview_record(ChangeRequest::Replace {
                        path: path.clone(),
                        draft,
                    })
                    .unwrap();
                assert!(!preview.no_op);
                (provider, preview)
            },
            |(provider, preview)| {
                black_box(
                    provider
                        .apply_record(black_box(&preview.preview_id))
                        .unwrap(),
                )
            },
            BatchSize::PerIteration,
        );
    });
    group.finish();
}

criterion_group! {
    name = priority;
    config = Criterion::default().sample_size(10)
        .warm_up_time(Duration::from_secs(1)).measurement_time(Duration::from_secs(2));
    targets = bench_queries, bench_record_mutation
}
criterion_main!(priority);
