use super::*;

pub struct PresentationStream {
    pub(super) receiver: Receiver<PresentationEvent>,
    pub(super) worker: thread::Thread,
    pub(super) cancelled: Arc<AtomicBool>,
    pub(super) finished: Arc<AtomicBool>,
    pub(super) inner: Arc<SessionInner>,
    pub(super) order: u64,
}

impl PresentationStream {
    pub fn recv(&self) -> Result<PresentationEvent, mpsc::RecvError> {
        let event = self.receiver.recv()?;
        self.worker.unpark();
        self.register_received_handles(&event);
        Ok(event)
    }

    pub fn try_recv(&self) -> Result<PresentationEvent, TryRecvError> {
        let event = self.receiver.try_recv()?;
        self.worker.unpark();
        self.register_received_handles(&event);
        Ok(event)
    }

    pub fn cancel(&self) {
        let _state = self.inner.state.lock().expect("presentation state");
        self.cancelled.store(true, Ordering::Release);
        self.worker.unpark();
    }

    pub fn is_finished(&self) -> bool {
        self.finished.load(Ordering::Acquire)
    }

    fn register_received_handles(&self, event: &PresentationEvent) {
        let mut state = self.inner.state.lock().expect("presentation state");
        if self.cancelled.load(Ordering::Acquire) || !state.current(event.target(), self.order) {
            return;
        }
        match event {
            PresentationEvent::Complete { target, .. } => {
                state.handles.retain(|(entry_target, _), entry| {
                    entry_target != target || entry.order >= self.order
                })
            }
            PresentationEvent::Entries {
                target, entries, ..
            } => register_handles(&mut state, self.order, target, entries),
            _ => {}
        }
    }
}

impl Drop for PresentationStream {
    fn drop(&mut self) {
        self.cancel();
        let mut state = self.inner.state.lock().expect("presentation state");
        state.requests.retain(|_, load| load.order != self.order);
    }
}

pub struct PresentationContentStream {
    pub(super) receiver: Receiver<PresentationContentEvent>,
    pub(super) worker: thread::Thread,
    pub(super) cancelled: Arc<AtomicBool>,
}

impl PresentationContentStream {
    pub fn recv(&self) -> Result<PresentationContentEvent, mpsc::RecvError> {
        let event = self.receiver.recv()?;
        self.worker.unpark();
        Ok(event)
    }

    pub fn try_recv(&self) -> Result<PresentationContentEvent, TryRecvError> {
        let event = self.receiver.try_recv()?;
        self.worker.unpark();
        Ok(event)
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.worker.unpark();
    }
}

impl Drop for PresentationContentStream {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[derive(Clone)]
pub struct PresentationSession {
    pub(super) inner: Arc<SessionInner>,
}

pub(super) struct SessionInner {
    pub(super) session_id: u64,
    pub(super) reader: Arc<dyn PresentationReader>,
    pub(super) state: Mutex<SessionLoadedState>,
    pub(super) next_handle: AtomicU64,
}

#[derive(Default)]
pub(super) struct SessionLoadedState {
    pub(super) scopes: BTreeMap<Option<PresentationScope>, Arc<LoadedScope>>,
    pub(super) handles: BTreeMap<(PresentationTarget, String), EmittedContent>,
    requests: BTreeMap<PresentationTarget, AcceptedLoad>,
    next_order: u64,
}

struct AcceptedLoad {
    order: u64,
    cancelled: Arc<AtomicBool>,
    worker: Option<thread::Thread>,
}

impl SessionLoadedState {
    pub(super) fn current(&self, target: &PresentationTarget, order: u64) -> bool {
        self.requests
            .get(target)
            .is_some_and(|load| load.order == order && !load.cancelled.load(Ordering::Acquire))
    }
}

fn targets_overlap(left: &PresentationTarget, right: &PresentationTarget) -> bool {
    matches!(left, PresentationTarget::Store)
        || matches!(right, PresentationTarget::Store)
        || target_contains(left, &target_root(right))
        || target_contains(right, &target_root(left))
}

impl PresentationSession {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self::with_reader(Arc::new(FsPresentationReader { root }))
    }

    pub(super) fn with_reader(reader: Arc<dyn PresentationReader>) -> Self {
        Self {
            inner: Arc::new(SessionInner {
                session_id: NEXT_PRESENTATION_SESSION.fetch_add(1, Ordering::Relaxed),
                reader,
                state: Mutex::new(SessionLoadedState::default()),
                next_handle: AtomicU64::new(1),
            }),
        }
    }

    pub fn load(&self, request: PresentationLoadRequest) -> Result<PresentationStream, StoreError> {
        let (sender, receiver) = mpsc::sync_channel(PRESENTATION_CHANNEL_CAPACITY);
        let cancelled = Arc::new(AtomicBool::new(false));
        let order = {
            let mut state = self.inner.state.lock().expect("presentation state");
            for (target, old) in &state.requests {
                if targets_overlap(target, &request.target) {
                    old.cancelled.store(true, Ordering::Release);
                    if let Some(worker) = &old.worker {
                        worker.unpark();
                    }
                }
            }
            state.next_order += 1;
            let order = state.next_order;
            state.requests.insert(
                request.target.clone(),
                AcceptedLoad {
                    order,
                    cancelled: cancelled.clone(),
                    worker: None,
                },
            );
            order
        };
        let target = request.target.clone();
        let worker_cancelled = cancelled.clone();
        let finished = Arc::new(AtomicBool::new(false));
        let worker_finished = finished.clone();
        let inner = self.inner.clone();
        let receiver_inner = inner.clone();
        let worker = thread::Builder::new()
            .name("casefile-presentation-loader".into())
            .spawn(move || {
                run_load(inner, request, order, sender, worker_cancelled);
                worker_finished.store(true, Ordering::Release);
            });
        let worker = match worker {
            Ok(worker) => worker,
            Err(error) => {
                self.inner
                    .state
                    .lock()
                    .expect("presentation state")
                    .requests
                    .retain(|_, load| load.order != order);
                return Err(error.into());
            }
        };
        if let Some(load) = self
            .inner
            .state
            .lock()
            .expect("presentation state")
            .requests
            .get_mut(&target)
            .filter(|load| load.order == order)
        {
            load.worker = Some(worker.thread().clone());
        }
        if cancelled.load(Ordering::Acquire) {
            worker.thread().unpark();
        }
        Ok(PresentationStream {
            order,
            worker: worker.thread().clone(),
            receiver,
            cancelled,
            finished,
            inner: receiver_inner,
        })
    }

    pub fn fetch_content(
        &self,
        request: PresentationContentRequest,
    ) -> Result<PresentationContentStream, StoreError> {
        let (sender, receiver) = mpsc::sync_channel(PRESENTATION_CHANNEL_CAPACITY);
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let inner = self.inner.clone();
        let worker = thread::Builder::new()
            .name("casefile-presentation-content".into())
            .spawn(move || run_content(inner, request, sender, worker_cancelled))?;
        Ok(PresentationContentStream {
            worker: worker.thread().clone(),
            receiver,
            cancelled,
        })
    }
}

pub(super) fn send_bounded<T>(
    sender: &SyncSender<T>,
    cancelled: &AtomicBool,
    mut event: T,
) -> bool {
    loop {
        if cancelled.load(Ordering::Acquire) {
            return false;
        }
        match sender.try_send(event) {
            Ok(()) => return true,
            Err(TrySendError::Full(value)) => {
                event = value;
                thread::park();
            }
            Err(TrySendError::Disconnected(_)) => return false,
        }
    }
}
