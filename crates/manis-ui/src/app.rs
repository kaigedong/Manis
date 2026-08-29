use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui::{
    AnyElement, Context, Div, Entity, FontWeight, IntoElement, ParentElement, Render, Role,
    Stateful, Styled, Subscription, Task, Toggled, Window, div, img, prelude::*, px,
};
use gpui_component::{
    Disableable, IconName, Selectable, Sizable,
    button::{Button, ButtonGroup, ButtonVariant, ButtonVariants},
    spinner::Spinner,
    status_bar::StatusBar,
    tab::{Tab, TabBar},
};
use manis_core::{
    CompactNavigation, KernelKind, ManagedPolicyGroup, ManagedPolicyIcon, ManagedPolicyStrategy,
    NodeIdentity, NodeWorkspaceState, PolicyCatalog, PolicyGroup, PolicyGroupId, PolicyNode,
    PolicyWorkspaceState, PrimaryWorkspace, ProxyId, ProxyMode, RoutingMode, WindowSizeClass,
};
use manis_mihomo::{Connection, ObservedRouteEvidence, RuntimeConfig};
use manis_profile::{QxRuleList, SecretUrl};

use crate::{
    assets, brand,
    components::{
        ActionRole, StatusTone, action_button, empty_state, page_heading, section_heading,
        status_badge,
    },
    core_update,
    diagnostics::{
        self, LogLevel, UiEvent, begin_operation, record_event, record_operation, trace_ui,
    },
    kernel::{self, KernelRuntime},
    localization::{CountNoun, Language, LanguagePreference, Localizer, Message, copy},
    mihomo::{
        self, ControllerRuntime, ControllerState, GeneratedProfileApply, KernelLogEntry,
        LiveRuntimeSession, LiveStreamStatus, LoadedProvider, LoadedSnapshot, ManagedRuntimeHealth,
        RemoteSourceRefreshInterval, StoredQxRuleSource, StoredSingleNode, StoredSubscription,
        SubscriptionPreviewError, SubscriptionStoreError,
    },
    rule_source::RuleDownloadError,
    subscription::{SourceKind, SubscriptionInputError, SubscriptionPreview},
    subscription_input::{SubscriptionInputChanged, SubscriptionTextInput, TextInputSpec},
    system_proxy::{ProxyPorts, SystemProxySession, TunDnsSession},
    theme::{ControlSize, LayoutMetric, Radius, Space, TextRole, Theme},
};

mod activity;
mod configuration;
mod logs;
mod nodes;
mod routing_apply;
mod stored_workspace;

