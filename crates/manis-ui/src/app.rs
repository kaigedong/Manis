use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui::{
    AnyElement, Context, Div, Entity, Focusable, FontWeight, IntoElement, ParentElement, Render,
    Role, Stateful, Styled, Subscription, Task, Toggled, Window, div, img, prelude::*, px,
};
use gpui_component::{
    Disableable, IconName, Selectable, Sizable, WindowExt as _,
    button::{Button, ButtonGroup, ButtonVariant, ButtonVariants},
    spinner::Spinner,
    status_bar::StatusBar,
    tab::{Tab, TabBar},
};
use manis_core::{
    CompactNavigation, DomainRoutePrediction, KernelKind, ManagedPolicyGroup, ManagedPolicyIcon,
    ManagedPolicyStrategy, NodeIdentity, NodeWorkspaceState, PolicyCatalog, PolicyGroup,
    PolicyGroupId, PolicyNode, PolicyWorkspaceState, PrimaryWorkspace, ProxyId, ProxyMode,
    RoutePredictionReason, RouteQuery, RouteQueryError, RouteTarget, RoutingMode, WindowSizeClass,
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
    localization::{CountNoun, Language, LanguagePreference, Localizer, Message},
    mihomo::{
        self, ControllerRuntime, ControllerState, GeneratedProfileApply, KernelLogEntry,
        LiveRuntimeSession, LiveStreamStatus, LoadedProvider, LoadedSnapshot, ManagedRuntimeHealth,
        RemoteSourceRefreshInterval, StoredQxRuleSource, StoredSubscription, StoredVlessNode,
        SubscriptionPreviewError, SubscriptionStoreError,
    },
    rule_source::RuleDownloadError,
    subscription::{SourceKind, SubscriptionInputError, SubscriptionPreview},
    subscription_input::{
        SubscriptionInputChanged, SubscriptionInputSubmitted, SubscriptionTextInput,
    },
    system_proxy::{ProxyPorts, SystemProxySession, TunDnsSession},
    theme::{ControlSize, LayoutMetric, Radius, Space, TextRole, Theme},
};

mod activity;
mod configuration;
mod logs;
mod nodes;

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

#[derive(Clone, Debug, Default, Eq, PartialEq)]
enum RouteInspectorPrediction {
    #[default]
    Idle,
    Invalid(RouteQueryError),
    Ready(DomainRoutePrediction),
}

enum SourceRuntimeApply {
    Applied(GeneratedProfileApply),
    Failed(String),
    ProxyModeLost(String),
}

impl SourceRuntimeApply {
    fn from_result(result: Result<GeneratedProfileApply, mihomo::LoadError>) -> Self {
        match result {
            Ok(outcome) => Self::Applied(outcome),
            Err(mihomo::LoadError::ProxyModeLost(message)) => Self::ProxyModeLost(message),
            Err(error) => Self::Failed(error.to_string()),
        }
    }

    fn reconcile_proxy_mode(&self, mode: &mut ProxyMode) -> bool {
        if matches!(self, Self::ProxyModeLost(_)) && *mode == ProxyMode::Tun {
            *mode = ProxyMode::Off;
            record_event(
                LogLevel::Error,
                "proxy.mode.restore.ui_fallback",
                "active=off reason=tun_restore_failed",
            );
            return true;
        }
        false
    }

    fn status_suffix(&self, language: Language) -> String {
        match self {
            Self::Applied(GeneratedProfileApply::Updated) => language
                .text(
                    " · written to the Manis-managed configuration",
                    " · 已写入 Manis 托管配置",
                )
                .to_owned(),
            Self::Applied(GeneratedProfileApply::Restarted) => language
                .text(
                    " · Manis-managed kernel safely reloaded",
                    " · Manis 托管内核已安全重载",
                )
                .to_owned(),
            Self::Failed(message) => format!(
                "{}{message}",
                language.text(
                    " · saved, but the managed configuration could not be applied: ",
                    " · 持久化已完成，但托管配置应用失败："
                )
            ),
            Self::ProxyModeLost(message) => format!(
                "{}{message}",
                language.text(
                    " · kernel reloaded, but TUN could not be restored and was turned off: ",
                    " · 内核已重载，但 TUN 恢复失败，已回退为关闭："
                )
            ),
        }
    }
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
    source: SecretUrl,
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
            source: stored.source,
            state: ImportedSubscriptionState::Pending(kind),
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
            !matches!(
                source.state,
                ImportedSubscriptionState::None
                    | ImportedSubscriptionState::Pending(_)
                    | ImportedSubscriptionState::Refreshing(_)
                    | ImportedSubscriptionState::Removing(_)
            ) && source
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
                    source
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
    Rules,
    Settings,
}

impl PolicyDetailTab {
    const fn index(self) -> usize {
        match self {
            Self::Nodes => 0,
            Self::Rules => 1,
            Self::Settings => 2,
        }
    }

