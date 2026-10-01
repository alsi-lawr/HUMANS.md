use super::*;
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

const PREVIEW_LIMIT: usize = 256;
static NEXT_INSTANCE: AtomicU64 = AtomicU64::new(1);

pub(super) enum StoredPreview {
    Record(Preview),
    RecordBatch(ChangeBatchPreview),
    Progress(ProgressPreview),
    Board(Preview),
    StrategyTransition(StrategyTransitionPreview),
    WriterBinding(WriterBindingPreview),
}

impl StoredPreview {
    pub(super) fn kind(&self) -> ProviderPreviewKind {
        match self {
            Self::Record(_) => ProviderPreviewKind::Record,
            Self::RecordBatch(_) => ProviderPreviewKind::RecordBatch,
            Self::Progress(_) => ProviderPreviewKind::Progress,
            Self::Board(_) => ProviderPreviewKind::DefaultDeliveryBoard,
            Self::StrategyTransition(_) => ProviderPreviewKind::StrategyTransition,
            Self::WriterBinding(_) => ProviderPreviewKind::WriterBinding,
        }
    }
}

pub(super) struct PreviewVault {
    prefix: String,
    next: u64,
    order: VecDeque<String>,
    values: BTreeMap<String, Arc<StoredPreview>>,
}

impl Default for PreviewVault {
    fn default() -> Self {
        let created = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("current time after epoch")
            .as_nanos();
        let instance = NEXT_INSTANCE.fetch_add(1, Ordering::Relaxed);
        Self {
            prefix: format!(
                "provider-preview-{}-{created}-{instance}",
                std::process::id()
            ),
            next: 0,
            order: VecDeque::new(),
            values: BTreeMap::new(),
        }
    }
}

impl PreviewVault {
    pub(super) fn instance(&self) -> &str {
        &self.prefix
    }

    pub(super) fn remember(&mut self, original: StoredPreview) -> ProviderPreview {
        self.next += 1;
        let id = format!("{}-{}", self.prefix, self.next);
        let review = review::preview(&id, &original);
        self.order.push_back(id.clone());
        self.values.insert(id, Arc::new(original));
        while self.order.len() > PREVIEW_LIMIT {
            if let Some(expired) = self.order.pop_front() {
                self.values.remove(&expired);
            }
        }
        review
    }

    pub(super) fn get(&self, id: &str) -> Result<Arc<StoredPreview>, ProviderError> {
        self.values
            .get(id)
            .cloned()
            .ok_or(ProviderError::PreviewIntegrity)
    }
}
