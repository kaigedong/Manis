use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui::{
    Animation, AnimationExt as _, Context, Div, Entity, FontWeight, IntoElement, ParentElement,
    Render, Role, Stateful, Styled, Subscription, Toggled, Window, div, prelude::*, px,
};
use relay_core::{
    CompactNavigation, NodeGroupIcon, NodeGroupStrategy, NodeIdentity, NodePolicyGroup,
    NodeWorkspaceState, PolicyCatalog, PolicyGroup, PolicyNode, PolicyWorkspaceState,
    PrimaryWorkspace, ProxyId, ProxyMode, RoutingMode, WindowSizeClass,
};
use relay_mihomo::{Connection, ObservedRouteEvidence, RuntimeConfig};
use relay_profile::{QxRuleList, SecretUrl};

use crate::{
    demo,
    diagnostics::{UiEvent, trace_ui},
    mihomo::{
        self, ControllerRuntime, ControllerState, GeneratedProfileApply, KernelLogEntry,
        LiveRuntimeSession, LiveStreamStatus, LoadedProvider, LoadedSnapshot, StoredQxRuleSource,
        StoredSubscription, StoredVlessNode, SubscriptionPreviewError, SubscriptionStoreError,
    },
    rule_source::RuleDownloadError,
    subscription::{SourceKind, SubscriptionInputError, SubscriptionPreview},
    subscription_input::{SubscriptionInputChanged, SubscriptionTextInput},
    system_proxy::{ProxyPorts, SystemProxySession},
    theme::Theme,
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

enum SourceRuntimeApply {
    Applied(GeneratedProfileApply),
    Failed(String),
}

impl SourceRuntimeApply {
    fn from_result(result: Result<GeneratedProfileApply, mihomo::LoadError>) -> Self {
        match result {
            Ok(outcome) => Self::Applied(outcome),
            Err(error) => Self::Failed(error.to_string()),
        }
    }

    fn status_suffix(&self) -> String {
        match self {
            Self::Applied(GeneratedProfileApply::NotManaged) => {
                " · 当前为外部/已有配置，未改写其配置".to_owned()
            }
            Self::Applied(GeneratedProfileApply::Updated) => " · 已写入 Relay 托管配置".to_owned(),
            Self::Applied(GeneratedProfileApply::Restarted) => {
                " · Relay 托管内核已安全重载".to_owned()
            }
            Self::Failed(message) => format!(" · 持久化已完成，但托管配置应用失败：{message}"),
        }
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
        }
    }
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
    InvalidDocument,
    DownloadFailed(RuleDownloadError),
    StoreFailed(SubscriptionStoreError),
}

enum ImportQxRuleError {
    Download(RuleDownloadError),
    InvalidDocument,
    Store(SubscriptionStoreError),
}

fn source_kind(subscription: &SecretUrl) -> SourceKind {
    if subscription.is_https() {
        SourceKind::HttpsSubscription
    } else {
        SourceKind::HttpSubscription
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum NodeGroupMatcherKind {
    #[default]
    All,
    NameContains,
    Explicit,
}

#[derive(Clone, Debug)]
struct NodeGroupDraft {
    editing_id: Option<String>,
    icon: NodeGroupIcon,
    strategy: NodeGroupStrategy,
    test_interval_secs: u32,
    matcher_kind: NodeGroupMatcherKind,
    explicit_members: BTreeSet<NodeIdentity>,
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
enum NodeGroupRuntimeState {
    #[default]
    LocalOnly,
    Loading {
        generation: u64,
    },
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
    Failed {
        generation: u64,
    },
}

impl NodeGroupRuntimeState {
    fn complete_refresh(
        &mut self,
        generation: u64,
        current: Option<String>,
        candidates: BTreeSet<String>,
    ) -> bool {
        if !matches!(
            self,
            Self::Loading { generation: active }
                | Self::Selecting {
                    generation: active,
                    ..
                } if *active == generation
        ) {
            return false;
        }
        *self = Self::Ready {
            generation,
            current,
            candidates,
        };
        true
    }

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

    fn fail(&mut self, generation: u64) -> bool {
        if !matches!(
            self,
            Self::Loading { generation: active }
                | Self::Selecting {
                    generation: active,
                    ..
                } if *active == generation
        ) {
            return false;
        }
        *self = Self::Failed { generation };
        true
    }
}

pub struct RelayApp {
    primary_workspace: PrimaryWorkspace,
    node_workspace: NodeWorkspaceState,
    workspace: PolicyWorkspaceState,
    catalog: PolicyCatalog,
    runtime: ControllerRuntime,
    controller: ControllerState,
    observed_routes: Vec<ObservedRouteEvidence>,
    source_providers: Vec<LoadedProvider>,
    subscription_preview_providers: Vec<LoadedProvider>,
    subscription_preview_generation: u64,
    subscription_store_dir: Option<PathBuf>,
    imported_subscriptions: Vec<ImportedSubscription>,
    saved_vless_nodes: Vec<StoredVlessNode>,
    qx_rule_sources: Vec<StoredQxRuleSource>,
    qx_rule_feedback: QxRuleImportFeedback,
    qx_rule_target_policy: String,
    qx_rule_import_generation: u64,
    node_policy_groups: Vec<NodePolicyGroup>,
    node_group_draft: Option<NodeGroupDraft>,
    group_benchmarks: BTreeMap<String, GroupBenchmarkState>,
    group_benchmark_generation: u64,
    group_benchmark_active_generation: Option<u64>,
    selected_node_group_id: Option<String>,
    node_group_runtime_states: BTreeMap<String, NodeGroupRuntimeState>,
    node_group_runtime_generation: u64,
    source_store_error: Option<SubscriptionStoreError>,
    proxy_mode: ProxyMode,
    proxy_mode_busy: bool,
    routing_mode: RoutingMode,
    routing_mode_busy: Option<RoutingMode>,
    global_selection_busy: Option<String>,
    proxy_runtime: RuntimeConfig,
    system_proxy: Arc<Mutex<SystemProxySession>>,
    active_connections: Vec<Connection>,
    live_runtime: Option<LiveRuntimeSession>,
    live_generation: u64,
    live_status: LiveStreamStatus,
    kernel_logs: VecDeque<KernelLogEntry>,
    dropped_kernel_logs: u64,
    inspector_open: bool,
    dark: bool,
    status: String,
    subscription_input: Option<Entity<SubscriptionTextInput>>,
    subscription_feedback: SubscriptionFeedback,
    subscription_input_events: Option<Subscription>,
    qx_rule_input: Option<Entity<SubscriptionTextInput>>,
    qx_rule_input_events: Option<Subscription>,
    node_group_name_input: Option<Entity<SubscriptionTextInput>>,
    node_group_filter_input: Option<Entity<SubscriptionTextInput>>,
}

struct StoredWorkspace {
    imported_subscriptions: Vec<ImportedSubscription>,
    saved_vless_nodes: Vec<StoredVlessNode>,
    qx_rule_sources: Vec<StoredQxRuleSource>,
    collapsed_groups: Vec<String>,
    node_policy_groups: Vec<NodePolicyGroup>,
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
                collapsed_groups: Vec::new(),
                node_policy_groups: Vec::new(),
                routing_mode: RoutingMode::Rule,
                error: None,
            };
        };
        let subscriptions = mihomo::load_subscription_sources_in(directory);
        let nodes = mihomo::load_vless_sources_in(directory);
        let qx_rule_sources = mihomo::load_qx_rule_sources_in(directory);
        let collapsed = mihomo::load_collapsed_groups_in(directory);
        let policy_groups = mihomo::load_node_policy_groups_in(directory);
        let routing_mode = mihomo::load_routing_mode_in(directory);
        let error = [
            subscriptions.is_err(),
            nodes.is_err(),
            qx_rule_sources.is_err(),
            collapsed.is_err(),
            policy_groups.is_err(),
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
            collapsed_groups: collapsed.unwrap_or_default(),
            node_policy_groups: policy_groups.unwrap_or_default(),
            routing_mode: routing_mode.unwrap_or_default(),
            error,
        }
    }
}

impl RelayApp {
    #[must_use]
    pub fn new() -> Self {
        let store = mihomo::imported_subscription_store_dir();
        Self::with_runtime_and_store(mihomo::configured_runtime(), store.ok())
    }

    #[must_use]
    pub fn with_controller(endpoint: impl Into<String>) -> Self {
        Self::with_runtime_and_store(
            ControllerRuntime::External {
                endpoint: endpoint.into(),
            },
            None,
        )
    }

    /// Creates a deterministic app instance backed by an explicit subscription store.
    ///
    /// This is primarily useful for native visual tests and embedders that manage their own
    /// application-data root.
    #[must_use]
    pub fn with_controller_and_subscription_store(
        endpoint: impl Into<String>,
        subscription_store_dir: PathBuf,
    ) -> Self {
        Self::with_runtime_and_store(
            ControllerRuntime::External {
                endpoint: endpoint.into(),
            },
            Some(subscription_store_dir),
        )
    }