    const fn from_index(index: usize) -> Self {
        match index {
            1 => Self::Rules,
            2 => Self::Settings,
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

pub struct ManisApp {
    localizer: Localizer,
    primary_workspace: PrimaryWorkspace,
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
    saved_vless_nodes: Vec<StoredVlessNode>,
    qx_rule_sources: Vec<StoredQxRuleSource>,
    routing_rule_group_order: Vec<String>,
    qx_rule_feedback: QxRuleImportFeedback,
    qx_rule_target_policy: String,
    qx_rule_import_generation: u64,
    qx_rule_source_refreshes: BTreeMap<String, QxRuleSourceRefreshState>,
    qx_rule_source_target_updates: BTreeMap<String, u64>,
    qx_rule_target_popover: Option<String>,
    source_refresh_retry_not_before: BTreeMap<String, u64>,
    source_refresh_scheduler: SourceRefreshSchedulerState,
    managed_policy_groups: Vec<ManagedPolicyGroup>,
    node_selection_preferences: mihomo::NodeSelectionPreferences,
    managed_policy_draft: Option<ManagedPolicyDraft>,
    managed_policy_editor_popover: Option<PolicyEditorPopover>,
    pending_policy_benchmark_name: Option<String>,
    group_benchmarks: BTreeMap<String, GroupBenchmarkState>,
    group_benchmark_generation: u64,
    group_benchmark_active_generation: Option<u64>,
    managed_policy_runtime_states: BTreeMap<String, ManagedPolicyRuntimeState>,
    managed_policy_runtime_generation: u64,
    source_store_error: Option<SubscriptionStoreError>,
    proxy_mode: ProxyMode,
    proxy_mode_busy: Option<ProxyMode>,
    routing_mode: RoutingMode,
    routing_mode_busy: Option<RoutingMode>,
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
    inspector_open: bool,
    route_prediction: RouteInspectorPrediction,
    dark: bool,
    status: String,
    subscription_input: Option<Entity<SubscriptionTextInput>>,
    subscription_feedback: SubscriptionFeedback,
    subscription_input_events: Option<Subscription>,
    qx_rule_input: Option<Entity<SubscriptionTextInput>>,
    qx_rule_input_events: Option<Subscription>,
    manual_rules: Vec<crate::manual_rule::ManualRule>,
    manual_rule_inputs: Vec<Entity<SubscriptionTextInput>>,
    manual_rule_kinds: Vec<crate::manual_rule::ManualRuleKind>,
    manual_rule_condition_count: usize,
    manual_rule_target: String,
    manual_rule_editor_state: ManualRuleEditorState,
    manual_rule_popover: Option<ManualRulePopover>,
    manual_rule_error: Option<crate::manual_rule::ManualRuleError>,
    policy_group_name_input: Option<Entity<SubscriptionTextInput>>,
    policy_group_filter_input: Option<Entity<SubscriptionTextInput>>,
    activity_search_input: Option<Entity<SubscriptionTextInput>>,
    activity_search_events: Option<Subscription>,
    logs_search_input: Option<Entity<SubscriptionTextInput>>,
    logs_search_events: Option<Subscription>,
    route_domain_input: Option<Entity<SubscriptionTextInput>>,
    route_domain_input_events: Vec<Subscription>,
    #[allow(dead_code)]
    app_lifecycle_events: Option<Subscription>,
}

struct RouteInspectorSheetContent {
    app: Entity<ManisApp>,
}

impl Render for RouteInspectorSheetContent {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let app = self.app.clone();
        let current = self.app.read(cx);
        current.inspector_sheet_body(current.theme(), move |_, _, cx| {
            app.update(cx, ManisApp::predict_route);
        })
    }
}

struct StoredWorkspace {
    imported_subscriptions: Vec<ImportedSubscription>,
    saved_vless_nodes: Vec<StoredVlessNode>,
    qx_rule_sources: Vec<StoredQxRuleSource>,
    routing_rule_group_order: Vec<String>,
    collapsed_groups: Vec<String>,
    managed_policy_groups: Vec<ManagedPolicyGroup>,
    node_selection_preferences: mihomo::NodeSelectionPreferences,
    routing_mode: RoutingMode,
    error: Option<SubscriptionStoreError>,
}

impl StoredWorkspace {
    fn load(directory: Option<&PathBuf>) -> Self {
        let Some(directory) = directory else {
            return Self {
                imported_subscriptions: Vec::new(),
                saved_vless_nodes: Vec::new(),
                qx_rule_sources: Vec::new(),
                routing_rule_group_order: Vec::new(),
                collapsed_groups: Vec::new(),
                managed_policy_groups: Vec::new(),
                node_selection_preferences: mihomo::NodeSelectionPreferences::default(),
                routing_mode: RoutingMode::Rule,
                error: None,
            };
        };
        let subscriptions = mihomo::load_subscription_sources_in(directory);
        let nodes = mihomo::load_vless_sources_in(directory);
        let qx_rule_sources = mihomo::load_qx_rule_sources_in(directory);
        let routing_rule_group_order = mihomo::load_routing_rule_group_order_in(directory);
        let collapsed = mihomo::load_collapsed_groups_in(directory);
        let policy_groups = mihomo::load_managed_policy_groups_in(directory);
        let node_selection_preferences = mihomo::load_node_selection_preferences_in(directory);
        let routing_mode = mihomo::load_routing_mode_in(directory);
        let error = [
            subscriptions.is_err(),
            nodes.is_err(),
            qx_rule_sources.is_err(),
            routing_rule_group_order.is_err(),
            collapsed.is_err(),
            policy_groups.is_err(),
            node_selection_preferences.is_err(),
            routing_mode.is_err(),
        ]
        .into_iter()
        .any(std::convert::identity)
        .then_some(SubscriptionStoreError::StoredSourceUnavailable);
        Self {
            imported_subscriptions: subscriptions
                .unwrap_or_default()
                .into_iter()
                .map(ImportedSubscription::from_stored)
                .collect(),
            saved_vless_nodes: nodes.unwrap_or_default(),
            qx_rule_sources: qx_rule_sources.unwrap_or_default(),
            routing_rule_group_order: routing_rule_group_order.unwrap_or_default(),
            collapsed_groups: collapsed.unwrap_or_default(),
            managed_policy_groups: policy_groups.unwrap_or_default(),
            node_selection_preferences: node_selection_preferences.unwrap_or_default(),
            routing_mode: routing_mode.unwrap_or_default(),
            error,
        }
    }
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
                runtime.profile_source().label()
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
        app.app_lifecycle_events = Some(cx.on_app_quit(Self::shutdown_for_quit));
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

    #[allow(clippy::too_many_lines)]
    fn with_runtime_and_store(
        runtime: KernelRuntime,
        subscription_store_dir: Option<PathBuf>,
    ) -> Self {
        let localizer = Localizer::load(subscription_store_dir.as_deref());
        let language = localizer.language();
        let mut status = runtime.initial_status_in(language);
        let StoredWorkspace {
            imported_subscriptions,
            saved_vless_nodes,
            qx_rule_sources,
            routing_rule_group_order,
            collapsed_groups,
            managed_policy_groups,
            node_selection_preferences,
            routing_mode,
            error: source_store_error,
        } = StoredWorkspace::load(subscription_store_dir.as_ref());
        let default_rule_target = managed_policy_groups
            .first()
            .map_or_else(|| "DIRECT".to_owned(), |group| group.name.clone());
        if let Some(directory) = subscription_store_dir.as_ref()
            && (!imported_subscriptions.is_empty()
                || !saved_vless_nodes.is_empty()
                || !qx_rule_sources.is_empty()
                || !managed_policy_groups.is_empty()
                || routing_mode != RoutingMode::Rule)
        {
            status = match runtime.apply_saved_sources(directory) {
                Ok(GeneratedProfileApply::Updated) => language
                    .text(
                        "Saved sources written to the Manis-managed configuration",
                        "已将保存来源写入 Manis 托管配置",
                    )
                    .to_owned(),
                Ok(GeneratedProfileApply::Restarted) => language
                    .text(
                        "Saved sources applied to the Manis-managed kernel",
                        "已将保存来源应用到 Manis 托管内核",
                    )
                    .to_owned(),
                Err(error) => format!(
                    "{}{error}",
                    language.text(
                        "Saved sources loaded, but the managed configuration was not applied: ",
                        "保存来源已载入，但托管配置未应用："
                    )
                ),
            };
        }
        let mut node_workspace = NodeWorkspaceState::default();
        node_workspace.replace_collapsed_groups(collapsed_groups.iter().map(String::as_str));
        Self {
            localizer,
            primary_workspace: PrimaryWorkspace::default(),
            node_workspace,
            workspace: PolicyWorkspaceState::default(),
            expanded_policy_group: None,
            policy_detail_tab: PolicyDetailTab::default(),
            catalog: None,
            runtime,
            kernel_switch_state: KernelSwitchState::Idle,
            mihomo_core_update_state: {
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
            },
            controller: ControllerState::Disconnected,
            observed_routes: Vec::new(),
            source_providers: Vec::new(),
            subscription_preview_providers: Vec::new(),
            subscription_preview_generation: 0,
            subscription_store_dir,
            imported_subscriptions,
            saved_vless_nodes,
            qx_rule_sources,
            routing_rule_group_order,
            qx_rule_feedback: QxRuleImportFeedback::Idle,
            qx_rule_target_policy: default_rule_target.clone(),
            qx_rule_import_generation: 0,
            qx_rule_source_refreshes: BTreeMap::new(),
            qx_rule_source_target_updates: BTreeMap::new(),
            qx_rule_target_popover: None,
            source_refresh_retry_not_before: BTreeMap::new(),
            source_refresh_scheduler: SourceRefreshSchedulerState::Stopped,
            managed_policy_groups,
            node_selection_preferences,
            managed_policy_draft: None,
            managed_policy_editor_popover: None,
            pending_policy_benchmark_name: None,
            group_benchmarks: BTreeMap::new(),
            group_benchmark_generation: 0,
            group_benchmark_active_generation: None,
            managed_policy_runtime_states: BTreeMap::new(),
            managed_policy_runtime_generation: 0,
            source_store_error,
            proxy_mode: ProxyMode::Off,
            proxy_mode_busy: None,
            routing_mode,
            routing_mode_busy: None,
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
            inspector_open: false,
            route_prediction: RouteInspectorPrediction::Idle,
            dark: false,
            status,
            subscription_input: None,
            subscription_feedback: SubscriptionFeedback::Idle,
            subscription_input_events: None,
            qx_rule_input: None,
            qx_rule_input_events: None,
            manual_rules: Vec::new(),
            manual_rule_inputs: Vec::new(),
            manual_rule_kinds: Vec::new(),
            manual_rule_condition_count: 1,
            manual_rule_target: default_rule_target,
            manual_rule_editor_state: ManualRuleEditorState::Closed,
            manual_rule_popover: None,
            manual_rule_error: None,
            policy_group_name_input: None,
            policy_group_filter_input: None,
            activity_search_input: None,
            activity_search_events: None,
            logs_search_input: None,
            logs_search_events: None,
            route_domain_input: None,
            route_domain_input_events: Vec::new(),
            app_lifecycle_events: None,
        }
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
                        language.text(
                            "System proxy recovery needs attention: ",
                            "系统代理恢复需要处理："
                        )
                    );
                }
            }
            Err(_poisoned) => {
                language
                    .text(
                        "System proxy recovery state is unavailable",
                        "系统代理恢复状态不可用",
                    )
                    .clone_into(&mut self.status);
            }
        }
        match self.tun_dns.lock() {
            Ok(mut dns) => {
                if let Err(error) = dns.recover_stale_with_language(language) {
                    self.status = format!(
                        "{}{error}",
                        language.text(
                            "TUN DNS recovery needs attention: ",
                            "TUN DNS 恢复需要处理："
                        )
                    );
                }
            }
            Err(_poisoned) => {
                language
                    .text(
                        "TUN DNS recovery state is unavailable",
                        "TUN DNS 恢复状态不可用",
                    )
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
        if let Some(input) = self.subscription_input.as_ref() {
            input.update(cx, |input, cx| {
                input.set_theme(theme, self.dark, cx);
                input.set_language(language, cx);
            });
            return;
        }

        let input = cx.new(|cx| {
            SubscriptionTextInput::new_with_language(language, theme, self.dark, window, cx)
        });
        let events = cx.subscribe(&input, |this, _input, _: &SubscriptionInputChanged, cx| {
            if this.subscription_feedback != SubscriptionFeedback::Idle {
                this.subscription_feedback = SubscriptionFeedback::Idle;
                cx.notify();
            }
        });
        self.subscription_input = Some(input);
        self.subscription_input_events = Some(events);
        self.restore_imported_subscriptions(cx);
    }

    fn ensure_qx_rule_input(&mut self, theme: Theme, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(input) = self.qx_rule_input.as_ref() {
            input.update(cx, |input, cx| input.set_theme(theme, self.dark, cx));
            return;
        }
        let input = cx.new(|cx| {
            SubscriptionTextInput::new_field(
                "qx-rule-url-input",
                "https://example.com/rules.list",
                16 * 1024,
                theme,
                self.dark,
                window,
                cx,
            )
        });
        let events = cx.subscribe(&input, |this, _input, _: &SubscriptionInputChanged, cx| {
            if this.qx_rule_feedback != QxRuleImportFeedback::Idle {
                this.qx_rule_feedback = QxRuleImportFeedback::Idle;
                cx.notify();
            }
        });
        self.qx_rule_input = Some(input);
        self.qx_rule_input_events = Some(events);
    }

    fn ensure_policy_group_inputs(
        &mut self,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let language = self.language();
        for input in [
            self.policy_group_name_input.as_ref(),
            self.policy_group_filter_input.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            input.update(cx, |input, cx| input.set_theme(theme, self.dark, cx));
        }
        if self.policy_group_name_input.is_none() {
            self.policy_group_name_input = Some(cx.new(|cx| {
                SubscriptionTextInput::new_field(
                    "policy-group-name-input",
                    language.text("For example: Hong Kong Auto", "例如：香港自动优选"),
                    96,
                    theme,
                    self.dark,
                    window,
                    cx,
                )
            }));
        }
        if self.policy_group_filter_input.is_none() {
            self.policy_group_filter_input = Some(cx.new(|cx| {
                SubscriptionTextInput::new_field(
                    "policy-group-filter-input",
                    language.text("For example: Hong Kong", "例如：Hong Kong"),
                    256,
                    theme,
                    self.dark,
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
        if let Some(input) = self.activity_search_input.as_ref() {
            input.update(cx, |input, cx| {
                input.set_theme(theme, self.dark, cx);
                input.set_placeholder(
                    language.text(
                        "Filter by target, process, rule, or route",
                        "筛选目标、进程、规则或路径",
                    ),
                    cx,
                );
            });
        } else {
            let input = cx.new(|cx| {
                SubscriptionTextInput::new_field(
                    "activity-search-input",
                    language.text(
                        "Filter by target, process, rule, or route",
                        "筛选目标、进程、规则或路径",
                    ),
                    256,
                    theme,
                    self.dark,
                    window,
                    cx,
                )
            });
            let events = cx.subscribe(&input, |_this, _input, _: &SubscriptionInputChanged, cx| {
                cx.notify();
            });
            self.activity_search_input = Some(input);
            self.activity_search_events = Some(events);
        }

        if let Some(input) = self.logs_search_input.as_ref() {
            input.update(cx, |input, cx| {
                input.set_theme(theme, self.dark, cx);
                input.set_placeholder(
                    language.text(
                        "Filter events, errors, or OP number",
                        "筛选事件、错误或 OP 编号",
                    ),
                    cx,
                );
            });
        } else {
            let input = cx.new(|cx| {
                SubscriptionTextInput::new_field(
                    "logs-search-input",
                    language.text(
                        "Filter events, errors, or OP number",
                        "筛选事件、错误或 OP 编号",
                    ),
                    256,
                    theme,
                    self.dark,
                    window,
                    cx,
                )
            });
            let events = cx.subscribe(&input, |_this, _input, _: &SubscriptionInputChanged, cx| {
                cx.notify();
            });
            self.logs_search_input = Some(input);
            self.logs_search_events = Some(events);
        }
    }

    fn ensure_route_domain_input(
        &mut self,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let language = self.language();
        let placeholder = language.text("For example: google.com:443", "例如：google.com:443");
        if let Some(input) = self.route_domain_input.as_ref() {
            input.update(cx, |input, cx| {
                input.set_theme(theme, self.dark, cx);
                input.set_placeholder(placeholder, cx);
            });
            return;
        }

        let input = cx.new(|cx| {
            SubscriptionTextInput::new_field(
                "route-domain-input",
                placeholder,
                253,
                theme,
                self.dark,
                window,
                cx,
            )
        });
        let changed = cx.subscribe(&input, |this, _input, _: &SubscriptionInputChanged, cx| {
            if this.route_prediction != RouteInspectorPrediction::Idle {
                this.route_prediction = RouteInspectorPrediction::Idle;
                cx.notify();
            }
        });
        let submitted = cx.subscribe(
            &input,
            |this, _input, _: &SubscriptionInputSubmitted, cx| {
                this.predict_route(cx);
            },
        );
        self.route_domain_input = Some(input);
        self.route_domain_input_events = vec![changed, submitted];
    }

    fn predict_route(&mut self, cx: &mut Context<Self>) {
        let Some(input) = self.route_domain_input.as_ref() else {
            return;
        };
        let value = input.read(cx).value().to_owned();
        let query = match RouteQuery::parse(&value) {
            Ok(query) => query,
            Err(error) => {
                self.route_prediction = RouteInspectorPrediction::Invalid(error);
                route_query_error_copy(error, self.language()).clone_into(&mut self.status);
                cx.notify();
                return;
            }
        };
        let Some(catalog) = self.catalog.as_ref() else {
            return;
        };
        let prediction = catalog.predict_route(&query);
        self.status = match &prediction {
            DomainRoutePrediction::Matched {
                query,
                uncertain_rules,
                ..
            } => {
                if self.language() == Language::English {
                    if uncertain_rules.is_empty() {
                        format!(
                            "Predicted route for {}:{}",
                            query.domain().as_str(),
                            query.port()
                        )
                    } else {
                        format!(
                            "Conditionally predicted route for {}:{}",
                            query.domain().as_str(),
                            query.port()
                        )
                    }
                } else {
                    if uncertain_rules.is_empty() {
                        format!("已预测 {}:{} 的路由", query.domain().as_str(), query.port())
                    } else {
                        format!(
                            "已按当前条件预测 {}:{} 的路由",
                            query.domain().as_str(),
                            query.port()
                        )
                    }
                }
            }
            DomainRoutePrediction::NeedsConnection { query, .. } => {
                if self.language() == Language::English {
                    format!(
                        "{}:{} needs more connection context",
                        query.domain().as_str(),
                        query.port()
                    )
                } else {
                    format!(
                        "{}:{} 需要更多连接条件",
                        query.domain().as_str(),
                        query.port()
                    )
                }
            }
        };
        self.route_prediction = RouteInspectorPrediction::Ready(prediction);
        trace_ui(UiEvent::RoutePredictionRequested);
        cx.notify();
    }

    #[allow(clippy::too_many_lines)]
    fn import_remote_subscription(
        &mut self,
        input: String,
        kind: SourceKind,
        cx: &mut Context<Self>,
    ) {
        let language = self.language();
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            self.subscription_feedback =
                SubscriptionFeedback::StoreFailed(SubscriptionStoreError::DataDirectoryUnavailable);
            language
                .text(
                    "The subscription storage location is unavailable",
                    "无法确定订阅保存位置",
                )
                .clone_into(&mut self.status);
            trace_ui(UiEvent::SourceImportFailed);
            cx.notify();
            return;
        };
        self.subscription_preview_generation = self.subscription_preview_generation.wrapping_add(1);
        let generation = self.subscription_preview_generation;
        self.subscription_feedback = SubscriptionFeedback::Importing(kind);
        language
            .text(
                "Validating nodes and importing subscription",
                "正在验证节点并导入订阅",
            )
            .clone_into(&mut self.status);
        trace_ui(UiEvent::SourceImportStarted);
        if let Some(input) = self.subscription_input.as_ref() {
            input.update(cx, |input, cx| input.set_enabled(false, cx));
        }

        let executor = cx.background_executor().clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    let providers = mihomo::preview_subscription(&input)
                        .map_err(ImportSubscriptionError::Preview)?;
                    let mut subscription = mihomo::save_subscription_source_in(&store_dir, &input)
                        .map_err(ImportSubscriptionError::Store)?;
                    let proxy_nameservers =
                        mihomo::discover_subscription_proxy_nameservers(&subscription.source);
                    if !proxy_nameservers.is_empty() {
                        subscription = mihomo::update_subscription_source_proxy_nameservers_in(
                            &store_dir,
                            &subscription.id,
                            &proxy_nameservers,
                        )
                        .map_err(ImportSubscriptionError::Store)?;
                    }
                    let apply = SourceRuntimeApply::from_result(
                        runtime.apply_saved_sources(&store_dir),
                    );
                    Ok::<_, ImportSubscriptionError>((subscription, providers, apply))
                })
                .await;
            this.update(cx, |this, cx| {
                let language = this.language();
                if this.subscription_preview_generation != generation {
                    return;
                }
                if let Some(input) = this.subscription_input.as_ref() {
                    input.update(cx, |input, cx| input.set_enabled(true, cx));
                }
                match result {
                    Ok((subscription, providers, apply)) => {
                        let node_count: usize =
                            providers.iter().map(|provider| provider.nodes.len()).sum();
                        let provider_count = providers.len();
                        let stored_id = subscription.id.clone();
                        if let Some(existing) = this
                            .imported_subscriptions
                            .iter_mut()
                            .find(|existing| existing.id == stored_id)
                        {
                            existing.source = subscription.source;
                            existing.state = ImportedSubscriptionState::Ready(kind);
                            existing.providers.clone_from(&providers);
                            existing.refresh_interval = subscription.refresh_interval;
                            existing.last_successful_update_unix_secs =
                                subscription.last_successful_update_unix_secs;
                        } else {
                            this.imported_subscriptions.push(ImportedSubscription {
                                id: stored_id,
                                source: subscription.source,
                                state: ImportedSubscriptionState::Ready(kind),
                                providers: providers.clone(),
                                generation,
                                refresh_interval: subscription.refresh_interval,
                                last_successful_update_unix_secs: subscription
                                    .last_successful_update_unix_secs,
                            });
                        }
                        this.subscription_preview_providers = providers;
                        this.subscription_feedback = SubscriptionFeedback::Idle;
                        if let Some(input) = this.subscription_input.as_ref() {
                            input.update(cx, SubscriptionTextInput::clear_without_event);
                        }
                        apply.reconcile_proxy_mode(&mut this.proxy_mode);
                        this.status = if language == Language::English {
                            format!(
                                "Subscription imported · {} groups · {provider_count} sources · {node_count} nodes{}",
                                this.imported_subscriptions.len(),
                                apply.status_suffix(language)
                            )
                        } else {
                            format!(
                                "订阅已导入 · 共 {} 个订阅组 · {provider_count} 个来源 · {node_count} 个节点{}",
                                this.imported_subscriptions.len(),
                                apply.status_suffix(language)
                            )
                        };
                        trace_ui(UiEvent::SourceImportSucceeded);
                    }
                    Err(ImportSubscriptionError::Preview(error)) => {
                        this.subscription_feedback = SubscriptionFeedback::PreviewFailed(error);
                        this.status = format!(
                            "{}{error}",
                            language.text(
                                "Subscription import failed: ",
                                "订阅导入失败："
                            )
                        );
                        trace_ui(UiEvent::SourceImportFailed);
                    }
                    Err(ImportSubscriptionError::Store(error)) => {
                        this.subscription_feedback = SubscriptionFeedback::StoreFailed(error);
                        this.status = format!(
                            "{}{error}",
                            language.text("Could not save subscription: ", "订阅保存失败：")
                        );
                        trace_ui(UiEvent::SourceImportFailed);
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
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

    #[allow(clippy::too_many_lines)]
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
        ) {
            return;
        }
        let kind = source_kind(&subscription.source);
        let source = subscription.source.clone();
        self.subscription_preview_generation = self.subscription_preview_generation.wrapping_add(1);
        let generation = self.subscription_preview_generation;
        subscription.generation = generation;
        subscription.state = ImportedSubscriptionState::Refreshing(kind);
        language
            .text("Updating subscription nodes", "正在更新订阅节点")
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
                    let mut stored = mihomo::mark_subscription_source_update_success_in(
                        &store_dir,
                        &task_id,
                        mihomo::current_unix_secs(),
                    )
                    .map_err(ImportSubscriptionError::Store)?;
                    if !proxy_nameservers.is_empty() {
                        stored = mihomo::update_subscription_source_proxy_nameservers_in(
                            &store_dir,
                            &task_id,
                            &proxy_nameservers,
                        )
                        .map_err(ImportSubscriptionError::Store)?;
                    }
                    let apply =
                        SourceRuntimeApply::from_result(runtime.apply_saved_sources(&store_dir));
                    Ok::<_, ImportSubscriptionError>((providers, stored, apply))
                })
                .await;
            this.update(cx, |this, cx| {
                let language = this.language();
                let Some(subscription) = this
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
                    Ok((providers, stored, apply)) => {
                        let node_count: usize =
                            providers.iter().map(|provider| provider.nodes.len()).sum();
                        subscription.providers = providers;
                        subscription.state = ImportedSubscriptionState::Ready(kind);
                        subscription.refresh_interval = stored.refresh_interval;
                        subscription.last_successful_update_unix_secs =
                            stored.last_successful_update_unix_secs;
                        this.source_refresh_retry_not_before
                            .remove(&DueRemoteSource::Subscription(id.clone()).scheduler_key());
                        apply.reconcile_proxy_mode(&mut this.proxy_mode);
                        this.status = if language == Language::English {
                            format!(
                                "Subscription updated · {node_count} nodes{}",
                                apply.status_suffix(language)
                            )
                        } else {
                            format!(
                                "订阅更新完成 · {node_count} 个节点{}",
                                apply.status_suffix(language)
                            )
                        };
                        trace_ui(UiEvent::SourceRestoreSucceeded);
                    }
                    Err(ImportSubscriptionError::Preview(error)) => {
                        subscription.state = ImportedSubscriptionState::Unavailable(kind, error);
                        this.status = format!(
                            "{}{error}",
                            language.text("Subscription update failed: ", "订阅更新失败：")
                        );
                        trace_ui(UiEvent::SourceRestoreFailed);
                    }
                    Err(ImportSubscriptionError::Store(error)) => {
                        subscription.state = ImportedSubscriptionState::StoreError(error);
                        this.status = format!(
                            "{}{error}",
                            language.text(
                                "Subscription loaded, but its update time could not be saved: ",
                                "订阅已读取，但更新时间保存失败："
                            )
                        );
                        trace_ui(UiEvent::SourceRestoreFailed);
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn source_refresh_busy(&self) -> bool {
        self.imported_subscriptions.iter().any(|source| {
            matches!(
                source.state,
                ImportedSubscriptionState::Refreshing(_) | ImportedSubscriptionState::Removing(_)
            )
        }) || self.qx_rule_feedback == QxRuleImportFeedback::Importing
            || self
                .qx_rule_source_refreshes
                .values()
                .any(QxRuleSourceRefreshState::is_refreshing)
            || !self.qx_rule_source_target_updates.is_empty()
    }

    fn refresh_next_due_source(&mut self, cx: &mut Context<Self>) {
        if self.source_refresh_busy() {
            return;
        }
        let now = mihomo::current_unix_secs();
        let due = next_due_remote_source(
            &self.imported_subscriptions,
            &self.qx_rule_sources,
            &self.source_refresh_retry_not_before,
            now,
        );
        if let Some(source) = due.as_ref() {
            self.source_refresh_retry_not_before
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
        if self.source_refresh_scheduler == SourceRefreshSchedulerState::Started
            || self.subscription_store_dir.is_none()
        {
            return;
        }
        self.source_refresh_scheduler = SourceRefreshSchedulerState::Started;
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
        self.subscription_feedback = SubscriptionFeedback::Idle;
        subscription.state = ImportedSubscriptionState::Removing(kind);
        language
            .text("Removing imported subscription", "正在移除已导入订阅")
            .clone_into(&mut self.status);
        trace_ui(UiEvent::SourceRemoveStarted);

        let executor = cx.background_executor().clone();
        let remove_id = id.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    mihomo::remove_subscription_source_in(&store_dir, &remove_id)?;
                    Ok::<_, SubscriptionStoreError>(SourceRuntimeApply::from_result(
                        runtime.apply_saved_sources(&store_dir),
                    ))
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
                    Ok(apply) => {
                        this.imported_subscriptions.remove(index);
                        this.source_refresh_retry_not_before
                            .remove(&DueRemoteSource::Subscription(id.clone()).scheduler_key());
                        apply.reconcile_proxy_mode(&mut this.proxy_mode);
                        this.status = format!(
                            "{}{}",
                            language.text("Imported subscription removed", "已移除导入订阅"),
                            apply.status_suffix(language)
                        );
                        trace_ui(UiEvent::SourceRemoveSucceeded);
                    }
                    Err(error) => {
                        this.imported_subscriptions[index].state =
                            ImportedSubscriptionState::StoreError(error);
                        this.status = format!(
                            "{}{error}",
                            language.text("Could not remove subscription: ", "移除订阅失败：")
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

    fn persist_node_workspace(&mut self) {
        let Some(store_dir) = self.subscription_store_dir.as_ref() else {
            return;
        };
        if let Err(error) =
            mihomo::save_collapsed_groups_in(store_dir, self.node_workspace.collapsed_group_ids())
        {
            self.source_store_error = Some(error);
            "无法保存节点来源展开状态".clone_into(&mut self.status);
        }
    }

    fn theme(&self) -> Theme {
        if self.dark {
            Theme::dark()
        } else {
            Theme::light()
        }
    }

    fn policy_groups(&self) -> impl Iterator<Item = &PolicyGroup> {
        self.catalog.iter().flat_map(PolicyCatalog::iter)
    }

    fn selected_policy(&self) -> Option<&PolicyGroup> {
        self.catalog
            .as_ref()
            .map(|catalog| catalog.select(self.workspace.selected_group.as_ref()))
    }

    fn policy_group_benchmarkable(group: &PolicyGroup) -> bool {
        !group.nodes.is_empty()
    }

    fn source_group_benchmark_key(id: &str) -> String {
        format!("source:{id}")
    }

    fn managed_policy_benchmark_key(id: &str) -> String {
        format!("user:{id}")
    }

    fn policy_group_benchmark_key(id: &manis_core::PolicyGroupId) -> String {
        format!("policy:{}", id.as_str())
    }

    fn begin_group_benchmark(&mut self, key: String) -> Option<u64> {
        if self.group_benchmark_active_generation.is_some() {
            return None;
        }
        self.group_benchmark_generation = self.group_benchmark_generation.wrapping_add(1);
        let generation = self.group_benchmark_generation;
        self.group_benchmarks
            .insert(key, GroupBenchmarkState::running(generation));
        self.group_benchmark_active_generation = Some(generation);
        Some(generation)
    }

    fn poll_group_benchmark_progress(
        &mut self,
        generation: u64,
        key: String,
        updates: GroupBenchmarkProgressQueue,
        cx: &mut Context<Self>,
    ) {
        let drained = updates
            .lock()
            .map(|mut updates| updates.drain(..).collect::<Vec<_>>())
            .unwrap_or_default();
        let mut changed = false;
        if let Some(state) = self.group_benchmarks.get_mut(&key) {
            for (name, delay) in drained {
                changed |= state.record(generation, &name, delay);
            }
        }
        if changed {
            cx.notify();
        }
        if self.group_benchmark_active_generation != Some(generation) {
            return;
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(40))
                .await;
            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    this.poll_group_benchmark_progress(generation, key, updates, cx);
                });
            }
        })
        .detach();
    }

    #[allow(clippy::too_many_lines)]
    fn policy_icon_visual(
        icon: ManagedPolicyIcon,
        policy_name: &str,
        size: f32,
        theme: Theme,
    ) -> Div {
        let glyph_color = theme.action_primary;
        let glyph = match icon {
            ManagedPolicyIcon::None => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .font_weight(FontWeight::BOLD)
                .text_color(glyph_color)
                .child(
                    policy_name
                        .chars()
                        .next()
                        .map_or_else(|| "?".to_owned(), |character| character.to_string()),
                ),
            ManagedPolicyIcon::Bolt => div()
                .relative()
                .size(px(20.0))
                .child(
                    div()
                        .absolute()
                        .left(px(9.0))
                        .top(px(1.0))
                        .w(px(5.0))
                        .h(px(8.0))
                        .rounded_sm()
                        .bg(glyph_color),
                )
                .child(
                    div()
                        .absolute()
                        .left(px(6.0))
                        .top(px(7.0))
                        .w(px(8.0))
                        .h(px(6.0))
                        .rounded_sm()
                        .bg(glyph_color),
                )
                .child(
                    div()
                        .absolute()
                        .left(px(6.0))
                        .top(px(12.0))
                        .w(px(5.0))
                        .h(px(7.0))
                        .rounded_sm()
                        .bg(glyph_color),
                ),
            ManagedPolicyIcon::Globe => div()
                .relative()
                .size(px(20.0))
                .rounded_full()
                .border_2()
                .border_color(glyph_color)
                .child(
                    div()
                        .absolute()
                        .left(px(7.0))
                        .top(px(1.0))
                        .w(px(2.0))
                        .h(px(14.0))
                        .rounded_full()
                        .bg(glyph_color),
                )
                .child(
                    div()
                        .absolute()
                        .left(px(1.0))
                        .top(px(7.0))
                        .w(px(14.0))
                        .h(px(2.0))
                        .rounded_full()
                        .bg(glyph_color),
                ),
            ManagedPolicyIcon::Shield => div()
                .size(px(19.0))
                .rounded_md()
                .border_2()
                .border_color(glyph_color)
                .flex()
                .items_center()
                .justify_center()
                .child(div().size(px(7.0)).rounded_full().bg(glyph_color)),
            ManagedPolicyIcon::Compass => div()
                .relative()
                .size(px(20.0))
                .rounded_full()
                .border_2()
                .border_color(glyph_color)
                .child(
                    div()
                        .absolute()
                        .left(px(7.0))
                        .top(px(3.0))
                        .w(px(3.0))
                        .h(px(10.0))
                        .rounded_full()
                        .bg(glyph_color),
                )
                .child(
                    div()
                        .absolute()
                        .left(px(5.0))
                        .top(px(7.0))
                        .size(px(7.0))
                        .rounded_full()
                        .border_2()
                        .border_color(theme.surface_high),
                ),
        };
        div()
            .size(px(size))
            .rounded_full()
            .bg(theme.action_soft)
            .flex()
            .items_center()
            .justify_center()
            .child(glyph)
    }

    fn policy_group_icon(
        id: &str,
        icon: ManagedPolicyIcon,
        policy_name: &str,
        benchmarkable: bool,
        running: bool,
        theme: Theme,
        listener: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
    ) -> Stateful<Div> {
        div()
            .id(format!("policy-icon-{id}"))
            .size(px(38.0))
            .flex_shrink_0()
            .rounded_full()
            .when(benchmarkable, |avatar| {
                avatar
                    .role(Role::Button)
                    .aria_label(if running {
                        "策略组测速中"
                    } else {
                        "测试策略组候选项延迟"
                    })
                    .tab_stop(!running)
                    .focusable()
                    .on_click(listener)
            })
            .when(benchmarkable && !running, gpui::Styled::cursor_pointer)
            .when(running, |avatar| {
                avatar
                    .bg(theme.action_soft)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(Self::benchmark_latency_spinner(
                        format!("{id}-policy-icon-spinner"),
                        theme,
                    ))
            })
            .when(!running, |avatar| {
                avatar.child(Self::policy_icon_visual(icon, policy_name, 38.0, theme))
            })
    }

    fn group_benchmark_icon(
        id: &str,
        running: bool,
        theme: Theme,
        listener: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
    ) -> Stateful<Div> {
        let bar_color = if running {
            theme.action_primary
        } else {
            theme.text_secondary
        };
        div()
            .id(format!("group-benchmark-{id}"))
            .role(Role::Button)
            .aria_label(if running {
                "分组测速中"
            } else {
                "测试该分组延迟"
            })
            .tab_stop(!running)
            .focusable()
            .size(px(30.0))
            .flex_shrink_0()
            .rounded_md()
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_high)
            .flex()
            .justify_center()
            .when(running, |button| {
                button.items_center().child(Self::benchmark_latency_spinner(
                    format!("{id}-button-spinner"),
                    theme,
                ))
            })
            .when(!running, |button| {
                button
                    .items_end()
                    .gap(px(2.0))
                    .pb(px(7.0))
                    .child(div().w(px(2.0)).h(px(5.0)).rounded_full().bg(bar_color))
                    .child(div().w(px(2.0)).h(px(9.0)).rounded_full().bg(bar_color))
                    .child(div().w(px(2.0)).h(px(13.0)).rounded_full().bg(bar_color))
            })
            .when(!running, gpui::Styled::cursor_pointer)
            .on_click(listener)
    }

    fn benchmark_latency_content(
        state: GroupBenchmarkNodeState,
        idle_label: String,
        spinner_id: &str,
        theme: Theme,
    ) -> Div {
        let cell = div()
            .min_w(px(42.0))
            .flex()
            .items_center()
            .justify_end()
            .font_weight(FontWeight::SEMIBOLD)
            .text_size(TextRole::Data.size())
            .line_height(TextRole::Data.line_height());
        match state {
            GroupBenchmarkNodeState::Idle => cell.text_color(theme.text_tertiary).child(idle_label),
            GroupBenchmarkNodeState::Pending => cell.child(Self::benchmark_latency_spinner(
                spinner_id.to_owned(),
                theme,
            )),
            GroupBenchmarkNodeState::Measured(delay) => cell
                .text_color(theme.status_success)
                .child(format!("{delay} ms")),
            GroupBenchmarkNodeState::Failed => cell.text_color(theme.route_trace).child("失败"),
        }
    }

    fn benchmark_latency_spinner(id: String, theme: Theme) -> impl IntoElement {
        div().id(id).size(px(14.0)).child(
            Spinner::new()
                .with_size(px(14.0))
                .color(theme.action_primary.into()),
        )
    }

    fn policy_benchmark_status(
        language: Language,
        kind: manis_core::PolicyGroupKind,
        current: Option<&str>,
        summary: GroupBenchmarkSummary,
    ) -> String {
        match (language, kind.is_automatic(), current) {
            (Language::English, true, Some(current)) => format!(
                "Policy benchmark complete: {}/{} succeeded · current optimum {current}",
                summary.succeeded, summary.total
            ),
            (Language::English, true, None) => format!(
                "Policy benchmark complete: {}/{} succeeded · no single fixed exit",
                summary.succeeded, summary.total
            ),
            (Language::English, false, _) => format!(
                "Policy benchmark complete: {}/{} candidates succeeded",
                summary.succeeded, summary.total
            ),
            (Language::SimplifiedChinese, true, Some(current)) => format!(
                "策略组测速完成：{}/{} 成功 · 当前优选 {current}",
                summary.succeeded, summary.total
            ),
            (Language::SimplifiedChinese, true, None) => format!(
                "策略组测速完成：{}/{} 成功 · 该策略没有单一固定出口",
                summary.succeeded, summary.total
            ),
            (Language::SimplifiedChinese, false, _) => format!(
                "策略组测速完成：{}/{} 个候选项成功",
                summary.succeeded, summary.total
            ),
        }
    }

    fn policy_group_benchmark_feedback(
        language: Language,
        state: &GroupBenchmarkState,
        total: usize,
        theme: Theme,
    ) -> Option<Div> {
        let (label, color) = match state {
            GroupBenchmarkState::Idle => return None,
            GroupBenchmarkState::Running { results, .. } => (
                if language == Language::English {
                    format!(
                        "Testing latency · {} of {} candidates returned",
                        results.len(),
                        total
                    )
                } else {
                    format!("正在测速 · {}/{} 个候选项已返回", results.len(), total)
                },
                theme.action_primary,
            ),
            GroupBenchmarkState::Complete { summary, .. } => {
                let latency = match (summary.minimum_ms, summary.average_ms) {
                    (Some(minimum), Some(average)) if language == Language::English => {
                        format!(" · min {minimum} ms · avg {average} ms")
                    }
                    (Some(minimum), Some(average)) => {
                        format!(" · 最低 {minimum} ms · 平均 {average} ms")
                    }
                    _ => String::new(),
                };
                (
                    if language == Language::English {
                        format!(
                            "Latency test complete · {}/{} candidates succeeded{latency}",
                            summary.succeeded, summary.total
                        )
                    } else {
                        format!(
                            "测速完成 · {}/{} 个候选项成功{latency}",
                            summary.succeeded, summary.total
                        )
                    },
                    theme.status_success,
                )
            }
            GroupBenchmarkState::Failed { .. } => (
                language
                    .text(
                        "Latency test failed · this policy group returned no delay data",
                        "测速失败 · 当前策略组未返回延迟，请检查 Mihomo 连接后重试",
                    )
                    .to_owned(),
                theme.route_trace,
            ),
        };
        Some(
            div()
                .mt(Space::Sm.px())
                .text_size(TextRole::Metadata.size())
                .line_height(TextRole::Metadata.line_height())
                .font_weight(TextRole::Label.weight())
                .text_color(color)
                .child(label),
        )
    }

    fn selected_node(&self) -> Option<PolicyNode> {
        let policy = self.selected_policy()?;
        Some(self.node_for_policy(policy))
    }

    fn node_for_policy(&self, policy: &PolicyGroup) -> PolicyNode {
        let selected = if policy.kind.allows_manual_selection() {
            self.workspace
                .selection_for(&policy.id)
                .and_then(|selected| policy.nodes.iter().find(|node| node.id == *selected))
                .or_else(|| policy.nodes.iter().find(|node| node.name == policy.target))
        } else {
            policy.nodes.iter().find(|node| node.name == policy.target)
        };
        selected
            .or_else(|| policy.nodes.first())
            .cloned()
            .unwrap_or_else(|| PolicyNode {
                id: ProxyId::new("unavailable"),
                name: self
                    .language()
                    .text("No available nodes", "暂无可用节点")
                    .to_owned(),
                kind: manis_core::PolicyCandidateKind::Node,
                provider: None,
                detail: self
                    .language()
                    .text("The kernel returned no group members", "内核未返回组内节点")
                    .to_owned(),
                latency_ms: None,
                alive: None,
            })
    }

    fn policy_node_source_label(&self, node: &PolicyNode, language: Language) -> String {
        if node.kind == manis_core::PolicyCandidateKind::PolicyGroup {
            return language.text("Policy group", "策略组").to_owned();
        }

        if let Some(index) = node
            .provider
            .as_deref()
            .and_then(managed_subscription_provider_index)
            && let Some(subscription) = self.imported_subscriptions.get(index)
        {
            return subscription.source.subscription_name().unwrap_or_else(|| {
                format!("{} {}", language.text("Subscription", "订阅"), index + 1)
            });
        }

        if let Some(provider) = node.provider.as_ref() {
            return provider.clone();
        }

        if self
            .saved_vless_nodes
            .iter()
            .any(|saved| saved.source.preview().name == node.name)
        {
            return language.text("Saved", "已保存").to_owned();
        }

        if let Some((index, subscription)) =
            self.imported_subscriptions
                .iter()
                .enumerate()
                .find(|(_, subscription)| {
                    subscription.providers.iter().any(|provider| {
                        provider
                            .nodes
                            .iter()
                            .any(|candidate| candidate.name == node.name)
                    })
                })
        {
            return subscription.source.subscription_name().unwrap_or_else(|| {
                format!("{} {}", language.text("Subscription", "订阅"), index + 1)
            });
        }

        language.text("Local configuration", "本地配置").to_owned()
    }

    fn switch_kernel(&mut self, requested: KernelKind, cx: &mut Context<Self>) {
        let language = self.language();
        if self.kernel_switch_state.is_busy() || self.runtime.kind() == requested {
            return;
        }
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            language
                .text(
                    "The local configuration directory is unavailable; the kernel cannot be changed",
                    "无法确定本机配置目录，不能切换内核",
                )
                .clone_into(&mut self.status);
            cx.notify();
            return;
        };
        self.kernel_switch_state = KernelSwitchState::Preparing;
        self.status = format!(
            "{} {} {}",
            language.text("Validating", "正在校验并准备"),
            requested.display_name(),
            language.text("configuration", "配置")
        );
        let previous = self.runtime.clone();
        let previous_kind = previous.kind();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    let prepared = KernelRuntime::prepare_with_language(
                        requested,
                        Some(&store_dir),
                        language,
                    )?;
                    kernel::save_kernel_kind_in(&store_dir, requested)
                        .map_err(|error| error.to_string())?;
                    if let Err(message) = previous.stop_managed() {
                        let _ = kernel::save_kernel_kind_in(&store_dir, previous_kind);
                        return Err(message);
                    }
                    Ok::<_, String>(prepared)
                })
                .await;
            this.update(cx, |this, cx| {
                let language = this.language();
                this.kernel_switch_state = KernelSwitchState::Idle;
                match result {
                    Ok(runtime) => {
                        this.runtime = runtime;
                        this.controller = ControllerState::Disconnected;
                        this.live_generation = this.live_generation.wrapping_add(1);
                        this.live_runtime = None;
                        this.proxy_mode = ProxyMode::Off;
                        this.status = if language == Language::English {
                            format!(
                                "Switched to {} · configuration valid; connect to start",
                                requested.display_name()
                            )
                        } else {
                            format!(
                                "已切换到 {} · 配置校验通过，点击连接启动",
                                requested.display_name()
                            )
                        };
                    }
                    Err(message) => {
                        this.status = if language == Language::English {
                            format!(
                                "Could not switch to {}: {message}",
                                requested.display_name()
                            )
                        } else {
                            format!("无法切换到 {}：{message}", requested.display_name())
                        };
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn update_mihomo_core(&mut self, cx: &mut Context<Self>) {
        if self.mihomo_core_update_state.is_busy() {
            return;
        }
        if self.proxy_mode != ProxyMode::Off {
            self.language()
                .text(
                    "Turn off the active proxy mode before updating Mihomo",
                    "请先关闭当前代理模式，再更新 Mihomo",
                )
                .clone_into(&mut self.status);
            cx.notify();
            return;
        }
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            self.language()
                .text(
                    "The Manis data directory is unavailable; Mihomo cannot be updated",
                    "无法确定 Manis 数据目录，不能更新 Mihomo",
                )
                .clone_into(&mut self.status);
            cx.notify();
            return;
        };

        let language = self.language();
        let reconnect = matches!(self.controller, ControllerState::Connected { .. });
        let previous = self.runtime.clone();
        self.mihomo_core_update_state = MihomoCoreUpdateState::Updating;
        self.live_generation = self.live_generation.wrapping_add(1);
        self.live_runtime = None;
        self.controller = ControllerState::Disconnected;
        language
            .text(
                "Downloading and verifying the stable Mihomo release…",
                "正在下载并校验 Mihomo 稳定版…",
            )
            .clone_into(&mut self.status);

        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let outcome = executor
                .spawn(async move {
                    perform_mihomo_core_update(&previous, &store_dir, language, reconnect)
                })
                .await;
            this.update(cx, |this, cx| {
                this.apply_mihomo_core_update_outcome(outcome, cx);
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn apply_mihomo_core_update_outcome(
        &mut self,
        outcome: MihomoCoreUpdateOutcome,
        cx: &mut Context<Self>,
    ) {
        let language = self.language();
        match outcome {
            MihomoCoreUpdateOutcome::Installed {
                version,
                runtime,
                snapshot,
            } => {
                self.runtime = runtime;
                self.mihomo_core_update_state = MihomoCoreUpdateState::Ready(version.clone());
                self.apply_core_update_snapshot(snapshot, cx);
                self.status = if language == Language::English {
                    format!("Mihomo {version} installed and verified")
                } else {
                    format!("Mihomo {version} 已安装并校验")
                };
            }
            MihomoCoreUpdateOutcome::Failed { message, recovered } => {
                self.mihomo_core_update_state = core_update::managed_core_binary_path()
                    .map_or(MihomoCoreUpdateState::Missing, |_path| {
                        MihomoCoreUpdateState::Ready(String::new())
                    });
                self.apply_core_update_snapshot(recovered, cx);
                self.status = format!(
                    "{}{message}",
                    language.text(
                        "Mihomo update failed; the previous core was restored: ",
                        "Mihomo 更新失败，已恢复原内核：",
                    )
                );
            }
        }
        cx.notify();
    }

    fn apply_core_update_snapshot(
        &mut self,
        result: Option<mihomo::RuntimeSnapshot>,
        cx: &mut Context<Self>,
    ) {
        let Some(result) = result else {
            return;
        };
        let controller_endpoint = result.controller_endpoint.clone();
        let controller_secret = result.controller_secret.clone();
        self.apply_mihomo_snapshot(result.endpoint, result.snapshot);
        self.start_live_runtime(&controller_endpoint, controller_secret.as_deref(), cx);
    }

    fn connect_mihomo(&mut self, cx: &mut Context<Self>) {
        if matches!(self.controller, ControllerState::Connecting { .. }) {
            return;
        }

        let language = self.language();
        let operation = begin_operation(
            "kernel.connect.requested",
            format!(
                "kernel={} profile={} endpoint={}",
                self.runtime.kind().display_name(),
                self.runtime.profile_source().label(),
                self.runtime.endpoint_label()
            ),
        );
        self.live_generation = self.live_generation.wrapping_add(1);
        self.live_runtime = None;
        self.live_status = LiveStreamStatus {
            activity: language.text("Reconnecting", "正在重新连接").to_owned(),
            logs: language.text("Reconnecting", "正在重新连接").to_owned(),
        };

        let endpoint = self.runtime.endpoint_label();
        let kernel_name = self.runtime.kind().display_name();
        let runtime = self.runtime.clone();
        self.controller = ControllerState::Connecting {
            endpoint: endpoint.clone(),
        };
        self.status = if language == Language::English {
            format!("Loading {kernel_name} data from {endpoint}")
        } else {
            format!("正在从 {endpoint} 读取 {kernel_name} 数据")
        };
        trace_ui(UiEvent::MihomoConnectStarted);

        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor.spawn(async move { runtime.connect() }).await;
            this.update(cx, |this, cx| {
                let language = this.language();
                match result {
                    Ok(result) => {
                        record_operation(
                            operation,
                            LogLevel::Info,
                            "kernel.connect.succeeded",
                            format!("endpoint={}", result.controller_endpoint),
                        );
                        let controller_endpoint = result.controller_endpoint;
                        let controller_secret = result.controller_secret;
                        this.apply_mihomo_snapshot(result.endpoint, result.snapshot);
                        this.start_live_runtime(
                            &controller_endpoint,
                            controller_secret.as_deref(),
                            cx,
                        );
                        this.sync_saved_node_selections(cx);
                        this.start_pending_policy_benchmark(cx);
                    }
                    Err(error) => {
                        this.pending_policy_benchmark_name = None;
                        record_operation(
                            operation,
                            LogLevel::Error,
                            "kernel.connect.failed",
                            error.to_string(),
                        );
                        trace_ui(UiEvent::MihomoConnectFailed);
                        let endpoint = this
                            .controller
                            .endpoint()
                            .unwrap_or(language.text("Local controller", "本地控制器"))
                            .to_owned();
                        let message = error.to_string();
                        this.controller = ControllerState::Failed {
                            endpoint,
                            message: message.clone(),
                        };
                        this.status = if language == Language::English {
                            format!("{kernel_name} connection failed: {message}")
                        } else {
                            format!("{kernel_name} 连接失败：{message}")
                        };
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn start_pending_policy_benchmark(&mut self, cx: &mut Context<Self>) {
        let Some(policy_name) = self.pending_policy_benchmark_name.take() else {
            return;
        };
        let Some(policy_id) = self
            .policy_groups()
            .find(|group| group.name == policy_name)
            .map(|group| group.id.clone())
        else {
            self.status = if self.language() == Language::English {
                format!("Policy group “{policy_name}” is not present in the active kernel")
            } else {
                format!("当前内核中没有策略组“{policy_name}”")
            };
            cx.notify();
            return;
        };
        self.expanded_policy_group = Some(policy_id.clone());
        self.start_policy_group_benchmark(&policy_id, cx);
    }

    fn apply_mihomo_snapshot(&mut self, endpoint: String, snapshot: LoadedSnapshot) {
        trace_ui(UiEvent::MihomoConnectSucceeded);
        let mut catalog = snapshot.catalog;
        for (group, target) in self.node_selection_preferences.iter_policy_targets() {
            if let Some(catalog) = catalog.as_mut() {
                let _ = catalog.apply_selector_target(group, target);
            }
        }
        let selection = catalog.as_ref().map(|catalog| {
            let primary = catalog.select(None);
            let selected_node = primary
                .nodes
                .iter()
                .find(|node| node.name == primary.target)
                .or_else(|| primary.nodes.first())
                .map(|node| node.id.clone());
            (primary.id.clone(), selected_node)
        });
        let policy_group_count = catalog.as_ref().map_or(0, |catalog| catalog.iter().count());
        self.catalog = catalog;
        self.route_prediction = RouteInspectorPrediction::Idle;
        if let Some((group, selected_node)) = selection {
            self.workspace
                .replace_source_selection(group, selected_node);
        } else {
            self.workspace.clear_source_selection();
        }
        self.source_providers = snapshot.providers;
        self.observed_routes = snapshot.observed_routes;
        self.active_connections = snapshot.connections;
        let system_proxy_applied = self
            .system_proxy
            .lock()
            .is_ok_and(|system| system.is_applied());
        self.proxy_mode = if snapshot.runtime.tun.enable {
            ProxyMode::Tun
        } else if system_proxy_applied {
            ProxyMode::System
        } else {
            ProxyMode::Off
        };
        self.routing_mode = snapshot.runtime.mode;
        self.proxy_runtime = snapshot.runtime;
        self.status = if self.language() == Language::English {
            format!(
                "Loaded {} policy groups · {} active connections",
                policy_group_count, snapshot.active_connections
            )
        } else {
            format!(
                "已读取 {} 个策略组 · {} 条活动连接",
                policy_group_count, snapshot.active_connections
            )
        };
        self.controller = ControllerState::Connected {
            endpoint,
            version: snapshot.version,
            active_connections: snapshot.active_connections,
            download_total: snapshot.download_total,
            upload_total: snapshot.upload_total,
        };
    }

    fn sync_saved_node_selections(&mut self, cx: &mut Context<Self>) {
        if !matches!(&*self.runtime, ControllerRuntime::Managed { .. })
            || !matches!(self.controller, ControllerState::Connected { .. })
        {
            return;
        }
        let mut targets = Vec::new();
        if let Some(global) = self.node_selection_preferences.global() {
            targets.push(("GLOBAL".to_owned(), global.node_name.clone()));
        }
        targets.extend(self.policy_groups().filter_map(|group| {
            if !group.kind.allows_manual_selection() || group.name.eq_ignore_ascii_case("GLOBAL") {
                return None;
            }
            self.node_selection_preferences
                .policy_target(&group.name)
                .map(|target| (group.name.clone(), target.to_owned()))
        }));
        if targets.is_empty() {
            return;
        }

        let runtime = self.runtime.clone();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let results = executor
                .spawn(async move {
                    targets
                        .into_iter()
                        .map(|(group, target)| {
                            let result = if group.eq_ignore_ascii_case("GLOBAL") {
                                runtime.select_global_node(&target)
                            } else {
                                runtime.select_policy_candidate(&group, &target)
                            };
                            (group, target, result)
                        })
                        .collect::<Vec<_>>()
                })
                .await;
            this.update(cx, |this, cx| {
                let mut applied = 0usize;
                let mut failed = 0usize;
                for (group, requested, result) in results {
                    match result {
                        Ok(snapshot) => {
                            let current = snapshot.current.as_deref().unwrap_or(&requested);
                            if let Some(catalog) = this.catalog.as_mut() {
                                let _ = catalog.apply_selector_target(&group, current);
                            }
                            if let Some(stored_group) = this
                                .managed_policy_groups
                                .iter()
                                .find(|candidate| candidate.name == group)
                            {
                                this.managed_policy_runtime_generation =
                                    this.managed_policy_runtime_generation.wrapping_add(1);
                                this.managed_policy_runtime_states.insert(
                                    stored_group.id.clone(),
                                    ManagedPolicyRuntimeState::Ready {
                                        generation: this.managed_policy_runtime_generation,
                                        current: snapshot.current,
                                        candidates: snapshot.candidates,
                                    },
                                );
                            }
                            record_event(
                                LogLevel::Info,
                                "node.selection.restored",
                                format!("group={group}"),
                            );
                            applied += 1;
                        }
                        Err(error) => {
                            record_event(
                                LogLevel::Warn,
                                "node.selection.restore_failed",
                                format!("group={group} error={error}"),
                            );
                            failed += 1;
                        }
                    }
                }
                if applied > 0 || failed > 0 {
                    this.status = if this.language() == Language::English {
                        format!(
                            "Restored {applied} saved node selections · {failed} could not be applied"
                        )
                    } else {
                        format!("已恢复 {applied} 个节点选择 · {failed} 个暂时无法应用")
                    };
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    #[allow(clippy::too_many_lines)]
    fn start_policy_group_benchmark(
        &mut self,
        id: &manis_core::PolicyGroupId,
        cx: &mut Context<Self>,
    ) {
        let language = self.language();
        if !matches!(self.controller, ControllerState::Connected { .. }) {
            language
                .text(
                    "Connect to the kernel before testing a live policy group",
                    "请先连接内核，再测试真实策略组",
                )
                .clone_into(&mut self.status);
            cx.notify();
            return;
        }
        let Some(group) = self.policy_groups().find(|group| group.id == *id).cloned() else {
            return;
        };
        let key = Self::policy_group_benchmark_key(&group.id);
        if matches!(
            self.group_benchmarks.get(&key),
            Some(GroupBenchmarkState::Running { .. })
        ) {
            return;
        }
        let candidate_names = group
            .nodes
            .iter()
            .map(|node| node.name.clone())
            .collect::<Vec<_>>();
        if candidate_names.is_empty() {
            language
                .text(
                    "This policy group has no testable candidates",
                    "当前策略组没有可测速候选项",
                )
                .clone_into(&mut self.status);
            cx.notify();
            return;
        }
        let Some(generation) = self.begin_group_benchmark(key.clone()) else {
            language
                .text(
                    "Another group is being tested; wait for it to finish",
                    "已有分组正在测速，请等待完成后再试",
                )
                .clone_into(&mut self.status);
            cx.notify();
            return;
        };
        self.status = if language == Language::English {
            format!(
                "Testing {} candidates in policy group “{}”",
                candidate_names.len(),
                group.name
            )
        } else {
            format!(
                "正在测试策略组“{}”的 {} 个候选项",
                group.name,
                candidate_names.len()
            )
        };
        trace_ui(UiEvent::GroupBenchmarkStarted);

        let runtime = self.runtime.clone();
        let group_id = group.id.clone();
        let group_name = group.name.clone();
        let group_kind = group.kind;
        let total = candidate_names.len();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    if group_kind == manis_core::PolicyGroupKind::Direct {
                        runtime
                            .test_proxy_candidates_delay(&group_name, &candidate_names)
                            .map(|delays| mihomo::PolicyGroupBenchmarkSnapshot {
                                delays,
                                current: None,
                            })
                    } else {
                        runtime.test_policy_group_delay(&group_name, &candidate_names)
                    }
                })
                .await;
            this.update(cx, |this, cx| {
                let language = this.language();
                if this.group_benchmark_active_generation != Some(generation) {
                    return;
                }
                this.group_benchmark_active_generation = None;
                let (delays, current, failure) = match result {
                    Ok(snapshot) => (Some(snapshot.delays), snapshot.current, None),
                    Err(error) => (None, None, Some(error.to_string())),
                };
                if let Some(delays) = delays.as_ref()
                    && let Some(catalog) = this.catalog.as_mut()
                {
                    let _ = catalog.apply_group_benchmark(&group_id, current.as_deref(), delays);
                }
                let Some(state) = this.group_benchmarks.get_mut(&key) else {
                    cx.notify();
                    return;
                };
                let accepted = match delays {
                    Some(delays) => state.complete(generation, total, delays),
                    None => state.fail(generation),
                };
                if !accepted {
                    return;
                }
                match state {
                    GroupBenchmarkState::Complete { summary, .. } => {
                        trace_ui(UiEvent::GroupBenchmarkSucceeded);
                        this.status = Self::policy_benchmark_status(
                            language,
                            group_kind,
                            current.as_deref(),
                            *summary,
                        );
                    }
                    GroupBenchmarkState::Failed { .. } => {
                        trace_ui(UiEvent::GroupBenchmarkFailed);
                        this.status = format!(
                            "{}：{}",
                            language.text("Policy group benchmark failed", "策略组测速失败"),
                            failure.as_deref().unwrap_or_else(
                                || language.text("unknown controller error", "未知控制器错误")
                            )
                        );
                    }
                    _ => return,
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn start_live_runtime(
        &mut self,
        endpoint: &str,
        controller_secret: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        let language = self.language();
        self.live_generation = self.live_generation.wrapping_add(1);
        let generation = self.live_generation;
        self.managed_health_tick = 0;
        self.live_runtime = match LiveRuntimeSession::start(endpoint, controller_secret) {
            Ok(session) => Some(session),
            Err(error) => {
                self.live_status = LiveStreamStatus {
                    activity: format!(
                        "{}{error}",
                        language.text("Could not start: ", "无法启动：")
                    ),
                    logs: format!(
                        "{}{error}",
                        language.text("Could not start: ", "无法启动：")
                    ),
                };
                None
            }
        };
        if self.live_runtime.is_some() {
            self.poll_live_runtime(generation, cx);
        }
    }

    fn poll_live_runtime(&mut self, generation: u64, cx: &mut Context<Self>) {
        if generation != self.live_generation {
            return;
        }
        self.managed_health_tick = self.managed_health_tick.wrapping_add(1);
        if self.managed_health_tick >= 10 {
            self.managed_health_tick = 0;
            if self.fail_safe_stopped_managed_kernel(cx) {
                return;
            }
        }
        let Some(session) = self.live_runtime.as_ref() else {
            return;
        };
        let update = session.drain();
        self.live_status = update.status;
        self.dropped_kernel_logs = self.dropped_kernel_logs.saturating_add(update.dropped_logs);
        for entry in update.logs {
            if self.kernel_logs.len() == 500 {
                self.kernel_logs.pop_front();
                self.dropped_kernel_logs = self.dropped_kernel_logs.saturating_add(1);
            }
            self.kernel_logs.push_back(entry);
        }
        if let Some(connections) = update.connections {
            self.active_connections = connections.connections;
            if let ControllerState::Connected {
                active_connections,
                download_total,
                upload_total,
                ..
            } = &mut self.controller
            {
                *active_connections = self.active_connections.len();
                *download_total = connections.download_total;
                *upload_total = connections.upload_total;
            }
        }
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(400))
                .await;
            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| this.poll_live_runtime(generation, cx));
            }
        })
        .detach();
    }

    fn fail_safe_stopped_managed_kernel(&mut self, cx: &mut Context<Self>) -> bool {
        let failure = match self.runtime.managed_health() {
            #[cfg(any(test, feature = "snapshot-fixtures"))]
            Ok(ManagedRuntimeHealth::NotManaged) => return false,
            Ok(ManagedRuntimeHealth::Running) => return false,
            Ok(ManagedRuntimeHealth::Stopped) => self
                .language()
                .text(
                    "The Manis-managed kernel stopped unexpectedly",
                    "Manis 托管内核已意外停止",
                )
                .to_owned(),
            Err(error) => error.to_string(),
        };

        let language = self.language();
        let mut recovery_error = None;
        let was_system_proxy = self.proxy_mode == ProxyMode::System;
        if was_system_proxy {
            match self.system_proxy.lock() {
                Ok(mut system) => {
                    if let Err(error) = system.disable_with_language(language) {
                        recovery_error = Some(error.to_string());
                    } else {
                        self.proxy_mode = ProxyMode::Off;
                    }
                }
                Err(_poisoned) => {
                    recovery_error = Some(
                        language
                            .text(
                                "system proxy state lock was damaged",
                                "系统代理状态锁已损坏",
                            )
                            .to_owned(),
                    );
                }
            }
        } else if self.proxy_mode == ProxyMode::Tun {
            match self.tun_dns.lock() {
                Ok(mut dns) => match dns.disable_with_language(language) {
                    Ok(()) => self.proxy_mode = ProxyMode::Off,
                    Err(error) => recovery_error = Some(error.to_string()),
                },
                Err(_poisoned) => {
                    recovery_error = Some(
                        language
                            .text("TUN DNS state lock was damaged", "TUN DNS 状态锁已损坏")
                            .to_owned(),
                    );
                }
            }
        }

        self.live_generation = self.live_generation.wrapping_add(1);
        self.live_runtime = None;
        let endpoint = self.runtime.endpoint_label();
        self.controller = ControllerState::Failed {
            endpoint,
            message: failure.clone(),
        };
        self.status = match recovery_error {
            None if was_system_proxy => {
                format!(
                    "{}{}",
                    failure,
                    language.text(
                        " · system proxy was restored; reconnect to restart the kernel",
                        " · 系统代理已恢复；重新连接即可重启内核",
                    )
                )
            }
            None => format!(
                "{}{}",
                failure,
                language.text(
                    " · reconnect to restart the kernel",
                    " · 重新连接即可重启内核",
                )
            ),
            Some(recovery_error) => {
                format!(
                    "{}{}{}",
                    failure,
                    language.text(
                        " · automatic system proxy recovery failed: ",
                        " · 系统代理自动恢复失败：",
                    ),
                    recovery_error
                )
            }
        };
        cx.notify();
        true
    }

    /// Reports why the requested proxy mode cannot be applied right now.
    ///
    /// The tray uses this to disable a menu item and explain itself instead of letting the user
    /// click an entry that would silently fail.
    pub(crate) fn proxy_mode_block(&self, requested: ProxyMode) -> Option<ProxyModeBlock> {
        proxy_mode_block(
            requested,
            self.proxy_mode_busy,
            if matches!(self.controller, ControllerState::Connected { .. }) {
                ControllerReadiness::Connected
            } else {
                ControllerReadiness::Disconnected
            },
            if self.runtime.is_fixture() {
                TunSupport::FixtureReadOnly
            } else if self.runtime.capabilities().tun {
                TunSupport::Supported
            } else {
                TunSupport::KernelUnsupported
            },
        )
    }

    /// Returns the proxy mode the tray shows as checked.
    pub(crate) const fn active_proxy_mode(&self) -> ProxyMode {
        self.proxy_mode
    }

    /// Applies the mode a checkable control stands for, clearing it when it is already active.
    pub(crate) fn toggle_proxy_mode(&mut self, selected: ProxyMode, cx: &mut Context<Self>) {
        self.apply_proxy_mode(self.proxy_mode.toggled(selected), cx);
    }

    #[allow(clippy::too_many_lines)]
    fn apply_proxy_mode(&mut self, requested: ProxyMode, cx: &mut Context<Self>) {
        let language = self.language();
        let operation = begin_operation(
            "proxy.mode.requested",
            format!(
                "from={:?} to={requested:?} controller_state={} profile={}",
                self.proxy_mode,
                controller_state_label(&self.controller),
                self.runtime.profile_source().label()
            ),
        );
        if self.proxy_mode_busy.is_some() || requested == self.proxy_mode {
            record_operation(
                operation,
                LogLevel::Warn,
                "proxy.mode.ignored",
                format!(
                    "busy={} already_selected={}",
                    self.proxy_mode_busy.is_some(),
                    requested == self.proxy_mode
                ),
            );
            return;
        }
        if !matches!(self.controller, ControllerState::Connected { .. }) {
            record_operation(
                operation,
                LogLevel::Error,
                "proxy.mode.rejected",
                "reason=controller_not_connected",
            );
            trace_ui(UiEvent::ProxyModeFailed);
            self.status = format!(
                "{} {}",
                language.text(
                    "Connect before changing proxy mode:",
                    "请先连接后再切换代理模式："
                ),
                self.runtime.kind().display_name(),
            );
            cx.notify();
            return;
        }
        if requested == ProxyMode::Tun && !self.runtime.capabilities().tun {
            record_operation(
                operation,
                LogLevel::Error,
                "proxy.mode.rejected",
                "reason=kernel_has_no_tun_capability",
            );
            trace_ui(UiEvent::ProxyModeFailed);
            language.text(
                "TUN is not yet available for the sing-box adapter; use the system HTTP/SOCKS proxy",
                "当前 sing-box 适配器尚未开放 TUN；可使用系统 HTTP/SOCKS 代理",
            )
                .clone_into(&mut self.status);
            cx.notify();
            return;
        }
        if requested == ProxyMode::Tun && self.runtime.is_fixture() {
            record_operation(
                operation,
                LogLevel::Error,
                "proxy.mode.rejected",
                "reason=fixture_read_only",
            );
            trace_ui(UiEvent::ProxyModeFailed);
            language
                .text(
                    "Test fixtures cannot enable TUN",
                    "测试快照不能启用 TUN 模式",
                )
                .clone_into(&mut self.status);
            cx.notify();
            return;
        }

        let runtime = self.runtime.clone();
        let system_proxy = self.system_proxy.clone();
        let tun_dns = self.tun_dns.clone();
        let previous = self.proxy_mode;
        let mixed_port = self.proxy_runtime.mixed_port.filter(|port| *port > 0);
        let ports = ProxyPorts {
            http: self
                .proxy_runtime
                .port
                .filter(|port| *port > 0)
                .or(mixed_port),
            socks: self
                .proxy_runtime
                .socks_port
                .filter(|port| *port > 0)
                .or(mixed_port),
        };
        self.proxy_mode_busy = Some(requested);
        self.status = match requested {
            ProxyMode::Tun => language
                .text(
                    "Preparing the macOS TUN helper and traffic route…",
                    "正在准备 macOS TUN 辅助服务与流量接管…",
                )
                .to_owned(),
            _ => format!(
                "{}{}…",
                language.text("Switching to ", "正在切换到"),
                proxy_mode_label(language, requested)
            ),
        };

        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    let mut system = system_proxy
                        .lock()
                        .map_err(|_| "系统代理状态锁已损坏".to_owned())?;
                    let mut dns = tun_dns
                        .lock()
                        .map_err(|_| "TUN DNS 状态锁已损坏".to_owned())?;
                    match (previous, requested) {
                        (ProxyMode::System, ProxyMode::Off) => {
                            system
                                .disable_with_language(language)
                                .map_err(|error| error.to_string())?;
                        }
                        (ProxyMode::Tun, ProxyMode::Off) => {
                            disable_tun_with_dns(&runtime, &mut dns, language)?;
                        }
                        (ProxyMode::Off, ProxyMode::System) => {
                            system
                                .enable_with_language(ports, language)
                                .map_err(|error| error.to_string())?;
                        }
                        (ProxyMode::Tun, ProxyMode::System) => {
                            disable_tun_with_dns(&runtime, &mut dns, language)?;
                            if let Err(error) = system.enable_with_language(ports, language) {
                                let rollback = enable_tun_with_dns(&runtime, &mut dns, language);
                                return Err(match rollback {
                                    Ok(()) => error.to_string(),
                                    Err(rollback) => {
                                        format!("{error}；恢复原 TUN 模式也失败：{rollback}")
                                    }
                                });
                            }
                        }
                        (ProxyMode::Off, ProxyMode::Tun) => {
                            enable_tun_with_dns(&runtime, &mut dns, language)?;
                        }
                        (ProxyMode::System, ProxyMode::Tun) => {
                            system
                                .disable_with_language(language)
                                .map_err(|error| error.to_string())?;
                            if let Err(error) = enable_tun_with_dns(&runtime, &mut dns, language) {
                                let rollback = system.enable_with_language(ports, language);
                                return Err(match rollback {
                                    Ok(()) => error.clone(),
                                    Err(rollback) => {
                                        format!("{error}；恢复原系统代理也失败：{rollback}")
                                    }
                                });
                            }
                        }
                        _ => {}
                    }
                    Ok::<(), String>(())
                })
                .await;
            this.update(cx, |this, cx| {
                let language = this.language();
                this.proxy_mode_busy = None;
                match result {
                    Ok(()) => {
                        record_operation(
                            operation,
                            LogLevel::Info,
                            "proxy.mode.succeeded",
                            format!("active={requested:?}"),
                        );
                        this.proxy_mode = requested;
                        match requested {
                            ProxyMode::Off => trace_ui(UiEvent::SystemProxyDisabled),
                            ProxyMode::System => trace_ui(UiEvent::SystemProxyEnabled),
                            ProxyMode::Tun => trace_ui(UiEvent::TunProxyEnabled),
                        }
                        this.status = format!(
                            "{}{}",
                            proxy_mode_label(language, requested),
                            language.text(" enabled", "已生效")
                        );
                    }
                    Err(message) => {
                        record_operation(
                            operation,
                            LogLevel::Error,
                            "proxy.mode.failed",
                            message.clone(),
                        );
                        trace_ui(UiEvent::ProxyModeFailed);
                        this.status = format!(
                            "{}{message}",
                            language.text("Failed to change proxy mode: ", "代理模式切换失败：")
                        );
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    #[allow(clippy::too_many_lines)]
    fn apply_routing_mode(&mut self, requested: RoutingMode, cx: &mut Context<Self>) {
        let language = self.language();
        let operation = begin_operation(
            "routing.mode.requested",
            format!(
                "from={:?} to={requested:?} controller_state={} profile={}",
                self.routing_mode,
                controller_state_label(&self.controller),
                self.runtime.profile_source().label()
            ),
        );
        if self.routing_mode_busy.is_some() || requested == self.routing_mode {
            record_operation(
                operation,
                LogLevel::Warn,
                "routing.mode.ignored",
                format!(
                    "busy={} already_selected={}",
                    self.routing_mode_busy.is_some(),
                    requested == self.routing_mode
                ),
            );
            return;
        }
        if !matches!(self.controller, ControllerState::Connected { .. }) {
            record_operation(
                operation,
                LogLevel::Error,
                "routing.mode.rejected",
                "reason=controller_not_connected",
            );
            trace_ui(UiEvent::RoutingModeFailed);
            language
                .text(
                    "Connect to the kernel before changing routing mode",
                    "请先连接内核，再切换路由模式",
                )
                .clone_into(&mut self.status);
            cx.notify();
            return;
        }
        if self.runtime.is_fixture() {
            record_operation(
                operation,
                LogLevel::Error,
                "routing.mode.rejected",
                "reason=fixture_read_only",
            );
            trace_ui(UiEvent::RoutingModeFailed);
            language
                .text(
                    "Test fixtures cannot change routing mode",
                    "测试快照不能切换路由模式",
                )
                .clone_into(&mut self.status);
            cx.notify();
            return;
        }

        self.routing_mode_busy = Some(requested);
        self.status = format!(
            "{}{}…",
            language.text("Switching to ", "正在切换到"),
            routing_mode_label(language, requested)
        );
        let runtime = self.runtime.clone();
        let store_dir = self.subscription_store_dir.clone();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    runtime.set_routing_mode(requested)?;
                    let persistence = store_dir
                        .as_deref()
                        .map(|directory| mihomo::save_routing_mode_in(directory, requested))
                        .transpose();
                    Ok::<_, mihomo::LoadError>(persistence)
                })
                .await;
            this.update(cx, |this, cx| {
                let language = this.language();
                this.routing_mode_busy = None;
                match result {
                    Ok(persistence) => {
                        record_operation(
                            operation,
                            LogLevel::Info,
                            "routing.mode.succeeded",
                            format!("active={requested:?} persisted={}", persistence.is_ok()),
                        );
                        this.routing_mode = requested;
                        this.proxy_runtime.mode = requested;
                        trace_ui(UiEvent::RoutingModeChanged);
                        this.status = match requested {
                            RoutingMode::Global => this.global_target().map_or_else(
                                || {
                                    language.text(
                                    "Global mode enabled; choose the global exit on the Nodes page",
                                    "全局模式已生效；请在节点页选择全局出口",
                                ).to_owned()
                                },
                                |target| {
                                    if language == Language::English {
                                        format!("Global mode enabled · current exit {target}")
                                    } else {
                                        format!("全局模式已生效 · 当前出口 {target}")
                                    }
                                },
                            ),
                            _ => format!(
                                "{}{}",
                                routing_mode_label(language, requested),
                                language.text(" enabled", "已生效")
                            ),
                        };
                        if persistence.is_err() {
                            this.status.push_str(language.text(
                                " · restart preference could not be saved",
                                " · 但未能保存重启偏好",
                            ));
                        }
                    }
                    Err(error) => {
                        record_operation(
                            operation,
                            LogLevel::Error,
                            "routing.mode.failed",
                            error.to_string(),
                        );
                        trace_ui(UiEvent::RoutingModeFailed);
                        this.status = format!(
                            "{}{error}",
                            language.text("Failed to change routing mode: ", "路由模式切换失败：")
                        );
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    #[allow(clippy::too_many_lines)]
    fn select_global_node(&mut self, selected: NodeIdentity, cx: &mut Context<Self>) {
        let language = self.language();
        let selected_name = selected.node_name.clone();
        let operation = begin_operation(
            "global.node.requested",
            format!(
                "controller_state={} profile={} candidate_selected=true",
                controller_state_label(&self.controller),
                self.runtime.profile_source().label()
            ),
        );
        if self.global_selection_busy.is_some() {
            record_operation(
                operation,
                LogLevel::Warn,
                "global.node.ignored",
                "reason=selection_busy",
            );
            return;
        }
        let previous = self.node_selection_preferences.clone();
        self.node_selection_preferences.set_global(selected);
        if let Some(directory) = self.subscription_store_dir.as_deref()
            && let Err(error) = mihomo::save_node_selection_preferences_in(
                directory,
                &self.node_selection_preferences,
            )
        {
            self.node_selection_preferences = previous;
            record_operation(
                operation,
                LogLevel::Error,
                "global.node.persistence_failed",
                error.to_string(),
            );
            trace_ui(UiEvent::GlobalNodeSelectionFailed);
            self.status = format!(
                "{}{error}",
                language.text("Could not save the global node: ", "无法保存全局节点：")
            );
            cx.notify();
            return;
        }
        record_operation(
            operation,
            LogLevel::Info,
            "global.node.saved",
            "saved_to_workspace=true",
        );
        trace_ui(UiEvent::GlobalNodeSelected);

        let can_apply_now = matches!(self.controller, ControllerState::Connected { .. })
            && matches!(&*self.runtime, ControllerRuntime::Managed { .. });
        if !can_apply_now {
            record_operation(
                operation,
                LogLevel::Info,
                "global.node.deferred",
                "reason=managed_controller_not_connected",
            );
            self.status = if language == Language::English {
                format!("Saved global exit “{selected_name}”; it applies in Global mode")
            } else {
                format!("已保存全局出口“{selected_name}”；全局模式将使用此节点")
            };
            cx.notify();
            return;
        }

        self.global_selection_busy = Some(selected_name.clone());
        self.status = if language == Language::English {
            format!("Selecting global node “{selected_name}”…")
        } else {
            format!("正在选择全局节点“{selected_name}”…")
        };
        let runtime = self.runtime.clone();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn({
                    let selected_name = selected_name.clone();
                    async move { runtime.select_global_node(&selected_name) }
                })
                .await;
            this.update(cx, |this, cx| {
                let language = this.language();
                this.global_selection_busy = None;
                match result {
                    Ok(snapshot) => {
                        record_operation(
                            operation,
                            LogLevel::Info,
                            "global.node.succeeded",
                            "global selector confirmed target",
                        );
                        let current = snapshot.current.as_deref().unwrap_or(&selected_name);
                        trace_ui(UiEvent::GlobalNodeSelected);
                        this.status = if language == Language::English {
                            if this.routing_mode == RoutingMode::Global {
                                format!("Global exit switched to “{current}”")
                            } else {
                                format!("Saved global exit “{current}”; it applies in Global mode")
                            }
                        } else if this.routing_mode == RoutingMode::Global {
                            format!("全局出口已切换到“{current}”")
                        } else {
                            format!("已保存全局出口“{current}”；切换到全局模式后生效")
                        };
                    }
                    Err(error) => {
                        record_operation(
                            operation,
                            LogLevel::Error,
                            "global.node.failed",
                            error.to_string(),
                        );
                        this.status = if language == Language::English {
                            format!(
                                "Saved global exit “{selected_name}”, but it could not be applied now: {error}"
                            )
                        } else {
                            format!(
                                "已保存全局出口“{selected_name}”，但暂时无法应用：{error}"
                            )
                        };
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn select_policy_node(
        &mut self,
        group_id: PolicyGroupId,
        group_name: String,
        node_id: ProxyId,
        node_name: String,
        cx: &mut Context<Self>,
    ) {
        let language = self.language();
        let operation = begin_operation(
            "policy.node.requested",
            format!("group={group_name} candidate_selected=true"),
        );
        if self.policy_selection_busy.is_some() {
            record_operation(
                operation,
                LogLevel::Warn,
                "policy.node.ignored",
                "reason=selection_busy",
            );
            return;
        }
        let stored_group = self
            .managed_policy_groups
            .iter()
            .find(|group| group.id == group_id.as_str() || group.name == group_name);
        if !matches!(self.controller, ControllerState::Connected { .. }) && stored_group.is_none() {
            record_operation(
                operation,
                LogLevel::Warn,
                "policy.node.rejected",
                "reason=runtime_policy_unavailable",
            );
            language
                .text(
                    "Connect the kernel before selecting a runtime policy node.",
                    "请先连接内核，再选择真实策略组节点。",
                )
                .clone_into(&mut self.status);
            cx.notify();
            return;
        }
        let catalog_allows = self
            .policy_groups()
            .find(|group| group.id == group_id || group.name == group_name)
            .map(|group| {
                group.kind.allows_manual_selection()
                    && group.nodes.iter().any(|node| node.name == node_name)
            });
        let stored_group_allows = stored_group.map(|group| {
            group.strategy == ManagedPolicyStrategy::Manual
                && self
                    .managed_policy_candidate_names(group)
                    .iter()
                    .any(|candidate| candidate == &node_name)
        });
        if !policy_target_is_selectable(
            matches!(self.controller, ControllerState::Connected { .. }),
            catalog_allows,
            stored_group_allows,
        ) {
            record_operation(
                operation,
                LogLevel::Error,
                "policy.node.rejected",
                "reason=not_manual_candidate",
            );
            language
                .text(
                    "Only a candidate inside a manual policy can be selected",
                    "只能选择手动策略组中的候选节点",
                )
                .clone_into(&mut self.status);
            cx.notify();
            return;
        }

        let previous = self.node_selection_preferences.clone();
        if let Err(error) = self
            .node_selection_preferences
            .set_policy_target(group_name.clone(), node_name.clone())
        {
            record_operation(
                operation,
                LogLevel::Error,
                "policy.node.rejected",
                error.to_string(),
            );
            language
                .text(
                    "This policy selection cannot be saved",
                    "无法保存这个策略组选择",
                )
                .clone_into(&mut self.status);
            cx.notify();
            return;
        }
        if let Some(directory) = self.subscription_store_dir.as_deref()
            && let Err(error) = mihomo::save_node_selection_preferences_in(
                directory,
                &self.node_selection_preferences,
            )
        {
            self.node_selection_preferences = previous;
            record_operation(
                operation,
                LogLevel::Error,
                "policy.node.persistence_failed",
                error.to_string(),
            );
            self.status = format!(
                "{}{error}",
                language.text(
                    "Could not save the policy selection: ",
                    "无法保存策略组选择："
                )
            );
            cx.notify();
            return;
        }
        let catalog_selection = self
            .policy_groups()
            .find(|group| group.name == group_name)
            .and_then(|group| {
                group
                    .nodes
                    .iter()
                    .find(|node| node.name == node_name)
                    .map(|node| (group.id.clone(), node.id.clone()))
            });
        if let Some(catalog) = self.catalog.as_mut() {
            let _ = catalog.apply_selector_target(&group_name, &node_name);
        }
        if self.workspace.selected_group.as_ref() == Some(&group_id) {
            self.workspace.select_node(node_id.clone());
        } else if let Some((catalog_group_id, catalog_node_id)) = catalog_selection
            && self.workspace.selected_group.as_ref() == Some(&catalog_group_id)
        {
            self.workspace.select_node(catalog_node_id);
        }
        record_operation(
            operation,
            LogLevel::Info,
            "policy.node.saved",
            format!("group={group_name}"),
        );

        let can_apply_now = matches!(self.controller, ControllerState::Connected { .. })
            && matches!(&*self.runtime, ControllerRuntime::Managed { .. });
        if !can_apply_now {
            self.status = if language == Language::English {
                format!(
                    "Saved “{node_name}” for manual policy “{group_name}”; it will apply when the managed kernel connects"
                )
            } else {
                format!("已为手动策略组“{group_name}”选择“{node_name}”；托管内核连接后生效")
            };
            cx.notify();
            return;
        }

        if let Some((stored_group_id, candidates)) = self
            .managed_policy_groups
            .iter()
            .find(|group| group.name == group_name)
            .map(|group| {
                (
                    group.id.clone(),
                    self.managed_policy_candidate_names(group)
                        .into_iter()
                        .collect::<BTreeSet<_>>(),
                )
            })
        {
            self.managed_policy_runtime_generation =
                self.managed_policy_runtime_generation.wrapping_add(1);
            let generation = self.managed_policy_runtime_generation;
            let state = self
                .managed_policy_runtime_states
                .entry(stored_group_id)
                .or_default();
            if !state.begin_selection(generation, &node_name) {
                *state = ManagedPolicyRuntimeState::Selecting {
                    generation,
                    current: previous.policy_target(&group_name).map(str::to_owned),
                    candidates,
                    pending: node_name.clone(),
                };
            }
        }
        self.policy_selection_busy = Some(node_name.clone());
        self.status = if language == Language::English {
            format!("Setting “{group_name}” to “{node_name}”…")
        } else {
            format!("正在将“{group_name}”设为“{node_name}”…")
        };
        let runtime = self.runtime.clone();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn({
                    let group_name = group_name.clone();
                    let node_name = node_name.clone();
                    async move { runtime.select_policy_candidate(&group_name, &node_name) }
                })
                .await;
            this.update(cx, |this, cx| {
                this.policy_selection_busy = None;
                match result {
                    Ok(snapshot) => {
                        let current = snapshot
                            .current
                            .clone()
                            .unwrap_or_else(|| node_name.clone());
                        if let Some(catalog) = this.catalog.as_mut() {
                            let _ = catalog.apply_selector_target(&group_name, &current);
                        }
                        if this.workspace.selected_group.as_ref() == Some(&group_id) {
                            this.workspace.select_node(node_id);
                        }
                        if let Some(stored_group) = this
                            .managed_policy_groups
                            .iter()
                            .find(|group| group.name == group_name)
                        {
                            this.managed_policy_runtime_generation =
                                this.managed_policy_runtime_generation.wrapping_add(1);
                            this.managed_policy_runtime_states.insert(
                                stored_group.id.clone(),
                                ManagedPolicyRuntimeState::Ready {
                                    generation: this.managed_policy_runtime_generation,
                                    current: snapshot.current,
                                    candidates: snapshot.candidates,
                                },
                            );
                        }
                        record_operation(
                            operation,
                            LogLevel::Info,
                            "policy.node.succeeded",
                            format!("group={group_name}"),
                        );
                        this.status = if this.language() == Language::English {
                            format!(
                                "“{group_name}” now uses “{current}” when a rule selects this policy"
                            )
                        } else {
                            format!("规则命中“{group_name}”时将使用“{current}”")
                        };
                    }
                    Err(error) => {
                        record_operation(
                            operation,
                            LogLevel::Error,
                            "policy.node.failed",
                            error.to_string(),
                        );
                        this.status = if this.language() == Language::English {
                            format!(
                                "Saved “{node_name}” for “{group_name}”, but it could not be applied now: {error}"
                            )
                        } else {
                            format!(
                                "已为“{group_name}”保存“{node_name}”，但暂时无法应用：{error}"
                            )
                        };
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn global_target_identity(&self) -> Option<&NodeIdentity> {
        self.node_selection_preferences.global()
    }

    fn global_target(&self) -> Option<&str> {
        self.global_target_identity()
            .map(|identity| identity.node_name.as_str())
            .or_else(|| self.runtime_global_target())
    }

    fn runtime_global_target(&self) -> Option<&str> {
        self.policy_groups()
            .find(|group| group.name.eq_ignore_ascii_case("GLOBAL"))
            .map(|group| group.target.as_str())
    }

    #[allow(clippy::too_many_lines)]
    fn chrome(&self, theme: Theme, size_class: WindowSizeClass, cx: &mut Context<Self>) -> Div {
        let compact = size_class == WindowSizeClass::Compact;
        let language = self.language();
        let theme_label = if self.dark {
            language.text("Light", "浅色")
        } else {
            language.text("Dark", "深色")
        };

        div()
            .h(ControlSize::Standard.height() + Space::Md.px())
            .flex_shrink_0()
            .flex()
            .items_center()
            .px(Space::Lg.px())
            .gap(Space::Md.px())
            .bg(theme.surface_chrome)
            .border_b_1()
            .border_color(theme.outline_subtle)
            .child(
                div()
                    .w(if compact {
                        LayoutMetric::CompactNavigation.px()
                    } else {
                        LayoutMetric::WideNavigation.px()
                    })
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap(Space::Sm.px())
                    .child(
                        div()
                            .size(ControlSize::Icon.min_pointer_target() - Space::Sm.px())
                            .flex_shrink_0()
                            .rounded(Radius::Control.px() - px(2.0))
                            .overflow_hidden()
                            .child(img(assets::BRAND_MARK_PATH).size_full()),
                    )
                    .when(!compact, |brand| {
                        brand.child(
                            div()
                                .text_size(TextRole::SectionTitle.size())
                                .line_height(TextRole::SectionTitle.line_height())
                                .font_weight(TextRole::SectionTitle.weight())
                                .text_color(theme.text_primary)
                                .child(brand::PRODUCT_NAME),
                        )
                    }),
            )
            .child(div().flex_1())
            .child(
                action_button(
                    "theme-toggle",
                    theme_label,
                    ActionRole::Secondary,
                    ControlSize::Compact,
                )
                .accessibility_label(theme_label)
                .border_color(theme.outline_subtle)
                .bg(theme.surface_high)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.dark = !this.dark;
                    crate::theme::sync_component_theme(this.theme(), this.dark, Some(window), cx);
                    let language = this.language();
                    if this.dark {
                        trace_ui(UiEvent::ThemeDarkSelected);
                        language.text("Dark theme enabled", "已切换到深色主题")
                    } else {
                        trace_ui(UiEvent::ThemeLightSelected);
                        language.text("Light theme enabled", "已切换到浅色主题")
                    }
                    .clone_into(&mut this.status);
                    cx.notify();
                })),
            )
            .child(self.proxy_control(theme, size_class != WindowSizeClass::Wide, cx))
            .child(self.routing_control(theme, size_class != WindowSizeClass::Wide, cx))
    }

    fn proxy_control(&self, theme: Theme, compact: bool, cx: &mut Context<Self>) -> AnyElement {
        let language = self.language();
        if compact {
            let next = self.proxy_mode.next();
            return Button::new("proxy-mode-cycle")
                .accessibility_label(language.text("Change proxy mode", "切换代理模式"))
                .label(compact_proxy_mode_label(
                    language,
                    self.proxy_mode,
                    self.proxy_mode_busy,
                ))
                .with_variant(ButtonVariant::Default)
                .with_size(ControlSize::Compact.component_size())
                .h(ControlSize::Compact.height())
                .px(Space::Md.px())
                .border_color(theme.outline_subtle)
                .bg(theme.surface_high)
                .text_color(theme.text_primary)
                .text_size(TextRole::Label.size())
                .when(self.proxy_mode_busy.is_none(), |button| {
                    button.icon(IconName::Redo2)
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.apply_proxy_mode(next, cx);
                }))
                .into_any_element();
        }

        let interactive = self.proxy_mode_busy.is_none();
        let mut modes = ButtonGroup::new("proxy-mode-options")
            .with_variant(ButtonVariant::Ghost)
            .with_size(ControlSize::Icon.component_size())
            .h_full();
        for mode in [ProxyMode::Off, ProxyMode::System, ProxyMode::Tun] {
            let selected = mode == self.proxy_mode;
            let pending = self.proxy_mode_busy == Some(mode);
            modes = modes.child(
                Button::new(format!("proxy-mode-{mode:?}"))
                    .accessibility_label(proxy_mode_label(language, mode))
                    .label(if pending {
                        match mode {
                            ProxyMode::Tun => language.text("Preparing TUN…", "准备 TUN…"),
                            ProxyMode::System => language.text("Enabling…", "启用中…"),
                            ProxyMode::Off => language.text("Turning off…", "关闭中…"),
                        }
                    } else {
                        proxy_mode_label(language, mode)
                    })
                    .selected(selected)
                    .tab_stop(interactive)
                    .disabled(!interactive)
                    .h_full()
                    .px(Space::Md.px())
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .font_weight(if pending || selected {
                        TextRole::Label.weight()
                    } else {
                        TextRole::Metadata.weight()
                    })
                    .bg(if pending || selected {
                        theme.action_primary
                    } else {
                        theme.surface_high
                    })
                    .text_color(if pending || selected {
                        theme.action_on_primary
                    } else {
                        theme.text_secondary
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.apply_proxy_mode(mode, cx);
                    })),
            );
        }
        div()
            .id("proxy-modes")
            .h(ControlSize::Compact.height())
            .p(px(2.0))
            .rounded(Radius::Control.px())
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_high)
            .flex()
            .items_center()
            .child(
                div()
                    .px(Space::Sm.px())
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .text_color(theme.text_tertiary)
                    .child(language.text("Proxy", "代理")),
            )
            .child(modes)
            .into_any_element()
    }

    fn routing_control(&self, theme: Theme, compact: bool, cx: &mut Context<Self>) -> AnyElement {
        let language = self.language();
        if compact {
            let next = match self.routing_mode {
                RoutingMode::Direct => RoutingMode::Global,
                RoutingMode::Global => RoutingMode::Rule,
                RoutingMode::Rule => RoutingMode::Direct,
            };
            let label = if self.routing_mode_busy.is_some() {
                language.text("Switching…", "切换中…")
            } else {
                match self.routing_mode {
                    RoutingMode::Direct => routing_mode_label(language, RoutingMode::Direct),
                    RoutingMode::Global => routing_mode_label(language, RoutingMode::Global),
                    RoutingMode::Rule => routing_mode_label(language, RoutingMode::Rule),
                }
            };
            return Button::new("routing-mode-cycle")
                .accessibility_label(language.text("Change routing mode", "切换路由模式"))
                .label(label)
                .with_variant(ButtonVariant::Default)
                .with_size(ControlSize::Compact.component_size())
                .h(ControlSize::Compact.height())
                .px(Space::Md.px())
                .border_color(theme.outline_subtle)
                .bg(theme.surface_high)
                .text_color(theme.text_primary)
                .text_size(TextRole::Label.size())
                .when(self.routing_mode_busy.is_none(), |button| {
                    button.icon(IconName::Redo2)
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.apply_routing_mode(next, cx);
                }))
                .into_any_element();
        }

        let mut modes = ButtonGroup::new("routing-mode-options")
            .with_variant(ButtonVariant::Ghost)
            .with_size(ControlSize::Icon.component_size())
            .h_full();
        for mode in [RoutingMode::Direct, RoutingMode::Global, RoutingMode::Rule] {
            let selected = mode == self.routing_mode;
            modes = modes.child(
                Button::new(format!("routing-mode-{mode:?}"))
                    .accessibility_label(routing_mode_label(language, mode))
                    .label(if self.routing_mode_busy == Some(mode) {
                        language.text("Switching…", "切换中…")
                    } else {
                        routing_mode_label(language, mode)
                    })
                    .selected(selected)
                    .h_full()
                    .px(Space::Md.px())
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .font_weight(if selected {
                        TextRole::Label.weight()
                    } else {
                        TextRole::Metadata.weight()
                    })
                    .bg(if selected {
                        theme.action_primary
                    } else {
                        theme.surface_high
                    })
                    .text_color(if selected {
                        theme.action_on_primary
                    } else {
                        theme.text_secondary
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.apply_routing_mode(mode, cx);
                    })),
            );
        }
        div()
            .id("routing-modes")
            .h(ControlSize::Compact.height())
            .p(px(2.0))
            .rounded(Radius::Control.px())
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_high)
            .flex()
            .items_center()
            .child(
                div()
                    .px(Space::Sm.px())
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .text_color(theme.text_tertiary)
                    .child(language.text("Routing", "路由")),
            )
            .child(modes)
            .into_any_element()
    }

    #[allow(clippy::too_many_lines)]
    fn navigation(&self, theme: Theme, size_class: WindowSizeClass, cx: &mut Context<Self>) -> Div {
        let language = self.language();
        let entries = [
            (
                language.message(Message::Nodes),
                language.text("Nodes", "节点"),
                PrimaryWorkspace::Nodes,
            ),
            (
                language.message(Message::PolicyGroups),
                language.text("Groups", "组"),
                PrimaryWorkspace::Policies,
            ),
            (
                language.message(Message::RoutingRules),
                language.text("Rules", "规则"),
                PrimaryWorkspace::RoutingRules,
            ),
            (
                language.message(Message::NetworkActivity),
                language.text("Activity", "活动"),
                PrimaryWorkspace::Activity,
            ),
            (
                language.message(Message::Logs),
                language.text("Logs", "日志"),
                PrimaryWorkspace::Logs,
            ),
            (
                language.message(Message::Configuration),
                language.message(Message::Configuration),
                PrimaryWorkspace::Configuration,
            ),
        ];
        let show_labels = size_class == WindowSizeClass::Wide;
        let width = match size_class {
            WindowSizeClass::Wide => LayoutMetric::WideNavigation.px(),
            WindowSizeClass::Medium => LayoutMetric::MediumNavigation.px(),
            WindowSizeClass::Compact => LayoutMetric::CompactNavigation.px(),
        };
        div()
            .w(width)
            .h_full()
            .flex_shrink_0()
            .p(Space::Sm.px())
            .flex()
            .flex_col()
            .gap(Space::Xs.px())
            .bg(theme.surface_low)
            .border_r_1()
            .border_color(theme.outline_subtle)
            .children(entries.into_iter().map(|(label, short_label, workspace)| {
                let selected = workspace == self.primary_workspace;
                div()
                    .id(format!("navigation-{workspace:?}"))
                    .role(Role::Button)
                    .aria_label(label)
                    .tab_stop(true)
                    .focusable()
                    .cursor_pointer()
                    .h(ControlSize::Standard.height())
                    .px(Space::Md.px())
                    .rounded(Radius::Row.px())
                    .flex()
                    .items_center()
                    .text_size(TextRole::Label.size())
                    .line_height(TextRole::Label.line_height())
                    .when(!show_labels, |row| {
                        row.justify_center()
                            .px_0()
                            .text_size(TextRole::Metadata.size())
                            .line_height(TextRole::Metadata.line_height())
                    })
                    .when(selected, |row| {
                        row.bg(theme.action_soft).text_color(theme.action_primary)
                    })
                    .when(!selected, |row| row.text_color(theme.text_secondary))
                    .font_weight(if selected {
                        TextRole::Label.weight()
                    } else {
                        TextRole::Metadata.weight()
                    })
                    .child(if show_labels { label } else { short_label })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.primary_workspace = workspace;
                        let language = this.language();
                        this.status = match workspace {
                            PrimaryWorkspace::Policies => {
                                trace_ui(UiEvent::WorkspacePoliciesOpened);
                                language
                                    .text("Policy groups opened", "已打开策略组工作区")
                                    .to_owned()
                            }
                            PrimaryWorkspace::Nodes => {
                                trace_ui(UiEvent::WorkspaceNodesOpened);
                                language.text("Nodes opened", "已打开节点工作区").to_owned()
                            }
                            PrimaryWorkspace::RoutingRules => {
                                trace_ui(UiEvent::WorkspaceRoutingRulesOpened);
                                language
                                    .text("Routing rules opened", "已打开分流规则")
                                    .to_owned()
                            }
                            PrimaryWorkspace::Activity => {
                                trace_ui(UiEvent::WorkspaceActivityOpened);
                                language
                                    .text("Network activity opened", "已打开网络活动")
                                    .to_owned()
                            }
                            PrimaryWorkspace::Logs => {
                                trace_ui(UiEvent::WorkspaceLogsOpened);
                                language.text("Logs opened", "已打开日志").to_owned()
                            }
                            PrimaryWorkspace::Configuration => {
                                trace_ui(UiEvent::WorkspaceConfigurationOpened);
                                language
                                    .text("Configuration opened", "已打开配置")
                                    .to_owned()
                            }
                        };
                        cx.notify();
                    }))
            }))
    }

    #[allow(clippy::too_many_lines)]
    fn empty_policy_workspace(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let language = self.language();
        let (title, description) = match &self.controller {
            ControllerState::Disconnected => (
                language.message(Message::NoPolicyGroups),
                language.text(
                    "Start or connect the kernel to load its real policy groups. Manis does not fill this page with sample data.",
                    "启动或连接内核后，这里会显示它返回的真实策略组。Manis 不再用示例数据填充此页面。",
                ),
            ),
            ControllerState::Connecting { .. } => (
                language.text("Loading policy groups…", "正在读取策略组…"),
                language.text(
                    "Waiting for the kernel to return its current groups and selected exits.",
                    "正在等待内核返回当前策略组及其已选出口。",
                ),
            ),
            ControllerState::Failed { .. } => (
                language.text("Policy groups unavailable", "暂时无法读取策略组"),
                language.text(
                    "The kernel connection failed. Check Logs for the exact error, then try again.",
                    "内核连接失败。请在“日志”中查看具体错误，然后重试。",
                ),
            ),
            ControllerState::Connected { .. } => (
                language.text("No policy groups returned", "内核未返回策略组"),
                language.text(
                    "The connected kernel did not expose any policy groups.",
                    "当前连接的内核没有提供任何策略组。",
                ),
            ),
        };

        let mut body = div()
            .id("offline-policy-scroll")
            .flex_1()
            .overflow_y_scroll()
            .p(Space::Xl.px());
        if self.managed_policy_groups.is_empty() {
            body = body.child(div().max_w(px(620.0)).child(empty_state(
                title,
                description,
                None,
                theme,
            )));
        } else {
            let mut rows = div().flex().flex_col().gap(Space::Sm.px());
            for policy in self.managed_policy_groups.clone() {
                let policy_group_id = PolicyGroupId::new(policy.id.clone());
                let expanded = self.expanded_policy_group.as_ref() == Some(&policy_group_id);
                let candidates = self.managed_policy_candidate_names(&policy);
                let candidate_count = candidates.len();
                let benchmarkable = candidate_count > 0;
                let benchmarking =
                    self.pending_policy_benchmark_name.as_deref() == Some(policy.name.as_str());
                let selected_name = self
                    .node_selection_preferences
                    .policy_target(&policy.name)
                    .map(str::to_owned);
                let edit_id = policy.id.clone();
                let remove_id = policy.id.clone();
                let benchmark_name = policy.name.clone();
                let benchmark_icon = policy.icon;
                let benchmark_policy_name = policy.name.clone();
                let toggle_id = policy_group_id.clone();
                let kind = match policy.strategy {
                    ManagedPolicyStrategy::Manual => language.text("Manual selection", "手动选择"),
                    ManagedPolicyStrategy::LowestLatency => {
                        language.text("Automatic selection", "自动选择")
                    }
                };
                let count_label = language.count(CountNoun::Node, candidate_count);
                let action = if expanded {
                    language.text("Collapse", "收起")
                } else {
                    language.text("Expand", "展开")
                };
                let header = div()
                    .id(format!("saved-policy-header-{}", policy.id))
                    .role(Role::Button)
                    .aria_label(format!("{action} {}", policy.name))
                    .tab_stop(true)
                    .focusable()
                    .cursor_pointer()
                    .min_h(px(64.0))
                    .px(Space::Lg.px())
                    .py(Space::Md.px())
                    .bg(theme.surface_low)
                    .flex()
                    .items_center()
                    .gap(Space::Md.px())
                    .child(Self::policy_group_icon(
                        &format!("saved-{}", policy.id),
                        benchmark_icon,
                        &benchmark_policy_name,
                        benchmarkable,
                        benchmarking,
                        theme,
                        cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            if !benchmarking {
                                this.pending_policy_benchmark_name = Some(benchmark_name.clone());
                                this.connect_mihomo(cx);
                            }
                        }),
                    ))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .child(
                                div()
                                    .overflow_x_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .text_size(TextRole::Body.size())
                                    .line_height(TextRole::Body.line_height())
                                    .font_weight(TextRole::Label.weight())
                                    .child(policy.name.clone()),
                            )
                            .child(
                                div()
                                    .mt(Space::Xs.px())
                                    .text_size(TextRole::Metadata.size())
                                    .line_height(TextRole::Metadata.line_height())
                                    .text_color(theme.text_secondary)
                                    .child(kind),
                            ),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .gap(Space::Md.px())
                            .child(
                                div()
                                    .text_size(TextRole::Metadata.size())
                                    .line_height(TextRole::Metadata.line_height())
                                    .text_color(theme.text_secondary)
                                    .child(count_label),
                            )
                            .child(
                                div()
                                    .min_w(px(36.0))
                                    .text_size(TextRole::Label.size())
                                    .line_height(TextRole::Label.line_height())
                                    .font_weight(TextRole::Label.weight())
                                    .text_color(theme.action_primary)
                                    .child(action),
                            ),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if this.expanded_policy_group.as_ref() == Some(&toggle_id) {
                            this.expanded_policy_group = None;
                        } else {
                            this.expanded_policy_group = Some(toggle_id.clone());
                        }
                        cx.notify();
                    }));
                let mut card = div()
                    .rounded(Radius::Pane.px())
                    .border_1()
                    .border_color(theme.outline_subtle)
                    .bg(theme.surface_high)
                    .overflow_hidden()
                    .child(header);
                if expanded {
                    if candidates.is_empty() {
                        card = card.child(
                            div()
                                .px_4()
                                .py(Space::Md.px())
                                .border_t_1()
                                .border_color(theme.outline_subtle)
                                .text_size(TextRole::Body.size())
                                .line_height(TextRole::Body.line_height())
                                .text_color(theme.text_secondary)
                                .child(language.text(
                                    "No imported nodes currently match this policy.",
                                    "当前没有已导入节点符合这个策略组。",
                                )),
                        );
                    } else {
                        for candidate in candidates {
                            card = card.child(Self::saved_policy_candidate_row(
                                candidate.clone(),
                                selected_name.as_deref() == Some(candidate.as_str()),
                                theme,
                            ));
                        }
                    }
                    card = card.child(
                        div()
                            .px_4()
                            .py(Space::Md.px())
                            .border_t_1()
                            .border_color(theme.outline_subtle)
                            .flex()
                            .justify_end()
                            .gap(Space::Sm.px())
                            .child(
                                action_button(
                                    format!("edit-offline-policy-{}", policy.id),
                                    language.text("Edit", "编辑"),
                                    ActionRole::Secondary,
                                    ControlSize::Compact,
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.start_managed_policy_edit(&edit_id, cx);
                                    },
                                )),
                            )
                            .child(
                                action_button(
                                    format!("remove-offline-policy-{}", policy.id),
                                    language.message(Message::Delete),
                                    ActionRole::Danger,
                                    ControlSize::Compact,
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.remove_managed_policy(&remove_id, cx);
                                    },
                                )),
                            ),
                    );
                }
                rows = rows.child(card);
            }
            body = body.child(rows);
        }

        div()
            .h_full()
            .flex_1()
            .min_w(px(0.0))
            .bg(theme.surface_low)
            .flex()
            .flex_col()
            .child(
                div()
                    .p(Space::Lg.px())
                    .border_b_1()
                    .border_color(theme.outline_subtle)
                    .bg(theme.surface_high)
                    .child(page_heading(
                        language.message(Message::PolicyGroups),
                        language.text(
                            "Routing rules choose policy groups; policies choose exits.",
                            "分流规则命中策略组，策略组再决定具体出口。",
                        ),
                        Some(
                            div()
                                .flex()
                                .items_center()
                                .gap(Space::Sm.px())
                                .child(Self::managed_policy_add_button(
                                    "add-policy-group-empty",
                                    language,
                                    theme,
                                    cx,
                                ))
                                .child(self.connection_button(theme, cx))
                                .into_any_element(),
                        ),
                        theme,
                    )),
            )
            .child(body)
    }

    fn saved_policy_candidate_row(name: String, current: bool, theme: Theme) -> Div {
        div()
            .min_h(px(48.0))
            .px(Space::Lg.px())
            .border_t_1()
            .border_color(theme.outline_subtle)
            .flex()
            .items_center()
            .gap(Space::Md.px())
            .bg(if current {
                theme.action_soft
            } else {
                theme.surface_high
            })
            .child(
                div()
                    .size(px(10.0))
                    .rounded_full()
                    .border_1()
                    .border_color(if current {
                        theme.action_primary
                    } else {
                        theme.outline_strong
                    })
                    .when(current, |dot| dot.bg(theme.action_primary)),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .overflow_x_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(TextRole::Body.size())
                    .line_height(TextRole::Body.line_height())
                    .text_color(theme.text_primary)
                    .child(name),
            )
    }

    #[allow(clippy::too_many_lines)]
    fn policy_list(&self, theme: Theme, width: Option<f32>, cx: &mut Context<Self>) -> Div {
        let language = self.language();
        let compact = width.is_none();
        let policy_count = self.policy_groups().count();
        let mut rows = div()
            .id("policy-scroll")
            .flex_1()
            .overflow_y_scroll()
            .p(Space::Md.px())
            .flex()
            .flex_col()
            .gap(Space::Sm.px());
        for item in self.policy_groups().cloned() {
            let selected = self.workspace.selected_group.as_ref() == Some(&item.id);
            let expanded = self.expanded_policy_group.as_ref() == Some(&item.id);
            let benchmarkable = Self::policy_group_benchmarkable(&item);
            let policy_icon = self
                .managed_policy_groups
                .iter()
                .find(|group| group.name == item.name)
                .map_or(ManagedPolicyIcon::None, |group| group.icon);
            let item_id = item.id.clone();
            let item_name = item.name.clone();
            let item_target_node = item
                .nodes
                .iter()
                .find(|node| node.name == item.target)
                .map(|node| node.id.clone());
            let benchmark_id = item.id.clone();
            let benchmark_key = Self::policy_group_benchmark_key(&item.id);
            let benchmarking = self
                .group_benchmarks
                .get(&benchmark_key)
                .is_some_and(GroupBenchmarkState::is_running);
            let action = if expanded {
                language.text("Collapse", "收起")
            } else {
                language.text("Expand", "展开")
            };
            let target = if language == Language::English {
                format!(
                    "{} · current {}",
                    policy_kind_label(language, item.kind),
                    item.target
                )
            } else {
                format!(
                    "{} · 当前 {}",
                    policy_kind_label(language, item.kind),
                    item.target
                )
            };
            let header = div()
                .id(format!("policy-{}", item.id.as_str()))
                .role(Role::Button)
                .aria_label(format!("{action} {}", item.name))
                .tab_stop(true)
                .focusable()
                .cursor_pointer()
                .min_h(px(64.0))
                .px(Space::Lg.px())
                .py(Space::Md.px())
                .flex()
                .items_center()
                .gap(Space::Md.px())
                .bg(if selected || expanded {
                    theme.surface_high
                } else {
                    theme.surface_low
                })
                .child(Self::policy_group_icon(
                    &benchmark_key,
                    policy_icon,
                    &item.name,
                    benchmarkable,
                    benchmarking,
                    theme,
                    cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        if benchmarkable && !benchmarking {
                            this.start_policy_group_benchmark(&benchmark_id, cx);
                        }
                    }),
                ))
                .child(
                    div()
                        .min_w(px(0.0))
                        .flex_1()
                        .child(
                            div()
                                .overflow_x_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_size(TextRole::Body.size())
                                .line_height(TextRole::Body.line_height())
                                .font_weight(TextRole::Label.weight())
                                .text_color(theme.text_primary)
                                .child(item.name.clone()),
                        )
                        .child(
                            div()
                                .mt(Space::Xs.px())
                                .overflow_x_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_size(TextRole::Metadata.size())
                                .line_height(TextRole::Metadata.line_height())
                                .text_color(theme.text_tertiary)
                                .child(target),
                        ),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .gap(Space::Md.px())
                        .child(
                            div()
                                .text_size(TextRole::Metadata.size())
                                .line_height(TextRole::Metadata.line_height())
                                .text_color(theme.text_secondary)
                                .child(language.count(CountNoun::Node, item.nodes.len())),
                        )
                        .child(
                            div()
                                .min_w(px(36.0))
                                .text_size(TextRole::Label.size())
                                .line_height(TextRole::Label.line_height())
                                .font_weight(TextRole::Label.weight())
                                .text_color(theme.action_primary)
                                .child(action),
                        ),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    if this.expanded_policy_group.as_ref() == Some(&item_id) {
                        this.expanded_policy_group = None;
                    } else {
                        this.expanded_policy_group = Some(item_id.clone());
                    }
                    this.policy_detail_tab = PolicyDetailTab::Nodes;
                    this.workspace.select_group(item_id.clone());
                    if compact {
                        this.workspace.navigate_back();
                    }
                    if let Some(target) = item_target_node.clone() {
                        this.workspace.select_node(target);
                    }
                    trace_ui(UiEvent::PolicyPreviewOpened);
                    this.status = if this.language() == Language::English {
                        format!("Policy group “{item_name}” {action}")
                    } else {
                        format!("策略组“{item_name}”已{action}")
                    };
                    cx.notify();
                }));

            let mut card = div()
                .rounded(Radius::Pane.px())
                .border_1()
                .border_color(theme.outline_subtle)
                .bg(theme.surface_high)
                .overflow_hidden()
                .child(header);
            if expanded {
                if let Some(feedback) =
                    self.group_benchmarks.get(&benchmark_key).and_then(|state| {
                        Self::policy_group_benchmark_feedback(
                            language,
                            state,
                            item.nodes.len(),
                            theme,
                        )
                    })
                {
                    card = card.child(feedback.mx_3().mb_2());
                }
                if item.nodes.is_empty() {
                    card = card.child(
                        div()
                            .px_4()
                            .py(Space::Md.px())
                            .border_t_1()
                            .border_color(theme.outline_subtle)
                            .text_size(TextRole::Body.size())
                            .line_height(TextRole::Body.line_height())
                            .text_color(theme.text_secondary)
                            .child(language.text(
                                "This policy has no candidate nodes.",
                                "这个策略组没有候选节点。",
                            )),
                    );
                } else {
                    for node in item.nodes.iter().cloned() {
                        let current = node.name == item.target;
                        let benchmark_state = self
                            .group_benchmarks
                            .get(&benchmark_key)
                            .map_or(GroupBenchmarkNodeState::Idle, |state| {
                                state.node_state(&node.name)
                            });
                        card = card.child(Self::policy_list_candidate_row(
                            node,
                            item.id.clone(),
                            item.name.clone(),
                            current,
                            item.kind.allows_manual_selection(),
                            self.policy_selection_busy.is_some(),
                            benchmark_state,
                            theme,
                            cx,
                        ));
                    }
                }
            }
            rows = rows.child(card);
        }

        div()
            .when_some(width, |list, width| list.w(px(width)).flex_shrink_0())
            .h_full()
            .flex()
            .flex_col()
            .bg(theme.surface_low)
            .border_r_1()
            .border_color(theme.outline_subtle)
            .child(
                div()
                    .p(Space::Lg.px())
                    .border_b_1()
                    .border_color(theme.outline_subtle)
                    .child(page_heading(
                        language.message(Message::PolicyGroups),
                        format!(
                            "{} · {}",
                            language.count(CountNoun::PolicyGroup, policy_count),
                            language.text(
                                "rules target a policy; open one to configure its exit",
                                "分流规则指定策略组；打开后配置它的出口",
                            )
                        ),
                        Some(
                            div()
                                .flex()
                                .items_center()
                                .gap(Space::Sm.px())
                                .child(Self::managed_policy_add_button(
                                    "add-policy-group-header",
                                    language,
                                    theme,
                                    cx,
                                ))
                                .child(self.connection_button(theme, cx))
                                .into_any_element(),
                        ),
                        theme,
                    )),
            )
            .child(rows)
    }

    #[allow(clippy::too_many_arguments)]
    fn policy_list_candidate_row(
        node: PolicyNode,
        policy_id: PolicyGroupId,
        policy_name: String,
        current: bool,
        manually_selectable: bool,
        selection_busy: bool,
        benchmark_state: GroupBenchmarkNodeState,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let node_id = node.id.clone();
        let node_name = node.name.clone();
        let idle_latency = node
            .latency_ms
            .map_or_else(|| "—".to_owned(), |latency| format!("{latency} ms"));
        div()
            .id(format!("policy-list-node-{}", node.id.as_str()))
            .tab_stop(manually_selectable && !selection_busy)
            .min_h(px(48.0))
            .px(Space::Lg.px())
            .border_t_1()
            .border_color(theme.outline_subtle)
            .flex()
            .items_center()
            .gap(Space::Md.px())
            .bg(if current {
                theme.action_soft
            } else {
                theme.surface_high
            })
            .child(
                div()
                    .size(px(10.0))
                    .rounded_full()
                    .border_1()
                    .border_color(if current {
                        theme.action_primary
                    } else {
                        theme.outline_strong
                    })
                    .when(current, |dot| dot.bg(theme.action_primary)),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .overflow_x_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(TextRole::Body.size())
                    .line_height(TextRole::Body.line_height())
                    .text_color(if manually_selectable {
                        theme.text_primary
                    } else {
                        theme.text_secondary
                    })
                    .child(node.name),
            )
            .child(Self::benchmark_latency_content(
                benchmark_state,
                idle_latency,
                &format!("policy-list-node-{}-spinner", node.id.as_str()),
                theme,
            ))
            .when(manually_selectable, |row| {
                row.role(Role::RadioButton)
                    .aria_toggled(if current {
                        Toggled::True
                    } else {
                        Toggled::False
                    })
                    .focusable()
                    .when(!selection_busy, gpui::Styled::cursor_pointer)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !selection_busy {
                            this.select_policy_node(
                                policy_id.clone(),
                                policy_name.clone(),
                                node_id.clone(),
                                node_name.clone(),
                                cx,
                            );
                        }
                    }))
            })
    }

    fn managed_policy_add_button(
        id: &'static str,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Button {
        action_button(
            id,
            language.message(Message::AddPolicyGroup),
            ActionRole::Primary,
            ControlSize::Compact,
        )
        .accessibility_label(language.message(Message::AddPolicyGroup))
        .cursor_pointer()
        .bg(theme.action_primary)
        .text_color(theme.action_on_primary)
        .font_weight(FontWeight::SEMIBOLD)
        .on_click(cx.listener(|this, _, _, cx| {
            this.workspace.compact_navigation = CompactNavigation::GroupDetail;
            this.start_managed_policy_create(cx);
        }))
    }

    fn connection_button(&self, theme: Theme, cx: &mut Context<Self>) -> Button {
        let connecting = matches!(self.controller, ControllerState::Connecting { .. });
        let language = self.language();
        action_button(
            "connect-mihomo",
            self.runtime
                .button_label_in(&self.controller, self.language()),
            ActionRole::Secondary,
            ControlSize::Compact,
        )
        .accessibility_label(
            if matches!(self.controller, ControllerState::Failed { .. }) {
                language.message(Message::Retry)
            } else {
                language.message(Message::ConnectMihomo)
            },
        )
        .tab_stop(!connecting)
        .px_3()
        .cursor_pointer()
        .border_color(if connecting {
            theme.outline_subtle
        } else {
            theme.action_primary
        })
        .bg(if connecting {
            theme.surface_high
        } else {
            theme.action_soft
        })
        .text_color(if connecting {
            theme.text_tertiary
        } else {
            theme.action_primary
        })
        .on_click(cx.listener(|this, _, _, cx| this.connect_mihomo(cx)))
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn node_row(
        item: PolicyNode,
        source: String,
        policy_id: PolicyGroupId,
        policy_name: String,
        current: bool,
        manually_selectable: bool,
        selection_busy: bool,
        benchmark_state: GroupBenchmarkNodeState,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let node_id = item.id.clone();
        let node_name = item.name.clone();
        let detail = if item.detail.trim().is_empty() {
            language.text("Unknown type", "类型未知").to_owned()
        } else {
            item.detail.clone()
        };
        let idle_latency = item
            .latency_ms
            .map_or_else(|| "—".to_owned(), |latency| format!("{latency} ms"));
        let spinner_id = format!("policy-node-{}-latency", item.id.as_str());
        let leading = if manually_selectable {
            div()
                .size(px(18.0))
                .rounded_full()
                .border_2()
                .border_color(if current {
                    theme.action_primary
                } else {
                    theme.outline_strong
                })
                .when(current, |dot| dot.bg(theme.action_primary))
        } else {
            div()
                .size(px(22.0))
                .rounded(Radius::Control.px())
                .bg(theme.surface_high)
                .text_size(TextRole::Metadata.size())
                .line_height(TextRole::Metadata.line_height())
                .font_weight(TextRole::Label.weight())
                .text_color(theme.text_tertiary)
                .flex()
                .items_center()
                .justify_center()
                .child(
                    if item.kind == manis_core::PolicyCandidateKind::PolicyGroup {
                        language.text("G", "组")
                    } else {
                        language.text("N", "点")
                    },
                )
        };
        div()
            .id(format!("node-{}", item.id.as_str()))
            .tab_stop(manually_selectable && !selection_busy)
            .min_h(px(64.0))
            .px(Space::Md.px())
            .flex()
            .items_center()
            .gap(Space::Md.px())
            .rounded(Radius::Row.px())
            .bg(if manually_selectable && current {
                theme.action_soft
            } else {
                theme.surface_low
            })
            .child(leading)
            .child(
                div()
                    .flex_1()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(Space::Sm.px())
                            .min_w(px(0.0))
                            .overflow_x_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(TextRole::Body.size())
                            .line_height(TextRole::Body.line_height())
                            .font_weight(TextRole::Label.weight())
                            .text_color(if manually_selectable {
                                theme.text_primary
                            } else {
                                theme.text_secondary
                            })
                            .child(item.name)
                            .when(current && !manually_selectable, |name| {
                                name.child(div().child(status_badge(
                                    language.text("Current", "当前出口"),
                                    StatusTone::Neutral,
                                    theme,
                                )))
                            }),
                    )
                    .child(
                        div()
                            .mt(Space::Xs.px())
                            .text_size(TextRole::Metadata.size())
                            .line_height(TextRole::Metadata.line_height())
                            .text_color(theme.text_tertiary)
                            .child(detail),
                    ),
            )
            .child(
                div()
                    .w(px(100.0))
                    .overflow_x_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .text_color(if manually_selectable {
                        theme.text_secondary
                    } else {
                        theme.text_tertiary
                    })
                    .child(source),
            )
            .child(
                div()
                    .w(px(64.0))
                    .min_h(px(18.0))
                    .flex()
                    .items_center()
                    .justify_end()
                    .child(Self::benchmark_latency_content(
                        benchmark_state,
                        idle_latency,
                        &spinner_id,
                        theme,
                    )),
            )
            .when(manually_selectable, |row| {
                row.role(Role::RadioButton)
                    .aria_toggled(if current {
                        Toggled::True
                    } else {
                        Toggled::False
                    })
                    .focusable()
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !selection_busy {
                            this.select_policy_node(
                                policy_id.clone(),
                                policy_name.clone(),
                                node_id.clone(),
                                node_name.clone(),
                                cx,
                            );
                        }
                    }))
            })
    }

    fn editable_policy_group_id(&self, policy_name: &str) -> Option<&str> {
        self.managed_policy_groups
            .iter()
            .find(|group| group.name == policy_name)
            .map(|group| group.id.as_str())
    }

    fn policy_detail_tabs(
        &self,
        editable_group_id: Option<String>,
        language: Language,
        cx: &mut Context<Self>,
    ) -> TabBar {
        let app = cx.entity();
        TabBar::new("policy-detail-tabs")
            .underline()
            .selected_index(self.policy_detail_tab.index())
            .child(
                Tab::new()
                    .label(language.text("Nodes", "节点"))
                    .aria_label(language.text("Nodes", "节点")),
            )
            .child(
                Tab::new()
                    .label(language.text("Rules", "规则"))
                    .aria_label(language.text("Rules", "规则")),
            )
            .child(
                Tab::new()
                    .label(language.message(Message::Settings))
                    .aria_label(language.message(Message::Settings)),
            )
            .on_click(move |index, _, cx| {
                let tab = PolicyDetailTab::from_index(*index);
                app.update(cx, |this, cx| {
                    this.policy_detail_tab = tab;
                    if tab == PolicyDetailTab::Settings {
                        if let Some(group_id) = editable_group_id.as_deref() {
                            let already_editing = this
                                .managed_policy_draft
                                .as_ref()
                                .and_then(|draft| draft.editing_id.as_deref())
                                == Some(group_id);
                            if !already_editing {
                                this.start_managed_policy_edit(group_id, cx);
                                return;
                            }
                        } else {
                            this.language()
                                .text(
                                    "This runtime policy is read-only in Manis",
                                    "这个运行时策略组在 Manis 中为只读",
                                )
                                .clone_into(&mut this.status);
                        }
                    }
                    cx.notify();
                });
            })
    }

    #[allow(clippy::too_many_lines)]
    fn detail(&self, theme: Theme, compact: bool, cx: &mut Context<Self>) -> Div {
        let language = self.language();
        let (Some(selected_policy), Some(selected_node)) =
            (self.selected_policy().cloned(), self.selected_node())
        else {
            return div().h_full().flex_1().bg(theme.surface_high);
        };
        let manually_selectable = selected_policy.kind.allows_manual_selection();
        let benchmark_id = selected_policy.id.clone();
        let benchmarkable = Self::policy_group_benchmarkable(&selected_policy);
        let benchmark_key = Self::policy_group_benchmark_key(&selected_policy.id);
        let benchmarking = self
            .group_benchmarks
            .get(&benchmark_key)
            .is_some_and(GroupBenchmarkState::is_running);
        let editable_group_id = self
            .editable_policy_group_id(&selected_policy.name)
            .map(str::to_owned);
        let display_icon = self
            .managed_policy_groups
            .iter()
            .find(|group| group.name == selected_policy.name)
            .map_or(ManagedPolicyIcon::None, |group| group.icon);
        let guidance = match selected_policy.kind {
            manis_core::PolicyGroupKind::Selector => language.text(
                "Choose the exit used when a routing rule targets this policy",
                "分流规则命中此策略组时，使用下方所选出口",
            ),
            manis_core::PolicyGroupKind::UrlTest => language.text(
                "Mihomo measures the configured URL on schedule; candidates are automatic",
                "Mihomo 按策略配置的 URL 和间隔自动测量，候选项不可手动切换",
            ),
            manis_core::PolicyGroupKind::Fallback => language.text(
                "Mihomo checks candidates on schedule and fails over automatically",
                "Mihomo 按策略配置的间隔自动检查并故障转移，候选项不可手动切换",
            ),
            manis_core::PolicyGroupKind::LoadBalance => language.text(
                "Mihomo distributes connections across candidates automatically",
                "Mihomo 自动在候选分组之间分配连接，候选项不可手动切换",
            ),
            manis_core::PolicyGroupKind::Direct => language.text(
                "Direct policies have no selectable exit",
                "直连策略没有可切换的出口",
            ),
        };
        let mut body = div()
            .id("detail-scroll")
            .flex_1()
            .overflow_y_scroll()
            .p(Space::Lg.px())
            .flex()
            .flex_col()
            .gap(Space::Md.px());

        if self.policy_detail_tab == PolicyDetailTab::Nodes {
            body = body.child(section_heading(
                language.text("Candidate nodes", "候选节点"),
                guidance,
                None,
                theme,
            ));
            if selected_policy.kind.is_automatic() {
                body = body.child(
                    div()
                        .p(Space::Md.px())
                        .rounded(Radius::Row.px())
                        .bg(theme.surface_low)
                        .text_size(TextRole::Metadata.size())
                        .line_height(TextRole::Metadata.line_height())
                        .text_color(theme.text_secondary)
                        .child(language.text(
                            "Automatic policy. Manis shows candidates for inspection; Mihomo selects the active exit from the policy settings.",
                            "自动策略。Manis 展示候选项供检查；实际出口由 Mihomo 按策略设置自动选择。",
                        )),
                );
            }
            if let Some(feedback) = self.group_benchmarks.get(&benchmark_key).and_then(|state| {
                Self::policy_group_benchmark_feedback(
                    language,
                    state,
                    selected_policy.nodes.len(),
                    theme,
                )
            }) {
                body = body.child(feedback);
            }
            body = body.child(
                div()
                    .mt(Space::Sm.px())
                    .px(Space::Md.px())
                    .flex()
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .font_weight(TextRole::Label.weight())
                    .text_color(theme.text_tertiary)
                    .child(
                        div()
                            .flex_1()
                            .child(language.text("Candidate / group", "候选节点 / 分组")),
                    )
                    .child(div().w(px(100.0)).child(language.text("Source", "来源")))
                    .child(div().w(px(64.0)).child(language.text("Latency", "延迟"))),
            );
            for item in selected_policy.nodes.iter().cloned() {
                let current = item.id == selected_node.id;
                let source = self.policy_node_source_label(&item, language);
                let benchmark_state = self
                    .group_benchmarks
                    .get(&benchmark_key)
                    .map_or(GroupBenchmarkNodeState::Idle, |state| {
                        state.node_state(&item.name)
                    });
                body = body.child(Self::node_row(
                    item,
                    source,
                    selected_policy.id.clone(),
                    selected_policy.name.clone(),
                    current,
                    manually_selectable,
                    self.policy_selection_busy.is_some(),
                    benchmark_state,
                    language,
                    theme,
                    cx,
                ));
            }
        }

        if self.policy_detail_tab == PolicyDetailTab::Rules {
            body = body.child(section_heading(
                language.text("Rules using this policy", "命中此策略的规则"),
                format!(
                    "{} · {}",
                    language.count(CountNoun::Rule, selected_policy.rules_count()),
                    language.text("matched in order", "按顺序匹配")
                ),
                None,
                theme,
            ));
            if selected_policy.rules.is_empty() {
                body = body.child(empty_state(
                    language.text("No rule preview", "暂无规则预览"),
                    language.text(
                        "No routing rule currently targets this policy group.",
                        "当前没有分流规则指向这个策略组。",
                    ),
                    None,
                    theme,
                ));
            }
            for rule in &selected_policy.rules {
                body = body.child(
                    div()
                        .min_h(px(50.0))
                        .flex()
                        .items_center()
                        .gap(Space::Md.px())
                        .border_t_1()
                        .border_color(theme.outline_subtle)
                        .child(
                            div()
                                .w(px(36.0))
                                .text_size(TextRole::Metadata.size())
                                .line_height(TextRole::Metadata.line_height())
                                .text_color(theme.text_tertiary)
                                .child(format!("#{}", rule.index)),
                        )
                        .child(
                            div()
                                .min_w(px(0.0))
                                .flex_1()
                                .overflow_x_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_size(TextRole::Body.size())
                                .line_height(TextRole::Body.line_height())
                                .child(format!("{}, {}", rule.kind, rule.payload)),
                        )
                        .child(status_badge(
                            language.text("Match", "命中"),
                            StatusTone::Success,
                            theme,
                        )),
                );
            }
        }

        if self.policy_detail_tab == PolicyDetailTab::Settings {
            if let Some(group_id) = editable_group_id.as_deref() {
                let edit_id = group_id.to_owned();
                let remove_id = group_id.to_owned();
                body = body
                    .child(section_heading(
                        language.text("Managed policy settings", "托管策略组设置"),
                        language.text(
                            "Saved in Manis and applied to the managed Mihomo configuration.",
                            "保存在 Manis 中，并会应用到 Manis 托管的 Mihomo 配置。",
                        ),
                        None,
                        theme,
                    ))
                    .child(
                        div()
                            .mt(Space::Sm.px())
                            .flex()
                            .gap(Space::Sm.px())
                            .child(
                                action_button(
                                    "edit-managed-policy",
                                    language.text("Edit policy group", "编辑策略组"),
                                    ActionRole::Secondary,
                                    ControlSize::Compact,
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.start_managed_policy_edit(&edit_id, cx);
                                    },
                                )),
                            )
                            .child(
                                action_button(
                                    "remove-managed-policy",
                                    language.text("Delete policy group", "删除策略组"),
                                    ActionRole::Danger,
                                    ControlSize::Compact,
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.remove_managed_policy(&remove_id, cx);
                                    },
                                )),
                            ),
                    );
            } else {
                body = body
                    .child(section_heading(
                        language.text("Runtime policy", "运行时策略组"),
                        language.text(
                            "This policy comes from the active kernel configuration and is read-only.",
                            "这个策略组来自当前内核配置，因此为只读。",
                        ),
                        Some(
                            status_badge(
                                language.text("Read-only", "只读"),
                                StatusTone::Neutral,
                                theme,
                            )
                            .into_any_element(),
                        ),
                        theme,
                    ))
                    .child(
                        Self::managed_policy_add_button(
                            "add-policy-group-readonly",
                            language,
                            theme,
                            cx,
                        )
                        .mt(Space::Sm.px()),
                    );
            }
        }

        div()
            .h_full()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .bg(theme.surface_high)
            .child(
                div()
                    .p(Space::Lg.px())
                    .border_b_1()
                    .border_color(theme.outline_subtle)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(Space::Md.px())
                            .when(compact, |header| {
                                header.child(
                                    Button::new("compact-back")
                                        .accessibility_label(
                                            language.text("Back to policy groups", "返回策略组"),
                                        )
                                        .label(language.text("Back", "返回"))
                                        .icon(IconName::ArrowLeft)
                                        .with_size(ControlSize::Compact.component_size())
                                        .h(ControlSize::Compact.height())
                                        .with_variant(ButtonVariant::Text)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.workspace.navigate_back();
                                            cx.notify();
                                        })),
                                )
                            })
                            .child(Self::policy_group_icon(
                                &benchmark_key,
                                display_icon,
                                &selected_policy.name,
                                benchmarkable,
                                benchmarking,
                                theme,
                                cx.listener(move |this, _, _, cx| {
                                    if benchmarkable && !benchmarking {
                                        this.start_policy_group_benchmark(&benchmark_id, cx);
                                    }
                                }),
                            ))
                            .child(
                                div()
                                    .min_w(px(0.0))
                                    .flex_1()
                                    .child(
                                        div()
                                            .overflow_x_hidden()
                                            .whitespace_nowrap()
                                            .text_ellipsis()
                                            .text_size(TextRole::PageTitle.size())
                                            .line_height(TextRole::PageTitle.line_height())
                                            .font_weight(TextRole::PageTitle.weight())
                                            .child(selected_policy.name.clone()),
                                    )
                                    .child(
                                        div()
                                            .mt(Space::Xs.px())
                                            .overflow_x_hidden()
                                            .whitespace_nowrap()
                                            .text_ellipsis()
                                            .text_size(TextRole::Metadata.size())
                                            .line_height(TextRole::Metadata.line_height())
                                            .text_color(theme.text_secondary)
                                            .child(format!(
                                                "{} · {} · {}",
                                                policy_kind_label(language, selected_policy.kind),
                                                language.count(
                                                    CountNoun::Node,
                                                    selected_policy.nodes.len()
                                                ),
                                                language.count(
                                                    CountNoun::Rule,
                                                    selected_policy.rules_count()
                                                )
                                            )),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .mt(Space::Lg.px())
                            .font_weight(TextRole::Label.weight())
                            .child(self.policy_detail_tabs(editable_group_id, language, cx)),
                    ),
            )
            .child(body)
    }

    fn signal_stage(
        index: &str,
        label: &str,
        value: String,
        detail: String,
        route: bool,
        theme: Theme,
    ) -> Div {
        div()
            .min_h(px(104.0))
            .flex()
            .gap_3()
            .child(
                div().w(px(40.0)).flex().justify_center().child(
                    div()
                        .mt_2()
                        .size(px(34.0))
                        .rounded_full()
                        .border_2()
                        .border_color(theme.outline_strong)
                        .bg(theme.surface_high)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(div().size(px(9.0)).rounded_full().bg(if route {
                            theme.route_trace
                        } else {
                            theme.action_primary
                        })),
                ),
            )
            .child(
                div()
                    .pt_2()
                    .flex_1()
                    .child(
                        div()
                            .text_size(TextRole::Label.size())
                            .line_height(TextRole::Label.line_height())
                            .font_weight(TextRole::Label.weight())
                            .text_color(theme.text_tertiary)
                            .child(format!("{index} · {label}")),
                    )
                    .child(div().mt_1().font_weight(FontWeight::BOLD).child(value))
                    .child(div().mt_1().text_color(theme.text_secondary).child(detail)),
            )
    }

    #[allow(clippy::too_many_lines)]
    fn route_prediction_panel(
        &self,
        prediction: &RouteInspectorPrediction,
        language: Language,
        theme: Theme,
    ) -> Div {
        match prediction {
            RouteInspectorPrediction::Idle => div()
                .p_4()
                .rounded_md()
                .border_1()
                .border_color(theme.outline_subtle)
                .bg(theme.surface_high)
                .font_weight(FontWeight::SEMIBOLD)
                .child(language.text(
                    "Enter a destination to test its first matching rule",
                    "输入目标地址，测试首条命中规则",
                ))
                .child(
                    div()
                        .mt_2()
                        .font_weight(FontWeight::NORMAL)
                        .text_color(theme.text_secondary)
                        .child(language.text(
                            "The result is a local prediction and does not create a connection.",
                            "结果是本地预测，不会发起网络连接。",
                        )),
                ),
            RouteInspectorPrediction::Invalid(_) => div()
                .p_4()
                .rounded_md()
                .border_1()
                .border_color(theme.status_error)
                .bg(theme.surface_high)
                .text_color(theme.text_secondary)
                .child(language.text(
                    "Correct the destination above, then test again.",
                    "请修正上方目标地址后重新测试。",
                )),
            RouteInspectorPrediction::Ready(DomainRoutePrediction::Matched {
                query,
                rule,
                target,
                uncertain_rules,
            }) => {
                let rule_detail = if rule.payload.is_empty() {
                    format!("{} #{}", language.text("Rule", "规则"), rule.index)
                } else {
                    format!(
                        "{} · {} #{}",
                        rule.payload,
                        language.text("rule", "规则"),
                        rule.index
                    )
                };
                let (policy, decision, exit, exit_detail) = match target {
                    RouteTarget::Policy(id) => self
                        .catalog
                        .as_ref()
                        .and_then(|catalog| catalog.group(id))
                        .map_or_else(
                            || {
                                (
                                    id.as_str().to_owned(),
                                    language
                                        .text("Runtime policy group", "运行时策略组")
                                        .to_owned(),
                                    language
                                        .text("Needs an actual connection", "需要实际连接确认")
                                        .to_owned(),
                                    language
                                        .text(
                                            "The policy group is not present in the current snapshot",
                                            "当前快照中没有这个策略组",
                                        )
                                        .to_owned(),
                                )
                            },
                            |group| {
                                let decision = if group.kind.allows_manual_selection() {
                                    format!(
                                        "{} · {}",
                                        policy_kind_label(language, group.kind),
                                        language.text("current selection", "当前选择")
                                    )
                                } else {
                                    format!(
                                        "{} · {}",
                                        policy_kind_label(language, group.kind),
                                        language.text("runtime decision", "运行时决策")
                                    )
                                };
                                if group.kind == manis_core::PolicyGroupKind::LoadBalance {
                                    return (
                                        group.name.clone(),
                                        decision,
                                        language
                                            .text("Per-connection choice", "按连接选择")
                                            .to_owned(),
                                        language
                                            .text(
                                                "Load balancing has no single predicted exit",
                                                "负载均衡没有单一的预测出口",
                                            )
                                            .to_owned(),
                                    );
                                }
                                let node = self.node_for_policy(group);
                                let exit_detail = format!(
                                    "{} · {}",
                                    node.latency_ms.map_or_else(
                                        || language.text("Unknown latency", "延迟未知").to_owned(),
                                        |latency| format!("{latency} ms")
                                    ),
                                    node.provider.as_deref().unwrap_or(language.text(
                                        "Built-in or runtime",
                                        "内置或运行时节点"
                                    ))
                                );
                                (group.name.clone(), decision, node.name, exit_detail)
                            },
                        ),
                    RouteTarget::Direct => (
                        "DIRECT".to_owned(),
                        language.text("Bypass proxy", "不经过代理").to_owned(),
                        language.text("Direct connection", "直连").to_owned(),
                        language
                            .text("No proxy node is selected", "不会选择代理节点")
                            .to_owned(),
                    ),
                    RouteTarget::Reject => (
                        "REJECT".to_owned(),
                        language.text("Block request", "阻止请求").to_owned(),
                        language.text("Connection blocked", "连接被阻止").to_owned(),
                        language
                            .text("No outbound connection is created", "不会建立出站连接")
                            .to_owned(),
                    ),
                    RouteTarget::Named(name) => (
                        name.clone(),
                        language.text("Runtime target", "运行时目标").to_owned(),
                        language
                            .text("Needs an actual connection", "需要实际连接确认")
                            .to_owned(),
                        language
                            .text(
                                "The target is not a visible policy group",
                                "该目标不是当前可见策略组",
                            )
                            .to_owned(),
                    ),
                };

                let query_detail = if language == Language::English {
                    if query.has_explicit_port() {
                        format!(
                            "Tested destination  {}:{}",
                            query.domain().as_str(),
                            query.port()
                        )
                    } else {
                        format!(
                            "Tested destination  {} · assumed port {}",
                            query.domain().as_str(),
                            query.port()
                        )
                    }
                } else if query.has_explicit_port() {
                    format!("测试目标  {}:{}", query.domain().as_str(), query.port())
                } else {
                    format!(
                        "测试目标  {} · 默认按 {} 端口",
                        query.domain().as_str(),
                        query.port()
                    )
                };
                let uncertainty = uncertain_rules.first().map(|first| {
                    let first_rule = if first.payload.is_empty() {
                        format!("#{} {}", first.index, first.kind)
                    } else {
                        format!("#{} {} · {}", first.index, first.kind, first.payload)
                    };
                    if language == Language::English {
                        format!(
                            "Conditional result: {} earlier rule(s) need more context and may override this match. First: {first_rule}",
                            uncertain_rules.len()
                        )
                    } else {
                        format!(
                            "条件预测：前面有 {} 条规则需要更多连接信息，实际连接时可能覆盖本次结果。第一条：{first_rule}",
                            uncertain_rules.len()
                        )
                    }
                });

                div()
                    .child(
                        div()
                            .mb_3()
                            .text_size(TextRole::Label.size())
                            .line_height(TextRole::Label.line_height())
                            .font_weight(TextRole::Label.weight())
                            .text_color(theme.text_secondary)
                            .child(query_detail),
                    )
                    .when_some(uncertainty, |panel, uncertainty| {
                        panel.child(
                            div()
                                .mb_3()
                                .p_3()
                                .rounded_md()
                                .bg(theme.route_soft)
                                .text_size(TextRole::Body.size())
                                .line_height(TextRole::Body.line_height())
                                .text_color(theme.text_secondary)
                                .child(uncertainty),
                        )
                    })
                    .child(
                        div()
                            .relative()
                            .child(
                                div()
                                    .absolute()
                                    .left(px(19.0))
                                    .top(px(28.0))
                                    .bottom(px(70.0))
                                    .w(px(2.0))
                                    .bg(theme.route_trace),
                            )
                            .child(Self::signal_stage(
                                "01",
                                language.text("First matching rule", "首条命中规则"),
                                rule.kind.clone(),
                                rule_detail,
                                true,
                                theme,
                            ))
                            .child(Self::signal_stage(
                                "02",
                                language.text("Policy group", "交给策略组"),
                                policy,
                                decision,
                                false,
                                theme,
                            ))
                            .child(Self::signal_stage(
                                "03",
                                language.text("Final exit", "最终出口"),
                                exit,
                                exit_detail,
                                false,
                                theme,
                            )),
                    )
            }
            RouteInspectorPrediction::Ready(DomainRoutePrediction::NeedsConnection {
                blocking_rule,
                reason,
                ..
            }) => {
                let rule = blocking_rule.as_ref().map(|rule| {
                    if rule.payload.is_empty() {
                        format!("#{} {}", rule.index, rule.kind)
                    } else {
                        format!("#{} {} · {}", rule.index, rule.kind, rule.payload)
                    }
                });
                div()
                    .p_4()
                    .rounded_md()
                    .border_1()
                    .border_color(theme.status_warning)
                    .bg(theme.surface_high)
                    .child(
                        div()
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.status_warning)
                            .child(
                                language.text(
                                    "An actual connection is required",
                                    "需要实际连接才能确认",
                                ),
                            ),
                    )
                    .child(
                        div()
                            .mt_2()
                            .text_color(theme.text_secondary)
                            .child(route_prediction_reason_copy(*reason, language)),
                    )
                    .when_some(rule, |panel, rule| {
                        panel.child(
                            div()
                                .mt_3()
                                .text_size(TextRole::Label.size())
                                .line_height(TextRole::Label.line_height())
                                .font_weight(TextRole::Label.weight())
                                .child(rule),
                        )
                    })
            }
        }
    }

    fn open_route_inspector(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let language = self.language();
        let size_class = WindowSizeClass::for_width(window.viewport_size().width.as_f32());
        language
            .text("Local route prediction opened", "已打开本地路由预测")
            .clone_into(&mut self.status);
        trace_ui(UiEvent::RouteInspectorOpened);

        if size_class == WindowSizeClass::Wide {
            if let Some(input) = self.route_domain_input.as_ref() {
                input.focus_handle(cx).focus(window, cx);
            }
            cx.notify();
            return;
        }

        if window.has_active_sheet(cx) {
            if let Some(input) = self.route_domain_input.as_ref() {
                input.focus_handle(cx).focus(window, cx);
            }
            return;
        }

        self.inspector_open = true;
        gpui_component::Theme::global_mut(cx).sheet.margin_top = px(48.0);
        let app = cx.entity();
        let content_app = app.clone();
        let content = cx.new(move |cx| {
            cx.observe(&content_app, |_, _, cx| cx.notify()).detach();
            RouteInspectorSheetContent { app: content_app }
        });
        window.open_sheet(cx, move |sheet, _, _| {
            let app_for_close = app.clone();
            sheet
                .title(language.text("Rule test", "规则测试"))
                .size(px(340.0))
                .resizable(false)
                .on_close(move |_, _, cx| {
                    app_for_close.update(cx, |this, cx| {
                        this.close_route_inspector(cx);
                    });
                })
                .child(content.clone())
        });
        if let Some(input) = self.route_domain_input.as_ref() {
            input.focus_handle(cx).focus(window, cx);
        }
        cx.notify();
    }

    fn close_route_inspector(&mut self, cx: &mut Context<Self>) {
        if self.inspector_open {
            self.inspector_open = false;
            trace_ui(UiEvent::RouteInspectorClosed);
            cx.notify();
        }
    }

    fn route_inspector_badge(&self, language: Language) -> &'static str {
        match &self.route_prediction {
            RouteInspectorPrediction::Ready(DomainRoutePrediction::Matched {
                uncertain_rules,
                ..
            }) if !uncertain_rules.is_empty() => language.text("Conditional path", "条件预测"),
            RouteInspectorPrediction::Ready(DomainRoutePrediction::Matched { .. }) => {
                language.text("Predicted path", "预测路径")
            }
            RouteInspectorPrediction::Ready(DomainRoutePrediction::NeedsConnection { .. }) => {
                language.text("Needs connection", "需要连接")
            }
            RouteInspectorPrediction::Idle | RouteInspectorPrediction::Invalid(_) => {
                language.text("Local model", "本地模型")
            }
        }
    }

    fn inspector_badge(&self, theme: Theme) -> Div {
        let language = self.language();
        status_badge(
            self.route_inspector_badge(language),
            StatusTone::Route,
            theme,
        )
    }

    fn inspector_title(&self, theme: Theme) -> Div {
        let language = self.language();
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .text_size(TextRole::SectionTitle.size())
                    .line_height(TextRole::SectionTitle.line_height())
                    .font_weight(TextRole::SectionTitle.weight())
                    .child(language.text("Rule test", "规则测试")),
            )
            .child(self.inspector_badge(theme))
    }

    fn inspector_form(
        &self,
        theme: Theme,
        on_predict: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
    ) -> Div {
        let language = self.language();
        let prediction = self.route_prediction.clone();
        let route_input = self.route_domain_input.clone();
        let input_error = match &prediction {
            RouteInspectorPrediction::Invalid(error) => {
                Some(route_query_error_copy(*error, language))
            }
            RouteInspectorPrediction::Idle | RouteInspectorPrediction::Ready(_) => None,
        };

        div()
            .child(
                div()
                    .text_color(theme.text_secondary)
                    .child(language.text(
                        "Test which ordered rule, policy group, and exit a destination will use",
                        "测试目标会依次命中哪条规则、策略组和出口",
                    )),
            )
            .child(
                div()
                    .mt_4()
                    .child(
                        div()
                            .text_size(TextRole::Label.size())
                            .line_height(TextRole::Label.line_height())
                            .font_weight(TextRole::Label.weight())
                            .child(language.text("Destination", "目标地址")),
                    )
                    .child(
                        div()
                            .mt_2()
                            .flex()
                            .gap_2()
                            .when_some(route_input, |row, input| {
                                row.child(div().min_w_0().flex_1().child(input))
                            })
                            .child(
                                action_button(
                                    "predict-route",
                                    language.text("Predict", "预测"),
                                    ActionRole::Primary,
                                    ControlSize::Standard,
                                )
                                    .accessibility_label(language.text(
                                        "Predict route for this domain",
                                        "预测此域名的路由",
                                    ))
                                    .px_3()
                                    .on_click(on_predict),
                            ),
                    )
                    .when_some(input_error, |form, error| {
                        form.child(
                            div()
                                .mt_2()
                                .text_size(TextRole::Body.size())
                                .line_height(TextRole::Body.line_height())
                                .font_weight(TextRole::Label.weight())
                                .text_color(theme.status_error)
                                .child(error),
                        )
                    })
                    .child(
                        div()
                            .mt_2()
                            .text_size(TextRole::Metadata.size())
                            .line_height(TextRole::Metadata.line_height())
                            .text_color(theme.text_tertiary)
                            .child(language.text(
                                "Enter a domain or domain:port. Port 443 is assumed when omitted; do not include a protocol, path, or wildcard.",
                                "输入域名或域名:端口；省略端口时默认按 443，不要包含协议、路径或通配符。",
                            )),
                    ),
            )
    }

    fn inspector_results(&self, theme: Theme) -> Div {
        let language = self.language();
        let prediction = self.route_prediction.clone();
        let observed_route = self.observed_routes.first().cloned();

        div()
            .child(self.route_prediction_panel(&prediction, language, theme))
            .when_some(observed_route, |panel, observed| {
                        let host = observed.host.unwrap_or_else(|| language.text("Unknown target", "目标未知").to_owned());
                        let rule = observed.rule.unwrap_or_else(|| language.text("Unknown rule", "规则未知").to_owned());
                        let payload = observed.rule_payload.unwrap_or_default();
                        let chain = activity::route_summary(&observed.chains, language)
                            .unwrap_or_else(|| language.text("No route returned", "链路未返回").to_owned());
                        panel.child(
                            div()
                                .mt_3()
                                .p_3()
                                .rounded_md()
                                .border_1()
                                .border_color(theme.action_primary)
                                .bg(theme.action_soft)
                                .child(
                                    div()
                                        .text_size(TextRole::Label.size())
                                        .line_height(TextRole::Label.line_height())
                                        .font_weight(TextRole::Label.weight())
                                        .text_color(theme.action_primary)
                                        .child(language.text("Recently observed, separate from this prediction · /connections", "最近已观察（独立于本次预测）· /connections")),
                                )
                                .child(div().mt_2().font_weight(FontWeight::BOLD).child(host))
                                .child(
                                    div()
                                        .mt_1()
                                        .text_color(theme.text_secondary)
                                        .child(format!("{rule} · {payload}")),
                                )
                                .child(
                                    div()
                                        .mt_2()
                                        .text_color(theme.text_primary)
                                        .child(chain),
                                ),
                        )
                    })
            .child(
                div()
                    .mt_4()
                    .pt_4()
                    .border_t_1()
                    .border_color(theme.outline_subtle)
                    .text_color(theme.text_secondary)
                    .child(language.text("Evaluation source        Current ordered rule snapshot", "评估来源             当前有序规则快照"))
                    .child(div().mt_2().child(language.text("DNS                      Not queried", "DNS                  未查询")))
                    .child(div().mt_2().child(language.text("Result type              Local prediction", "结果类型             本地预测"))),
            )
            .child(
                div()
                    .mt_5()
                    .pt_4()
                    .border_t_1()
                    .border_color(theme.outline_subtle)
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .text_color(theme.text_tertiary)
                    .child(language.text("This is not an established Mihomo connection. Only routes from /connections are marked as observed.", "这不是 Mihomo 已建立的连接。只有来自 /connections 的链路才能标为“已观察”。")),
            )
    }

    fn inspector_sheet_body(
        &self,
        theme: Theme,
        on_predict: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
    ) -> Div {
        div()
            .min_h_full()
            .bg(theme.surface_low)
            .px_4()
            .pb_4()
            .text_color(theme.text_primary)
            .child(div().mb_3().child(self.inspector_badge(theme)))
            .child(self.inspector_form(theme, on_predict))
            .child(div().mt_4().child(self.inspector_results(theme)))
    }

    fn inspector(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        div()
            .w(px(340.0))
            .h_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .bg(theme.surface_low)
            .border_l_1()
            .border_color(theme.outline_subtle)
            .child(
                div()
                    .p_4()
                    .border_b_1()
                    .border_color(theme.outline_subtle)
                    .child(self.inspector_title(theme))
                    .child(div().mt_2().child(self.inspector_form(
                        theme,
                        cx.listener(|this, _, _, cx| this.predict_route(cx)),
                    ))),
            )
            .child(
                div()
                    .id("inspector-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .p_4()
                    .child(self.inspector_results(theme)),
            )
    }

    fn status_bar(&self, theme: Theme) -> StatusBar {
        let language = self.language();
        let kernel_name = self.runtime.kind().display_name();
        let source = controller_status_label(&self.controller, kernel_name, language);
        let values = status_bar_values(&self.controller, language, theme);

        let left = div()
            .flex()
            .items_center()
            .gap(Space::Md.px())
            .min_w_0()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(Space::Sm.px())
                    .flex_none()
                    .child(div().size(px(8.0)).rounded_full().bg(values.dot))
                    .child(status_badge(source, values.tone, theme)),
            )
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_size(TextRole::Data.size())
                    .line_height(TextRole::Data.line_height())
                    .font_weight(TextRole::Data.weight())
                    .text_color(theme.text_secondary)
                    .child(values.endpoint),
            )
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .text_color(theme.text_tertiary)
                    .child(self.status.clone()),
            );
        let right = div()
            .flex()
            .items_center()
            .gap(Space::Md.px())
            .text_size(TextRole::Data.size())
            .line_height(TextRole::Data.line_height())
            .font_weight(TextRole::Data.weight())
            .text_color(theme.text_secondary)
            .child(values.download)
            .child(values.upload);

        StatusBar::new()
            .h(ControlSize::Icon.min_pointer_target())
            .flex_shrink_0()
            .py_0()
            .px(Space::Md.px())
            .border_t_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_chrome)
            .text_size(TextRole::Metadata.size())
            .line_height(TextRole::Metadata.line_height())
            .text_color(theme.text_secondary)
            .left(left)
            .right(right)
    }
}

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
            endpoint: language.text("No runtime data", "无运行数据").to_owned(),
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
                language.text("Total ", "累计"),
                format_bytes(*download_total)
            ),
            upload: format!(
                "{}↑ {}",
                language.text("Total ", "累计"),
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
            format!("{kernel_name} {}", language.text("disconnected", "未连接"))
        }
        ControllerState::Connecting { .. } => {
            format!("{kernel_name} {}", language.text("connecting", "连接中"))
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
            language.text("connection failed", "连接失败")
        ),
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
                language.text(
                    "TUN is disabled, but restoring the original DNS failed; recovery will be retried",
                    "TUN 已关闭，但恢复原 DNS 失败；Manis 将继续重试恢复"
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
        ProxyMode::Off => language.text("Off", "关闭代理"),
        ProxyMode::System => language.text("System proxy", "系统代理"),
        ProxyMode::Tun => language.text("TUN proxy", "TUN 代理"),
    }
}

