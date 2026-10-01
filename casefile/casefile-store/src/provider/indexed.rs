use super::*;
use crate::{
    IndexPublicationId,
    scanning::{MetadataInventory, metadata_inventory, require_inventory_unchanged},
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceReadToken {
    token: WorkspaceTokenDomain,
    pub source_revision: Revision,
    pub publication_id: IndexPublicationId,
    pub provider_instance: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum WorkspaceTokenDomain {
    #[serde(rename = "workspace_index")]
    Index,
}

impl<C: ProviderCache> Provider<C> {
    pub(super) fn full_cache_phase<R, T>(
        &self,
        inspect: impl Fn(&C, &Revision, bool) -> (CacheState, Option<R>),
        project: impl FnOnce(&C, &Revision, R) -> Result<T, String>,
    ) -> Result<(CacheState, Option<T>), ProviderError> {
        if !self.cache.configured() {
            return Ok((CacheState::NotConfigured, None));
        }
        let mut watch = self.watch.lock().expect("cache refresh");
        let watch = watch.as_mut().expect("configured cache watcher");
        let observation = watch.observe();
        if !observation.dirty {
            let baseline = metadata_inventory(self.store.observation_root())?;
            let (state, ready) = inspect(&self.cache, &baseline.revision, false);
            if matches!(state, CacheState::Current { .. }) {
                return self.finish_projection(
                    watch,
                    &observation,
                    baseline,
                    (state, ready),
                    project,
                );
            }
            if matches!(state, CacheState::Degraded { .. }) {
                return Ok((state, None));
            }
        }
        let derived = self.store.derived_snapshot()?;
        let state = self.refresh_cache(&derived);
        if !matches!(state, CacheState::Current { .. }) {
            return Ok((state, None));
        }
        let baseline = metadata_inventory(self.store.observation_root())?;
        if baseline.revision != derived.source_revision {
            return Ok(degraded("canonical content changed during cache refresh"));
        }
        let (state, ready) = inspect(&self.cache, &baseline.revision, true);
        self.finish_projection(watch, &observation, baseline, (state, ready), project)
    }

    fn finish_projection<R, T>(
        &self,
        watch: &watching::CacheWatch,
        observation: &watching::Observation,
        baseline: MetadataInventory,
        inspected: (CacheState, Option<R>),
        project: impl FnOnce(&C, &Revision, R) -> Result<T, String>,
    ) -> Result<(CacheState, Option<T>), ProviderError> {
        let (state, ready) = inspected;
        if !matches!(state, CacheState::Current { .. }) {
            return Ok((state, None));
        }
        let Some(ready) = ready else {
            return Ok(degraded("current cache projection is unavailable"));
        };
        let value = match project(&self.cache, &baseline.revision, ready) {
            Ok(value) => value,
            Err(message) => return Ok(degraded(message)),
        };
        if let Err(error) = require_inventory_unchanged(self.store.observation_root(), &baseline) {
            return Ok(degraded(error.to_string()));
        }
        if let Err(message) = watch.reconcile(observation) {
            return Ok(degraded(message));
        }
        Ok((state, Some(value)))
    }
}

fn degraded<T>(message: impl Into<String>) -> (CacheState, Option<T>) {
    (
        CacheState::Degraded {
            message: message.into(),
        },
        None,
    )
}

type IndexedRecordOutcome<T> = (
    ProviderApplyOutcome<ProviderRecordApplyResult>,
    Option<(WorkspaceReadToken, T)>,
);

impl<C: DerivedIndex> Provider<C>
where
    C::Error: Display,
{
    pub fn read_full_index<T, E: Display>(
        &self,
        read: impl FnOnce(&C, &WorkspaceReadToken) -> Result<T, E>,
    ) -> Result<(WorkspaceReadToken, T), ProviderError> {
        let (state, value) = self.index_projection(read)?;
        value.ok_or_else(|| ProviderError::CacheRead(format!("{state:?}")))
    }

    pub fn apply_record_with_index<T, E: Display>(
        &self,
        preview_id: &str,
        read: impl FnOnce(&C, &WorkspaceReadToken) -> Result<T, E>,
    ) -> Result<IndexedRecordOutcome<T>, ProviderError> {
        let result = self.apply_record_canonical(preview_id)?;
        let (mut cache, value) = self
            .index_projection(read)
            .unwrap_or_else(|error| degraded(error.to_string()));
        if matches!(cache, CacheState::Missing | CacheState::Stale { .. }) {
            cache = CacheState::Degraded {
                message: format!("committed index projection is unavailable: {cache:?}"),
            };
        }
        Ok((ProviderApplyOutcome { result, cache }, value))
    }

    fn index_projection<T, E: Display>(
        &self,
        read: impl FnOnce(&C, &WorkspaceReadToken) -> Result<T, E>,
    ) -> Result<(CacheState, Option<(WorkspaceReadToken, T)>), ProviderError> {
        self.full_cache_phase(
            |cache, revision, _| match cache.publication(revision) {
                Ok(Indexed::Current {
                    source_revision,
                    value,
                }) => (CacheState::Current { source_revision }, Some(value)),
                Ok(Indexed::Missing) => (CacheState::Missing, None),
                Ok(Indexed::Stale {
                    indexed_revision,
                    current_revision,
                }) => (
                    CacheState::Stale {
                        indexed_revision,
                        current_revision,
                    },
                    None,
                ),
                Err(error) => degraded(error.to_string()),
            },
            |cache, revision, publication_id| {
                let token = WorkspaceReadToken {
                    token: WorkspaceTokenDomain::Index,
                    source_revision: revision.clone(),
                    publication_id,
                    provider_instance: self
                        .previews
                        .lock()
                        .expect("preview vault")
                        .instance()
                        .to_owned(),
                };
                let value = read(cache, &token).map_err(|error| error.to_string())?;
                match cache
                    .publication(revision)
                    .map_err(|error| error.to_string())?
                {
                    Indexed::Current {
                        value: observed, ..
                    } if observed == token.publication_id => Ok((token, value)),
                    _ => Err("index publication changed during projection".into()),
                }
            },
        )
    }
}