    fn with_runtime_and_store(
        runtime: ControllerRuntime,
        subscription_store_dir: Option<PathBuf>,
    ) -> Self {
        let mut status = runtime.initial_status();
        let StoredWorkspace {
            imported_subscriptions,
            saved_vless_nodes,
            qx_rule_sources,
            collapsed_groups,
            node_policy_groups,
            routing_mode,
            error: source_store_error,
        } = StoredWorkspace::load(subscription_store_dir.as_ref());
        if let Some(directory) = subscription_store_dir.as_ref()
            && (!imported_subscriptions.is_empty()
                || !saved_vless_nodes.is_empty()
                || !qx_rule_sources.is_empty()
                || !node_policy_groups.is_empty()
                || routing_mode != RoutingMode::Rule)
        {
            status = match runtime.apply_saved_sources(directory) {
                Ok(GeneratedProfileApply::Updated) => "已将保存来源写入 Relay 托管配置".to_owned(),
                Ok(GeneratedProfileApply::Restarted) => {
                    "已将保存来源应用到 Relay 托管内核".to_owned()
                }
                Ok(GeneratedProfileApply::NotManaged) => status,
                Err(error) => format!("保存来源已载入，但托管配置未应用：{error}"),
            };
        }
        let mut node_workspace = NodeWorkspaceState::default();
        node_workspace.replace_collapsed_groups(collapsed_groups.iter().map(String::as_str));
        Self {
            primary_workspace: PrimaryWorkspace::default(),
            node_workspace,
            workspace: PolicyWorkspaceState::demo(),
            catalog: demo::catalog(),
            runtime,
            controller: ControllerState::Demo,
            observed_routes: Vec::new(),
            source_providers: Vec::new(),
            subscription_preview_providers: Vec::new(),
            subscription_preview_generation: 0,
            subscription_store_dir,
            imported_subscriptions,
            saved_vless_nodes,
            qx_rule_sources,
            qx_rule_feedback: QxRuleImportFeedback::Idle,
            qx_rule_target_policy: "Proxy".to_owned(),
            qx_rule_import_generation: 0,
            node_policy_groups,
            node_group_draft: None,
            group_benchmarks: BTreeMap::new(),
            group_benchmark_generation: 0,
            group_benchmark_active_generation: None,
            selected_node_group_id: None,
            node_group_runtime_states: BTreeMap::new(),
            node_group_runtime_generation: 0,
            source_store_error,
            proxy_mode: ProxyMode::Off,
            proxy_mode_busy: false,
            routing_mode,
            routing_mode_busy: None,
            global_selection_busy: None,
            proxy_runtime: RuntimeConfig::default(),
            system_proxy: Arc::new(Mutex::new(SystemProxySession::default())),
            active_connections: Vec::new(),
            live_runtime: None,
            live_generation: 0,
            live_status: LiveStreamStatus::default(),
            kernel_logs: VecDeque::with_capacity(500),
            dropped_kernel_logs: 0,
            inspector_open: false,
            dark: false,
            status,
            subscription_input: None,
            subscription_feedback: SubscriptionFeedback::Idle,
            subscription_input_events: None,
            qx_rule_input: None,
            qx_rule_input_events: None,
            node_group_name_input: None,
            node_group_filter_input: None,
        }
    }