fn compact_proxy_mode_label(
    language: Language,
    current: ProxyMode,
    pending: Option<ProxyMode>,
) -> &'static str {
    match pending {
        Some(ProxyMode::Tun) => language.text("Preparing TUN…", "准备 TUN…"),
        Some(ProxyMode::System) => language.text("Enabling…", "启用中…"),
        Some(ProxyMode::Off) => language.text("Turning off…", "关闭中…"),
        None => match current {
            ProxyMode::Off => language.text("Off", "关闭"),
            ProxyMode::System => language.text("System", "系统代理"),
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
            Self::Busy => language.text("switching", "切换中"),
            Self::ControllerNotConnected => language.text("connect first", "需先连接"),
            Self::KernelHasNoTun => language.text("kernel has no TUN", "当前内核无 TUN"),
            Self::FixtureReadOnly => language.text("test fixture is read-only", "测试快照只读"),
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
        RoutingMode::Direct => language.text("Direct", "直连"),
        RoutingMode::Global => language.text("Global", "全局"),
        RoutingMode::Rule => language.text("Rules", "规则"),
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

fn route_query_error_copy(error: RouteQueryError, language: Language) -> &'static str {
    match error {
        RouteQueryError::Domain(manis_core::RouteDomainError::Empty) => {
            language.text("Enter a domain to predict its route", "请输入要预测的域名")
        }
        RouteQueryError::Domain(manis_core::RouteDomainError::TooLong) => language.text(
            "The domain is longer than the 253-byte DNS limit",
            "域名超过 DNS 的 253 字节限制",
        ),
        RouteQueryError::Domain(manis_core::RouteDomainError::IpAddress) => language.text(
            "Enter a domain rather than an IP address",
            "这里请输入域名，而不是 IP 地址",
        ),
        RouteQueryError::Domain(manis_core::RouteDomainError::InvalidFormat) => language.text(
            "Enter a domain or domain:port, such as google.com:443",
            "请输入域名或域名:端口，例如 google.com:443",
        ),
        RouteQueryError::InvalidPort => language.text(
            "Enter a destination port between 1 and 65535",
            "请输入 1 到 65535 之间的目标端口",
        ),
    }
}

fn route_prediction_reason_copy(reason: RoutePredictionReason, language: Language) -> &'static str {
    match reason {
        RoutePredictionReason::RuleNeedsConnectionContext => language.text(
            "An earlier rule depends on information the domain alone cannot provide, such as process, port, rule-set, network type, or a resolved IP.",
            "更靠前的规则依赖进程、端口、规则集、网络类型或 DNS 解析后的 IP，仅凭域名无法判断。",
        ),
        RoutePredictionReason::NoMatchingRule => language.text(
            "No rule that can be determined from the domain matched this snapshot.",
            "当前快照中没有仅凭该域名即可确定命中的规则。",
        ),
    }
}

