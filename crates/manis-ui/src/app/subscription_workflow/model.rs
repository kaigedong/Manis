use super::{
    LoadedProvider, RemoteSourceRefreshInterval, SecretUrl, SourceKind, SourceMutation,
    StoredSubscription, SubscriptionInputError, SubscriptionPreview, SubscriptionPreviewError,
    SubscriptionStoreError, source_kind,
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(in crate::app) enum SubscriptionFeedback {
    #[default]
    Idle,
    Importing(SourceKind),
    Valid(SubscriptionPreview),
    InvalidInput(SubscriptionInputError),
    PreviewFailed(SubscriptionPreviewError),
    StoreFailed(SubscriptionStoreError),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::app) enum ImportedSubscriptionState {
    #[default]
    None,
    Pending(SourceKind),
    Refreshing(SourceKind),
    Ready(SourceKind),
    Unavailable(SourceKind, SubscriptionPreviewError),
    StoreError(SubscriptionStoreError),
    Removing(SourceKind),
}

#[derive(Clone, Debug)]
pub(in crate::app) struct ImportedSubscription {
    pub(in crate::app) id: String,
    pub(in crate::app) name: String,
    pub(in crate::app) source: SecretUrl,
    pub(in crate::app) enabled: bool,
    pub(in crate::app) state: ImportedSubscriptionState,
    pub(in crate::app) providers: Vec<LoadedProvider>,
    pub(in crate::app) generation: u64,
    pub(in crate::app) refresh_interval: RemoteSourceRefreshInterval,
    pub(in crate::app) last_successful_update_unix_secs: u64,
}

impl ImportedSubscription {
    pub(in crate::app) fn from_stored(stored: StoredSubscription) -> Self {
        let kind = source_kind(&stored.source);
        Self {
            id: stored.id,
            name: stored.name,
            source: stored.source,
            enabled: stored.enabled,
            state: if stored.enabled {
                ImportedSubscriptionState::Pending(kind)
            } else {
                ImportedSubscriptionState::None
            },
            providers: Vec::new(),
            generation: 0,
            refresh_interval: stored.refresh_interval,
            last_successful_update_unix_secs: stored.last_successful_update_unix_secs,
        }
    }
}

pub(in crate::app) fn managed_subscription_provider_index(provider: &str) -> Option<usize> {
    provider
        .strip_prefix("Subscription ")?
        .parse::<usize>()
        .ok()?
        .checked_sub(1)
}

pub(in crate::app) enum ImportSubscriptionError {
    Preview(SubscriptionPreviewError),
    Store(SubscriptionStoreError),
}

pub(in crate::app) struct SubscriptionImportRequest {
    pub(in crate::app) input: String,
    pub(in crate::app) name: String,
    pub(in crate::app) refresh_interval: RemoteSourceRefreshInterval,
    pub(in crate::app) enabled: bool,
    pub(in crate::app) editing_id: Option<String>,
    pub(in crate::app) kind: SourceKind,
}

pub(in crate::app) struct SourceLoadOutcome<T> {
    pub(in crate::app) providers: Vec<LoadedProvider>,
    pub(in crate::app) mutation: SourceMutation<T>,
}

pub(in crate::app) type SubscriptionRefreshResult =
    Result<SourceLoadOutcome<StoredSubscription>, ImportSubscriptionError>;
pub(in crate::app) type SubscriptionImportResult =
    Result<SourceLoadOutcome<StoredSubscription>, ImportSubscriptionError>;
