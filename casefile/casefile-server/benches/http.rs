#![allow(dead_code)]

#[path = "../src/api.rs"]
mod api;
#[path = "../src/assets.rs"]
mod assets;
#[path = "../../benchmarks/support/mod.rs"]
mod support;
#[path = "../src/transport.rs"]
mod transport;
#[path = "../src/workbench.rs"]
mod workbench;

use casefile_store::{Provider, Store};
use casefile_store_sqlite::SqliteIndex;
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::{
    hint::black_box,
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::Arc,
    time::Duration,
};
use support::{Fixture, INVESTIGATION};
use tempfile::TempDir;

struct Loopback {
    runtime: tokio::runtime::Runtime,
    address: SocketAddr,
    worker: tokio::task::JoinHandle<anyhow::Result<()>>,
}

impl Loopback {
    fn new(store: Store, index_path: &std::path::Path) -> Self {
        let index = SqliteIndex::open(index_path, store.observation_root()).unwrap();
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let runtime = transport::runtime().unwrap();
        let listener = {
            let _entered = runtime.enter();
            tokio::net::TcpListener::from_std(listener).unwrap()
        };
        let host = Arc::new(api::Host::new(
            workbench::Workbench::new(Provider::new(store, index)),
            address.port(),
            false,
            "benchmark-only".into(),
        ));
        let worker = runtime.spawn(transport::serve(listener, host));
        Self {
            runtime,
            address,
            worker,
        }
    }

    fn request(&self, body: &str) -> Vec<u8> {
        let mut stream = TcpStream::connect(self.address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .unwrap();
        write!(stream, "POST /api/query HTTP/1.0\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", self.address, body.len(), body).unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).unwrap();
        response
    }
}

impl Drop for Loopback {
    fn drop(&mut self) {
        self.worker.abort();
        self.runtime.block_on(async {
            let _ = (&mut self.worker).await;
        });
    }
}

fn bench_http(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("http_loopback_records");
    for count in [250, 1_000] {
        for (name, search) in [
            ("unchanged", None),
            ("search_hit", Some("HMD-100000")),
            ("search_miss", Some("no-such-record")),
        ] {
            group.bench_with_input(BenchmarkId::new(name, count), &(count, search), |bench, &(count, search)| {
                let fixture = Fixture::new().tickets(count, false).progress_notes(500, false);
                let store = Store::open(fixture.root.path()).unwrap();
                assert_eq!(store.check(Some(INVESTIGATION)).unwrap().valid, Some(true));
                let index = TempDir::new().unwrap();
                let server = Loopback::new(store, &index.path().join("index.sqlite"));
                let body = serde_json::json!({
                    "query": "records", "scope": {"project": "demo", "investigation": "sample"}, "search": search,
                }).to_string();
                {
                    let response = String::from_utf8(server.request(&body)).unwrap();
                    let (headers, json) = response.split_once("\r\n\r\n").unwrap();
                    assert_eq!(headers.lines().next().unwrap().split_whitespace().nth(1), Some("200"), "{response}");
                    let result: serde_json::Value = serde_json::from_str(json).unwrap();
                    let rows = result["Current"]["value"].as_array().expect("current indexed response");
                    match name {
                        "unchanged" => assert!(rows.len() >= count),
                        "search_hit" => assert!(!rows.is_empty()),
                        _ => assert!(rows.is_empty()),
                    }
                }
                bench.iter(|| black_box(server.request(black_box(&body))));
            });
        }
    }
    group.finish();
}

criterion_group! {
    name = http;
    config = Criterion::default().sample_size(10)
        .warm_up_time(Duration::from_secs(1)).measurement_time(Duration::from_secs(2));
    targets = bench_http
}
criterion_main!(http);
