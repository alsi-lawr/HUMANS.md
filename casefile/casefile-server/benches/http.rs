#![allow(dead_code)]

#[path = "../src/api.rs"]
mod api;
#[path = "../src/assets.rs"]
mod assets;
#[path = "../../benchmarks/support/mod.rs"]
mod support;
#[path = "../src/workbench.rs"]
mod workbench;

use casefile_store::{Provider, Store};
use casefile_store_sqlite::SqliteIndex;
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::{
    hint::black_box,
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};
use support::{Fixture, INVESTIGATION};
use tempfile::TempDir;
use tiny_http::Server;

struct Loopback {
    server: Arc<Server>,
    address: SocketAddr,
    stopped: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Loopback {
    fn new(store: Store, index_path: &std::path::Path) -> Self {
        let root = store.observation_root();
        let provider_index = SqliteIndex::open(index_path, root).unwrap();
        let query_index = SqliteIndex::open(index_path, root).unwrap();
        let server = Arc::new(Server::http(("127.0.0.1", 0)).unwrap());
        let address = server.server_addr().to_ip().unwrap();
        let host = api::Host::new(
            workbench::Workbench::new(Provider::new(store, provider_index), query_index),
            address.port(),
            false,
            "benchmark-only".into(),
        );
        let stopped = Arc::new(AtomicBool::new(false));
        let worker_server = server.clone();
        let worker_stopped = stopped.clone();
        let worker = thread::spawn(move || {
            while !worker_stopped.load(Ordering::Relaxed) {
                if let Some(request) = worker_server
                    .recv_timeout(Duration::from_millis(100))
                    .unwrap()
                {
                    host.handle(request).unwrap();
                }
            }
        });
        Self {
            server,
            address,
            stopped,
            worker: Some(worker),
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
        self.stopped.store(true, Ordering::Relaxed);
        self.server.unblock();
        self.worker.take().unwrap().join().unwrap();
    }
}

fn bench_http(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("http_loopback_records");
    for count in [250, 1_000] {
        let fixture = Fixture::new()
            .tickets(count, false)
            .progress_notes(500, false);
        let store = Store::open(fixture.root.path()).unwrap();
        assert_eq!(store.check(Some(INVESTIGATION)).unwrap().valid, Some(true));
        let index = TempDir::new().unwrap();
        let server = Loopback::new(store, &index.path().join("index.sqlite"));
        for (name, search) in [
            ("unchanged", None),
            ("search_hit", Some("HMD-100000")),
            ("search_miss", Some("no-such-record")),
        ] {
            let body = serde_json::json!({
                "query": "records", "scope": {"project": "demo", "investigation": "sample"}, "search": search,
            }).to_string();
            let response = String::from_utf8(server.request(&body)).unwrap();
            let (headers, json) = response.split_once("\r\n\r\n").unwrap();
            assert_eq!(
                headers.lines().next().unwrap().split_whitespace().nth(1),
                Some("200"),
                "{response}"
            );
            let result: serde_json::Value = serde_json::from_str(json).unwrap();
            let rows = result["Current"]["value"]
                .as_array()
                .expect("current indexed response");
            match name {
                "unchanged" => assert!(rows.len() >= count),
                "search_hit" => assert!(!rows.is_empty()),
                _ => assert!(rows.is_empty()),
            }
            group.bench_with_input(BenchmarkId::new(name, count), &body, |bench, body| {
                bench.iter(|| black_box(server.request(black_box(body))));
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