    fn ensure_subscription_input(&mut self, theme: Theme, cx: &mut Context<Self>) {
        if let Some(input) = self.subscription_input.as_ref() {
            input.update(cx, |input, cx| input.set_theme(theme, self.dark, cx));
            return;
        }

        let input = cx.new(|cx| SubscriptionTextInput::new(theme, self.dark, cx));
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

    fn ensure_qx_rule_input(&mut self, theme: Theme, cx: &mut Context<Self>) {
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

    fn ensure_node_group_inputs(&mut self, theme: Theme, cx: &mut Context<Self>) {
        for input in [
            self.node_group_name_input.as_ref(),
            self.node_group_filter_input.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            input.update(cx, |input, cx| input.set_theme(theme, self.dark, cx));
        }
        if self.node_group_name_input.is_none() {
            self.node_group_name_input = Some(cx.new(|cx| {
                SubscriptionTextInput::new_field(
                    "node-group-name-input",
                    "例如：香港自动优选",
                    96,
                    theme,
                    self.dark,
                    cx,
                )
            }));
        }
        if self.node_group_filter_input.is_none() {
            self.node_group_filter_input = Some(cx.new(|cx| {
                SubscriptionTextInput::new_field(
                    "node-group-filter-input",
                    "例如：Hong Kong",
                    256,
                    theme,
                    self.dark,
                    cx,
                )
            }));
        }
    }

    fn import_remote_subscription(
        &mut self,
        input: String,
        kind: SourceKind,
        cx: &mut Context<Self>,
    ) {
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            self.subscription_feedback =
                SubscriptionFeedback::StoreFailed(SubscriptionStoreError::DataDirectoryUnavailable);
            "无法确定订阅保存位置".clone_into(&mut self.status);
            trace_ui(UiEvent::SourceImportFailed);
            cx.notify();
            return;
        };
        self.subscription_preview_generation = self.subscription_preview_generation.wrapping_add(1);
        let generation = self.subscription_preview_generation;
        self.subscription_feedback = SubscriptionFeedback::Importing(kind);
        "正在验证节点并导入订阅".clone_into(&mut self.status);
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
                    let subscription = mihomo::save_subscription_source_in(&store_dir, &input)
                        .map_err(ImportSubscriptionError::Store)?;
                    let apply = SourceRuntimeApply::from_result(
                        runtime.apply_saved_sources(&store_dir),
                    );
                    Ok::<_, ImportSubscriptionError>((subscription, providers, apply))
                })
                .await;
            this.update(cx, |this, cx| {
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
                        } else {
                            this.imported_subscriptions.push(ImportedSubscription {
                                id: stored_id,
                                source: subscription.source,
                                state: ImportedSubscriptionState::Ready(kind),
                                providers: providers.clone(),
                                generation,
                            });
                        }
                        this.subscription_preview_providers = providers;
                        this.subscription_feedback = SubscriptionFeedback::Idle;
                        if let Some(input) = this.subscription_input.as_ref() {
                            input.update(cx, SubscriptionTextInput::clear_without_event);
                        }
                        this.status = format!(
                            "订阅已导入 · 共 {} 个订阅组 · {provider_count} 个来源 · {node_count} 个节点{}",
                            this.imported_subscriptions.len(),
                            apply.status_suffix()
                        );
                        trace_ui(UiEvent::SourceImportSucceeded);
                    }
                    Err(ImportSubscriptionError::Preview(error)) => {
                        this.subscription_feedback = SubscriptionFeedback::PreviewFailed(error);
                        this.status = format!("订阅导入失败：{error}");
                        trace_ui(UiEvent::SourceImportFailed);
                    }
                    Err(ImportSubscriptionError::Store(error)) => {
                        this.subscription_feedback = SubscriptionFeedback::StoreFailed(error);
                        this.status = format!("订阅保存失败：{error}");
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
            self.restore_imported_subscription(id, cx);
        }
    }

    fn restore_imported_subscription(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(subscription) = self
            .imported_subscriptions
            .iter_mut()
            .find(|subscription| subscription.id == id)
        else {
            return;
        };
        let ImportedSubscriptionState::Pending(kind) = subscription.state else {
            return;
        };
        let source = subscription.source.clone();
        self.subscription_preview_generation = self.subscription_preview_generation.wrapping_add(1);
        let generation = self.subscription_preview_generation;
        subscription.generation = generation;
        subscription.state = ImportedSubscriptionState::Refreshing(kind);
        "正在恢复已导入订阅的节点组".clone_into(&mut self.status);
        trace_ui(UiEvent::SourceRestoreStarted);

        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move { mihomo::preview_imported_subscription(source) })
                .await;
            this.update(cx, |this, cx| {
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
                    Ok(providers) => {
                        let node_count: usize =
                            providers.iter().map(|provider| provider.nodes.len()).sum();
                        subscription.providers = providers;
                        subscription.state = ImportedSubscriptionState::Ready(kind);
                        this.status = format!("已恢复订阅节点组 · {node_count} 个节点");
                        trace_ui(UiEvent::SourceRestoreSucceeded);
                    }
                    Err(error) => {
                        subscription.state = ImportedSubscriptionState::Unavailable(kind, error);
                        this.status = format!("订阅节点组刷新失败：{error}");
                        trace_ui(UiEvent::SourceRestoreFailed);
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn remove_imported_subscription(&mut self, id: String, cx: &mut Context<Self>) {
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
        "正在移除已导入订阅".clone_into(&mut self.status);
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
                        this.status = format!("已移除导入订阅{}", apply.status_suffix());
                        trace_ui(UiEvent::SourceRemoveSucceeded);
                    }
                    Err(error) => {
                        this.imported_subscriptions[index].state =
                            ImportedSubscriptionState::StoreError(error);
                        this.status = format!("移除订阅失败：{error}");
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
            "无法保存节点分组状态".clone_into(&mut self.status);
        }
    }

    fn theme(&self) -> Theme {
        if self.dark {
            Theme::dark()
        } else {
            Theme::light()
        }
    }

    fn selected_policy(&self) -> &PolicyGroup {
        self.catalog.select(self.workspace.selected_group.as_ref())
    }

    fn source_group_benchmark_key(id: &str) -> String {
        format!("source:{id}")
    }

    fn user_group_benchmark_key(id: &str) -> String {
        format!("user:{id}")
    }

    fn policy_group_benchmark_key(id: &relay_core::PolicyGroupId) -> String {
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
            .text_size(px(11.0));
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
        div().relative().size(px(14.0)).with_animation(
            id,
            Animation::new(Duration::from_millis(720)).repeat(),
            move |spinner, delta| {
                let active = Self::benchmark_latency_spinner_frame(delta);
                (0..8).fold(spinner, |spinner, index| {
                    spinner.child(Self::benchmark_latency_dot(
                        index,
                        active,
                        theme.action_primary,
                    ))
                })
            },
        )
    }

    fn benchmark_latency_spinner_frame(delta: f32) -> usize {
        if delta < 0.125 {
            0
        } else if delta < 0.25 {
            1
        } else if delta < 0.375 {
            2
        } else if delta < 0.5 {
            3
        } else if delta < 0.625 {
            4
        } else if delta < 0.75 {
            5
        } else if delta < 0.875 {
            6
        } else {
            7
        }
    }

    fn benchmark_latency_dot(index: usize, active: usize, color: gpui::Rgba) -> Div {
        const POSITIONS: [(f32, f32); 8] = [
            (5.5, 0.0),
            (9.5, 1.5),
            (11.0, 5.5),
            (9.5, 9.5),
            (5.5, 11.0),
            (1.5, 9.5),
            (0.0, 5.5),
            (1.5, 1.5),
        ];
        const OPACITY: [f32; 8] = [1.0, 0.82, 0.68, 0.54, 0.42, 0.32, 0.24, 0.18];
        let distance = (index + 8 - active) % 8;
        let (left, top) = POSITIONS[index];
        div()
            .absolute()
            .left(px(left))
            .top(px(top))
            .size(px(3.0))
            .rounded_full()
            .bg(color.opacity(OPACITY[distance]))
    }

    fn policy_benchmark_status(
        kind: relay_core::PolicyGroupKind,
        current: Option<&str>,
        summary: GroupBenchmarkSummary,
    ) -> String {
        match (kind.is_automatic(), current) {
            (true, Some(current)) => format!(
                "策略组测速完成：{}/{} 成功 · Mihomo 当前优选 {current}",
                summary.succeeded, summary.total
            ),
            (true, None) => format!(
                "策略组测速完成：{}/{} 成功 · 该策略没有单一固定出口",
                summary.succeeded, summary.total
            ),
            (false, _) => format!(
                "策略组测速完成：{}/{} 个候选项成功",
                summary.succeeded, summary.total
            ),
        }
    }

    fn policy_group_benchmark_feedback(
        state: &GroupBenchmarkState,
        total: usize,
        theme: Theme,
    ) -> Option<Div> {
        let (label, color) = match state {
            GroupBenchmarkState::Idle => return None,
            GroupBenchmarkState::Running { results, .. } => (
                format!("正在测速 · {}/{} 个候选项已返回", results.len(), total),
                theme.action_primary,
            ),
            GroupBenchmarkState::Complete { summary, .. } => {
                let latency = match (summary.minimum_ms, summary.average_ms) {
                    (Some(minimum), Some(average)) => {
                        format!(" · 最低 {minimum} ms · 平均 {average} ms")
                    }
                    _ => String::new(),
                };
                (
                    format!(
                        "测速完成 · {}/{} 个候选项成功{latency}",
                        summary.succeeded, summary.total
                    ),
                    theme.status_success,
                )
            }
            GroupBenchmarkState::Failed { .. } => (
                "测速失败 · 当前策略组未返回延迟，请检查 Mihomo 连接后重试".to_owned(),
                theme.route_trace,
            ),
        };
        Some(
            div()
                .mt_2()
                .text_size(px(11.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(color)
                .child(label),
        )
    }

    fn selected_node(&self) -> PolicyNode {
        let policy = self.selected_policy();
        let selected = if policy.kind.allows_manual_selection() {
            self.workspace
                .selected_node
                .as_ref()
                .and_then(|selected| policy.nodes.iter().find(|node| node.id == *selected))
        } else {
            policy.nodes.iter().find(|node| node.name == policy.target)
        };
        selected
            .or_else(|| policy.nodes.first())
            .cloned()
            .unwrap_or_else(|| PolicyNode {
                id: ProxyId::new("unavailable"),
                name: "暂无可用节点".to_owned(),
                kind: relay_core::PolicyCandidateKind::Node,
                provider: None,
                detail: "Mihomo 未返回组内节点".to_owned(),
                latency_ms: None,
                alive: None,
            })
    }

    fn connect_mihomo(&mut self, cx: &mut Context<Self>) {
        if matches!(self.controller, ControllerState::Connecting { .. }) {
            return;
        }

        self.live_generation = self.live_generation.wrapping_add(1);
        self.live_runtime = None;
        self.live_status = LiveStreamStatus {
            activity: "正在重新连接".to_owned(),
            logs: "正在重新连接".to_owned(),
        };

        let endpoint = self.runtime.endpoint_label();
        let runtime = self.runtime.clone();
        self.controller = ControllerState::Connecting {
            endpoint: endpoint.clone(),
        };
        self.status = format!("正在从 {endpoint} 读取 Mihomo 数据");
        trace_ui(UiEvent::MihomoConnectStarted);

        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor.spawn(async move { runtime.connect() }).await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(result) => {
                        let controller_endpoint = result.controller_endpoint;
                        this.apply_mihomo_snapshot(result.endpoint, result.snapshot);
                        this.start_live_runtime(&controller_endpoint, cx);
                    }
                    Err(error) => {
                        trace_ui(UiEvent::MihomoConnectFailed);
                        let endpoint = this
                            .controller
                            .endpoint()
                            .unwrap_or("本地控制器")
                            .to_owned();
                        let message = error.to_string();
                        this.controller = ControllerState::Failed {
                            endpoint,
                            message: message.clone(),
                        };
                        this.status = format!("Mihomo 连接失败：{message}");
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn apply_mihomo_snapshot(&mut self, endpoint: String, snapshot: LoadedSnapshot) {
        trace_ui(UiEvent::MihomoConnectSucceeded);
        let primary = snapshot.catalog.select(None);
        let group = primary.id.clone();
        let selected_node = primary
            .nodes
            .iter()
            .find(|node| node.name == primary.target)
            .or_else(|| primary.nodes.first())
            .map(|node| node.id.clone());
        self.workspace
            .replace_source_selection(group, selected_node);
        self.catalog = snapshot.catalog;
        self.source_providers = snapshot.providers;
        self.observed_routes = snapshot.observed_routes;
        self.active_connections = snapshot.connections;
        self.proxy_mode = if snapshot.runtime.tun.enable {
            ProxyMode::Tun
        } else {
            ProxyMode::Off
        };
        self.routing_mode = snapshot.runtime.mode;
        self.proxy_runtime = snapshot.runtime;
        self.status = format!(
            "已读取 {} 个策略组 · {} 条活动连接",
            self.catalog.iter().count(),
            snapshot.active_connections
        );
        self.controller = ControllerState::Connected {
            endpoint,
            version: snapshot.version,
            active_connections: snapshot.active_connections,
            download_total: snapshot.download_total,
            upload_total: snapshot.upload_total,
        };
    }

    fn start_policy_group_benchmark(
        &mut self,
        id: &relay_core::PolicyGroupId,
        cx: &mut Context<Self>,
    ) {
        if !matches!(self.controller, ControllerState::Connected { .. }) {
            "请先连接 Mihomo，再测试真实策略组".clone_into(&mut self.status);
            cx.notify();
            return;
        }
        let Some(group) = self.catalog.iter().find(|group| group.id == *id).cloned() else {
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
            "当前策略组没有可测速候选项".clone_into(&mut self.status);
            cx.notify();
            return;
        }
        let Some(generation) = self.begin_group_benchmark(key.clone()) else {
            "已有分组正在测速，请等待完成后再试".clone_into(&mut self.status);
            cx.notify();
            return;
        };
        self.status = format!(
            "正在测试策略组“{}”的 {} 个候选项",
            group.name,
            candidate_names.len()
        );
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
                    if group_kind == relay_core::PolicyGroupKind::Direct {
                        runtime
                            .test_node_group_delay(&group_name, &candidate_names)
                            .map(|delays| mihomo::PolicyGroupBenchmarkSnapshot {
                                delays,
                                current: None,
                            })
                    } else {
                        runtime.test_policy_group_delay(&group_name)
                    }
                })
                .await;
            this.update(cx, |this, cx| {
                if this.group_benchmark_active_generation != Some(generation) {
                    return;
                }
                this.group_benchmark_active_generation = None;
                let (delays, current) = match result {
                    Ok(snapshot) => (Some(snapshot.delays), snapshot.current),
                    Err(_error) => (None, None),
                };
                if let Some(delays) = delays.as_ref() {
                    this.catalog
                        .apply_group_benchmark(&group_id, current.as_deref(), delays);
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
                        this.status =
                            Self::policy_benchmark_status(group_kind, current.as_deref(), *summary);
                    }
                    GroupBenchmarkState::Failed { .. } => {
                        trace_ui(UiEvent::GroupBenchmarkFailed);
                        "策略组测速失败，请检查 Mihomo 连接与网络后重试"
                            .clone_into(&mut this.status);
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

    fn start_live_runtime(&mut self, endpoint: &str, cx: &mut Context<Self>) {
        self.live_generation = self.live_generation.wrapping_add(1);
        let generation = self.live_generation;
        self.live_runtime = match LiveRuntimeSession::start(endpoint) {
            Ok(session) => Some(session),
            Err(error) => {
                self.live_status = LiveStreamStatus {
                    activity: format!("无法启动：{error}"),
                    logs: format!("无法启动：{error}"),
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

    #[allow(clippy::too_many_lines)]
    fn apply_proxy_mode(&mut self, requested: ProxyMode, cx: &mut Context<Self>) {
        if self.proxy_mode_busy || requested == self.proxy_mode {
            return;
        }
        if !matches!(self.controller, ControllerState::Connected { .. }) {
            trace_ui(UiEvent::ProxyModeFailed);
            "请先连接 Mihomo，再切换代理模式".clone_into(&mut self.status);
            cx.notify();
            return;
        }
        if requested == ProxyMode::Tun && matches!(self.runtime, ControllerRuntime::External { .. })
        {
            trace_ui(UiEvent::ProxyModeFailed);
            "外部控制器保持只读；请使用 Relay 托管内核启用 TUN 模式".clone_into(&mut self.status);
            cx.notify();
            return;
        }

        let runtime = self.runtime.clone();
        let system_proxy = self.system_proxy.clone();
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
        self.proxy_mode_busy = true;
        self.status = format!("正在切换到{}…", requested.label());

        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    let mut system = system_proxy
                        .lock()
                        .map_err(|_| "系统代理状态锁已损坏".to_owned())?;
                    match (previous, requested) {
                        (ProxyMode::System, ProxyMode::Off) => {
                            system.disable().map_err(|error| error.to_string())?;
                        }
                        (ProxyMode::Tun, ProxyMode::Off) => {
                            runtime
                                .set_tun_enabled(false)
                                .map_err(|error| error.to_string())?;
                        }
                        (ProxyMode::Off, ProxyMode::System) => {
                            system.enable(ports).map_err(|error| error.to_string())?;
                        }
                        (ProxyMode::Tun, ProxyMode::System) => {
                            runtime
                                .set_tun_enabled(false)
                                .map_err(|error| error.to_string())?;
                            if let Err(error) = system.enable(ports) {
                                let rollback = runtime.set_tun_enabled(true);
                                return Err(match rollback {
                                    Ok(()) => error.to_string(),
                                    Err(rollback) => {
                                        format!("{error}；恢复原 TUN 模式也失败：{rollback}")
                                    }
                                });
                            }
                        }
                        (ProxyMode::Off, ProxyMode::Tun) => {
                            runtime
                                .set_tun_enabled(true)
                                .map_err(|error| error.to_string())?;
                        }
                        (ProxyMode::System, ProxyMode::Tun) => {
                            system.disable().map_err(|error| error.to_string())?;
                            if let Err(error) = runtime.set_tun_enabled(true) {
                                let rollback = system.enable(ports);
                                return Err(match rollback {
                                    Ok(()) => error.to_string(),
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
                this.proxy_mode_busy = false;
                match result {
                    Ok(()) => {
                        this.proxy_mode = requested;
                        match requested {
                            ProxyMode::Off => trace_ui(UiEvent::SystemProxyDisabled),
                            ProxyMode::System => trace_ui(UiEvent::SystemProxyEnabled),
                            ProxyMode::Tun => trace_ui(UiEvent::TunProxyEnabled),
                        }
                        this.status = format!("{}已生效", requested.label());
                    }
                    Err(message) => {
                        trace_ui(UiEvent::ProxyModeFailed);
                        this.status = format!("代理模式切换失败：{message}");
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn apply_routing_mode(&mut self, requested: RoutingMode, cx: &mut Context<Self>) {
        if self.routing_mode_busy.is_some() || requested == self.routing_mode {
            return;
        }
        if !matches!(self.controller, ControllerState::Connected { .. }) {
            trace_ui(UiEvent::RoutingModeFailed);
            "请先连接 Mihomo，再切换路由模式".clone_into(&mut self.status);
            cx.notify();
            return;
        }
        if matches!(self.runtime, ControllerRuntime::External { .. }) {
            trace_ui(UiEvent::RoutingModeFailed);
            "外部控制器保持只读；请使用 Relay 托管内核切换路由模式".clone_into(&mut self.status);
            cx.notify();
            return;
        }

        self.routing_mode_busy = Some(requested);
        self.status = format!("正在切换到{}…", requested.label());
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
                this.routing_mode_busy = None;
                match result {
                    Ok(persistence) => {
                        this.routing_mode = requested;
                        this.proxy_runtime.mode = requested;
                        trace_ui(UiEvent::RoutingModeChanged);
                        this.status = match requested {
                            RoutingMode::Global => this.global_target().map_or_else(
                                || "全局模式已生效；请在节点页选择全局出口".to_owned(),
                                |target| format!("全局模式已生效 · 当前出口 {target}"),
                            ),
                            _ => format!("{}已生效", requested.label()),
                        };
                        if persistence.is_err() {
                            this.status.push_str(" · 但未能保存重启偏好");
                        }
                    }
                    Err(error) => {
                        trace_ui(UiEvent::RoutingModeFailed);
                        this.status = format!("路由模式切换失败：{error}");
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn select_global_node(&mut self, selected_name: String, cx: &mut Context<Self>) {
        if self.global_selection_busy.is_some() {
            return;
        }
        if !matches!(self.controller, ControllerState::Connected { .. }) {
            trace_ui(UiEvent::GlobalNodeSelectionFailed);
            "请先连接 Mihomo，再选择全局节点".clone_into(&mut self.status);
            cx.notify();
            return;
        }
        if matches!(self.runtime, ControllerRuntime::External { .. }) {
            trace_ui(UiEvent::GlobalNodeSelectionFailed);
            "外部控制器保持只读；请使用 Relay 托管内核选择全局节点".clone_into(&mut self.status);
            cx.notify();
            return;
        }
        let is_candidate = self
            .catalog
            .iter()
            .find(|group| group.name.eq_ignore_ascii_case("GLOBAL"))
            .is_some_and(|group| group.nodes.iter().any(|node| node.name == selected_name));
        if !is_candidate {
            trace_ui(UiEvent::GlobalNodeSelectionFailed);
            "这个节点不在 Mihomo 的 GLOBAL 候选项中".clone_into(&mut self.status);
            cx.notify();
            return;
        }

        self.global_selection_busy = Some(selected_name.clone());
        self.status = format!("正在选择全局节点“{selected_name}”…");
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
                this.global_selection_busy = None;
                match result {
                    Ok(snapshot) => {
                        let current = snapshot.current.as_deref().unwrap_or(&selected_name);
                        let _ = this.catalog.apply_selector_target("GLOBAL", current);
                        trace_ui(UiEvent::GlobalNodeSelected);
                        this.status = if this.routing_mode == RoutingMode::Global {
                            format!("全局出口已切换到“{current}”")
                        } else {
                            format!("已保存全局出口“{current}”；切换到全局模式后生效")
                        };
                    }
                    Err(error) => {
                        trace_ui(UiEvent::GlobalNodeSelectionFailed);
                        this.status = format!("全局节点切换失败：{error}");
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn global_target(&self) -> Option<&str> {
        self.catalog
            .iter()
            .find(|group| group.name.eq_ignore_ascii_case("GLOBAL"))
            .map(|group| group.target.as_str())
    }

    fn is_global_candidate(&self, name: &str) -> bool {
        self.catalog
            .iter()
            .find(|group| group.name.eq_ignore_ascii_case("GLOBAL"))
            .is_some_and(|group| group.nodes.iter().any(|node| node.name == name))
    }

    #[allow(clippy::too_many_lines)]
    fn chrome(&self, theme: Theme, size_class: WindowSizeClass, cx: &mut Context<Self>) -> Div {
        let compact = size_class == WindowSizeClass::Compact;
        let theme_label = if self.dark { "浅色" } else { "深色" };

        div()
            .h(px(48.0))
            .flex_shrink_0()
            .flex()
            .items_center()
            .px_4()
            .gap_3()
            .bg(theme.surface_chrome)
            .border_b_1()
            .border_color(theme.outline_subtle)
            .child(
                div()
                    .w(if compact { px(86.0) } else { px(220.0) })
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap(if compact { px(8.0) } else { px(12.0) })
                    .child(
                        div()
                            .w(if compact { px(14.0) } else { px(20.0) })
                            .h(px(3.0))
                            .bg(theme.route_trace),
                    )
                    .child(
                        div()
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.text_primary)
                            .child("Relay"),
                    )
                    .when(!compact, |brand| {
                        brand.child(
                            div()
                                .text_size(px(10.0))
                                .text_color(theme.text_tertiary)
                                .child("PROTOTYPE"),
                        )
                    }),
            )
            .when(!compact, |chrome| {
                chrome.child(
                    div()
                        .h(px(34.0))
                        .max_w(px(520.0))
                        .flex_1()
                        .flex()
                        .items_center()
                        .px_3()
                        .rounded_md()
                        .border_1()
                        .border_color(theme.outline_subtle)
                        .bg(theme.surface_high)
                        .text_color(theme.text_tertiary)
                        .child("搜索策略、规则、连接     ⌘K"),
                )
            })
            .child(div().flex_1())
            .child(
                div()
                    .id("theme-toggle")
                    .role(Role::Button)
                    .tab_stop(true)
                    .focusable()
                    .cursor_pointer()
                    .h(px(34.0))
                    .px_3()
                    .rounded_md()
                    .border_1()
                    .border_color(theme.outline_subtle)
                    .bg(theme.surface_high)
                    .flex()
                    .items_center()
                    .child(theme_label)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.dark = !this.dark;
                        if this.dark {
                            trace_ui(UiEvent::ThemeDarkSelected);
                            "已切换到深色主题"
                        } else {
                            trace_ui(UiEvent::ThemeLightSelected);
                            "已切换到浅色主题"
                        }
                        .clone_into(&mut this.status);
                        cx.notify();
                    })),
            )
            .child(self.proxy_control(theme, size_class != WindowSizeClass::Wide, cx))
            .child(self.routing_control(theme, size_class != WindowSizeClass::Wide, cx))
    }

    #[allow(clippy::too_many_lines)]
    fn proxy_control(&self, theme: Theme, compact: bool, cx: &mut Context<Self>) -> Stateful<Div> {
        if compact {
            let next = self.proxy_mode.next();
            return div()
                .id("proxy-mode-cycle")
                .role(Role::Button)
                .aria_label("切换代理模式")
                .tab_stop(true)
                .focusable()
                .cursor_pointer()
                .h(px(34.0))
                .px_3()
                .rounded_md()
                .border_1()
                .border_color(theme.outline_subtle)
                .bg(theme.surface_high)
                .text_color(theme.text_primary)
                .flex()
                .items_center()
                .gap_2()
                .child(if self.proxy_mode_busy {
                    "接入 · 切换中…"
                } else {
                    match self.proxy_mode {
                        ProxyMode::Off => "接入 · 关闭",
                        ProxyMode::System => "接入 · 系统",
                        ProxyMode::Tun => "接入 · TUN",
                    }
                })
                .when(!self.proxy_mode_busy, |control| {
                    control.child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme.text_tertiary)
                            .child("↻"),
                    )
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.apply_proxy_mode(next, cx);
                }));
        }

        let mut control = div()
            .id("proxy-modes")
            .h(px(34.0))
            .p(px(2.0))
            .rounded_md()
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_high)
            .flex()
            .items_center()
            .child(
                div()
                    .px_2()
                    .text_size(px(10.0))
                    .text_color(theme.text_tertiary)
                    .child("接入"),
            );
        for mode in [ProxyMode::Off, ProxyMode::System, ProxyMode::Tun] {
            let selected = mode == self.proxy_mode;
            control = control.child(
                div()
                    .id(format!("proxy-mode-{mode:?}"))
                    .role(Role::Button)
                    .aria_label(mode.label())
                    .aria_toggled(if selected {
                        Toggled::True
                    } else {
                        Toggled::False
                    })
                    .tab_stop(true)
                    .focusable()
                    .cursor_pointer()
                    .h_full()
                    .px_3()
                    .rounded_sm()
                    .flex()
                    .items_center()
                    .text_size(px(11.0))
                    .font_weight(if selected {
                        FontWeight::SEMIBOLD
                    } else {
                        FontWeight::NORMAL
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
                    .child(if self.proxy_mode_busy && selected {
                        "切换中…"
                    } else {
                        mode.label()
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.apply_proxy_mode(mode, cx);
                    })),
            );
        }
        control
    }

    #[allow(clippy::too_many_lines)]
    fn routing_control(
        &self,
        theme: Theme,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        if compact {
            let next = match self.routing_mode {
                RoutingMode::Direct => RoutingMode::Global,
                RoutingMode::Global => RoutingMode::Rule,
                RoutingMode::Rule => RoutingMode::Direct,
            };
            return div()
                .id("routing-mode-cycle")
                .role(Role::Button)
                .aria_label("切换路由模式")
                .tab_stop(true)
                .focusable()
                .cursor_pointer()
                .h(px(34.0))
                .px_3()
                .rounded_md()
                .border_1()
                .border_color(theme.outline_subtle)
                .bg(theme.surface_high)
                .text_color(theme.text_primary)
                .flex()
                .items_center()
                .gap_2()
                .child(if self.routing_mode_busy.is_some() {
                    "路由 · 切换中…"
                } else {
                    match self.routing_mode {
                        RoutingMode::Direct => "路由 · 直连",
                        RoutingMode::Global => "路由 · 全局",
                        RoutingMode::Rule => "路由 · 规则",
                    }
                })
                .when(self.routing_mode_busy.is_none(), |control| {
                    control.child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme.text_tertiary)
                            .child("↻"),
                    )
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.apply_routing_mode(next, cx);
                }));
        }

        let mut control = div()
            .id("routing-modes")
            .h(px(34.0))
            .p(px(2.0))
            .rounded_md()
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_high)
            .flex()
            .items_center()
            .child(
                div()
                    .px_2()
                    .text_size(px(10.0))
                    .text_color(theme.text_tertiary)
                    .child("路由"),
            );
        for mode in [RoutingMode::Direct, RoutingMode::Global, RoutingMode::Rule] {
            let selected = mode == self.routing_mode;
            control = control.child(
                div()
                    .id(format!("routing-mode-{mode:?}"))
                    .role(Role::Button)
                    .aria_label(mode.label())
                    .aria_toggled(if selected {
                        Toggled::True
                    } else {
                        Toggled::False
                    })
                    .tab_stop(true)
                    .focusable()
                    .cursor_pointer()
                    .h_full()
                    .px_3()
                    .rounded_sm()
                    .flex()
                    .items_center()
                    .text_size(px(11.0))
                    .font_weight(if selected {
                        FontWeight::SEMIBOLD
                    } else {
                        FontWeight::NORMAL
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
                    .child(if self.routing_mode_busy == Some(mode) {
                        "切换中…"
                    } else {
                        mode.label()
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.apply_routing_mode(mode, cx);
                    })),
            );
        }
        control
    }

    fn navigation(&self, theme: Theme, size_class: WindowSizeClass, cx: &mut Context<Self>) -> Div {
        let entries = [
            ("节点", "节点", PrimaryWorkspace::Nodes),
            ("策略组", "策略", PrimaryWorkspace::Policies),
            ("分流规则", "规则", PrimaryWorkspace::RoutingRules),
            ("网络活动", "活动", PrimaryWorkspace::Activity),
            ("日志", "日志", PrimaryWorkspace::Logs),
            ("配置", "配置", PrimaryWorkspace::Configuration),
        ];
        let show_labels = size_class == WindowSizeClass::Wide;
        let source_label = if show_labels {
            self.controller.compact_label()
        } else {
            match &self.controller {
                ControllerState::Demo => "演示".to_owned(),
                ControllerState::Connecting { .. } => "连接中".to_owned(),
                ControllerState::Connected { .. } => "已连接".to_owned(),
                ControllerState::Failed { .. } => "失败".to_owned(),
            }
        };
        let width = match size_class {
            WindowSizeClass::Wide => 220.0,
            WindowSizeClass::Medium => 66.0,
            WindowSizeClass::Compact => 56.0,
        };
        div()
            .w(px(width))
            .h_full()
            .flex_shrink_0()
            .p_2()
            .flex()
            .flex_col()
            .gap_1()
            .bg(theme.surface_low)
            .border_r_1()
            .border_color(theme.outline_subtle)
            .children(entries.into_iter().map(|(label, short_label, workspace)| {
                let selected = workspace == self.primary_workspace;
                div()
                    .id(format!("navigation-{label}"))
                    .role(Role::Button)
                    .aria_label(label)
                    .tab_stop(true)
                    .focusable()
                    .cursor_pointer()
                    .h(px(40.0))
                    .px_3()
                    .rounded_md()
                    .flex()
                    .items_center()
                    .when(!show_labels, |row| row.justify_center().px_0())
                    .when(selected, |row| {
                        row.bg(theme.action_soft).text_color(theme.action_primary)
                    })
                    .when(!selected, |row| row.text_color(theme.text_secondary))
                    .font_weight(if selected {
                        FontWeight::SEMIBOLD
                    } else {
                        FontWeight::NORMAL
                    })
                    .child(if show_labels { label } else { short_label })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.primary_workspace = workspace;
                        this.status = match workspace {
                            PrimaryWorkspace::Policies => {
                                trace_ui(UiEvent::WorkspacePoliciesOpened);
                                "已打开策略组工作区".to_owned()
                            }
                            PrimaryWorkspace::Nodes => {
                                trace_ui(UiEvent::WorkspaceNodesOpened);
                                "已打开节点工作区".to_owned()
                            }
                            PrimaryWorkspace::RoutingRules => {
                                trace_ui(UiEvent::WorkspaceRoutingRulesOpened);
                                "已打开分流规则".to_owned()
                            }
                            PrimaryWorkspace::Activity => {
                                trace_ui(UiEvent::WorkspaceActivityOpened);
                                "已打开网络活动".to_owned()
                            }
                            PrimaryWorkspace::Logs => {
                                trace_ui(UiEvent::WorkspaceLogsOpened);
                                "已打开日志".to_owned()
                            }
                            PrimaryWorkspace::Configuration => {
                                trace_ui(UiEvent::WorkspaceConfigurationOpened);
                                "已打开代理来源".to_owned()
                            }
                        };
                        cx.notify();
                    }))
            }))
            .child(div().flex_1())
            .child(
                div()
                    .p_2()
                    .text_size(px(11.0))
                    .text_color(theme.text_tertiary)
                    .child(source_label),
            )
    }

    #[allow(clippy::too_many_lines)]
    fn policy_list(&self, theme: Theme, width: Option<f32>, cx: &mut Context<Self>) -> Div {
        let mut rows = div()
            .id("policy-scroll")
            .flex_1()
            .overflow_y_scroll()
            .p_2()
            .flex()
            .flex_col()
            .gap_1();
        for item in self.catalog.iter().cloned() {
            let selected = self.workspace.selected_group.as_ref() == Some(&item.id);
            let item_id = item.id.clone();
            let item_name = item.name.clone();
            let benchmark_id = item.id.clone();
            let benchmark_key = Self::policy_group_benchmark_key(&item.id);
            let benchmarking = self
                .group_benchmarks
                .get(&benchmark_key)
                .is_some_and(GroupBenchmarkState::is_running);
            rows = rows.child(
                div()
                    .id(format!("policy-{}", item.id.as_str()))
                    .role(Role::Button)
                    .tab_stop(true)
                    .focusable()
                    .cursor_pointer()
                    .min_h(px(72.0))
                    .p_3()
                    .flex()
                    .items_center()
                    .gap_3()
                    .rounded_md()
                    .border_1()
                    .border_color(if selected {
                        theme.action_primary
                    } else {
                        theme.surface_low
                    })
                    .bg(if selected {
                        theme.action_soft
                    } else {
                        theme.surface_low
                    })
                    .child(Self::group_benchmark_icon(
                        &benchmark_key,
                        benchmarking,
                        theme,
                        cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            if !benchmarking {
                                this.start_policy_group_benchmark(&benchmark_id, cx);
                            }
                        }),
                    ))
                    .child(
                        div()
                            .flex_1()
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme.text_primary)
                                    .child(format!("{}  {}", item.name, item.rules_count())),
                            )
                            .child(
                                div()
                                    .mt_1()
                                    .text_size(px(11.0))
                                    .text_color(theme.text_secondary)
                                    .child(item.kind.label()),
                            ),
                    )
                    .child(div().text_color(theme.text_primary).child(item.target))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.workspace.select_group(item_id.clone());
                        trace_ui(UiEvent::PolicyPreviewOpened);
                        this.status = format!("已打开策略组“{item_name}”");
                        cx.notify();
                    })),
            );
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
                    .p_4()
                    .border_b_1()
                    .border_color(theme.outline_subtle)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(18.0))
                                    .font_weight(FontWeight::BOLD)
                                    .child("策略组"),
                            )
                            .child(self.connection_button(theme, cx)),
                    )
                    .child(
                        div()
                            .mt_1()
                            .text_color(theme.text_secondary)
                            .child("节点选择与故障转移，不需要编辑 YAML"),
                    )
                    .child(
                        div()
                            .mt_4()
                            .h(px(36.0))
                            .px_3()
                            .rounded_md()
                            .border_1()
                            .border_color(theme.outline_subtle)
                            .bg(theme.surface_high)
                            .flex()
                            .items_center()
                            .text_color(theme.text_tertiary)
                            .child("筛选策略组"),
                    ),
            )
            .child(rows)
    }

    fn small_button(id: &'static str, label: &'static str, theme: Theme) -> Stateful<Div> {
        div()
            .id(id)
            .role(Role::Button)
            .tab_stop(true)
            .focusable()
            .cursor_pointer()
            .h(px(34.0))
            .px_3()
            .rounded_md()
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_high)
            .flex()
            .items_center()
            .text_color(theme.text_primary)
            .child(label)
    }

    fn connection_button(&self, theme: Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        let connecting = matches!(self.controller, ControllerState::Connecting { .. });
        div()
            .id("connect-mihomo")
            .role(Role::Button)
            .aria_label("连接或刷新 Mihomo 只读数据")
            .tab_stop(!connecting)
            .focusable()
            .cursor_pointer()
            .h(px(34.0))
            .px_3()
            .rounded_md()
            .border_1()
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
            .flex()
            .items_center()
            .child(self.runtime.button_label(&self.controller))
            .on_click(cx.listener(|this, _, _, cx| this.connect_mihomo(cx)))
    }

    #[allow(clippy::too_many_lines)]
    fn node_row(
        item: PolicyNode,
        current: bool,
        manually_selectable: bool,
        benchmark_state: GroupBenchmarkNodeState,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let node_id = item.id.clone();
        let node_name = item.name.clone();
        let provider = if item.kind == relay_core::PolicyCandidateKind::NodeGroup {
            "节点分组".to_owned()
        } else {
            item.provider
                .clone()
                .unwrap_or_else(|| "内置节点".to_owned())
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
                .rounded_sm()
                .bg(theme.surface_high)
                .text_size(px(9.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.text_tertiary)
                .flex()
                .items_center()
                .justify_center()
                .child(if item.kind == relay_core::PolicyCandidateKind::NodeGroup {
                    "组"
                } else {
                    "点"
                })
        };
        div()
            .id(format!("node-{}", item.id.as_str()))
            .tab_stop(manually_selectable)
            .min_h(px(64.0))
            .px_3()
            .flex()
            .items_center()
            .gap_3()
            .rounded_md()
            .border_1()
            .border_color(if manually_selectable && current {
                theme.action_primary
            } else {
                theme.outline_subtle
            })
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
                            .gap_2()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(if manually_selectable {
                                theme.text_primary
                            } else {
                                theme.text_secondary
                            })
                            .child(item.name)
                            .when(current && !manually_selectable, |name| {
                                name.child(
                                    div()
                                        .px_2()
                                        .py_1()
                                        .rounded_sm()
                                        .bg(theme.surface_high)
                                        .text_size(px(9.0))
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(theme.text_secondary)
                                        .child("当前出口"),
                                )
                            }),
                    )
                    .child(
                        div()
                            .mt_1()
                            .text_size(px(11.0))
                            .text_color(theme.text_tertiary)
                            .child(item.detail),
                    ),
            )
            .child(
                div()
                    .w(px(100.0))
                    .text_color(if manually_selectable {
                        theme.text_secondary
                    } else {
                        theme.text_tertiary
                    })
                    .child(provider),
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
                        this.workspace.select_node(node_id.clone());
                        trace_ui(UiEvent::PolicyPreviewOpened);
                        this.status = format!("已选择 {node_name} · 只读模式未写入 Mihomo");
                        cx.notify();
                    }))
            })
    }

    #[allow(clippy::too_many_lines)]
    fn detail(&self, theme: Theme, compact: bool, cx: &mut Context<Self>) -> Div {
        let selected_policy = self.selected_policy().clone();
        let selected_node = self.selected_node();
        let manually_selectable = selected_policy.kind.allows_manual_selection();
        let benchmark_id = selected_policy.id.clone();
        let benchmark_key = Self::policy_group_benchmark_key(&selected_policy.id);
        let benchmarking = self
            .group_benchmarks
            .get(&benchmark_key)
            .is_some_and(GroupBenchmarkState::is_running);
        let guidance = match selected_policy.kind {
            relay_core::PolicyGroupKind::Selector => "选择此策略当前使用的出口",
            relay_core::PolicyGroupKind::UrlTest => {
                "Mihomo 按策略配置的 URL 和间隔自动测量，候选项不可手动切换"
            }
            relay_core::PolicyGroupKind::Fallback => {
                "Mihomo 按策略配置的间隔自动检查并故障转移，候选项不可手动切换"
            }
            relay_core::PolicyGroupKind::LoadBalance => {
                "Mihomo 自动在候选分组之间分配连接，候选项不可手动切换"
            }
            relay_core::PolicyGroupKind::Direct => "直连策略没有可切换的出口",
        };
        let mut body = div()
            .id("detail-scroll")
            .flex_1()
            .overflow_y_scroll()
            .p_4()
            .flex()
            .flex_col()
            .gap_2();

        body = body.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(div().text_color(theme.text_secondary).child(guidance))
                .when(manually_selectable, |header| {
                    header.child(Self::small_button("add-node", "＋ 添加候选项", theme))
                }),
        );
        if selected_policy.kind.is_automatic() {
            body = body.child(
                div()
                    .mt_2()
                    .p_3()
                    .rounded_md()
                    .border_1()
                    .border_color(theme.outline_subtle)
                    .bg(theme.surface_low)
                    .child(
                        div()
                            .text_size(px(11.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text_secondary)
                            .child("自动策略 · 只读候选项"),
                    )
                    .child(
                        div()
                            .mt_1()
                            .text_size(px(10.0))
                            .text_color(theme.text_tertiary)
                            .child("名称和候选节点分组由用户配置；重新检查间隔属于策略设置。"),
                    ),
            );
        }
        if let Some(feedback) = self.group_benchmarks.get(&benchmark_key).and_then(|state| {
            Self::policy_group_benchmark_feedback(state, selected_policy.nodes.len(), theme)
        }) {
            body = body.child(feedback);
        }
        body = body.child(
            div()
                .mt_2()
                .px_3()
                .flex()
                .text_size(px(11.0))
                .text_color(theme.text_tertiary)
                .child(div().flex_1().child("候选节点 / 分组"))
                .child(div().w(px(100.0)).child("来源"))
                .child(div().w(px(64.0)).child("延迟")),
        );
        for item in selected_policy.nodes.iter().cloned() {
            let current = item.id == selected_node.id;
            let benchmark_state = self
                .group_benchmarks
                .get(&benchmark_key)
                .map_or(GroupBenchmarkNodeState::Idle, |state| {
                    state.node_state(&item.name)
                });
            body = body.child(Self::node_row(
                item,
                current,
                manually_selectable,
                benchmark_state,
                theme,
                cx,
            ));
        }

        body = body.child(
            div()
                .mt_5()
                .mb_1()
                .flex()
                .justify_between()
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("命中此策略的规则"),
                )
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(theme.text_tertiary)
                        .child(format!("{} 条，按顺序匹配", selected_policy.rules_count())),
                ),
        );
        for rule in &selected_policy.rules {
            body = body.child(
                div()
                    .h(px(50.0))
                    .flex()
                    .items_center()
                    .gap_3()
                    .border_t_1()
                    .border_color(theme.outline_subtle)
                    .child(
                        div()
                            .w(px(36.0))
                            .text_color(theme.text_tertiary)
                            .child(format!("#{}", rule.index)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .child(format!("{}, {}", rule.kind, rule.payload)),
                    )
                    .child(div().text_color(theme.status_success).child("命中")),
            );
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
                    .p_4()
                    .border_b_1()
                    .border_color(theme.outline_subtle)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .when(compact, |header| {
                                header.child(
                                    div()
                                        .id("compact-back")
                                        .role(Role::Button)
                                        .tab_stop(true)
                                        .focusable()
                                        .cursor_pointer()
                                        .child("← 返回")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.workspace.navigate_back();
                                            cx.notify();
                                        })),
                                )
                            })
                            .child(
                                div()
                                    .size(px(16.0))
                                    .rounded_full()
                                    .border_2()
                                    .border_color(theme.route_trace),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .child(
                                        div()
                                            .text_size(px(18.0))
                                            .font_weight(FontWeight::BOLD)
                                            .child(selected_policy.name.clone()),
                                    )
                                    .child(div().mt_1().text_color(theme.text_secondary).child(
                                        format!(
                                            "{} · {} 个节点 · {} 条规则",
                                            selected_policy.kind.label(),
                                            selected_policy.nodes.len(),
                                            selected_policy.rules_count()
                                        ),
                                    )),
                            )
                            .when(compact, |header| {
                                header.child(Self::group_benchmark_icon(
                                    &benchmark_key,
                                    benchmarking,
                                    theme,
                                    cx.listener(move |this, _, _, cx| {
                                        if !benchmarking {
                                            this.start_policy_group_benchmark(&benchmark_id, cx);
                                        }
                                    }),
                                ))
                            })
                            .child(
                                div()
                                    .id("open-inspector")
                                    .role(Role::Button)
                                    .tab_stop(true)
                                    .focusable()
                                    .cursor_pointer()
                                    .h(px(34.0))
                                    .px_3()
                                    .rounded_md()
                                    .border_1()
                                    .border_color(theme.outline_subtle)
                                    .flex()
                                    .items_center()
                                    .child("解释路由")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.inspector_open = true;
                                        trace_ui(UiEvent::RouteInspectorOpened);
                                        "已打开本地路由预测 · 演示数据"
                                            .clone_into(&mut this.status);
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .mt_4()
                            .flex()
                            .gap_5()
                            .font_weight(FontWeight::MEDIUM)
                            .child(
                                div()
                                    .pb_2()
                                    .border_b_2()
                                    .border_color(theme.action_primary)
                                    .child("节点"),
                            )
                            .child(div().text_color(theme.text_secondary).child("规则"))
                            .child(div().text_color(theme.text_secondary).child("设置")),
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
                            .text_size(px(11.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text_tertiary)
                            .child(format!("{index} · {label}")),
                    )
                    .child(div().mt_1().font_weight(FontWeight::BOLD).child(value))
                    .child(div().mt_1().text_color(theme.text_secondary).child(detail)),
            )
    }

    #[allow(clippy::too_many_lines)]
    fn inspector(&self, theme: Theme, overlay: bool, cx: &mut Context<Self>) -> Div {
        let selected_policy = self.selected_policy().clone();
        let selected_node = self.selected_node();
        let decision_copy = if selected_policy.kind.allows_manual_selection() {
            format!("{} · 当前人工选择", selected_policy.kind.label())
        } else {
            format!("{} · 由 Mihomo 自动决策", selected_policy.kind.label())
        };
        let domain = if selected_policy.id.as_str() == "search" {
            "openai.com"
        } else {
            "youtube.com"
        };
        let rule_index = selected_policy.rules.first().map_or(18, |rule| rule.index);
        let observed_route = self.observed_routes.first().cloned();

        div()
            .w(px(340.0))
            .h_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .bg(theme.surface_low)
            .border_l_1()
            .border_color(theme.outline_subtle)
            .when(overlay, |panel| panel.absolute().top_0().right_0().bottom_0().shadow_xl())
            .child(
                div()
                    .p_4()
                    .border_b_1()
                    .border_color(theme.outline_subtle)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(div().text_size(px(18.0)).font_weight(FontWeight::BOLD).child("路由解释"))
                            .child(div().px_2().py_1().rounded_sm().bg(theme.route_soft).text_size(px(10.0)).font_weight(FontWeight::SEMIBOLD).text_color(theme.route_trace).child("预测路径 · 演示数据"))
                            .child(div().flex_1())
                            .when(overlay, |header| {
                                header.child(
                                    div()
                                        .id("close-inspector")
                                        .role(Role::Button)
                                        .tab_stop(true)
                                        .focusable()
                                        .cursor_pointer()
                                        .child("关闭")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.inspector_open = false;
                                            trace_ui(UiEvent::RouteInspectorClosed);
                                            cx.notify();
                                        })),
                                )
                            }),
                    )
                    .child(div().mt_2().text_color(theme.text_secondary).child("按本地规则模型预览可能选择的路径"))
                    .child(
                        div()
                            .mt_4()
                            .flex()
                            .gap_2()
                            .child(div().h(px(38.0)).flex_1().px_3().rounded_md().border_1().border_color(theme.outline_subtle).bg(theme.surface_high).flex().items_center().child(domain))
                            .child(
                                div()
                                    .id("predict-route")
                                    .role(Role::Button)
                                    .tab_stop(true)
                                    .focusable()
                                    .cursor_pointer()
                                    .h(px(38.0))
                                    .px_3()
                                    .rounded_md()
                                    .bg(theme.action_primary)
                                    .text_color(theme.action_on_primary)
                                    .flex()
                                    .items_center()
                                    .child("预测路由")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        trace_ui(UiEvent::RoutePredictionRequested);
                                        this.status = format!("已预测 {domain}：{} → {}", this.selected_policy().name, this.selected_node().name);
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
            .child(
                div()
                    .id("inspector-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .p_4()
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
                            .child(Self::signal_stage("01", "预测首条命中规则", "DOMAIN-SUFFIX".to_owned(), format!("{domain} · 规则 #{rule_index}"), true, theme))
                            .child(Self::signal_stage("02", "交给策略组", selected_policy.name.clone(), decision_copy, false, theme))
                            .child(Self::signal_stage("03", "最终出口", selected_node.name.clone(), format!("{} · {}", selected_node.latency_ms.map_or_else(|| "延迟未知".to_owned(), |latency| format!("{latency} ms")), selected_node.provider.as_deref().unwrap_or("内置节点")), false, theme)),
                    )
                    .when_some(observed_route, |panel, observed| {
                        let host = observed.host.unwrap_or_else(|| "目标未知".to_owned());
                        let rule = observed.rule.unwrap_or_else(|| "规则未知".to_owned());
                        let payload = observed.rule_payload.unwrap_or_default();
                        let chain = if observed.chains.is_empty() {
                            "链路未返回".to_owned()
                        } else {
                            observed.chains.join(" → ")
                        };
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
                                        .text_size(px(11.0))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(theme.action_primary)
                                        .child("最近已观察 · /connections"),
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
                            .child("匹配方式                         规则模式")
                            .child(div().mt_2().child("DNS                     未查询（域名规则）"))
                            .child(div().mt_2().child("结果类型                   本地规则预测")),
                    )
                    .child(
                        div()
                            .mt_5()
                            .pt_4()
                            .border_t_1()
                            .border_color(theme.outline_subtle)
                            .text_size(px(11.0))
                            .text_color(theme.text_tertiary)
                            .child("这不是 Mihomo 已建立的连接。只有来自 /connections 的链路才能标为“已观察”。"),
                    ),
            )
    }

    fn status_bar(&self, theme: Theme) -> Div {
        let (source, endpoint, download, upload, dot) = match &self.controller {
            ControllerState::Demo => (
                "Mihomo 未连接".to_owned(),
                "配置：演示数据".to_owned(),
                "↓ —".to_owned(),
                "↑ —".to_owned(),
                theme.route_trace,
            ),
            ControllerState::Connecting { endpoint } => (
                "Mihomo 连接中".to_owned(),
                endpoint.clone(),
                "↓ —".to_owned(),
                "↑ —".to_owned(),
                theme.route_trace,
            ),
            ControllerState::Connected {
                endpoint,
                version,
                active_connections,
                download_total,
                upload_total,
            } => (
                format!("Mihomo {version} · {active_connections} 条连接"),
                endpoint.clone(),
                format!("累计↓ {}", format_bytes(*download_total)),
                format!("累计↑ {}", format_bytes(*upload_total)),
                theme.status_success,
            ),
            ControllerState::Failed { endpoint, .. } => (
                "Mihomo 连接失败".to_owned(),
                endpoint.clone(),
                "↓ —".to_owned(),
                "↑ —".to_owned(),
                theme.route_trace,
            ),
        };

        div()
            .h(px(28.0))
            .flex_shrink_0()
            .flex()
            .items_center()
            .px_3()
            .gap_4()
            .border_t_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_chrome)
            .text_size(px(11.0))
            .text_color(theme.text_secondary)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().size(px(8.0)).rounded_full().bg(dot))
                    .child(source),
            )
            .child(endpoint)
            .child(self.status.clone())
            .child(div().flex_1())
            .child(download)
            .child(upload)
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

