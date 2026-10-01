#[path = "../../benchmarks/support/mod.rs"]
#[allow(dead_code)]
mod support;

use crate::workbench::App;
use crossterm::event::KeyCode;
use ratatui::Terminal;

fn interaction_fixture() -> (
    support::Fixture,
    App,
    Terminal<ratatui::backend::TestBackend>,
) {
    use casefile_store::Store;
    use support::{Fixture, INVESTIGATION};
    let fixture = Fixture::new()
        .tickets(250, false)
        .progress_notes(500, false)
        .long_ticket_body(80);
    let store = Store::open(fixture.root.path()).expect("store");
    assert_eq!(store.check(Some(INVESTIGATION)).unwrap().valid, Some(true));
    let app = App::new(store.scan().unwrap(), store.derived_snapshot().unwrap());
    let terminal = Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
    (fixture, app, terminal)
}

fn selected_scope(app: &mut App) {
    for key in [KeyCode::Char('1'), KeyCode::Enter, KeyCode::Enter] {
        app.handle(key);
    }
}

fn bench_interactions(criterion: &mut criterion::Criterion) {
    use std::hint::black_box;
    let mut group = criterion.benchmark_group("tui_250_records_500_notes");
    group.bench_function("selected_scope_navigation", |bench| {
        let (_fixture, mut app, mut terminal) = interaction_fixture();
        {
            let before = app.resume();
            selected_scope(&mut app);
            assert_ne!(app.resume(), before);
        }
        bench.iter(|| {
            for key in [KeyCode::Char('1'), KeyCode::Enter, KeyCode::Enter] {
                app.handle(key);
                terminal
                    .draw(|frame| app.render(frame.area(), frame.buffer_mut()))
                    .unwrap();
            }
            black_box(terminal.backend().buffer());
        });
    });
    group.bench_function("list_scroll_20_frames", |bench| {
        let (_fixture, mut app, mut terminal) = interaction_fixture();
        selected_scope(&mut app);
        app.handle(KeyCode::Char('3'));
        app.handle(KeyCode::Home);
        {
            let before = app.resume();
            app.handle(KeyCode::Down);
            assert_ne!(app.resume(), before);
            app.handle(KeyCode::Up);
        }
        bench.iter(|| {
            for key in
                std::iter::repeat_n(KeyCode::Down, 10).chain(std::iter::repeat_n(KeyCode::Up, 10))
            {
                app.handle(key);
                terminal
                    .draw(|frame| app.render(frame.area(), frame.buffer_mut()))
                    .unwrap();
            }
            black_box(terminal.backend().buffer());
        });
    });
    group.bench_function("detail_scroll_20_frames", |bench| {
        let (_fixture, mut app, mut terminal) = interaction_fixture();
        selected_scope(&mut app);
        app.handle(KeyCode::Char('3'));
        app.handle(KeyCode::Home);
        app.handle(KeyCode::Right);
        app.handle(KeyCode::Tab);
        terminal
            .draw(|frame| app.render(frame.area(), frame.buffer_mut()))
            .unwrap();
        {
            let before = app.resume();
            app.handle(KeyCode::Down);
            assert_ne!(app.resume(), before);
            app.handle(KeyCode::Up);
        }
        bench.iter(|| {
            for key in
                std::iter::repeat_n(KeyCode::Down, 10).chain(std::iter::repeat_n(KeyCode::Up, 10))
            {
                app.handle(key);
                terminal
                    .draw(|frame| app.render(frame.area(), frame.buffer_mut()))
                    .unwrap();
            }
            black_box(terminal.backend().buffer());
        });
    });
    group.finish();
}

pub(crate) fn run() {
    let mut criterion = criterion::Criterion::default()
        .sample_size(10)
        .warm_up_time(std::time::Duration::from_secs(1))
        .measurement_time(std::time::Duration::from_secs(2))
        .configure_from_args();
    bench_interactions(&mut criterion);
    criterion.final_summary();
}
