use notify::{Config, Event, EventKindMask, RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

#[derive(Default)]
struct Invalidation {
    epoch: u64,
    reconciled: u64,
    error: Option<String>,
}

pub(super) struct CacheWatch {
    root: PathBuf,
    state: Arc<Mutex<Invalidation>>,
    watcher: Option<RecommendedWatcher>,
}

pub(super) struct Observation {
    pub(super) epoch: u64,
    pub(super) dirty: bool,
}

impl CacheWatch {
    pub(super) fn new(root: &Path) -> Self {
        let mut watch = Self {
            root: root.to_owned(),
            state: Arc::new(Mutex::new(Invalidation::default())),
            watcher: None,
        };
        watch.start();
        watch
    }

    fn start(&mut self) {
        self.watcher = None;
        let root = self.root.clone();
        let state = Arc::clone(&self.state);
        let attempt = RecommendedWatcher::new(
            move |event| invalidate(&state, &root, event),
            Config::default()
                .with_event_kinds(EventKindMask::CORE)
                .with_follow_symlinks(false),
        )
        .and_then(|mut watcher| {
            let metadata = std::fs::symlink_metadata(&self.root).map_err(notify::Error::io)?;
            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                return Err(notify::Error::generic(
                    "Store watch root must be a non-symlink directory",
                ));
            }
            watcher.watch(&self.root, RecursiveMode::Recursive)?;
            Ok(watcher)
        });
        let mut state = self.state.lock().expect("cache invalidation");
        match attempt {
            Ok(watcher) => {
                self.watcher = Some(watcher);
                state.error = None;
            }
            Err(error) => {
                state.epoch += 1;
                state.error = Some(error.to_string());
            }
        }
    }

    pub(super) fn observe(&mut self) -> Observation {
        let restart = self.watcher.is_none()
            || self
                .state
                .lock()
                .expect("cache invalidation")
                .error
                .is_some();
        if restart {
            self.start();
        }
        let state = self.state.lock().expect("cache invalidation");
        Observation {
            epoch: state.epoch,
            dirty: state.epoch != state.reconciled || state.error.is_some(),
        }
    }

    #[cfg(test)]
    pub(super) fn inject(&self, event: notify::Result<Event>) {
        invalidate(&self.state, &self.root, event);
    }

    pub(super) fn reconcile(&self, observation: &Observation) -> Result<(), String> {
        let mut state = self.state.lock().expect("cache invalidation");
        if let Some(error) = &state.error {
            return Err(error.clone());
        }
        if state.epoch != observation.epoch {
            return Err("canonical filesystem events arrived during cache reconciliation".into());
        }
        state.reconciled = observation.epoch;
        Ok(())
    }
}

fn invalidate(state: &Mutex<Invalidation>, root: &Path, event: notify::Result<Event>) {
    let relevant = match &event {
        Ok(event) => {
            event.need_rescan()
                || event.paths.is_empty()
                || event.paths.iter().any(|path| {
                    path.strip_prefix(root)
                        .is_ok_and(|relative| !crate::is_store_path_excluded(relative))
                })
        }
        Err(_) => true,
    };
    if !relevant {
        return;
    }
    let mut state = state.lock().expect("cache invalidation");
    state.epoch += 1;
    if let Err(error) = event {
        state.error = Some(error.to_string());
    }
}