impl Default for RelayApp {
    fn default() -> Self {
        Self::new()
    }
}

impl Render for RelayApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let width = window.viewport_size().width.as_f32();
        self.workspace.resize(width);
        let size_class = self.workspace.size_class;
        let theme = self.theme();
        self.ensure_subscription_input(theme, cx);
        self.ensure_qx_rule_input(theme, cx);
        self.ensure_node_group_inputs(theme, cx);
        let compact = size_class == WindowSizeClass::Compact;
        let show_groups =
            !compact || self.workspace.compact_navigation == CompactNavigation::GroupList;
        let show_detail =
            !compact || self.workspace.compact_navigation == CompactNavigation::GroupDetail;
        let overlay_inspector = size_class != WindowSizeClass::Wide;
        let show_inspector = size_class == WindowSizeClass::Wide || self.inspector_open;
        let policies_active = self.primary_workspace == PrimaryWorkspace::Policies;
        let nodes_active = self.primary_workspace == PrimaryWorkspace::Nodes;
        let routing_rules_active = self.primary_workspace == PrimaryWorkspace::RoutingRules;
        let activity_active = self.primary_workspace == PrimaryWorkspace::Activity;
        let logs_active = self.primary_workspace == PrimaryWorkspace::Logs;

        div()
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
                        main.child(self.routing_rules_workspace(theme, size_class, cx))
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
                    .when(policies_active && show_groups, |main| {
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
                    })
                    .when(policies_active && show_detail, |main| {
                        main.child(self.detail(theme, compact, cx))
                    })
                    .when(policies_active && show_inspector, |main| {
                        main.child(self.inspector(theme, overlay_inspector, cx))
                    }),
            )
            .child(self.status_bar(theme))
    }
}

