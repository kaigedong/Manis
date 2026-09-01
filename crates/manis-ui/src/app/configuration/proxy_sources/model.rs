use super::{
    Entity, ProxySourceEditorKind, RemoteSourceRefreshInterval, SubscriptionFeedback,
    SubscriptionTextInput,
};
use gpui::Subscription;

pub(in crate::app) enum ProxySourceEditorTarget {
    New { kind: ProxySourceEditorKind },
    Subscription { id: String },
    SingleNode { id: String },
}

impl ProxySourceEditorTarget {
    pub(in crate::app) const fn kind(&self) -> ProxySourceEditorKind {
        match self {
            Self::New { kind } => *kind,
            Self::Subscription { .. } => ProxySourceEditorKind::Subscription,
            Self::SingleNode { .. } => ProxySourceEditorKind::SingleNode,
        }
    }

    pub(in crate::app) fn editing_id(&self) -> Option<&str> {
        match self {
            Self::New { .. } => None,
            Self::Subscription { id } | Self::SingleNode { id } => Some(id),
        }
    }

    pub(in crate::app) fn reset(&mut self) {
        *self = Self::New { kind: self.kind() };
    }
}

pub(in crate::app) struct ProxySourceEditorState {
    pub(in crate::app) import_generation: u64,
    pub(in crate::app) input: Option<Entity<SubscriptionTextInput>>,
    pub(in crate::app) name_input: Option<Entity<SubscriptionTextInput>>,
    pub(in crate::app) target: ProxySourceEditorTarget,
    pub(in crate::app) refresh_interval: RemoteSourceRefreshInterval,
    pub(in crate::app) interval_popover: bool,
    pub(in crate::app) enabled: bool,
    pub(in crate::app) error: Option<String>,
    pub(in crate::app) feedback: SubscriptionFeedback,
    pub(in crate::app) input_events: Option<Subscription>,
}

impl ProxySourceEditorState {
    pub(in crate::app) fn is_importing(&self) -> bool {
        matches!(self.feedback, SubscriptionFeedback::Importing(_))
    }
}

impl Default for ProxySourceEditorState {
    fn default() -> Self {
        Self {
            import_generation: 0,
            input: None,
            name_input: None,
            target: ProxySourceEditorTarget::New {
                kind: ProxySourceEditorKind::default(),
            },
            refresh_interval: RemoteSourceRefreshInterval::Manual,
            interval_popover: false,
            enabled: true,
            error: None,
            feedback: SubscriptionFeedback::default(),
            input_events: None,
        }
    }
}