fn policy_kind_label(language: Language, kind: manis_core::PolicyGroupKind) -> &'static str {
    match kind {
        manis_core::PolicyGroupKind::Selector => language.text("Manual", "手动选择"),
        manis_core::PolicyGroupKind::UrlTest => language.text("Auto select", "自动选择"),
        manis_core::PolicyGroupKind::Fallback => language.text("Fallback", "故障转移"),
        manis_core::PolicyGroupKind::LoadBalance => language.text("Load balance", "负载均衡"),
        manis_core::PolicyGroupKind::Direct => language.text("Direct", "直连"),
    }
}

impl Default for ManisApp {
    fn default() -> Self {
        Self::new()
    }
}

impl Render for ManisApp {
    #[allow(clippy::too_many_lines)]
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let width = window.viewport_size().width.as_f32();
        self.workspace.resize(width);
        let size_class = self.workspace.size_class;
        let theme = self.theme();
        self.ensure_subscription_input(theme, window, cx);
        self.ensure_qx_rule_input(theme, window, cx);
        self.ensure_policy_group_inputs(theme, window, cx);
        self.ensure_runtime_search_inputs(theme, window, cx);
        self.ensure_route_domain_input(theme, window, cx);
        self.ensure_source_refresh_scheduler(cx);
        let compact = size_class == WindowSizeClass::Compact;
        let show_groups =
            !compact || self.workspace.compact_navigation == CompactNavigation::GroupList;
        let show_detail =
            !compact || self.workspace.compact_navigation == CompactNavigation::GroupDetail;
        if size_class == WindowSizeClass::Wide && self.inspector_open && window.has_active_sheet(cx)
        {
            self.inspector_open = false;
            window.close_sheet(cx);
        }
        let show_inspector = size_class == WindowSizeClass::Wide;
        let policies_active = self.primary_workspace == PrimaryWorkspace::Policies;
        let policy_editor_active = policies_active && self.managed_policy_draft.is_some();
        let has_policy_catalog = self.catalog.is_some();
        let nodes_active = self.primary_workspace == PrimaryWorkspace::Nodes;
        let routing_rules_active = self.primary_workspace == PrimaryWorkspace::RoutingRules;
        let activity_active = self.primary_workspace == PrimaryWorkspace::Activity;
        let logs_active = self.primary_workspace == PrimaryWorkspace::Logs;

