use std::{any::Any, sync::Arc};

use futures::future::BoxFuture;

use crate::{
    cache::CachePolicy,
    model::{ActivityEntry, CollectOptions, ConnectionId, Source, SupportedOs},
    provider::{
        CredentialSetup,
        connection::Instances,
        provider_check_result::ProviderCheckResult,
        provider_config::{ConfigBinding, ProviderConfigKey},
        provider_context::ProviderContext,
    },
};

/// State shared by all connections of one kind during one collection.
pub type CollectionState = Arc<dyn Any + Send + Sync>;

/// One connection's collected entries.
pub type ConnectionEntries = (ConnectionId, Vec<ActivityEntry>);

pub trait ProviderAdapter: Send + Sync {
    fn id(&self) -> Source;
    fn supported_os(&self) -> &'static [SupportedOs];
    /// What the kind collects, in a few words, for the tile that adds it.
    fn description(&self) -> &'static str;

    /// Whether the kind can be configured more than once.
    fn instances(&self) -> Instances {
        Instances::Single
    }

    /// Created once per collection and handed to every connection of the kind, for work that
    /// spans connections, such as Git matching commits against emails found in all of them.
    fn collection_state(&self) -> Option<CollectionState> {
        None
    }

    /// Runs each time a connection of the kind settles, with the state from
    /// [`Self::collection_state`] and the entries of those that collected so far, in connection
    /// order. `complete` is false while another connection of the kind is still being checked or
    /// collected: that pass is provisional and runs again. A kind using it must not use the day
    /// cache, which would store entries before this step.
    fn finish_kind(
        &self,
        _state: Option<&CollectionState>,
        results: Vec<ConnectionEntries>,
        _complete: bool,
    ) -> Vec<ConnectionEntries> {
        results
    }

    fn config_binding(&self) -> Option<ConfigBinding> {
        None
    }

    fn credential_setup(&self) -> Option<&'static CredentialSetup> {
        None
    }

    fn config_requirements(&self) -> &'static [ProviderConfigKey] {
        match self.config_binding() {
            Some(binding) => binding.requirements,
            None => &[],
        }
    }

    /// Opt in to the per-day disk cache; meant for providers that call remote APIs.
    fn cache_policy(&self) -> Option<CachePolicy> {
        None
    }

    fn startup_check(&self, context: ProviderContext) -> BoxFuture<'_, ProviderCheckResult>;
    fn collect(
        &self,
        options: CollectOptions,
        context: ProviderContext,
    ) -> BoxFuture<'_, anyhow::Result<Vec<ActivityEntry>>>;

    /// Adds what is true now, not on the entries' days, to one connection's collected entries,
    /// such as change stacks. Runs after the day cache on every collection, so nothing it adds
    /// is cached. It keeps its own deadline and cannot fail: what it cannot find stays absent
    /// and is logged. It may change entries' fields but cannot add, drop or reorder them, and
    /// it writes them only once it is done looking (no await in between), so a caller that
    /// stops it leaves each entry as it was or as written. Runs inside the caller's future:
    /// it spawns nothing, so dropping it stops every request it sent.
    fn annotate<'a>(
        &'a self,
        _entries: &'a mut [ActivityEntry],
        _context: ProviderContext,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async {})
    }
}