#[cfg(all(test, not(windows)))]
mod tests {
    use std::fs;

    use relay_core::{
        PolicyCandidateKind, PolicyCatalog, PolicyGroup, PolicyGroupId, PolicyGroupKind,
        PolicyNode, ProxyId,
    };

    use super::{ImportedSubscriptionState, RelayApp};
    use crate::mihomo;
    use crate::subscription::SourceKind;

    #[test]
    fn app_startup_detects_a_privately_imported_subscription() {
        let root = std::env::temp_dir().join(format!("relay-app-import-{}", std::process::id()));
        if root.exists() {
            fs::remove_dir_all(&root).expect("remove stale fixture");
        }
        let store = root.join("subscriptions");
        mihomo::save_imported_subscription_in(
            &store,
            "https://subscription.example.invalid/client?token=fixture",
        )
        .expect("save fixture subscription");

        let app = RelayApp::with_controller_and_subscription_store("http://127.0.0.1:9090", store);

        assert_eq!(app.imported_subscriptions.len(), 1);
        assert_eq!(
            app.imported_subscriptions[0].state,
            ImportedSubscriptionState::Pending(SourceKind::HttpsSubscription)
        );
        fs::remove_dir_all(root).expect("remove fixture");
    }

    #[test]
    fn global_node_state_comes_from_the_runtime_global_selector() {
        let mut app = RelayApp::with_controller("http://127.0.0.1:9090");
        app.catalog = PolicyCatalog::try_new(vec![PolicyGroup {
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
        .expect("fixture global group");

        assert_eq!(app.global_target(), Some("Tokyo"));
        assert!(app.is_global_candidate("Singapore"));
        assert!(!app.is_global_candidate("Unknown"));
    }

    #[test]
    fn app_startup_restores_saved_qx_rule_sources() {
        let root = std::env::temp_dir().join(format!("relay-app-qx-rule-{}", std::process::id()));
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

        let app = RelayApp::with_controller_and_subscription_store("http://127.0.0.1:9090", store);

        assert_eq!(app.qx_rule_sources.len(), 1);
        assert_eq!(app.qx_rule_sources[0].rule_count, 1);
        assert_eq!(app.qx_rule_sources[0].target_policy.as_str(), "Proxy");
        fs::remove_dir_all(root).expect("remove fixture");
    }
}