use routing_apply::{
    RoutingApplyRollback, RoutingApplyState, SourceMutation, SourceRuntimeApply,
    mutate_saved_sources,
};
use stored_workspace::StoredWorkspace;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum ConfigurationSection {
    General,
    Runtime,
    #[default]
    ProxySources,
    RuleSources,
    Advanced,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum ProxySourceEditorKind {
    #[default]
    Subscription,
    SingleNode,
}

impl ConfigurationSection {
    const ALL: [Self; 5] = [
        Self::General,
        Self::Runtime,
        Self::ProxySources,
        Self::RuleSources,
        Self::Advanced,
    ];

    const fn key(self) -> &'static str {
        match self {
            Self::General => "general",
            Self::Runtime => "runtime",
            Self::ProxySources => "proxy-sources",
            Self::RuleSources => "rule-sources",
            Self::Advanced => "advanced",
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
enum SubscriptionFeedback {
    #[default]
    Idle,
    Importing(SourceKind),
    Valid(SubscriptionPreview),
    InvalidInput(SubscriptionInputError),
    PreviewFailed(SubscriptionPreviewError),
    StoreFailed(SubscriptionStoreError),
}

enum MihomoCoreUpdateOutcome {
    Installed {
        version: String,
        runtime: KernelRuntime,
        snapshot: Option<mihomo::RuntimeSnapshot>,
    },
    Failed {
        message: String,
        recovered: Option<mihomo::RuntimeSnapshot>,
    },
}

fn perform_mihomo_core_update(
    previous: &KernelRuntime,
    store_dir: &std::path::Path,
    language: Language,
    reconnect: bool,
) -> MihomoCoreUpdateOutcome {
    if let Err(message) = previous.stop_managed() {
        return MihomoCoreUpdateOutcome::Failed {
            message,
            recovered: None,
        };
    }

    let mut prepared = None;
    let install = core_update::install_latest_core_update(|| {
        let runtime =
            KernelRuntime::prepare_with_language(KernelKind::Mihomo, Some(store_dir), language)
                .map_err(|_message| core_update::CoreUpdateError::PublishFailed)?;
        let snapshot = reconnect
            .then(|| runtime.connect())
            .transpose()
            .map_err(|_error| core_update::CoreUpdateError::PublishFailed)?;
        #[cfg(target_os = "macos")]
        crate::macos_privileged::MacosPrivilegedProcessSpawner::sync_managed_core_if_available()
            .map_err(|_error| core_update::CoreUpdateError::PublishFailed)?;
        prepared = Some((runtime, snapshot));
        Ok(())
    });

    match (install, prepared) {
        (Ok(installed), Some((runtime, snapshot))) => MihomoCoreUpdateOutcome::Installed {
            version: installed.version,
            runtime,
            snapshot,
        },
        (Ok(_installed), None) => MihomoCoreUpdateOutcome::Failed {
            message: core_update::CoreUpdateError::PublishFailed
                .localized_message(language)
                .to_owned(),
            recovered: reconnect.then(|| previous.connect()).and_then(Result::ok),
        },
        (Err(error), _) => MihomoCoreUpdateOutcome::Failed {
            message: error.localized_message(language).to_owned(),
            recovered: reconnect.then(|| previous.connect()).and_then(Result::ok),
        },
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum ImportedSubscriptionState {
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
struct ImportedSubscription {
    id: String,
    name: String,
    source: SecretUrl,
    enabled: bool,
    state: ImportedSubscriptionState,
    providers: Vec<LoadedProvider>,
    generation: u64,
    refresh_interval: RemoteSourceRefreshInterval,
    last_successful_update_unix_secs: u64,
}

impl ImportedSubscription {
    fn from_stored(stored: StoredSubscription) -> Self {
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

fn managed_subscription_provider_index(provider: &str) -> Option<usize> {
    provider
        .strip_prefix("Subscription ")?
        .parse::<usize>()
        .ok()?
        .checked_sub(1)
}

enum ImportSubscriptionError {
    Preview(SubscriptionPreviewError),
    Store(SubscriptionStoreError),
}

struct SubscriptionImportRequest {
    input: String,
    name: String,
    refresh_interval: RemoteSourceRefreshInterval,
    enabled: bool,
    editing_id: Option<String>,
    kind: SourceKind,
}

type SubscriptionRefreshResult =
    Result<(Vec<LoadedProvider>, SourceMutation<StoredSubscription>), ImportSubscriptionError>;
type SubscriptionImportResult =
    Result<(SourceMutation<StoredSubscription>, Vec<LoadedProvider>), ImportSubscriptionError>;
type SingleNodeImportResult =
    Result<(SourceMutation<StoredSingleNode>, Vec<LoadedProvider>), SubscriptionStoreError>;
type QxRuleRefreshResult = Result<SourceMutation<StoredQxRuleSource>, ImportQxRuleError>;
type RoutingModeApplyResult = Result<Result<Option<()>, SubscriptionStoreError>, mihomo::LoadError>;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
enum QxRuleImportFeedback {
    #[default]
    Idle,
    Importing,
    Imported {
        rule_count: usize,
        diagnostic_count: usize,
    },
    AlreadyExists {
        source_id: String,
        rule_count: usize,
        target_policy: String,
    },
    InvalidDocument,
    DownloadFailed(RuleDownloadError),
    StoreFailed(SubscriptionStoreError),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum QxRuleEditorPopover {
    #[default]
    None,
    Target,
    Interval,
}

enum ImportQxRuleError {
    Download(RuleDownloadError),
    InvalidDocument,
    Store(SubscriptionStoreError),
}

enum ImportQxRuleSuccess {
    Imported {
        stored: StoredQxRuleSource,
        apply: SourceRuntimeApply,
    },
    AlreadyExists {
        stored: StoredQxRuleSource,
    },
    RolledBack {
        apply: SourceRuntimeApply,
        rollback_error: Option<SubscriptionStoreError>,
    },
}

type QxRuleImportResult = Result<ImportQxRuleSuccess, ImportQxRuleError>;

struct PolicyCandidateRowContext {
    policy_id: PolicyGroupId,
    policy_name: String,
    current: bool,
    manually_selectable: bool,
    selection_busy: bool,
    benchmark_state: GroupBenchmarkNodeState,
    theme: Theme,
}

#[derive(Clone)]
struct PolicySelectionRequest {
    group_id: PolicyGroupId,
    group_name: String,
    node_id: ProxyId,
    node_name: String,
}

struct PolicyNodeRowContext {
    source: String,
    selection: PolicySelectionRequest,
    current: bool,
    manually_selectable: bool,
    selection_busy: bool,
    benchmark_state: GroupBenchmarkNodeState,
    language: Language,
    theme: Theme,
}

struct OfflinePolicyCardView {
    policy: ManagedPolicyGroup,
    candidates: Vec<String>,
    selected_name: Option<String>,
    expanded: bool,
    benchmarking: bool,
}

struct PolicyListCardView {
    item: PolicyGroup,
    selected: bool,
    expanded: bool,
    icon: ManagedPolicyIcon,
    benchmark_key: String,
    benchmarking: bool,
}

struct PolicyDetailView {
    policy: PolicyGroup,
    selected_node_id: ProxyId,
    benchmark_key: String,
    benchmarkable: bool,
    benchmarking: bool,
    editable_group_id: Option<String>,
    display_icon: ManagedPolicyIcon,
}

struct PolicyBenchmarkRun {
    key: String,
    generation: u64,
    group_id: PolicyGroupId,
    group_kind: manis_core::PolicyGroupKind,
    total: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum QxRuleSourceRefreshState {
    Refreshing { generation: u64 },
    Failed { generation: u64, message: String },
}

impl QxRuleSourceRefreshState {
    fn is_refreshing(&self) -> bool {
        matches!(self, Self::Refreshing { .. })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum DueRemoteSource {
    Subscription(String),
    QxRule(String),
}

impl DueRemoteSource {
    fn scheduler_key(&self) -> String {
        match self {
            Self::Subscription(id) => format!("subscription:{id}"),
            Self::QxRule(id) => format!("qx-rule:{id}"),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum SourceRefreshSchedulerState {
    #[default]
    Stopped,
    Started,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum KernelSwitchState {
    #[default]
    Idle,
    Preparing,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
enum MihomoCoreUpdateState {
    #[default]
    Missing,
    Ready(String),
    Updating,
}

impl MihomoCoreUpdateState {
    const fn is_busy(&self) -> bool {
        matches!(self, Self::Updating)
    }
}

impl KernelSwitchState {
    const fn is_busy(self) -> bool {
        matches!(self, Self::Preparing)
    }
}

fn next_due_remote_source(
    subscriptions: &[ImportedSubscription],
    qx_rule_sources: &[StoredQxRuleSource],
    retry_not_before: &BTreeMap<String, u64>,
    now_unix_secs: u64,
) -> Option<DueRemoteSource> {
    subscriptions
        .iter()
        .find(|source| {
            source.enabled
                && !matches!(
                    source.state,
                    ImportedSubscriptionState::None
                        | ImportedSubscriptionState::Pending(_)
                        | ImportedSubscriptionState::Refreshing(_)
                        | ImportedSubscriptionState::Removing(_)
                )
                && source
                    .refresh_interval
                    .is_due(source.last_successful_update_unix_secs, now_unix_secs)
                && retry_not_before
                    .get(&DueRemoteSource::Subscription(source.id.clone()).scheduler_key())
                    .is_none_or(|retry_at| now_unix_secs >= *retry_at)
        })
        .map(|source| DueRemoteSource::Subscription(source.id.clone()))
        .or_else(|| {
            qx_rule_sources
                .iter()
                .find(|source| {
                    source.enabled
                        && source
                            .refresh_interval
                            .is_due(source.last_successful_update_unix_secs, now_unix_secs)
                        && retry_not_before
                            .get(&DueRemoteSource::QxRule(source.id.clone()).scheduler_key())
                            .is_none_or(|retry_at| now_unix_secs >= *retry_at)
                })
                .map(|source| DueRemoteSource::QxRule(source.id.clone()))
        })
}

fn source_kind(subscription: &SecretUrl) -> SourceKind {
    if subscription.is_https() {
        SourceKind::HttpsSubscription
    } else {
        SourceKind::HttpSubscription
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum PolicyCandidateMatcherKind {
    #[default]
    All,
    NameContains,
    Explicit,
}

#[derive(Clone, Debug)]
struct ManagedPolicyDraft {
    editing_id: Option<String>,
    icon: ManagedPolicyIcon,
    strategy: ManagedPolicyStrategy,
    test_interval_secs: u32,
    matcher_kind: PolicyCandidateMatcherKind,
    explicit_members: BTreeSet<NodeIdentity>,
}

// Policy editor design contract — QX-inspired, adapted to the Manis desktop shell.
// THESIS: creating a policy is one focused task, never a form wedged beside the policy list.
// OWN-WORLD: quiet grouped rows, white editing surfaces, teal actions, and restrained dividers.
// STORY: choose a policy type, name it, define its node scope, then save with confidence.
// FIRST VIEWPORT: editor chrome above one narrow settings column; menus stay anchored to each row.
// FORM: one page with contextual popovers; unreviewed and undocumented is unfinished.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PolicyEditorPopover {
    Strategy,
    Icon,
    CandidateMode,
    CandidateNodes,
    Interval,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ManualRulePopover {
    Kind(usize),
    Target,
}

struct ManualRuleConditionEditor {
    kind: crate::manual_rule::ManualRuleKind,
    input: Entity<SubscriptionTextInput>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum ManualRuleEditorState {
    #[default]
    Closed,
    Creating,
    Editing(usize),
}

impl ManualRuleEditorState {
    const fn is_open(self) -> bool {
        !matches!(self, Self::Closed)
    }

    const fn editing_index(self) -> Option<usize> {
        match self {
            Self::Editing(index) => Some(index),
            Self::Closed | Self::Creating => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum PolicyDetailTab {
    #[default]
    Nodes,
    Settings,
}

impl PolicyDetailTab {
    const fn index(self) -> usize {
        match self {
            Self::Nodes => 0,
            Self::Settings => 1,
        }
    }

    const fn from_index(index: usize) -> Self {
        match index {
            1 => Self::Settings,
            _ => Self::Nodes,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct GroupBenchmarkSummary {
    total: usize,
    succeeded: usize,
    failed: usize,
    minimum_ms: Option<u16>,
    maximum_ms: Option<u16>,
    average_ms: Option<u16>,
}

impl GroupBenchmarkSummary {
    fn from_delays(total: usize, delays: impl IntoIterator<Item = u16>) -> Self {
        let delays = delays
            .into_iter()
            .filter(|delay| *delay > 0)
            .collect::<Vec<_>>();
        let succeeded = delays.len().min(total);
        let sum = delays.iter().map(|delay| u64::from(*delay)).sum::<u64>();
        let divisor = u64::try_from(delays.len()).unwrap_or(1);
        Self {
            total,
            succeeded,
            failed: total.saturating_sub(succeeded),
            minimum_ms: delays.iter().copied().min(),
            maximum_ms: delays.iter().copied().max(),
            average_ms: (!delays.is_empty()).then(|| {
                let rounded = (sum + divisor / 2) / divisor;
                u16::try_from(rounded).unwrap_or(u16::MAX)
            }),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
enum GroupBenchmarkState {
    #[default]
    Idle,
    Running {
        generation: u64,
        results: BTreeMap<String, Option<u16>>,
    },
    Complete {
        generation: u64,
        summary: GroupBenchmarkSummary,
        delays: BTreeMap<String, u16>,
    },
    Failed {
        generation: u64,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GroupBenchmarkNodeState {
    Idle,
    Pending,
    Measured(u16),
    Failed,
}

type GroupBenchmarkProgressQueue = Arc<Mutex<VecDeque<(String, Option<u16>)>>>;

impl GroupBenchmarkState {
    fn running(generation: u64) -> Self {
        Self::Running {
            generation,
            results: BTreeMap::new(),
        }
    }

    fn is_running(&self) -> bool {
        matches!(self, Self::Running { .. })
    }

    fn record(&mut self, generation: u64, name: &str, delay: Option<u16>) -> bool {
        let Self::Running {
            generation: active,
            results,
        } = self
        else {
            return false;
        };
        if *active != generation {
            return false;
        }
        results.insert(name.to_owned(), delay.filter(|delay| *delay > 0));
        true
    }

    fn node_state(&self, name: &str) -> GroupBenchmarkNodeState {
        match self {
            Self::Idle => GroupBenchmarkNodeState::Idle,
            Self::Running { results, .. } => match results.get(name) {
                Some(Some(delay)) => GroupBenchmarkNodeState::Measured(*delay),
                Some(None) => GroupBenchmarkNodeState::Failed,
                None => GroupBenchmarkNodeState::Pending,
            },
            Self::Complete { delays, .. } => {
                delays.get(name).copied().filter(|delay| *delay > 0).map_or(
                    GroupBenchmarkNodeState::Failed,
                    GroupBenchmarkNodeState::Measured,
                )
            }
            Self::Failed { .. } => GroupBenchmarkNodeState::Failed,
        }
    }

    fn complete(&mut self, generation: u64, total: usize, delays: BTreeMap<String, u16>) -> bool {
        if !matches!(self, Self::Running { generation: current, .. } if *current == generation) {
            return false;
        }
        let summary = GroupBenchmarkSummary::from_delays(total, delays.values().copied());
        *self = Self::Complete {
            generation,
            summary,
            delays,
        };
        true
    }

    fn fail(&mut self, generation: u64) -> bool {
        if !matches!(self, Self::Running { generation: current, .. } if *current == generation) {
            return false;
        }
        *self = Self::Failed { generation };
        true
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
enum ManagedPolicyRuntimeState {
    #[default]
    LocalOnly,
    Ready {
        generation: u64,
        current: Option<String>,
        candidates: BTreeSet<String>,
    },
    Selecting {
        generation: u64,
        current: Option<String>,
        candidates: BTreeSet<String>,
        pending: String,
    },
}

impl ManagedPolicyRuntimeState {
    fn begin_selection(&mut self, generation: u64, selected: &str) -> bool {
        let Self::Ready {
            current,
            candidates,
            ..
        } = self
        else {
            return false;
        };
        if !candidates.contains(selected) {
            return false;
        }
        *self = Self::Selecting {
            generation,
            current: current.clone(),
            candidates: candidates.clone(),
            pending: selected.to_owned(),
        };
        true
    }
}

struct ProxySourceEditorState {
    input: Option<Entity<SubscriptionTextInput>>,
    name_input: Option<Entity<SubscriptionTextInput>>,
    subscription_source_id: Option<String>,
    single_node_source_id: Option<String>,
    kind: ProxySourceEditorKind,
    refresh_interval: RemoteSourceRefreshInterval,
    interval_popover: bool,
    enabled: bool,
    error: Option<String>,
    feedback: SubscriptionFeedback,
    input_events: Option<Subscription>,
}

impl Default for ProxySourceEditorState {
    fn default() -> Self {
        Self {
            input: None,
            name_input: None,
            subscription_source_id: None,
            single_node_source_id: None,
            kind: ProxySourceEditorKind::default(),
            refresh_interval: RemoteSourceRefreshInterval::Manual,
            interval_popover: false,
            enabled: true,
            error: None,
            feedback: SubscriptionFeedback::default(),
            input_events: None,
        }
    }
}

#[derive(Default)]
struct WorkspaceInputs {
    qx_rule: Option<Entity<SubscriptionTextInput>>,
    qx_rule_events: Option<Subscription>,
    policy_group_name: Option<Entity<SubscriptionTextInput>>,
    policy_group_filter: Option<Entity<SubscriptionTextInput>>,
    activity_search: Option<Entity<SubscriptionTextInput>>,
    activity_search_events: Option<Subscription>,
    logs_search: Option<Entity<SubscriptionTextInput>>,
    logs_search_events: Option<Subscription>,
}

#[derive(Default)]
struct LifecycleSubscriptions {
    window_bounds: Option<Subscription>,
    app_lifecycle: Option<Subscription>,
}

struct RuleSourceState {
    sources: Vec<StoredQxRuleSource>,
    group_order: Vec<String>,
    feedback: QxRuleImportFeedback,
    target_policy: String,
    editor_source_id: Option<String>,
    editor_refresh_interval: RemoteSourceRefreshInterval,
    editor_popover: QxRuleEditorPopover,
    import_generation: u64,
    refreshes: BTreeMap<String, QxRuleSourceRefreshState>,
    target_updates: BTreeMap<String, u64>,
    target_popover: Option<String>,
    refresh_retry_not_before: BTreeMap<String, u64>,
    refresh_scheduler: SourceRefreshSchedulerState,
}

impl RuleSourceState {
    fn restored(
        sources: Vec<StoredQxRuleSource>,
        group_order: Vec<String>,
        target_policy: String,
    ) -> Self {
        Self {
            sources,
            group_order,
            feedback: QxRuleImportFeedback::Idle,
            target_policy,
            editor_source_id: None,
            editor_refresh_interval: RemoteSourceRefreshInterval::Manual,
            editor_popover: QxRuleEditorPopover::None,
            import_generation: 0,
            refreshes: BTreeMap::new(),
            target_updates: BTreeMap::new(),
            target_popover: None,
            refresh_retry_not_before: BTreeMap::new(),
            refresh_scheduler: SourceRefreshSchedulerState::Stopped,
        }
    }
}

struct ManagedPolicyState {
    groups: Vec<ManagedPolicyGroup>,
    node_selections: mihomo::NodeSelectionPreferences,
    draft: Option<ManagedPolicyDraft>,
    editor_popover: Option<PolicyEditorPopover>,
    pending_benchmark_name: Option<String>,
    benchmarks: BTreeMap<String, GroupBenchmarkState>,
    benchmark_generation: u64,
    active_benchmark_generation: Option<u64>,
    runtime_states: BTreeMap<String, ManagedPolicyRuntimeState>,
    runtime_generation: u64,
}

impl ManagedPolicyState {
    fn restored(
        groups: Vec<ManagedPolicyGroup>,
        node_selections: mihomo::NodeSelectionPreferences,
    ) -> Self {
        Self {
            groups,
            node_selections,
            draft: None,
            editor_popover: None,
            pending_benchmark_name: None,
            benchmarks: BTreeMap::new(),
            benchmark_generation: 0,
            active_benchmark_generation: None,
            runtime_states: BTreeMap::new(),
            runtime_generation: 0,
        }
    }
}

pub struct ManisApp {
    localizer: Localizer,
    primary_workspace: PrimaryWorkspace,
    configuration_section: ConfigurationSection,
    configuration_add_section: Option<ConfigurationSection>,
    node_workspace: NodeWorkspaceState,
    workspace: PolicyWorkspaceState,
    expanded_policy_group: Option<PolicyGroupId>,
    policy_detail_tab: PolicyDetailTab,
    catalog: Option<PolicyCatalog>,
    runtime: KernelRuntime,
    kernel_switch_state: KernelSwitchState,
    mihomo_core_update_state: MihomoCoreUpdateState,
    controller: ControllerState,
    observed_routes: Vec<ObservedRouteEvidence>,
    source_providers: Vec<LoadedProvider>,
    subscription_preview_providers: Vec<LoadedProvider>,
    subscription_preview_generation: u64,
    subscription_store_dir: Option<PathBuf>,
    imported_subscriptions: Vec<ImportedSubscription>,
    saved_single_nodes: Vec<StoredSingleNode>,
    rule_sources: RuleSourceState,
    managed_policies: ManagedPolicyState,
    source_store_error: Option<SubscriptionStoreError>,
    proxy_mode: ProxyMode,
    proxy_mode_busy: Option<ProxyMode>,
    routing_mode: RoutingMode,
    routing_mode_busy: Option<RoutingMode>,
    routing_apply_state: RoutingApplyState,
    global_selection_busy: Option<String>,
    policy_selection_busy: Option<String>,
    proxy_runtime: RuntimeConfig,
    system_proxy: Arc<Mutex<SystemProxySession>>,
    tun_dns: Arc<Mutex<TunDnsSession>>,
    active_connections: Vec<Connection>,
    live_runtime: Option<LiveRuntimeSession>,
    live_generation: u64,
    managed_health_tick: u8,
    live_status: LiveStreamStatus,
    kernel_logs: VecDeque<KernelLogEntry>,
    dropped_kernel_logs: u64,
    dark: bool,
    status: String,
    proxy_source_editor: ProxySourceEditorState,
    inputs: WorkspaceInputs,
    manual_rules: Vec<crate::manual_rule::ManualRule>,
    manual_rule_conditions: Vec<ManualRuleConditionEditor>,
    manual_rule_condition_count: usize,
    manual_rule_target: String,
    manual_rule_editor_state: ManualRuleEditorState,
    manual_rule_popover: Option<ManualRulePopover>,
    manual_rule_error: Option<crate::manual_rule::ManualRuleError>,
    lifecycle_subscriptions: LifecycleSubscriptions,
}

impl ManisApp {
    #[must_use]
    pub fn new() -> Self {
        let store = mihomo::imported_subscription_store_dir();
        let store = store.ok();
        diagnostics::initialize(store.as_deref().and_then(std::path::Path::parent));
        #[cfg(not(test))]
        match core_update::install_bundled_seed_if_missing() {
            Ok(core_update::SeedInstallOutcome::Installed(path)) => record_event(
                LogLevel::Info,
                "core.seed.installed",
                format!("path={}", path.display()),
            ),
            Ok(
                core_update::SeedInstallOutcome::AlreadyPresent(_)
                | core_update::SeedInstallOutcome::MissingSeed { .. },
            ) => {}
            Err(error) => record_event(LogLevel::Warn, "core.seed.failed", error.to_string()),
        }
        let language = Localizer::load(store.as_deref()).language();
        let runtime = KernelRuntime::configured(store.as_deref(), language);
        diagnostics::record_event(
            LogLevel::Info,
            "app.runtime.prepared",
            format!(
                "kernel={} ownership={} profile={}",
                runtime.kind().display_name(),
                if matches!(&*runtime, ControllerRuntime::Managed { .. }) {
                    "managed"
                } else if runtime.is_fixture() {
                    "fixture"
                } else {
                    "invalid"
                },
                runtime.profile_source().diagnostic_key()
            ),
        );
        let app = Self::with_runtime_and_store(runtime, store);
        #[cfg(not(test))]
        let app = {
            let mut app = app;
            app.recover_stale_system_proxy();
            app
        };
        app
    }

    #[must_use]
    pub fn new_with_lifecycle(cx: &mut Context<Self>) -> Self {
        let mut app = Self::new();
        app.lifecycle_subscriptions.app_lifecycle = Some(cx.on_app_quit(Self::shutdown_for_quit));
        app.restore_imported_subscriptions(cx);
        if matches!(app.mihomo_core_update_state, MihomoCoreUpdateState::Missing) {
            app.update_mihomo_core(cx);
        }
        app
    }

    #[must_use]
    #[cfg(any(test, feature = "snapshot-fixtures"))]
    #[doc(hidden)]
    pub fn with_fixture_controller(endpoint: impl Into<String>) -> Self {
        Self::with_runtime_and_store(
            KernelRuntime::mihomo(ControllerRuntime::Fixture {
                endpoint: endpoint.into(),
            }),
            None,
        )
    }

    /// Creates a deterministic fixture backed by an explicit subscription store.
    #[must_use]
    #[cfg(any(test, feature = "snapshot-fixtures"))]
    #[doc(hidden)]
    pub fn with_fixture_controller_and_subscription_store(
        endpoint: impl Into<String>,
        subscription_store_dir: PathBuf,
    ) -> Self {
        Self::with_runtime_and_store(
            KernelRuntime::mihomo(ControllerRuntime::Fixture {
                endpoint: endpoint.into(),
            }),
            Some(subscription_store_dir),
        )
    }

    fn with_runtime_and_store(
        runtime: KernelRuntime,
        subscription_store_dir: Option<PathBuf>,
    ) -> Self {
        let localizer = Localizer::load(subscription_store_dir.as_deref());
        let language = localizer.language();
        let stored_workspace = StoredWorkspace::load(subscription_store_dir.as_ref());
        let status = Self::restored_workspace_status(
            &runtime,
            subscription_store_dir.as_ref(),
            &stored_workspace,
            language,
        );
        let StoredWorkspace {
            imported_subscriptions,
            saved_single_nodes,
            qx_rule_sources,
            routing_rule_group_order,
            collapsed_groups,
            managed_policy_groups,
            node_selection_preferences,
            routing_mode,
            error: source_store_error,
        } = stored_workspace;
        let default_rule_target = managed_policy_groups
            .first()
            .map_or_else(|| "DIRECT".to_owned(), |group| group.name.clone());
        let mut node_workspace = NodeWorkspaceState::default();
        node_workspace.replace_collapsed_groups(collapsed_groups.iter().map(String::as_str));
        Self {
            localizer,
            primary_workspace: PrimaryWorkspace::default(),
            configuration_section: ConfigurationSection::default(),
            configuration_add_section: None,
            node_workspace,
            workspace: PolicyWorkspaceState::default(),
            expanded_policy_group: None,
            policy_detail_tab: PolicyDetailTab::default(),
            catalog: None,
            runtime,
            kernel_switch_state: KernelSwitchState::Idle,
            mihomo_core_update_state: Self::initial_mihomo_core_update_state(),
            controller: ControllerState::Disconnected,
            observed_routes: Vec::new(),
            source_providers: Vec::new(),
            subscription_preview_providers: Vec::new(),
            subscription_preview_generation: 0,
            subscription_store_dir,
            imported_subscriptions,
            saved_single_nodes,
            rule_sources: RuleSourceState::restored(
                qx_rule_sources,
                routing_rule_group_order,
                default_rule_target.clone(),
            ),
            managed_policies: ManagedPolicyState::restored(
                managed_policy_groups,
                node_selection_preferences,
            ),
            source_store_error,
            proxy_mode: ProxyMode::Off,
            proxy_mode_busy: None,
            routing_mode,
            routing_mode_busy: None,
            routing_apply_state: RoutingApplyState::default(),
            global_selection_busy: None,
            policy_selection_busy: None,
            proxy_runtime: RuntimeConfig::default(),
            system_proxy: Arc::new(Mutex::new(SystemProxySession::default())),
            tun_dns: Arc::new(Mutex::new(TunDnsSession::default())),
            active_connections: Vec::new(),
            live_runtime: None,
            live_generation: 0,
            managed_health_tick: 0,
            live_status: LiveStreamStatus::default(),
            kernel_logs: VecDeque::with_capacity(500),
            dropped_kernel_logs: 0,
            dark: false,
            status,
            proxy_source_editor: ProxySourceEditorState::default(),
            inputs: WorkspaceInputs::default(),
            manual_rules: Vec::new(),
            manual_rule_conditions: Vec::new(),
            manual_rule_condition_count: 1,
            manual_rule_target: default_rule_target,
            manual_rule_editor_state: ManualRuleEditorState::Closed,
            manual_rule_popover: None,
            manual_rule_error: None,
            lifecycle_subscriptions: LifecycleSubscriptions::default(),
        }
    }

    fn restored_workspace_status(
        runtime: &KernelRuntime,
        directory: Option<&PathBuf>,
        workspace: &StoredWorkspace,
        language: Language,
    ) -> String {
        let Some(directory) = directory else {
            return runtime.initial_status_in(language);
        };
        let has_saved_configuration = !workspace.imported_subscriptions.is_empty()
            || !workspace.saved_single_nodes.is_empty()
            || !workspace.qx_rule_sources.is_empty()
            || !workspace.managed_policy_groups.is_empty()
            || workspace.routing_mode != RoutingMode::Rule;
        if !has_saved_configuration {
            return runtime.initial_status_in(language);
        }
        match runtime.apply_saved_sources(directory) {
            Ok(GeneratedProfileApply::Updated) => language
                .localized(copy::app::SAVED_SOURCES_ARE_READY)
                .to_owned(),
            Ok(GeneratedProfileApply::Restarted) => language
                .localized(copy::app::SAVED_SOURCES_ARE_READY_AND_MIHOMO_WAS_RESTARTED)
                .to_owned(),
            Err(error) => format!(
                "{}{error}",
                language
                    .localized(copy::app::SAVED_SOURCES_WERE_LOADED_BUT_THE_CHANGES_COULD_NOT_BE)
            ),
        }
    }

    fn initial_mihomo_core_update_state() -> MihomoCoreUpdateState {
        #[cfg(test)]
        {
            MihomoCoreUpdateState::Missing
        }
        #[cfg(not(test))]
        {
            core_update::managed_core_binary_path()
                .map_or(MihomoCoreUpdateState::Missing, |_path| {
                    MihomoCoreUpdateState::Ready(String::new())
                })
        }
    }

    pub(crate) fn attach_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.workspace.resize(window.viewport_size().width.as_f32());
        self.sync_window_inputs(window, cx);
        self.ensure_source_refresh_scheduler(cx);
        let _previous =
            self.lifecycle_subscriptions
                .window_bounds
                .replace(cx.observe_window_bounds(window, |this, window, cx| {
                    this.workspace.resize(window.viewport_size().width.as_f32());
                    cx.notify();
                }));
    }

    fn sync_window_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let theme = self.theme();
        self.ensure_subscription_input(theme, window, cx);
        self.ensure_qx_rule_input(theme, window, cx);
        self.ensure_policy_group_inputs(theme, window, cx);
        self.ensure_runtime_search_inputs(theme, window, cx);
    }

    fn shutdown_for_quit(&mut self, _cx: &mut Context<Self>) -> Task<()> {
        let language = self.language();
        let operation = begin_operation(
            "app.shutdown.requested",
            format!("proxy_mode={:?}", self.proxy_mode),
        );
        if self.proxy_mode == ProxyMode::Tun {
            match self.runtime.set_tun_enabled(false) {
                Ok(()) => record_operation(
                    operation,
                    LogLevel::Info,
                    "tun.shutdown.succeeded",
                    "controller accepted disable request",
                ),
                Err(error) => record_operation(
                    operation,
                    LogLevel::Error,
                    "tun.shutdown.failed",
                    error.to_string(),
                ),
            }
        }
        if let Ok(mut system) = self.system_proxy.lock()
            && let Err(error) = system.shutdown_with_language(language)
        {
            record_operation(
                operation,
                LogLevel::Error,
                "system_proxy.shutdown.failed",
                error.to_string(),
            );
        }
        if let Ok(mut dns) = self.tun_dns.lock()
            && let Err(error) = dns.shutdown_with_language(language)
        {
            record_operation(
                operation,
                LogLevel::Error,
                "tun_dns.shutdown.failed",
                error.to_string(),
            );
        }
        match self.runtime.stop_managed() {
            Ok(()) => record_operation(
                operation,
                LogLevel::Info,
                "kernel.shutdown.succeeded",
                "managed kernel stop completed",
            ),
            Err(error) => {
                record_operation(operation, LogLevel::Error, "kernel.shutdown.failed", error);
            }
        }
        Task::ready(())
    }

    #[cfg(not(test))]
    fn recover_stale_system_proxy(&mut self) {
        let language = self.language();
        match self.system_proxy.lock() {
            Ok(mut system) => {
                if let Err(error) = system.recover_stale_with_language(language) {
                    self.status = format!(
                        "{}{error}",
                        language.localized(copy::app::SYSTEM_PROXY_RECOVERY_NEEDS_ATTENTION)
                    );
                }
            }
            Err(_poisoned) => {
                language
                    .localized(copy::app::SYSTEM_PROXY_RECOVERY_STATE_IS_UNAVAILABLE)
                    .clone_into(&mut self.status);
            }
        }
        match self.tun_dns.lock() {
            Ok(mut dns) => {
                if let Err(error) = dns.recover_stale_with_language(language) {
                    self.status = format!(
                        "{}{error}",
                        language.localized(copy::app::TUN_DNS_RECOVERY_NEEDS_ATTENTION)
                    );
                }
            }
            Err(_poisoned) => {
                language
                    .localized(copy::app::TUN_DNS_RECOVERY_STATE_IS_UNAVAILABLE)
                    .clone_into(&mut self.status);
            }
        }
    }

    #[must_use]
    pub(super) fn language(&self) -> Language {
        self.localizer.language()
    }

    #[must_use]
    fn language_preference(&self) -> LanguagePreference {
        self.localizer.preference()
    }

    fn ensure_subscription_input(
        &mut self,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let language = self.language();
        if let Some(input) = self.proxy_source_editor.input.as_ref() {
            input.update(cx, |input, cx| {
                input.set_theme(theme, self.dark, cx);
                input.set_language(language, cx);
            });
            if let Some(name_input) = self.proxy_source_editor.name_input.as_ref() {
                name_input.update(cx, |input, cx| {
                    input.set_theme(theme, self.dark, cx);
                    input.set_placeholder(
                        language.localized(copy::common::FOR_EXAMPLE_MY_SUBSCRIPTION),
                        cx,
                    );
                });
            }
            return;
        }

        let input = cx.new(|cx| {
            SubscriptionTextInput::new_with_language(language, theme, self.dark, window, cx)
        });
        let events = cx.subscribe(&input, |this, _input, _: &SubscriptionInputChanged, cx| {
            if this.proxy_source_editor.feedback != SubscriptionFeedback::Idle {
                this.proxy_source_editor.feedback = SubscriptionFeedback::Idle;
                cx.notify();
            }
        });
        self.proxy_source_editor.input = Some(input);
        self.proxy_source_editor.name_input = Some(cx.new(|cx| {
            SubscriptionTextInput::new_field(
                TextInputSpec::new(
                    "subscription-name-input",
                    language.localized(copy::common::FOR_EXAMPLE_MY_SUBSCRIPTION),
                    96,
                    theme,
                    self.dark,
                ),
                window,
                cx,
            )
        }));
        self.proxy_source_editor.input_events = Some(events);
        self.restore_imported_subscriptions(cx);
    }

    fn ensure_qx_rule_input(&mut self, theme: Theme, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(input) = self.inputs.qx_rule.as_ref() {
            input.update(cx, |input, cx| input.set_theme(theme, self.dark, cx));
            return;
        }
        let input = cx.new(|cx| {
            SubscriptionTextInput::new_field(
                TextInputSpec::new(
                    "qx-rule-url-input",
                    "https://example.com/rules.list",
                    16 * 1024,
                    theme,
                    self.dark,
                ),
                window,
                cx,
            )
        });
        let events = cx.subscribe(&input, |this, _input, _: &SubscriptionInputChanged, cx| {
            if this.rule_sources.feedback != QxRuleImportFeedback::Idle {
                this.rule_sources.feedback = QxRuleImportFeedback::Idle;
                cx.notify();
            }
        });
        self.inputs.qx_rule = Some(input);
        self.inputs.qx_rule_events = Some(events);
    }

    fn ensure_policy_group_inputs(
        &mut self,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let language = self.language();
        for input in [
            self.inputs.policy_group_name.as_ref(),
            self.inputs.policy_group_filter.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            input.update(cx, |input, cx| input.set_theme(theme, self.dark, cx));
        }
        if self.inputs.policy_group_name.is_none() {
            self.inputs.policy_group_name = Some(cx.new(|cx| {
                SubscriptionTextInput::new_field(
                    TextInputSpec::new(
                        "policy-group-name-input",
                        language.localized(copy::common::FOR_EXAMPLE_HONG_KONG_AUTO),
                        96,
                        theme,
                        self.dark,
                    ),
                    window,
                    cx,
                )
            }));
        }
        if self.inputs.policy_group_filter.is_none() {
            self.inputs.policy_group_filter = Some(cx.new(|cx| {
                SubscriptionTextInput::new_field(
                    TextInputSpec::new(
                        "policy-group-filter-input",
                        language.localized(copy::common::FOR_EXAMPLE_HONG_KONG),
                        256,
                        theme,
                        self.dark,
                    ),
                    window,
                    cx,
                )
            }));
        }
    }

    fn ensure_runtime_search_inputs(
        &mut self,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let language = self.language();
        if let Some(input) = self.inputs.activity_search.as_ref() {
            input.update(cx, |input, cx| {
                input.set_theme(theme, self.dark, cx);
                input.set_placeholder(
                    language.localized(copy::app::FILTER_BY_TARGET_PROCESS_RULE_OR_ROUTE),
                    cx,
                );
            });
        } else {
            let input = cx.new(|cx| {
                SubscriptionTextInput::new_field(
                    TextInputSpec::new(
                        "activity-search-input",
                        language.localized(copy::app::FILTER_BY_TARGET_PROCESS_RULE_OR_ROUTE),
                        256,
                        theme,
                        self.dark,
                    ),
                    window,
                    cx,
                )
            });
            let events = cx.subscribe(&input, |_this, _input, _: &SubscriptionInputChanged, cx| {
                cx.notify();
            });
            self.inputs.activity_search = Some(input);
            self.inputs.activity_search_events = Some(events);
        }

        if let Some(input) = self.inputs.logs_search.as_ref() {
            input.update(cx, |input, cx| {
                input.set_theme(theme, self.dark, cx);
                input.set_placeholder(
                    language.localized(copy::app::SEARCH_OPERATIONS_ERRORS_OR_LOG_LEVELS),
                    cx,
                );
            });
        } else {
            let input = cx.new(|cx| {
                SubscriptionTextInput::new_field(
                    TextInputSpec::new(
                        "logs-search-input",
                        language.localized(copy::app::SEARCH_OPERATIONS_ERRORS_OR_LOG_LEVELS),
                        256,
                        theme,
                        self.dark,
                    ),
                    window,
                    cx,
                )
            });
            let events = cx.subscribe(&input, |_this, _input, _: &SubscriptionInputChanged, cx| {
                cx.notify();
            });
            self.inputs.logs_search = Some(input);
            self.inputs.logs_search_events = Some(events);
        }
    }

    fn import_remote_subscription(
        &mut self,
        request: SubscriptionImportRequest,
        cx: &mut Context<Self>,
    ) {
        let SubscriptionImportRequest {
            input,
            name,
            refresh_interval,
            enabled,
            editing_id,
            kind,
        } = request;
        let language = self.language();
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            self.proxy_source_editor.feedback =
                SubscriptionFeedback::StoreFailed(SubscriptionStoreError::DataDirectoryUnavailable);
            language
                .localized(copy::app::THE_SUBSCRIPTION_STORAGE_LOCATION_IS_UNAVAILABLE)
                .clone_into(&mut self.status);
            trace_ui(UiEvent::SourceImportFailed);
            cx.notify();
            return;
        };
        self.subscription_preview_generation = self.subscription_preview_generation.wrapping_add(1);
        let generation = self.subscription_preview_generation;
        self.proxy_source_editor.feedback = SubscriptionFeedback::Importing(kind);
        language
            .localized(copy::app::VALIDATING_NODES_AND_IMPORTING_SUBSCRIPTION)
            .clone_into(&mut self.status);
        trace_ui(UiEvent::SourceImportStarted);
        if let Some(input) = self.proxy_source_editor.input.as_ref() {
            input.update(cx, |input, cx| input.set_enabled(false, cx));
        }
        if let Some(input) = self.proxy_source_editor.name_input.as_ref() {
            input.update(cx, |input, cx| input.set_enabled(false, cx));
        }

        let executor = cx.background_executor().clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    let providers = mihomo::preview_subscription(&input)
                        .map_err(ImportSubscriptionError::Preview)?;
                    let transaction = mutate_saved_sources(&runtime, &store_dir, || {
                        let mut subscription = if let Some(id) = editing_id.as_deref() {
                            mihomo::update_subscription_source_in(
                                &store_dir,
                                id,
                                &input,
                                &name,
                                refresh_interval,
                                enabled,
                            )
                        } else {
                            mihomo::save_subscription_source_with_options_in(
                                &store_dir,
                                &input,
                                &name,
                                refresh_interval,
                                enabled,
                            )
                        }?;
                        let proxy_nameservers =
                            mihomo::discover_subscription_proxy_nameservers(&subscription.source);
                        if !proxy_nameservers.is_empty() {
                            subscription = mihomo::update_subscription_source_proxy_nameservers_in(
                                &store_dir,
                                &subscription.id,
                                &proxy_nameservers,
                            )?;
                        }
                        Ok(subscription)
                    })
                    .map_err(ImportSubscriptionError::Store)?;
                    Ok::<_, ImportSubscriptionError>((transaction, providers))
                })
                .await;
            this.update(cx, |this, cx| {
                if this.subscription_preview_generation != generation {
                    return;
                }
                if let Some(input) = this.proxy_source_editor.input.as_ref() {
                    input.update(cx, |input, cx| input.set_enabled(true, cx));
                }
                if let Some(input) = this.proxy_source_editor.name_input.as_ref() {
                    input.update(cx, |input, cx| input.set_enabled(true, cx));
                }
                this.finish_subscription_import(generation, kind, result, cx);
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn finish_subscription_import(
        &mut self,
        generation: u64,
        kind: SourceKind,
        result: SubscriptionImportResult,
        cx: &mut Context<Self>,
    ) {
        let language = self.language();
        match result {
            Ok((transaction, providers)) if transaction.value.is_some() => {
                let subscription = transaction.value.expect("checked committed mutation");
                let node_count: usize = providers.iter().map(|provider| provider.nodes.len()).sum();
                let provider_count = providers.len();
                self.merge_imported_subscription(subscription, &providers, generation, kind);
                self.subscription_preview_providers = providers;
                self.proxy_source_editor.feedback = SubscriptionFeedback::Idle;
                if let Some(input) = self.proxy_source_editor.input.as_ref() {
                    input.update(cx, SubscriptionTextInput::clear_without_event);
                }
                if let Some(input) = self.proxy_source_editor.name_input.as_ref() {
                    input.update(cx, SubscriptionTextInput::clear_without_event);
                }
                self.configuration_add_section = None;
                self.proxy_source_editor.subscription_source_id = None;
                self.proxy_source_editor.error = None;
                transaction.apply.reconcile_proxy_mode(&mut self.proxy_mode);
                self.status = copy::app::subscription_imported(
                    language,
                    self.imported_subscriptions.len(),
                    provider_count,
                    node_count,
                    &transaction.apply.status_suffix(language),
                );
                trace_ui(UiEvent::SourceImportSucceeded);
            }
            Ok((transaction, _providers)) => {
                self.proxy_source_editor.feedback =
                    SubscriptionFeedback::StoreFailed(SubscriptionStoreError::StoreUnavailable);
                self.status = format!(
                    "{}{}",
                    language.localized(copy::app::COULD_NOT_SAVE_SUBSCRIPTION),
                    transaction.apply.status_suffix_after_rollback_attempt(
                        language,
                        transaction.rollback_error.as_ref(),
                    )
                );
                trace_ui(UiEvent::SourceImportFailed);
            }
            Err(ImportSubscriptionError::Preview(error)) => {
                self.proxy_source_editor.feedback = SubscriptionFeedback::PreviewFailed(error);
                self.status = format!(
                    "{}{error}",
                    language.localized(copy::app::SUBSCRIPTION_IMPORT_FAILED)
                );
                trace_ui(UiEvent::SourceImportFailed);
            }
            Err(ImportSubscriptionError::Store(error)) => {
                self.proxy_source_editor.feedback = SubscriptionFeedback::StoreFailed(error);
                self.status = format!(
                    "{}{error}",
                    language.localized(copy::app::COULD_NOT_SAVE_SUBSCRIPTION_2)
                );
                trace_ui(UiEvent::SourceImportFailed);
            }
        }
        cx.notify();
    }

    fn merge_imported_subscription(
        &mut self,
        subscription: StoredSubscription,
        providers: &[LoadedProvider],
        generation: u64,
        kind: SourceKind,
    ) {
        if let Some(existing) = self
            .imported_subscriptions
            .iter_mut()
            .find(|existing| existing.id == subscription.id)
        {
            existing.name.clone_from(&subscription.name);
            existing.source = subscription.source;
            existing.enabled = subscription.enabled;
            existing.state = if subscription.enabled {
                ImportedSubscriptionState::Ready(kind)
            } else {
                ImportedSubscriptionState::None
            };
            existing.providers = providers.to_vec();
            existing.refresh_interval = subscription.refresh_interval;
            existing.last_successful_update_unix_secs =
                subscription.last_successful_update_unix_secs;
            return;
        }
        self.imported_subscriptions.push(ImportedSubscription {
            id: subscription.id,
            name: subscription.name,
            source: subscription.source,
            enabled: subscription.enabled,
            state: if subscription.enabled {
                ImportedSubscriptionState::Ready(kind)
            } else {
                ImportedSubscriptionState::None
            },
            providers: providers.to_vec(),
            generation,
            refresh_interval: subscription.refresh_interval,
            last_successful_update_unix_secs: subscription.last_successful_update_unix_secs,
        });
    }

    fn restore_imported_subscriptions(&mut self, cx: &mut Context<Self>) {
        let pending: Vec<_> = self
            .imported_subscriptions
            .iter()
            .filter(|subscription| {
                matches!(subscription.state, ImportedSubscriptionState::Pending(_))
            })
            .map(|subscription| subscription.id.clone())
            .collect();
        for id in pending {
            self.refresh_imported_subscription(id, cx);
        }
    }

    fn refresh_imported_subscription(&mut self, id: String, cx: &mut Context<Self>) {
        let language = self.language();
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            return;
        };
        let Some(subscription) = self
            .imported_subscriptions
            .iter_mut()
            .find(|subscription| subscription.id == id)
        else {
            return;
        };
        if matches!(
            subscription.state,
            ImportedSubscriptionState::None
                | ImportedSubscriptionState::Refreshing(_)
                | ImportedSubscriptionState::Removing(_)
        ) || !subscription.enabled
        {
            return;
        }
        let kind = source_kind(&subscription.source);
        let source = subscription.source.clone();
        self.subscription_preview_generation = self.subscription_preview_generation.wrapping_add(1);
        let generation = self.subscription_preview_generation;
        subscription.generation = generation;
        subscription.state = ImportedSubscriptionState::Refreshing(kind);
        language
            .localized(copy::app::UPDATING_SUBSCRIPTION_NODES)
            .clone_into(&mut self.status);
        trace_ui(UiEvent::SourceRestoreStarted);

        let executor = cx.background_executor().clone();
        let runtime = self.runtime.clone();
        let task_id = id.clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    let proxy_nameservers =
                        mihomo::discover_subscription_proxy_nameservers(&source);
                    let providers = mihomo::preview_imported_subscription(source)
                        .map_err(ImportSubscriptionError::Preview)?;
                    let transaction = mutate_saved_sources(&runtime, &store_dir, || {
                        let mut stored = mihomo::mark_subscription_source_update_success_in(
                            &store_dir,
                            &task_id,
                            mihomo::current_unix_secs(),
                        )?;
                        if !proxy_nameservers.is_empty() {
                            stored = mihomo::update_subscription_source_proxy_nameservers_in(
                                &store_dir,
                                &task_id,
                                &proxy_nameservers,
                            )?;
                        }
                        Ok(stored)
                    })
                    .map_err(ImportSubscriptionError::Store)?;
                    Ok::<_, ImportSubscriptionError>((providers, transaction))
                })
                .await;
            this.update(cx, |this, cx| {
                this.finish_subscription_refresh(&id, generation, kind, result, cx);
            })
            .ok();
        })
        .detach();
    }

    fn finish_subscription_refresh(
        &mut self,
        id: &str,
        generation: u64,
        kind: SourceKind,
        result: SubscriptionRefreshResult,
        cx: &mut Context<Self>,
    ) {
        let language = self.language();
        let Some(subscription) = self
            .imported_subscriptions
            .iter_mut()
            .find(|subscription| subscription.id == id)
        else {
            return;
        };
        if subscription.generation != generation {
            return;
        }
        match result {
            Ok((providers, transaction)) if transaction.value.is_some() => {
                let stored = transaction.value.expect("checked committed mutation");
                let node_count: usize = providers.iter().map(|provider| provider.nodes.len()).sum();
                subscription.providers = providers;
                subscription.state = ImportedSubscriptionState::Ready(kind);
                subscription.refresh_interval = stored.refresh_interval;
                subscription.last_successful_update_unix_secs =
                    stored.last_successful_update_unix_secs;
                self.rule_sources
                    .refresh_retry_not_before
                    .remove(&DueRemoteSource::Subscription(id.to_owned()).scheduler_key());
                transaction.apply.reconcile_proxy_mode(&mut self.proxy_mode);
                self.status = copy::app::subscription_updated(
                    language,
                    node_count,
                    &transaction.apply.status_suffix(language),
                );
                trace_ui(UiEvent::SourceRestoreSucceeded);
            }
            Ok((_providers, transaction)) => {
                subscription.state = ImportedSubscriptionState::Pending(kind);
                self.status = format!(
                    "{}{}",
                    language.localized(copy::app::SUBSCRIPTION_UPDATE_FAILED),
                    transaction.apply.status_suffix_after_rollback_attempt(
                        language,
                        transaction.rollback_error.as_ref(),
                    )
                );
                trace_ui(UiEvent::SourceRestoreFailed);
            }
            Err(ImportSubscriptionError::Preview(error)) => {
                subscription.state = ImportedSubscriptionState::Unavailable(kind, error);
                self.status = format!(
                    "{}{error}",
                    language.localized(copy::app::SUBSCRIPTION_UPDATE_FAILED_2)
                );
                trace_ui(UiEvent::SourceRestoreFailed);
            }
            Err(ImportSubscriptionError::Store(error)) => {
                subscription.state = ImportedSubscriptionState::StoreError(error);
                self.status = format!(
                    "{}{error}",
                    language.localized(
                        copy::app::SUBSCRIPTION_LOADED_BUT_ITS_UPDATE_TIME_COULD_NOT_BE_SAVED
                    )
                );
                trace_ui(UiEvent::SourceRestoreFailed);
            }
        }
        cx.notify();
    }

    fn source_refresh_busy(&self) -> bool {
        self.imported_subscriptions.iter().any(|source| {
            matches!(
                source.state,
                ImportedSubscriptionState::Refreshing(_) | ImportedSubscriptionState::Removing(_)
            )
        }) || self.rule_sources.feedback == QxRuleImportFeedback::Importing
            || self
                .rule_sources
                .refreshes
                .values()
                .any(QxRuleSourceRefreshState::is_refreshing)
            || !self.rule_sources.target_updates.is_empty()
    }

    fn refresh_next_due_source(&mut self, cx: &mut Context<Self>) {
        if self.source_refresh_busy() {
            return;
        }
        let now = mihomo::current_unix_secs();
        let due = next_due_remote_source(
            &self.imported_subscriptions,
            &self.rule_sources.sources,
            &self.rule_sources.refresh_retry_not_before,
            now,
        );
        if let Some(source) = due.as_ref() {
            self.rule_sources
                .refresh_retry_not_before
                .insert(source.scheduler_key(), now.saturating_add(300));
        }
        match due {
            Some(DueRemoteSource::Subscription(id)) => {
                self.refresh_imported_subscription(id, cx);
            }
            Some(DueRemoteSource::QxRule(id)) => self.refresh_qx_rule_source(id, cx),
            None => {}
        }
    }

    fn ensure_source_refresh_scheduler(&mut self, cx: &mut Context<Self>) {
        if self.rule_sources.refresh_scheduler == SourceRefreshSchedulerState::Started
            || self.subscription_store_dir.is_none()
        {
            return;
        }
        self.rule_sources.refresh_scheduler = SourceRefreshSchedulerState::Started;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_secs(60))
                    .await;
                let Some(this) = this.upgrade() else {
                    break;
                };
                this.update(cx, ManisApp::refresh_next_due_source);
            }
        })
        .detach();
    }

    fn remove_imported_subscription(&mut self, id: String, cx: &mut Context<Self>) {
        let language = self.language();
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            return;
        };
        let Some(subscription) = self
            .imported_subscriptions
            .iter_mut()
            .find(|subscription| subscription.id == id)
        else {
            return;
        };
        let kind = source_kind(&subscription.source);
        self.subscription_preview_generation = self.subscription_preview_generation.wrapping_add(1);
        let generation = self.subscription_preview_generation;
        subscription.generation = generation;
        self.proxy_source_editor.feedback = SubscriptionFeedback::Idle;
        subscription.state = ImportedSubscriptionState::Removing(kind);
        language
            .localized(copy::app::REMOVING_IMPORTED_SUBSCRIPTION)
            .clone_into(&mut self.status);
        trace_ui(UiEvent::SourceRemoveStarted);

        let executor = cx.background_executor().clone();
        let remove_id = id.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    mutate_saved_sources(&runtime, &store_dir, || {
                        mihomo::remove_subscription_source_in(&store_dir, &remove_id)
                    })
                })
                .await;
            this.update(cx, |this, cx| {
                let language = this.language();
                let Some(index) = this
                    .imported_subscriptions
                    .iter()
                    .position(|subscription| subscription.id == id)
                else {
                    return;
                };
                if this.imported_subscriptions[index].generation != generation {
                    return;
                }
                match result {
                    Ok(transaction) if transaction.value.is_some() => {
                        this.imported_subscriptions.remove(index);
                        this.rule_sources
                            .refresh_retry_not_before
                            .remove(&DueRemoteSource::Subscription(id.clone()).scheduler_key());
                        transaction.apply.reconcile_proxy_mode(&mut this.proxy_mode);
                        this.status = format!(
                            "{}{}",
                            language.localized(copy::app::IMPORTED_SUBSCRIPTION_REMOVED),
                            transaction.apply.status_suffix(language)
                        );
                        trace_ui(UiEvent::SourceRemoveSucceeded);
                    }
                    Ok(transaction) => {
                        this.imported_subscriptions[index].state =
                            ImportedSubscriptionState::Ready(kind);
                        this.status = format!(
                            "{}{}",
                            language.localized(copy::app::IMPORTED_SUBSCRIPTION_REMOVAL_FAILED),
                            transaction
                                .apply
                                .status_suffix_after_source_rollback(language)
                        );
                        trace_ui(UiEvent::SourceRemoveFailed);
                    }
                    Err(error) => {
                        this.imported_subscriptions[index].state =
                            ImportedSubscriptionState::StoreError(error);
                        this.status = format!(
                            "{}{error}",
                            language.localized(copy::app::COULD_NOT_REMOVE_SUBSCRIPTION)
                        );
                        trace_ui(UiEvent::SourceRemoveFailed);
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}
include!("app/policy_presentation.rs");
include!("app/runtime_lifecycle.rs");
include!("app/routing_controls.rs");
include!("app/policy_workspace.rs");

struct StatusBarValues {
    endpoint: String,
    download: String,
    upload: String,
    dot: gpui::Rgba,
    tone: StatusTone,
}

fn status_bar_values(
    controller: &ControllerState,
    language: Language,
    theme: Theme,
) -> StatusBarValues {
    match controller {
        ControllerState::Disconnected => StatusBarValues {
            endpoint: language.localized(copy::app::NO_RUNTIME_DATA).to_owned(),
            download: "↓ —".to_owned(),
            upload: "↑ —".to_owned(),
            dot: theme.route_trace,
            tone: StatusTone::Warning,
        },
        ControllerState::Connecting { endpoint } => StatusBarValues {
            endpoint: endpoint.clone(),
            download: "↓ —".to_owned(),
            upload: "↑ —".to_owned(),
            dot: theme.route_trace,
            tone: StatusTone::Route,
        },
        ControllerState::Failed { endpoint, .. } => StatusBarValues {
            endpoint: endpoint.clone(),
            download: "↓ —".to_owned(),
            upload: "↑ —".to_owned(),
            dot: theme.status_error,
            tone: StatusTone::Error,
        },
        ControllerState::Connected {
            endpoint,
            download_total,
            upload_total,
            ..
        } => StatusBarValues {
            endpoint: endpoint.clone(),
            download: format!(
                "{}↓ {}",
                language.localized(copy::app::TOTAL),
                format_bytes(*download_total)
            ),
            upload: format!(
                "{}↑ {}",
                language.localized(copy::app::TOTAL),
                format_bytes(*upload_total)
            ),
            dot: theme.status_success,
            tone: StatusTone::Success,
        },
    }
}

fn format_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;
    const GIB: u64 = MIB * 1024;

    if bytes >= GIB {
        format_bytes_in_unit(bytes, GIB, "GiB")
    } else if bytes >= MIB {
        format_bytes_in_unit(bytes, MIB, "MiB")
    } else if bytes >= KIB {
        format_bytes_in_unit(bytes, KIB, "KiB")
    } else {
        format!("{bytes} B")
    }
}

fn format_bytes_in_unit(bytes: u64, unit: u64, suffix: &str) -> String {
    let whole = bytes / unit;
    let tenth = (bytes % unit) * 10 / unit;
    format!("{whole}.{tenth} {suffix}")
}

/// Builds the one-line controller summary shown in the status bar.
///
/// The kernel name is supplied by the caller so the line always names the kernel that is
/// actually running rather than assuming Mihomo.
fn controller_status_label(
    controller: &ControllerState,
    kernel_name: &str,
    language: Language,
) -> String {
    match controller {
        ControllerState::Disconnected => {
            format!(
                "{kernel_name} {}",
                language.localized(copy::app::DISCONNECTED)
            )
        }
        ControllerState::Connecting { .. } => {
            format!(
                "{kernel_name} {}",
                language.localized(copy::app::CONNECTING)
            )
        }
        ControllerState::Connected {
            version,
            active_connections,
            ..
        } => format!(
            "{kernel_name} {version} · {}",
            language.count(CountNoun::Connection, *active_connections)
        ),
        // The reason travels with the label: the sidebar used to be the only place it appeared.
        ControllerState::Failed { message, .. } => format!(
            "{kernel_name} {} · {message}",
            language.localized(copy::app::CONNECTION_FAILED)
        ),
    }
}

fn apply_proxy_mode_transition(
    runtime: &KernelRuntime,
    system_proxy: &Arc<Mutex<SystemProxySession>>,
    tun_dns: &Arc<Mutex<TunDnsSession>>,
    previous: ProxyMode,
    requested: ProxyMode,
    ports: ProxyPorts,
    language: Language,
) -> Result<(), String> {
    let mut system = system_proxy
        .lock()
        .map_err(|_| "系统代理状态锁已损坏".to_owned())?;
    let mut dns = tun_dns
        .lock()
        .map_err(|_| "TUN DNS 状态锁已损坏".to_owned())?;
    match (previous, requested) {
        (ProxyMode::System, ProxyMode::Off) => system
            .disable_with_language(language)
            .map_err(|error| error.to_string()),
        (ProxyMode::Tun, ProxyMode::Off) => disable_tun_with_dns(runtime, &mut dns, language),
        (ProxyMode::Off, ProxyMode::System) => system
            .enable_with_language(ports, language)
            .map_err(|error| error.to_string()),
        (ProxyMode::Tun, ProxyMode::System) => {
            disable_tun_with_dns(runtime, &mut dns, language)?;
            if let Err(error) = system.enable_with_language(ports, language) {
                let rollback = enable_tun_with_dns(runtime, &mut dns, language);
                return Err(match rollback {
                    Ok(()) => error.to_string(),
                    Err(rollback) => format!("{error}；恢复原 TUN 模式也失败：{rollback}"),
                });
            }
            Ok(())
        }
        (ProxyMode::Off, ProxyMode::Tun) => enable_tun_with_dns(runtime, &mut dns, language),
        (ProxyMode::System, ProxyMode::Tun) => {
            system
                .disable_with_language(language)
                .map_err(|error| error.to_string())?;
            if let Err(error) = enable_tun_with_dns(runtime, &mut dns, language) {
                let rollback = system.enable_with_language(ports, language);
                return Err(match rollback {
                    Ok(()) => error,
                    Err(rollback) => format!("{error}；恢复原系统代理也失败：{rollback}"),
                });
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn enable_tun_with_dns(
    runtime: &KernelRuntime,
    dns: &mut TunDnsSession,
    language: Language,
) -> Result<(), String> {
    record_event(
        LogLevel::Info,
        "controller.tun.dns.requested",
        "action=prepare resolver=114.114.114.114",
    );
    dns.prepare_with_language(language).map_err(|error| {
        record_event(
            LogLevel::Error,
            "controller.tun.dns.failed",
            format!("action=prepare error={error}"),
        );
        error.to_string()
    })?;
    record_event(
        LogLevel::Info,
        "controller.tun.dns.succeeded",
        "action=prepare recovery=saved",
    );
    if let Err(error) = runtime.set_tun_enabled(true) {
        let rollback = dns.disable_with_language(language);
        return Err(match rollback {
            Ok(()) => error.to_string(),
            Err(rollback) => {
                format!("{error}；恢复原 DNS 设置也失败：{rollback}")
            }
        });
    }

    record_event(
        LogLevel::Info,
        "controller.tun.dns.requested",
        "action=install resolver=114.114.114.114",
    );
    if let Err(error) = dns.activate_with_language(language) {
        let dns_rollback = dns.disable_with_language(language);
        let tun_rollback = runtime.set_tun_enabled(false);
        record_event(
            LogLevel::Error,
            "controller.tun.dns.failed",
            format!("action=install error={error}"),
        );
        return Err(match (dns_rollback, tun_rollback) {
            (Ok(()), Ok(())) => error.to_string(),
            (Err(dns_rollback), Ok(())) => {
                format!("{error}；恢复原 DNS 设置也失败：{dns_rollback}")
            }
            (Ok(()), Err(tun_rollback)) => {
                format!("{error}；关闭已启动的 TUN 也失败：{tun_rollback}")
            }
            (Err(dns_rollback), Err(tun_rollback)) => format!(
                "{error}；恢复原 DNS 设置失败：{dns_rollback}；关闭已启动的 TUN 失败：{tun_rollback}"
            ),
        });
    }
    record_event(
        LogLevel::Info,
        "controller.tun.dns.succeeded",
        "action=install recovery=retained",
    );
    Ok(())
}

fn disable_tun_with_dns(
    runtime: &KernelRuntime,
    dns: &mut TunDnsSession,
    language: Language,
) -> Result<(), String> {
    runtime
        .set_tun_enabled(false)
        .map_err(|error| error.to_string())?;
    record_event(
        LogLevel::Info,
        "controller.tun.dns.requested",
        "action=restore",
    );
    dns.disable_with_language(language).map_or_else(
        |error| {
            record_event(
                LogLevel::Warn,
                "controller.tun.dns.restore_deferred",
                format!("error={error}"),
            );
            Err(format!(
                "{}：{error}",
                language.localized(
                    copy::app::TUN_IS_DISABLED_BUT_RESTORING_THE_ORIGINAL_DNS_FAILED_RECOVERY
                )
            ))
        },
        |()| {
            record_event(
                LogLevel::Info,
                "controller.tun.dns.succeeded",
                "action=restore recovery=removed",
            );
            Ok(())
        },
    )
}

fn proxy_mode_label(language: Language, mode: ProxyMode) -> &'static str {
    match mode {
        ProxyMode::Off => language.localized(copy::common::OFF),
        ProxyMode::System => language.localized(copy::common::SYSTEM_PROXY),
        ProxyMode::Tun => language.localized(copy::common::TUN_PROXY),
    }
}

fn compact_proxy_mode_label(
    language: Language,
    current: ProxyMode,
    pending: Option<ProxyMode>,
) -> &'static str {
    match pending {
        Some(ProxyMode::Tun) => language.localized(copy::app::PREPARING_TUN),
        Some(ProxyMode::System) => language.localized(copy::app::ENABLING),
        Some(ProxyMode::Off) => language.localized(copy::app::TURNING_OFF),
        None => match current {
            ProxyMode::Off => language.localized(copy::app::OFF),
            ProxyMode::System => language.localized(copy::app::SYSTEM),
            ProxyMode::Tun => "TUN",
        },
    }
}

/// Whether the controller Manis talks to is currently usable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ControllerReadiness {
    Connected,
    Disconnected,
}

/// Whether the active kernel and controller can hand traffic to a TUN device.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TunSupport {
    Supported,
    KernelUnsupported,
    FixtureReadOnly,
}

/// The reason a proxy mode cannot be applied.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProxyModeBlock {
    Busy,
    ControllerNotConnected,
    KernelHasNoTun,
    FixtureReadOnly,
}

impl ProxyModeBlock {
    /// A short phrase that fits after a tray menu label.
    pub(crate) const fn tray_reason(self, language: Language) -> &'static str {
        match self {
            Self::Busy => language.localized(copy::app::SWITCHING_2),
            Self::ControllerNotConnected => language.localized(copy::app::CONNECT_FIRST),
            Self::KernelHasNoTun => language.localized(copy::app::KERNEL_HAS_NO_TUN),
            Self::FixtureReadOnly => language.localized(copy::app::TEST_FIXTURE_IS_READ_ONLY),
        }
    }
}

/// Decides whether `requested` can be applied, mirroring the guards in `apply_proxy_mode`.
///
/// Keeping this pure lets the tray disable an entry for exactly the reason the switch would
/// have failed, instead of duplicating the conditions and drifting from them.
const fn proxy_mode_block(
    requested: ProxyMode,
    switching: Option<ProxyMode>,
    controller: ControllerReadiness,
    tun: TunSupport,
) -> Option<ProxyModeBlock> {
    if switching.is_some() {
        return Some(ProxyModeBlock::Busy);
    }
    if matches!(controller, ControllerReadiness::Disconnected) {
        return Some(ProxyModeBlock::ControllerNotConnected);
    }
    if !matches!(requested, ProxyMode::Tun) {
        return None;
    }
    match tun {
        TunSupport::Supported => None,
        TunSupport::KernelUnsupported => Some(ProxyModeBlock::KernelHasNoTun),
        TunSupport::FixtureReadOnly => Some(ProxyModeBlock::FixtureReadOnly),
    }
}

fn routing_mode_label(language: Language, mode: RoutingMode) -> &'static str {
    match mode {
        RoutingMode::Direct => language.localized(copy::common::DIRECT),
        RoutingMode::Global => language.localized(copy::app::GLOBAL),
        RoutingMode::Rule => language.localized(copy::app::RULES),
    }
}

fn controller_state_label(state: &ControllerState) -> &'static str {
    match state {
        ControllerState::Disconnected => "disconnected",
        ControllerState::Connecting { .. } => "connecting",
        ControllerState::Connected { .. } => "connected",
        ControllerState::Failed { .. } => "failed",
    }
}

fn policy_target_is_selectable(
    connected: bool,
    catalog_allows: Option<bool>,
    stored_group_allows: Option<bool>,
) -> bool {
    if connected {
        catalog_allows == Some(true)
    } else {
        stored_group_allows == Some(true)
    }
}

fn policy_kind_label(language: Language, kind: manis_core::PolicyGroupKind) -> &'static str {
    match kind {
        manis_core::PolicyGroupKind::Selector => language.localized(copy::app::MANUAL),
        manis_core::PolicyGroupKind::UrlTest => language.localized(copy::app::AUTO_SELECT),
        manis_core::PolicyGroupKind::Fallback => language.localized(copy::app::FALLBACK),
        manis_core::PolicyGroupKind::LoadBalance => language.localized(copy::app::LOAD_BALANCE),
        manis_core::PolicyGroupKind::Direct => language.localized(copy::common::DIRECT),
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum PolicyWorkspaceRender {
    Inactive,
    Editor,
    Empty,
    Catalog,
}

#[derive(Clone, Copy)]
struct WorkspaceRenderState {
    size_class: WindowSizeClass,
    policy_workspace: PolicyWorkspaceRender,
}

impl WorkspaceRenderState {
    fn from_app(app: &ManisApp, size_class: WindowSizeClass) -> Self {
        let policies_active = app.primary_workspace == PrimaryWorkspace::Policies;
        let editing_new_policy = app
            .managed_policies
            .draft
            .as_ref()
            .is_some_and(|draft| draft.editing_id.is_none());
        let policy_workspace = if !policies_active {
            PolicyWorkspaceRender::Inactive
        } else if editing_new_policy {
            PolicyWorkspaceRender::Editor
        } else if app.catalog.is_some() {
            PolicyWorkspaceRender::Catalog
        } else {
            PolicyWorkspaceRender::Empty
        };
        Self {
            size_class,
            policy_workspace,
        }
    }

    fn compact(self) -> bool {
        self.size_class == WindowSizeClass::Compact
    }

    fn shows_policy_groups(self, navigation: CompactNavigation) -> bool {
        !self.compact() || navigation == CompactNavigation::GroupList
    }

    fn shows_policy_detail(self, navigation: CompactNavigation) -> bool {
        !self.compact() || navigation == CompactNavigation::GroupDetail
    }
}

impl ManisApp {
    fn workspace_content(
        &mut self,
        state: WorkspaceRenderState,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .relative()
            .flex_1()
            .overflow_hidden()
            .flex()
            .child(self.navigation(theme, state.size_class, cx))
            .when(self.primary_workspace == PrimaryWorkspace::Nodes, |main| {
                main.child(self.node_workspace(theme, state.size_class, cx))
            })
            .when(
                self.primary_workspace == PrimaryWorkspace::RoutingRules,
                |main| {
                    main.child(self.routing_rules_workspace(theme, state.size_class, window, cx))
                },
            )
            .when(
                self.primary_workspace == PrimaryWorkspace::Activity,
                |main| main.child(self.activity_workspace(theme, state.size_class, cx)),
            )
            .when(self.primary_workspace == PrimaryWorkspace::Logs, |main| {
                main.child(self.logs_workspace(theme, state.size_class, cx))
            })
            .when(
                self.primary_workspace == PrimaryWorkspace::Configuration,
                |main| main.child(self.configuration_workspace(theme, state.size_class, cx)),
            )
            .when(
                state.policy_workspace == PolicyWorkspaceRender::Editor,
                |main| {
                    let draft = self
                        .managed_policies
                        .draft
                        .as_ref()
                        .expect("policy editor state requires a draft");
                    main.child(self.managed_policy_editor_workspace(
                        draft,
                        state.compact(),
                        self.language(),
                        theme,
                        cx,
                    ))
                },
            )
            .when(
                state.policy_workspace == PolicyWorkspaceRender::Empty,
                |main| main.child(self.empty_policy_workspace(theme, cx)),
            )
            .when(
                state.policy_workspace == PolicyWorkspaceRender::Catalog
                    && state.shows_policy_groups(self.workspace.compact_navigation),
                |main| {
                    main.child(
                        self.policy_list(theme, state.policy_list_width(), cx)
                            .when(state.compact(), Styled::flex_1),
                    )
                },
            )
            .when(
                state.policy_workspace == PolicyWorkspaceRender::Catalog
                    && state.shows_policy_detail(self.workspace.compact_navigation),
                |main| main.child(self.detail(theme, state.compact(), cx)),
            )
    }
}

impl WorkspaceRenderState {
    fn policy_list_width(self) -> Option<f32> {
        match self.size_class {
            WindowSizeClass::Compact => None,
            WindowSizeClass::Medium => Some(LayoutMetric::MediumPolicyList.px().as_f32()),
            WindowSizeClass::Wide => Some(LayoutMetric::WidePolicyList.px().as_f32()),
        }
    }
}

impl Default for ManisApp {
    fn default() -> Self {
        Self::new()
    }
}

impl Render for ManisApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let size_class = WindowSizeClass::for_width(window.viewport_size().width.as_f32());
        let theme = self.theme();
        let state = WorkspaceRenderState::from_app(self, size_class);

        div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.surface_base)
            .text_color(theme.text_primary)
            .text_size(px(13.0))
            .child(self.chrome(theme, size_class, cx))
            .child(self.workspace_content(state, theme, window, cx))
            .children(gpui_component::Root::render_sheet_layer(window, cx))
            .child(self.status_bar(theme))
    }
}

#[cfg(all(test, not(windows)))]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;

    use manis_core::{
        ManagedPolicyGroup, NodeIdentity, PolicyCandidateKind, PolicyCatalog, PolicyGroup,
        PolicyGroupId, PolicyGroupKind, PolicyNode, ProxyId,
    };

    use manis_core::ProxyMode;

    use super::{
        ControllerReadiness, DueRemoteSource, ImportedSubscription, ImportedSubscriptionState,
        ManisApp, PolicyDetailTab, ProxyModeBlock, SourceRuntimeApply, TunSupport,
        proxy_mode_block,
    };
    use crate::subscription::SourceKind;
    use crate::{
        localization::Language,
        mihomo::{self, ControllerState},
    };

    #[test]
    fn policy_detail_tabs_round_trip_through_component_indices() {
        assert_eq!(PolicyDetailTab::Nodes.index(), 0);
        assert_eq!(PolicyDetailTab::Settings.index(), 1);
        assert_eq!(PolicyDetailTab::from_index(0), PolicyDetailTab::Nodes);
        assert_eq!(PolicyDetailTab::from_index(1), PolicyDetailTab::Settings);
        assert_eq!(PolicyDetailTab::from_index(2), PolicyDetailTab::Nodes);
        assert_eq!(PolicyDetailTab::from_index(99), PolicyDetailTab::Nodes);
    }

    #[test]
    fn app_startup_detects_a_privately_imported_subscription() {
        let root = std::env::temp_dir().join(format!("manis-app-import-{}", std::process::id()));
        if root.exists() {
            fs::remove_dir_all(&root).expect("remove stale fixture");
        }
        let store = root.join("subscriptions");
        mihomo::save_imported_subscription_in(
            &store,
            "https://subscription.example.invalid/client?token=fixture",
        )
        .expect("save fixture subscription");

        let app = ManisApp::with_fixture_controller_and_subscription_store(
            "http://127.0.0.1:9090",
            store,
        );

        assert_eq!(app.imported_subscriptions.len(), 1);
        assert_eq!(
            app.imported_subscriptions[0].state,
            ImportedSubscriptionState::Pending(SourceKind::HttpsSubscription)
        );
        assert_eq!(
            app.imported_subscriptions[0].refresh_interval,
            mihomo::RemoteSourceRefreshInterval::Manual
        );
        assert_eq!(
            app.imported_subscriptions[0].last_successful_update_unix_secs,
            0
        );
        fs::remove_dir_all(root).expect("remove fixture");
    }

    #[test]
    fn policy_node_source_uses_the_imported_subscription_name() {
        let mut app = ManisApp::with_fixture_controller("http://127.0.0.1:9090");
        app.imported_subscriptions.push(ImportedSubscription {
            id: "subscription:fixture".to_owned(),
            name: "NaiU_Net".to_owned(),
            source: manis_profile::SecretUrl::parse_subscription(
                "https://subscription.example.invalid/client?name=NaiU_Net",
            )
            .expect("fixture subscription"),
            enabled: true,
            state: ImportedSubscriptionState::Ready(SourceKind::HttpsSubscription),
            providers: Vec::new(),
            generation: 0,
            refresh_interval: mihomo::RemoteSourceRefreshInterval::Manual,
            last_successful_update_unix_secs: 0,
        });
        let node = PolicyNode {
            id: ProxyId::new("HK 03"),
            name: "HK 03".to_owned(),
            kind: PolicyCandidateKind::Node,
            provider: Some("Subscription 1".to_owned()),
            detail: "Trojan".to_owned(),
            latency_ms: None,
            alive: None,
        };

        assert_eq!(
            app.policy_node_source_label(&node, Language::SimplifiedChinese),
            "NaiU_Net"
        );
    }

    #[test]
    fn scheduled_refresh_selects_one_due_source_with_subscriptions_first() {
        let subscription = ImportedSubscription {
            id: "subscription:fixture".to_owned(),
            name: "Fixture".to_owned(),
            source: manis_profile::SecretUrl::parse_subscription(
                "https://subscription.example.invalid/client",
            )
            .expect("fixture subscription"),
            enabled: true,
            state: ImportedSubscriptionState::Ready(SourceKind::HttpsSubscription),
            providers: Vec::new(),
            generation: 0,
            refresh_interval: mihomo::RemoteSourceRefreshInterval::Hourly,
            last_successful_update_unix_secs: 100,
        };
        let mut rule_source = mihomo::StoredQxRuleSource {
            id: "qx-rule-source:fixture".to_owned(),
            source: manis_profile::SecretUrl::parse_https("https://rules.example.invalid/list")
                .expect("fixture rule URL"),
            enabled: true,
            target_policy: manis_profile::Name::parse("Proxy").expect("fixture policy"),
            content: "DOMAIN-SUFFIX,example.com,Proxy".to_owned(),
            rule_count: 1,
            diagnostic_count: 0,
            refresh_interval: mihomo::RemoteSourceRefreshInterval::Hourly,
            last_successful_update_unix_secs: 100,
        };

        assert_eq!(
            super::next_due_remote_source(
                std::slice::from_ref(&subscription),
                std::slice::from_ref(&rule_source),
                &BTreeMap::new(),
                3_700,
            ),
            Some(DueRemoteSource::Subscription(subscription.id.clone()))
        );

        let mut disabled_subscription = subscription.clone();
        disabled_subscription.enabled = false;
        assert_eq!(
            super::next_due_remote_source(
                &[disabled_subscription],
                std::slice::from_ref(&rule_source),
                &BTreeMap::new(),
                3_700,
            ),
            Some(DueRemoteSource::QxRule(rule_source.id.clone()))
        );

        let mut second_subscription = subscription.clone();
        second_subscription.id = "subscription:second".to_owned();
        let retry_not_before = BTreeMap::from([(
            DueRemoteSource::Subscription(subscription.id.clone()).scheduler_key(),
            4_000,
        )]);
        assert_eq!(
            super::next_due_remote_source(
                &[subscription, second_subscription.clone()],
                std::slice::from_ref(&rule_source),
                &retry_not_before,
                3_700,
            ),
            Some(DueRemoteSource::Subscription(second_subscription.id))
        );

        rule_source.enabled = false;
        assert_eq!(
            super::next_due_remote_source(&[], &[rule_source.clone()], &BTreeMap::new(), 3_700),
            None
        );
        rule_source.enabled = true;
        rule_source.refresh_interval = mihomo::RemoteSourceRefreshInterval::Manual;
        assert_eq!(
            super::next_due_remote_source(&[], &[rule_source], &BTreeMap::new(), 3_700),
            None
        );
    }

    #[test]
    fn disconnected_app_starts_without_mock_policy_groups() {
        let app = ManisApp::with_fixture_controller("http://127.0.0.1:9090");

        assert!(app.catalog.is_none());
        assert_eq!(app.workspace.selected_group, None);
        assert_eq!(app.workspace.selected_node, None);
    }

    #[test]
    fn manual_selector_with_candidates_is_benchmarkable() {
        let selector = PolicyGroup {
            id: PolicyGroupId::new("Manual Route"),
            name: "Manual Route".to_owned(),
            kind: PolicyGroupKind::Selector,
            target: "Hong Kong".to_owned(),
            nodes: vec![PolicyNode {
                id: ProxyId::new("Hong Kong"),
                name: "Hong Kong".to_owned(),
                kind: PolicyCandidateKind::Node,
                provider: Some("Fixture".to_owned()),
                detail: "VLESS".to_owned(),
                latency_ms: None,
                alive: None,
            }],
            rules_total: 0,
            rules: Vec::new(),
        };

        assert!(ManisApp::policy_group_benchmarkable(&selector));

        let empty_selector = PolicyGroup {
            nodes: Vec::new(),
            ..selector
        };
        assert!(!ManisApp::policy_group_benchmarkable(&empty_selector));
    }

    #[test]
    fn policy_settings_only_match_a_saved_manis_group_by_exact_name() {
        let mut app = ManisApp::with_fixture_controller("http://127.0.0.1:9090");
        app.managed_policies.groups.push(
            ManagedPolicyGroup::new("group-deadbeef", "Hong Kong")
                .expect("valid managed policy group"),
        );

        assert_eq!(
            app.editable_policy_group_id("Hong Kong"),
            Some("group-deadbeef")
        );
        assert_eq!(app.editable_policy_group_id("Hong Kong Auto"), None);
        assert_eq!(app.editable_policy_group_id("GLOBAL"), None);
    }

    #[test]
    fn runtime_snapshot_populates_real_policy_groups() {
        let mut app = ManisApp::with_fixture_controller("http://127.0.0.1:9090");
        let policy_id = PolicyGroupId::new("runtime-policy");
        let node_id = ProxyId::new("runtime-node");
        let catalog = PolicyCatalog::try_new(vec![PolicyGroup {
            id: policy_id.clone(),
            name: "Runtime policy".to_owned(),
            kind: PolicyGroupKind::Selector,
            target: "Runtime node".to_owned(),
            nodes: vec![PolicyNode {
                id: node_id.clone(),
                name: "Runtime node".to_owned(),
                kind: PolicyCandidateKind::Node,
                provider: Some("Runtime provider".to_owned()),
                detail: "VLESS".to_owned(),
                latency_ms: Some(42),
                alive: Some(true),
            }],
            rules_total: 1,
            rules: Vec::new(),
        }])
        .expect("runtime policy catalog");

        app.apply_mihomo_snapshot(
            "http://127.0.0.1:9090".to_owned(),
            mihomo::LoadedSnapshot {
                catalog: Some(catalog),
                providers: Vec::new(),
                version: "fixture".to_owned(),
                active_connections: 0,
                download_total: 0,
                upload_total: 0,
                observed_routes: Vec::new(),
                connections: Vec::new(),
                runtime: manis_mihomo::RuntimeConfig::default(),
            },
        );

        assert_eq!(app.policy_groups().count(), 1);
        assert_eq!(app.workspace.selected_group, Some(policy_id));
        assert_eq!(app.workspace.selected_node, Some(node_id));
    }

    #[test]
    fn runtime_snapshot_without_user_policy_groups_still_connects_cleanly() {
        let mut app = ManisApp::with_fixture_controller("http://127.0.0.1:9090");
        app.workspace.replace_source_selection(
            PolicyGroupId::new("stale-policy"),
            Some(ProxyId::new("stale-node")),
        );

        app.apply_mihomo_snapshot(
            "http://127.0.0.1:9090".to_owned(),
            mihomo::LoadedSnapshot {
                catalog: None,
                providers: Vec::new(),
                version: "fixture".to_owned(),
                active_connections: 0,
                download_total: 0,
                upload_total: 0,
                observed_routes: Vec::new(),
                connections: Vec::new(),
                runtime: manis_mihomo::RuntimeConfig::default(),
            },
        );

        assert!(app.catalog.is_none());
        assert_eq!(app.workspace.selected_group, None);
        assert_eq!(app.workspace.selected_node, None);
        assert!(matches!(app.controller, ControllerState::Connected { .. }));
    }

    #[test]
    fn saved_global_node_overrides_runtime_target_without_losing_runtime_state() {
        let mut app = ManisApp::with_fixture_controller("http://127.0.0.1:9090");
        app.catalog = Some(
            PolicyCatalog::try_new(vec![PolicyGroup {
                id: PolicyGroupId::new("GLOBAL"),
                name: "GLOBAL".to_owned(),
                kind: PolicyGroupKind::Selector,
                target: "Tokyo".to_owned(),
                nodes: ["Tokyo", "Singapore"]
                    .into_iter()
                    .map(|name| PolicyNode {
                        id: ProxyId::new(name),
                        name: name.to_owned(),
                        kind: PolicyCandidateKind::Node,
                        provider: None,
                        detail: "VLESS".to_owned(),
                        latency_ms: None,
                        alive: None,
                    })
                    .collect(),
                rules_total: 0,
                rules: Vec::new(),
            }])
            .expect("fixture global group"),
        );

        assert_eq!(app.global_target(), Some("Tokyo"));
        assert_eq!(app.runtime_global_target(), Some("Tokyo"));

        app.managed_policies.node_selections.set_global(
            NodeIdentity::new("saved", "Singapore").expect("valid saved node identity"),
        );
        assert_eq!(app.global_target(), Some("Singapore"));
        assert_eq!(app.runtime_global_target(), Some("Tokyo"));
    }

    #[test]
    fn app_startup_restores_global_and_manual_policy_node_selections() {
        let root =
            std::env::temp_dir().join(format!("manis-app-node-selections-{}", std::process::id()));
        if root.exists() {
            fs::remove_dir_all(&root).expect("remove stale fixture");
        }
        let store = root.join("subscriptions");
        let mut preferences = mihomo::NodeSelectionPreferences::default();
        preferences.set_global(
            NodeIdentity::new("saved", "Singapore").expect("valid saved node identity"),
        );
        preferences
            .set_policy_target("Manual Video", "Tokyo")
            .expect("valid manual policy target");
        mihomo::save_node_selection_preferences_in(&store, &preferences)
            .expect("save node selections");

        let app = ManisApp::with_fixture_controller_and_subscription_store(
            "http://127.0.0.1:9090",
            store,
        );

        assert_eq!(
            app.global_target_identity()
                .map(|identity| (identity.source_id.as_str(), identity.node_name.as_str())),
            Some(("saved", "Singapore"))
        );
        assert_eq!(
            app.managed_policies
                .node_selections
                .policy_target("Manual Video"),
            Some("Tokyo")
        );
        fs::remove_dir_all(root).expect("remove selection fixture");
    }

    #[test]
    fn manual_policy_detail_falls_back_to_the_catalog_target() {
        let mut app = ManisApp::with_fixture_controller("http://127.0.0.1:9090");
        let policy_id = PolicyGroupId::new("manual-video");
        app.catalog = Some(
            PolicyCatalog::try_new(vec![PolicyGroup {
                id: policy_id.clone(),
                name: "Manual Video".to_owned(),
                kind: PolicyGroupKind::Selector,
                target: "Singapore".to_owned(),
                nodes: ["Tokyo", "Singapore"]
                    .into_iter()
                    .map(|name| PolicyNode {
                        id: ProxyId::new(name),
                        name: name.to_owned(),
                        kind: PolicyCandidateKind::Node,
                        provider: None,
                        detail: "fixture".to_owned(),
                        latency_ms: None,
                        alive: None,
                    })
                    .collect(),
                rules_total: 0,
                rules: Vec::new(),
            }])
            .expect("manual policy catalog"),
        );
        app.workspace.select_group(policy_id);

        assert_eq!(
            app.selected_node().map(|node| node.name),
            Some("Singapore".to_owned())
        );
    }

    #[test]
    fn offline_manual_policy_selection_uses_only_saved_group_candidates() {
        assert!(super::policy_target_is_selectable(
            false,
            Some(false),
            Some(true)
        ));
        assert!(!super::policy_target_is_selectable(
            false,
            Some(true),
            Some(false)
        ));
        assert!(!super::policy_target_is_selectable(false, Some(true), None));
    }

    #[test]
    fn connected_manual_policy_selection_uses_runtime_catalog() {
        assert!(super::policy_target_is_selectable(
            true,
            Some(true),
            Some(false)
        ));
        assert!(!super::policy_target_is_selectable(
            true,
            Some(false),
            Some(true)
        ));
        assert!(!super::policy_target_is_selectable(true, None, Some(true)));
    }

    #[test]
    fn app_startup_restores_saved_qx_rule_sources() {
        let root = std::env::temp_dir().join(format!("manis-app-qx-rule-{}", std::process::id()));
        if root.exists() {
            fs::remove_dir_all(&root).expect("remove stale fixture");
        }
        let store = root.join("subscriptions");
        mihomo::save_qx_rule_source_in(
            &store,
            "https://rules.example.invalid/airports.list",
            "Proxy",
            "DOMAIN-SUFFIX,example.com,Proxy",
        )
        .expect("save QX rule fixture");

        let app = ManisApp::with_fixture_controller_and_subscription_store(
            "http://127.0.0.1:9090",
            store,
        );

        assert_eq!(app.rule_sources.sources.len(), 1);
        assert!(app.rule_sources.sources[0].enabled);
        assert_eq!(app.rule_sources.sources[0].rule_count, 1);
        assert_eq!(app.rule_sources.sources[0].target_policy.as_str(), "Proxy");
        fs::remove_dir_all(root).expect("remove fixture");
    }

    #[test]
    fn tun_is_blocked_until_a_capable_managed_kernel_is_connected() {
        assert_eq!(
            proxy_mode_block(
                ProxyMode::Tun,
                None,
                ControllerReadiness::Disconnected,
                TunSupport::Supported
            ),
            Some(ProxyModeBlock::ControllerNotConnected)
        );
        assert_eq!(
            proxy_mode_block(
                ProxyMode::Tun,
                None,
                ControllerReadiness::Connected,
                TunSupport::KernelUnsupported
            ),
            Some(ProxyModeBlock::KernelHasNoTun)
        );
        assert_eq!(
            proxy_mode_block(
                ProxyMode::Tun,
                None,
                ControllerReadiness::Connected,
                TunSupport::FixtureReadOnly
            ),
            Some(ProxyModeBlock::FixtureReadOnly)
        );
        assert_eq!(
            proxy_mode_block(
                ProxyMode::Tun,
                None,
                ControllerReadiness::Connected,
                TunSupport::Supported
            ),
            None
        );
    }

    #[test]
    fn the_system_proxy_only_needs_a_connected_controller() {
        assert_eq!(
            proxy_mode_block(
                ProxyMode::System,
                None,
                ControllerReadiness::Disconnected,
                TunSupport::Supported
            ),
            Some(ProxyModeBlock::ControllerNotConnected)
        );
        assert_eq!(
            proxy_mode_block(
                ProxyMode::System,
                None,
                ControllerReadiness::Connected,
                TunSupport::FixtureReadOnly
            ),
            None
        );
        assert_eq!(
            proxy_mode_block(
                ProxyMode::System,
                None,
                ControllerReadiness::Connected,
                TunSupport::KernelUnsupported
            ),
            None
        );
    }

    #[test]
    fn a_switch_in_flight_blocks_every_mode() {
        assert_eq!(
            proxy_mode_block(
                ProxyMode::System,
                Some(ProxyMode::Tun),
                ControllerReadiness::Connected,
                TunSupport::Supported
            ),
            Some(ProxyModeBlock::Busy)
        );
        assert_eq!(
            proxy_mode_block(
                ProxyMode::Tun,
                Some(ProxyMode::System),
                ControllerReadiness::Connected,
                TunSupport::Supported
            ),
            Some(ProxyModeBlock::Busy)
        );
    }

    #[test]
    fn source_reload_tun_restore_failure_forces_the_ui_mode_off() {
        let mut mode = ProxyMode::Tun;
        let apply = SourceRuntimeApply::from_result(Err(mihomo::LoadError::ProxyModeLost(
            "fixture restore failure".to_owned(),
        )));

        assert!(apply.reconcile_proxy_mode(&mut mode));
        assert_eq!(mode, ProxyMode::Off);
    }

    #[test]
    fn successful_source_reload_keeps_the_active_tun_mode() {
        let mut mode = ProxyMode::Tun;
        let apply = SourceRuntimeApply::from_result(Ok(mihomo::GeneratedProfileApply::Restarted));

        assert!(!apply.reconcile_proxy_mode(&mut mode));
        assert_eq!(mode, ProxyMode::Tun);
    }

    #[test]
    fn the_status_line_names_the_kernel_that_is_actually_running() {
        let connected = crate::mihomo::ControllerState::Connected {
            endpoint: "http://127.0.0.1:9090".to_owned(),
            version: "1.13.19".to_owned(),
            active_connections: 5,
            download_total: 0,
            upload_total: 0,
        };

        // Hard-coding "Mihomo" here would mislabel every sing-box session.
        assert_eq!(
            super::controller_status_label(
                &connected,
                "sing-box",
                crate::localization::Language::SimplifiedChinese
            ),
            "sing-box 1.13.19 · 5 条活动连接"
        );
        assert_eq!(
            super::controller_status_label(
                &connected,
                "Mihomo",
                crate::localization::Language::English
            ),
            "Mihomo 1.13.19 · 5 active connections"
        );
        assert_eq!(
            super::controller_status_label(
                &crate::mihomo::ControllerState::Disconnected,
                "sing-box",
                crate::localization::Language::SimplifiedChinese
            ),
            "sing-box 未连接"
        );
        // The failure reason must survive; the status bar is now its only home.
        assert_eq!(
            super::controller_status_label(
                &crate::mihomo::ControllerState::Failed {
                    endpoint: "http://127.0.0.1:9090".to_owned(),
                    message: "connection refused".to_owned(),
                },
                "Mihomo",
                crate::localization::Language::SimplifiedChinese
            ),
            "Mihomo 连接失败 · connection refused"
        );
    }
}