        div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.surface_base)
            .text_color(theme.text_primary)
            .text_size(px(13.0))
            .child(self.chrome(theme, size_class, cx))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .overflow_hidden()
                    .flex()
                    .child(self.navigation(theme, size_class, cx))
                    .when(nodes_active, |main| {
                        main.child(self.node_workspace(theme, size_class, cx))
                    })
                    .when(routing_rules_active, |main| {
                        main.child(self.routing_rules_workspace(theme, size_class, window, cx))
                    })
                    .when(activity_active, |main| {
                        main.child(self.activity_workspace(theme, size_class, cx))
                    })
                    .when(logs_active, |main| {
                        main.child(self.logs_workspace(theme, size_class, cx))
                    })
                    .when(
                        self.primary_workspace == PrimaryWorkspace::Configuration,
                        |main| main.child(self.configuration_workspace(theme, size_class, cx)),
                    )
                    .when(policy_editor_active, |main| {
                        let draft = self
                            .managed_policy_draft
                            .as_ref()
                            .expect("policy editor state requires a draft");
                        main.child(self.managed_policy_editor_workspace(
                            draft,
                            compact,
                            self.language(),
                            theme,
                            cx,
                        ))
                    })
                    .when(
                        policies_active && !policy_editor_active && !has_policy_catalog,
                        |main| main.child(self.empty_policy_workspace(theme, cx)),
                    )
                    .when(
                        policies_active
                            && !policy_editor_active
                            && has_policy_catalog
                            && show_groups,
                        |main| {
                            main.child(
                                self.policy_list(
                                    theme,
                                    if compact {
                                        None
                                    } else if size_class == WindowSizeClass::Medium {
                                        Some(292.0)
                                    } else {
                                        Some(326.0)
                                    },
                                    cx,
                                )
                                .when(compact, Styled::flex_1),
                            )
                        },
                    )
                    .when(
                        policies_active
                            && !policy_editor_active
                            && has_policy_catalog
                            && show_detail,
                        |main| main.child(self.detail(theme, compact, cx)),
                    )
                    .when(routing_rules_active && show_inspector, |main| {
                        main.child(self.inspector(theme, cx))
                    }),
            )
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
        assert_eq!(PolicyDetailTab::Rules.index(), 1);
        assert_eq!(PolicyDetailTab::Settings.index(), 2);
        assert_eq!(PolicyDetailTab::from_index(0), PolicyDetailTab::Nodes);
        assert_eq!(PolicyDetailTab::from_index(1), PolicyDetailTab::Rules);
        assert_eq!(PolicyDetailTab::from_index(2), PolicyDetailTab::Settings);
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
            source: manis_profile::SecretUrl::parse_subscription(
                "https://subscription.example.invalid/client?name=NaiU_Net",
            )
            .expect("fixture subscription"),
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
            source: manis_profile::SecretUrl::parse_subscription(
                "https://subscription.example.invalid/client",
            )
            .expect("fixture subscription"),
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
        app.managed_policy_groups.push(
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

        app.node_selection_preferences.set_global(
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
            app.node_selection_preferences.policy_target("Manual Video"),
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

        assert_eq!(app.qx_rule_sources.len(), 1);
        assert_eq!(app.qx_rule_sources[0].rule_count, 1);
        assert_eq!(app.qx_rule_sources[0].target_policy.as_str(), "Proxy");
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
