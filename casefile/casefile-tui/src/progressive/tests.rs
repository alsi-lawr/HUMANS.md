use super::*;
use casefile_store::Store;
use std::{
    fs,
    path::Path,
    sync::mpsc,
    time::{Duration, Instant},
};
use tempfile::TempDir;

const EVIDENCE: &str = "projects/demo/investigations/sample/evidence/observation.md";

#[test]
fn catalogue_is_navigable_before_completion_and_post_start_observation_is_reported() {
    let root = fixture();
    let store = Store::open(root.path()).expect("store");
    let (observation_sender, observation_receiver) = mpsc::channel();
    let (report_sender, report_receiver) = mpsc::channel();
    let mut coordinator = Coordinator::start(
        store.presentation_session(),
        Some(ObservationHandoff::new(observation_receiver, report_sender)),
    )
    .expect("coordinator");

    let first = coordinator
        .active
        .as_ref()
        .expect("active")
        .stream
        .recv()
        .expect("catalogue");
    assert!(matches!(first, PresentationEvent::Catalogue { .. }));
    assert_eq!(
        coordinator.apply_load_event(first),
        ProjectionChange::Partial
    );
    let projection = coordinator.projection();
    assert!(projection.provisional);
    assert_eq!(
        projection.scan.investigation_roots.get("demo"),
        Some(&vec!["sample".into()])
    );
    assert!(matches!(
        report_receiver.recv().expect("start report"),
        RefreshReport::Started {
            generation: 1,
            observation_generation: 0,
            ..
        }
    ));

    observation_sender
        .send(RefreshObservation {
            generation: 1,
            minimum_scope: RefreshMinimumScope::Contextual,
        })
        .expect("observation");
    assert!(coordinator.drain().dirty);
    finish_active(&mut coordinator);

    assert!(!coordinator.projection().provisional);
    assert!(
        coordinator
            .status()
            .contains("newer observation remains uncovered")
    );
    assert!(matches!(
        report_receiver.recv().expect("success report"),
        RefreshReport::Succeeded {
            started_observation_generation: 0,
            completed_observation_generation: 1,
            ..
        }
    ));
}

#[test]
fn store_minimum_scope_allows_contextual_refresh_and_reports_its_exact_coverage() {
    let root = fixture();
    let store = Store::open(root.path()).expect("store");
    let (_observation_sender, observation_receiver) = mpsc::channel();
    let (report_sender, report_receiver) = mpsc::channel();
    let mut coordinator = Coordinator::start(
        store.presentation_session(),
        Some(ObservationHandoff::new(observation_receiver, report_sender)),
    )
    .expect("coordinator");
    finish_active(&mut coordinator);
    assert!(matches!(
        report_receiver.recv().expect("initial start report"),
        RefreshReport::Started {
            target: PresentationTarget::Store,
            ..
        }
    ));
    assert!(matches!(
        report_receiver.recv().expect("initial success report"),
        RefreshReport::Succeeded {
            target: PresentationTarget::Store,
            ..
        }
    ));
    assert!(coordinator.observe(RefreshObservation {
        generation: 7,
        minimum_scope: RefreshMinimumScope::Store {
            reason: "activation changed".into(),
        },
    }));
    let generation = coordinator.next_generation;
    let target = PresentationTarget::Project {
        project: "demo".into(),
    };

    coordinator
        .refresh(target.clone())
        .expect("contextual Project refresh");
    assert_eq!(coordinator.next_generation, generation + 1);
    assert_eq!(
        coordinator.active.as_ref().map(|active| &active.target),
        Some(&target)
    );
    assert!(matches!(
        report_receiver.recv().expect("contextual start report"),
        RefreshReport::Started {
            target: PresentationTarget::Project { ref project },
            observation_generation: 7,
            ..
        } if project == "demo"
    ));
    finish_active(&mut coordinator);
    assert!(matches!(
        report_receiver.recv().expect("contextual success report"),
        RefreshReport::Succeeded {
            target: PresentationTarget::Project { ref project },
            started_observation_generation: 7,
            completed_observation_generation: 7,
            ..
        } if project == "demo"
    ));
    assert!(matches!(
        coordinator.observation.minimum_scope,
        RefreshMinimumScope::Store { .. }
    ));
}

#[test]
fn obsolete_generation_is_discarded_and_refresh_failure_keeps_complete_data() {
    let root = fixture();
    let store = Store::open(root.path()).expect("store");
    let mut coordinator =
        Coordinator::start(store.presentation_session(), None).expect("coordinator");
    finish_active(&mut coordinator);
    let complete_paths = coordinator
        .projection()
        .scan
        .snapshot
        .entries
        .iter()
        .map(|entry| entry.path.clone())
        .collect::<Vec<_>>();

    coordinator
        .refresh(PresentationTarget::Store)
        .expect("refresh two");
    let obsolete = coordinator.active.as_ref().expect("second").generation;
    coordinator
        .refresh(PresentationTarget::Store)
        .expect("refresh three");
    assert_eq!(
        coordinator.apply_load_event(PresentationEvent::Failure {
            generation: obsolete,
            target: PresentationTarget::Store,
            coverage: pending_coverage(),
            progress: PresentationProgress {
                completed: 0,
                total: None,
            },
            message: "obsolete".into(),
        }),
        ProjectionChange::None
    );
    assert!(!coordinator.status().contains("obsolete"));

    let active = coordinator.active.take().expect("current refresh");
    coordinator.finish_failure(active, "current failure".into());
    assert!(coordinator.status().contains("last complete data retained"));
    assert_eq!(
        coordinator
            .projection()
            .scan
            .snapshot
            .entries
            .iter()
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>(),
        complete_paths
    );
}

#[test]
fn selected_lazy_content_exposes_loaded_and_fresh_failure_states() {
    let root = fixture();
    let store = Store::open(root.path()).expect("store");
    let mut coordinator =
        Coordinator::start(store.presentation_session(), None).expect("coordinator");
    finish_active(&mut coordinator);
    assert!(coordinator.request_content(Some(EVIDENCE)));
    finish_content(&mut coordinator);
    let loaded = coordinator
        .projection()
        .scan
        .snapshot
        .entries
        .into_iter()
        .find(|entry| entry.path == EVIDENCE)
        .expect("evidence");
    assert!(!loaded.original_bytes.is_empty());
    assert!(coordinator.status().contains("Selected content loaded"));

    let root = fixture();
    let store = Store::open(root.path()).expect("store");
    let mut failed = Coordinator::start(store.presentation_session(), None).expect("coordinator");
    finish_active(&mut failed);
    fs::remove_file(root.path().join(EVIDENCE)).expect("remove");
    assert!(failed.request_content(Some(EVIDENCE)));
    finish_content(&mut failed);
    assert!(failed.status().contains("Content load failed"));
    let evidence = failed
        .projection()
        .scan
        .snapshot
        .entries
        .into_iter()
        .find(|entry| entry.path == EVIDENCE)
        .expect("evidence");
    assert!(evidence.original_bytes.is_empty());
}

#[test]
fn catalogue_resolves_full_nested_investigation_target() {
    let root = fixture();
    let original = root.path().join("projects/demo/investigations/sample");
    let nested = root
        .path()
        .join("projects/demo/investigations/alpha/shared");
    fs::create_dir_all(nested.parent().expect("nested parent")).expect("nested parent");
    fs::rename(original, &nested).expect("nested investigation");
    let activation = fs::read_to_string(root.path().join("casefile.toml"))
        .expect("activation")
        .replace(
            "projects/demo/investigations/sample",
            "projects/demo/investigations/alpha/shared",
        );
    fs::write(root.path().join("casefile.toml"), activation).expect("activation");
    let store = Store::open(root.path()).expect("store");
    let mut coordinator =
        Coordinator::start(store.presentation_session(), None).expect("coordinator");
    let deadline = Instant::now() + Duration::from_secs(5);
    while coordinator
        .investigation_target("demo", "alpha/shared")
        .is_none()
        && Instant::now() < deadline
    {
        coordinator.drain();
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        coordinator.investigation_target("demo", "alpha/shared"),
        Some(PresentationTarget::Investigation {
            project: "demo".into(),
            path: "projects/demo/investigations/alpha/shared".into(),
        })
    );
}

fn finish_active(coordinator: &mut Coordinator) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while coordinator.active.is_some() && Instant::now() < deadline {
        let update = coordinator.drain();
        if !update.dirty {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    assert!(coordinator.active.is_none(), "load did not finish");
}

fn finish_content(coordinator: &mut Coordinator) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while coordinator.content.is_some() && Instant::now() < deadline {
        let update = coordinator.drain();
        if !update.dirty {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    assert!(coordinator.content.is_none(), "content did not finish");
}

fn pending_coverage() -> PresentationCoverage {
    PresentationCoverage {
        catalogue: casefile_store::PresentationCoverageState::Pending,
        payload: casefile_store::PresentationCoverageState::Pending,
        facts: casefile_store::PresentationCoverageState::Pending,
    }
}

fn fixture() -> TempDir {
    let temporary = TempDir::new().expect("temporary root");
    copy_tree(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../casefile-store/tests/fixtures/minimum")
            .as_path(),
        temporary.path(),
    );
    temporary
}

fn copy_tree(from: &Path, to: &Path) {
    for entry in fs::read_dir(from).expect("fixture entries") {
        let entry = entry.expect("fixture entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("fixture type").is_dir() {
            fs::create_dir_all(&target).expect("fixture directory");
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).expect("fixture file");
        }
    }
}
