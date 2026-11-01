use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui::{Context, Div, IntoElement, ParentElement, Render, Styled, Window, div, prelude::*, px};
use manis_core::{
    NodeWorkspaceState, PolicyCatalog, PolicyGroupId, PolicyWorkspaceState, PrimaryWorkspace,
    ProxyMode, RoutingMode, WindowSizeClass,
};
use manis_mihomo::{Connection, ObservedRouteEvidence, RuntimeConfig};
use manis_profile::{QxRuleList, SecretUrl};

use crate::{
    auto_connect, autostart,
    components::StatusTone,
    diagnostics::{self, LogLevel, UiEvent, record_event, trace_ui},
    kernel::KernelRuntime,
    localization::{CountNoun, Language, LanguagePreference, Localizer, Message, copy},
    mihomo::{
        self, ControllerRuntime, ControllerState, GeneratedProfileApply, KernelLogEntry,
        LiveRuntimeSession, LiveStreamStatus, LoadedProvider, RemoteSourceRefreshInterval,
        StoredQxRuleSource, StoredSingleNode, StoredSubscription, SubscriptionPreviewError,
        SubscriptionStoreError,
    },
    subscription::{SourceKind, SubscriptionInputError, SubscriptionPreview},
    subscription_input::{SubscriptionInputChanged, SubscriptionTextInput, TextInputSpec},
    system_proxy::{ProxyPorts, SystemProxySession, TunDnsSession},
    theme::Theme,
};

mod about;
mod activity;
mod configuration;
mod exit_ip;
mod logs;
mod nodes;
mod policy_presentation;
mod policy_workspace;
mod presentation;
mod proxy_transition;
mod routing_apply;
mod routing_controls;
mod runtime_lifecycle;
mod stored_workspace;
mod subscription_workflow;
pub(crate) mod windows;
mod workspace_inputs;

#[cfg(not(test))]
use crate::core_update;
use configuration::{
    ImportQxRuleError, ManualRuleConditionEditor, ManualRuleEditorState, ManualRuleInsert,
    ManualRulePopover, ProxySourceEditorState, QxRuleEditorPopover, QxRuleImportFeedback,
    QxRuleImportResult, QxRuleSourceRefreshState, RuleSourceState,
};
use nodes::{
    GroupBenchmarkNodeState, GroupBenchmarkProgressQueue, GroupBenchmarkState,
    GroupBenchmarkSummary, ManagedPolicyRuntimeState, ManagedPolicyState, PolicyBenchmarkRun,
    benchmark_timestamp,
};
use policy_workspace::PolicySelectionRequest;
use presentation::{
    compact_proxy_mode_label, controller_state_label, controller_status_label, format_bytes,
    policy_kind_label, policy_target_is_selectable, proxy_mode_label, routing_mode_label,
    status_bar_values,
};
use proxy_transition::apply_proxy_mode_transition;
#[cfg(all(test, not(windows)))]
use proxy_transition::tun_dns_log_details;
use routing_apply::{
    RoutingApplyRollback, RoutingApplyState, SourceMutation, SourceRuntimeApply,
    mutate_saved_sources,
};
use runtime_lifecycle::LifecycleSubscriptions;
use runtime_lifecycle::{AppUpdateState, MihomoCoreUpdateState};
use stored_workspace::StoredWorkspace;
use subscription_workflow::{
    DueRemoteSource, ImportedSubscription, ImportedSubscriptionState, SourceLoadOutcome,
    SourceRefreshSchedulerState, SubscriptionFeedback, SubscriptionImportRequest,
    managed_subscription_provider_index, source_kind,
};
#[cfg(all(test, not(windows)))]
use subscription_workflow::{ImportSubscriptionError, next_due_remote_source};
use workspace_inputs::WorkspaceInputs;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum ConfigurationSection {
    #[default]
    General,
    Runtime,
    ProxySources,
    RuleSources,
    Advanced,
    Updates,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum ProxySourceEditorKind {
    #[default]
    Subscription,
    SingleNode,
}

impl ConfigurationSection {
    const ALL: [Self; 6] = [
        Self::General,
        Self::Runtime,
        Self::ProxySources,
        Self::RuleSources,
        Self::Advanced,
        Self::Updates,
    ];

    const fn key(self) -> &'static str {
        match self {
            Self::General => "general",
            Self::Runtime => "runtime",
            Self::ProxySources => "proxy-sources",
            Self::RuleSources => "rule-sources",
            Self::Advanced => "advanced",
            Self::Updates => "updates",
        }
    }
}

type SingleNodeImportResult = Result<SourceLoadOutcome<StoredSingleNode>, SubscriptionStoreError>;
type QxRuleRefreshResult = Result<SourceMutation<StoredQxRuleSource>, ImportQxRuleError>;

enum PreferencePersistence {
    Saved,
    Skipped,
    Failed(SubscriptionStoreError),
}

type RoutingModeApplyResult = Result<PreferencePersistence, mihomo::LoadError>;

/// Whether opening Manis starts the managed kernel, and which step of that attempt is running.
#[derive(Debug, Default)]
pub(in crate::app) struct AutoConnectState {
    pub(super) enabled: bool,
    pub(super) stage: AutoConnectStage,
    pub(super) attempts: u8,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::app) enum AutoConnectStage {
    #[default]
    Idle,
    WaitingForCore,
    Connecting,
}

pub struct ManisApp {
    localizer: Localizer,
    primary_workspace: PrimaryWorkspace,
    configuration_section: ConfigurationSection,
    configuration_scroll: gpui::ScrollHandle,
    routing_rules_scroll: gpui::ScrollHandle,
    configuration_navigation_scroll: gpui::ScrollHandle,
    configuration_add_section: Option<ConfigurationSection>,
    configuration_transfer: configuration::ConfigurationTransfer,
    node_workspace: NodeWorkspaceState,
    workspace: PolicyWorkspaceState,
    expanded_policy_group: Option<PolicyGroupId>,
    catalog: Option<PolicyCatalog>,
    runtime: KernelRuntime,
    mihomo_core_update_state: MihomoCoreUpdateState,
    app_update_state: AppUpdateState,
    controller: ControllerState,
    observed_routes: Vec<ObservedRouteEvidence>,
    source_providers: Vec<LoadedProvider>,
    subscription_preview_providers: Vec<LoadedProvider>,
    subscription_action_generation: u64,
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
    exit_ips: BTreeMap<String, (Option<String>, u64)>,
    exit_ip_probe_active: bool,
    dark: bool,
    status: String,
    launch_at_login: bool,
    auto_connect: AutoConnectState,
    proxy_source_editor: ProxySourceEditorState,
    inputs: WorkspaceInputs,
    manual_rules: Vec<crate::manual_rule::ManualRule>,
    manual_rule_conditions: Vec<ManualRuleConditionEditor>,
    manual_rule_condition_count: usize,
    /// Where a new rule is inserted; rules created from the activity list must win.
    manual_rule_insert: ManualRuleInsert,
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
        Self::start_app_update_polling(cx);
        #[cfg(not(test))]
        app.sync_launch_at_login();
        if matches!(app.mihomo_core_update_state, MihomoCoreUpdateState::Missing) {
            app.update_mihomo_core(cx);
        }
        app.maybe_auto_connect(cx);
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
        let launch_at_login = autostart::load_preference(subscription_store_dir.as_deref());
        let stored_workspace = StoredWorkspace::load(subscription_store_dir.as_deref());
        let status = Self::restored_workspace_status(
            &runtime,
            subscription_store_dir.as_deref(),
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
            benchmarks,
            routing_mode,
            error: source_store_error,
        } = stored_workspace;
        let default_rule_target = managed_policy_groups
            .first()
            .map_or_else(|| "DIRECT".to_owned(), |group| group.name.clone());
        let mut node_workspace = NodeWorkspaceState::default();
        node_workspace.replace_collapsed_groups(collapsed_groups.iter().map(String::as_str));
        let auto_connect_enabled = auto_connect::load_preference(subscription_store_dir.as_deref());
        Self {
            localizer,
            primary_workspace: PrimaryWorkspace::default(),
            configuration_section: ConfigurationSection::default(),
            configuration_scroll: gpui::ScrollHandle::new(),
            routing_rules_scroll: gpui::ScrollHandle::new(),
            configuration_navigation_scroll: gpui::ScrollHandle::new(),
            configuration_add_section: None,
            configuration_transfer: configuration::ConfigurationTransfer::default(),
            node_workspace,
            workspace: PolicyWorkspaceState::default(),
            expanded_policy_group: None,
            catalog: None,
            runtime,
            mihomo_core_update_state: Self::initial_mihomo_core_update_state(),
            app_update_state: AppUpdateState::Idle,
            controller: ControllerState::Disconnected,
            observed_routes: Vec::new(),
            source_providers: Vec::new(),
            subscription_preview_providers: Vec::new(),
            subscription_action_generation: 0,
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
                benchmarks,
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
            exit_ips: BTreeMap::new(),
            exit_ip_probe_active: false,
            dark: false,
            status,
            launch_at_login,
            auto_connect: AutoConnectState {
                enabled: auto_connect_enabled,
                ..AutoConnectState::default()
            },
            proxy_source_editor: ProxySourceEditorState::default(),
            inputs: WorkspaceInputs::default(),
            manual_rules: Vec::new(),
            manual_rule_conditions: Vec::new(),
            manual_rule_condition_count: 1,
            manual_rule_insert: ManualRuleInsert::Append,
            manual_rule_target: default_rule_target,
            manual_rule_editor_state: ManualRuleEditorState::Closed,
            manual_rule_popover: None,
            manual_rule_error: None,
            lifecycle_subscriptions: LifecycleSubscriptions::default(),
        }
    }

    pub(crate) fn attach_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.workspace.resize(window.viewport_size().width.as_f32());
        crate::theme::sync_component_theme(self.theme(), self.dark, Some(window), cx);
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
            if !this.proxy_source_editor.is_importing()
                && this.proxy_source_editor.feedback != SubscriptionFeedback::Idle
            {
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
        let language = self.language();
        if let Some(input) = self.inputs.qx_rule.as_ref() {
            input.update(cx, |input, cx| input.set_theme(theme, self.dark, cx));
            if let Some(input) = self.inputs.qx_rule_name.as_ref() {
                input.update(cx, |input, cx| {
                    input.set_theme(theme, self.dark, cx);
                    input.set_placeholder(
                        language.localized(copy::configuration::RULE_SOURCE_NAME_PLACEHOLDER),
                        cx,
                    );
                });
            }
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
        let name_input = cx.new(|cx| {
            SubscriptionTextInput::new_field(
                TextInputSpec::new(
                    "qx-rule-name-input",
                    language.localized(copy::configuration::RULE_SOURCE_NAME_PLACEHOLDER),
                    96,
                    theme,
                    self.dark,
                ),
                window,
                cx,
            )
        });
        self.inputs.qx_rule_name_events = Some(cx.subscribe(
            &name_input,
            |this, _input, _: &SubscriptionInputChanged, cx| {
                if this.rule_sources.feedback != QxRuleImportFeedback::Idle {
                    this.rule_sources.feedback = QxRuleImportFeedback::Idle;
                    cx.notify();
                }
            },
        ));
        self.inputs.qx_rule_name = Some(name_input);
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
    FixtureReadOnly,
}

/// The reason a proxy mode cannot be applied.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProxyModeBlock {
    Busy,
    ControllerNotConnected,
    FixtureReadOnly,
}

impl ProxyModeBlock {
    /// A short phrase that fits after a tray menu label.
    pub(crate) const fn tray_reason(self, language: Language) -> &'static str {
        match self {
            Self::Busy => language.localized(copy::app::SWITCHING_STATUS),
            Self::ControllerNotConnected => language.localized(copy::app::CONNECT_FIRST),
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
    match requested {
        ProxyMode::Off | ProxyMode::System => None,
        ProxyMode::Tun => match tun {
            TunSupport::Supported => None,
            TunSupport::FixtureReadOnly => Some(ProxyModeBlock::FixtureReadOnly),
        },
    }
}

impl ManisApp {
    fn workspace_content(
        &mut self,
        size_class: WindowSizeClass,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .relative()
            .flex_1()
            .overflow_hidden()
            .flex()
            .child(self.navigation(theme, size_class, cx))
            .when(self.primary_workspace == PrimaryWorkspace::Nodes, |main| {
                main.child(self.node_workspace(theme, size_class, cx))
            })
            .when(
                self.primary_workspace == PrimaryWorkspace::RoutingRules,
                |main| main.child(self.routing_rules_workspace(theme, size_class, window, cx)),
            )
            .when(
                self.primary_workspace == PrimaryWorkspace::Activity,
                |main| main.child(self.activity_workspace(theme, size_class, cx)),
            )
            .when(self.primary_workspace == PrimaryWorkspace::Logs, |main| {
                main.child(self.logs_workspace(theme, size_class, cx))
            })
            .when(
                self.primary_workspace == PrimaryWorkspace::Configuration,
                |main| main.child(self.configuration_workspace(theme, size_class, cx)),
            )
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

        div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.window_backdrop)
            .text_color(theme.text_primary)
            .text_size(px(13.0))
            .child(self.chrome(theme, size_class, cx))
            .child(self.workspace_content(size_class, theme, window, cx))
            .child(self.status_bar(theme, size_class))
    }
}

/// Read-only root payload for the desktop NSH host.
///
/// It reuses the desktop storage loader so the NSH shell sees the same saved
/// subscriptions, nodes, groups and routing mode. Connection state and writes
/// still belong to the running Manis app.
#[must_use]
pub fn nsh_root_data() -> serde_json::Value {
    use serde_json::json;

    let directory = crate::brand::config_dir();
    let workspace = StoredWorkspace::load(directory.as_deref());
    let subscriptions: Vec<_> = workspace
        .imported_subscriptions
        .iter()
        .enumerate()
        .map(|(index, subscription)| {
            json!({
                "index": index,
                "id": subscription.id,
                "name": subscription.name,
                "displayUrl": subscription.source.expose_to(mask_url),
                "enabled": subscription.enabled,
            })
        })
        .collect();
    let nodes: Vec<_> = workspace
        .saved_single_nodes
        .iter()
        .enumerate()
        .map(|(index, node)| {
            json!({
                "index": index,
                "id": node.id,
                "name": node.name,
                "enabled": node.enabled,
            })
        })
        .collect();
    let groups: Vec<_> = workspace
        .managed_policy_groups
        .iter()
        .enumerate()
        .map(|(index, group)| {
            json!({
                "index": index,
                "name": group.name,
                "strategy": match group.strategy {
                    manis_core::ManagedPolicyStrategy::LowestLatency => "urltest",
                    manis_core::ManagedPolicyStrategy::Manual => "select",
                },
                "subscriptionIndexes": [],
                "nodeIndexes": [],
            })
        })
        .collect();
    let live = live_snapshot();
    let active = live
        .as_ref()
        .is_some_and(|snapshot| snapshot.runtime.tun.enable);
    let global = live.as_ref().and_then(|snapshot| {
        snapshot.policy_groups().into_iter().find(|group| {
            group.name == "__MANIS_GLOBAL__" || group.name.eq_ignore_ascii_case("GLOBAL")
        })
    });
    let live_node = global.and_then(|group| group.current).unwrap_or_default();
    let mode = live.as_ref().map_or_else(
        || workspace.routing_mode.wire_value(),
        |snapshot| snapshot.runtime.mode.wire_value(),
    );
    let kernel_version = live
        .as_ref()
        .and_then(|snapshot| snapshot.version.version.clone())
        .filter(|version| !version.is_empty())
        .unwrap_or_else(|| "—".into());
    let summary = live.as_ref().map_or_else(
        || "—".into(),
        |snapshot| {
            format!(
                "{} 个代理 · {} 条规则",
                snapshot.proxies.len(),
                snapshot.rules.len()
            )
        },
    );
    let subscription_count = workspace.imported_subscriptions.len();
    let node_count = workspace.saved_single_nodes.len();
    let group_count = workspace.managed_policy_groups.len();
    let has_sources = subscription_count > 0 || node_count > 0;

    let node_page = live.as_ref().map_or_else(
        || {
            json!({
                "global": { "label": "全局", "current": null, "total": 0 },
                "sources": [],
                "policyGroups": []
            })
        },
        |snapshot| nsh_node_page(snapshot, &workspace, &live_node),
    );
    let rules_page = live.as_ref().map_or_else(
        || {
            json!({
                "rules": [],
                "kinds": [
                    "DOMAIN-SUFFIX", "DOMAIN", "DOMAIN-KEYWORD", "DOMAIN-WILDCARD",
                    "IP-CIDR", "IP6-CIDR", "GEOIP", "IP-ASN", "DST-PORT", "FINAL"
                ],
                "policies": ["DIRECT", "REJECT", "__MANIS_GLOBAL__"],
                "warnings": [],
                "summary": ""
            })
        },
        nsh_rules_page,
    );

    json!({
        "pending": false,
        "nodes": node_page,
        "subscriptions": {
            "subscriptions": subscriptions,
            "nodes": nodes,
            "groups": groups
        },
        "rules": rules_page,
        "settings": {
            "status": if active { "已连接" } else { "未连接" },
            "mode": mode,
            "proxyDns": { "mode": "doh", "custom": [], "discovered": [], "label": "加密 DNS（DoH）" },
            "kernelVersion": kernel_version,
            "summary": summary,
            "subscriptions": subscription_count,
            "nodes": node_count,
            "groups": group_count,
            "abi": "desktop",
            "dataDir": directory.map(|path| path.display().to_string()).unwrap_or_else(|| "—".into()),
            "installedVersion": crate::version(),
            "update": { "state": "idle" }
        },
        "connection": {
            "active": active,
            "node": live_node,
            "mode": mode,
            "summary": summary,
            "error": "",
            "unreachable": false,
            "warnings": [],
            "warningCount": 0,
            "hasSources": has_sources
        }
    })
}

#[cfg(unix)]
fn live_client() -> Option<manis_mihomo::MihomoClient<manis_mihomo::UnixSocketTransport>> {
    let socket = crate::brand::data_dir()?.join("mihomo/controller.sock");
    if !socket.exists() {
        return None;
    }
    Some(manis_mihomo::MihomoClient::new(
        manis_mihomo::ControllerConfig::default(),
        manis_mihomo::UnixSocketTransport::new(socket),
    ))
}

#[cfg(unix)]
fn live_snapshot() -> Option<manis_mihomo::MihomoSnapshot> {
    live_client()?.fetch_snapshot().ok()
}

#[cfg(not(unix))]
fn live_snapshot() -> Option<manis_mihomo::MihomoSnapshot> {
    None
}

/// Selects a node in the live desktop controller's global group.
///
/// # Errors
/// Returns a controller error when no live controller or group exists.
pub fn nsh_select_global_node(name: &str) -> Result<(), String> {
    #[cfg(unix)]
    {
        let client = live_client().ok_or_else(|| "桌面内核未运行".to_owned())?;
        let snapshot = client.fetch_snapshot().map_err(|error| error.to_string())?;
        let group = snapshot
            .policy_groups()
            .into_iter()
            .find(|group| {
                group.name == "__MANIS_GLOBAL__" || group.name.eq_ignore_ascii_case("GLOBAL")
            })
            .ok_or_else(|| "找不到全局策略组".to_owned())?;
        client
            .select_policy_group_node(&group.name, name)
            .map_err(|error| error.to_string())
    }
    #[cfg(not(unix))]
    {
        let _ = name;
        Err("桌面控制器只在 Unix 上接入".to_owned())
    }
}

/// Selects a node in one live desktop policy group.
///
/// # Errors
/// Returns a controller error when no live controller exists.
pub fn nsh_select_group_node(group: &str, name: &str) -> Result<(), String> {
    #[cfg(unix)]
    {
        live_client()
            .ok_or_else(|| "桌面内核未运行".to_owned())?
            .select_policy_group_node(group, name)
            .map_err(|error| error.to_string())
    }
    #[cfg(not(unix))]
    {
        let _ = (group, name);
        Err("桌面控制器只在 Unix 上接入".to_owned())
    }
}

/// Switches the live desktop controller's routing mode.
///
/// # Errors
/// Returns a controller error for an unknown mode or unavailable controller.
pub fn nsh_set_routing_mode(mode: &str) -> Result<(), String> {
    #[cfg(unix)]
    {
        let mode = manis_mihomo::RoutingMode::parse_wire_value(mode)
            .ok_or_else(|| "未知路由模式".to_owned())?;
        live_client()
            .ok_or_else(|| "桌面内核未运行".to_owned())?
            .set_routing_mode(mode)
            .map_err(|error| error.to_string())
    }
    #[cfg(not(unix))]
    {
        let _ = mode;
        Err("桌面控制器只在 Unix 上接入".to_owned())
    }
}

fn nsh_node_page(
    snapshot: &manis_mihomo::MihomoSnapshot,
    workspace: &StoredWorkspace,
    global_current: &str,
) -> serde_json::Value {
    use serde_json::json;

    let delay = |name: &str| {
        snapshot
            .proxies
            .iter()
            .find(|proxy| proxy.name == name)
            .and_then(manis_mihomo::Proxy::latest_latency_ms)
            .map(|value| value.round() as i64)
    };
    let node_item = |name: &str, current: &str| {
        json!({
            "name": name,
            "displayName": name,
            "delayMs": delay(name),
            "current": name == current,
        })
    };

    let subscription_providers: Vec<_> = snapshot
        .providers
        .iter()
        .filter(|provider| provider.name.starts_with("Subscription "))
        .collect();
    let sources: Vec<_> = workspace
        .imported_subscriptions
        .iter()
        .enumerate()
        .map(|(index, subscription)| {
            let provider = subscription_providers.get(index).copied();
            let nodes = provider.map_or_else(Vec::new, |provider| {
                provider
                    .proxies
                    .iter()
                    .map(|proxy| node_item(&proxy.name, global_current))
                    .collect()
            });
            json!({
                "id": format!("subscription-{index}"),
                "name": subscription.name,
                "detail": provider.map_or("订阅 · 节点未加载", |_| "订阅 · 实时节点"),
                "kind": "subscription",
                "enabled": subscription.enabled,
                "nodes": nodes,
            })
        })
        .collect();
    let manual_nodes: Vec<_> = workspace
        .saved_single_nodes
        .iter()
        .map(|node| node_item(&node.name, global_current))
        .collect();
    let mut sources = sources;
    if !manual_nodes.is_empty() {
        sources.push(json!({
            "id": "manual",
            "name": "手动节点",
            "detail": "分享链接导入",
            "kind": "manual",
            "enabled": true,
            "nodes": manual_nodes,
        }));
    }

    let policy_groups: Vec<_> = snapshot
        .policy_groups()
        .into_iter()
        .filter(|group| {
            group.name != "__MANIS_GLOBAL__" && !group.name.eq_ignore_ascii_case("GLOBAL")
        })
        .map(|group| {
            let members: Vec<_> = group
                .nodes
                .iter()
                .map(|name| node_item(name, group.current.as_deref().unwrap_or("")))
                .collect();
            json!({
                "id": group.name,
                "label": group.name,
                "kind": match group.kind {
                    manis_mihomo::GroupKind::UrlTest => "urltest",
                    manis_mihomo::GroupKind::Fallback => "fallback",
                    manis_mihomo::GroupKind::LoadBalance => "load-balance",
                    manis_mihomo::GroupKind::Selector => "select",
                },
                "current": group.current,
                "members": members,
            })
        })
        .collect();

    json!({
        "global": {
            "label": "全局",
            "current": if global_current.is_empty() { serde_json::Value::Null } else { json!(global_current) },
            "total": snapshot.providers.iter().map(|provider| provider.proxies.len()).sum::<usize>()
                + workspace.saved_single_nodes.len(),
        },
        "sources": sources,
        "policyGroups": policy_groups,
    })
}

fn nsh_rules_page(snapshot: &manis_mihomo::MihomoSnapshot) -> serde_json::Value {
    use serde_json::json;

    let rules: Vec<_> = snapshot
        .rules
        .iter()
        .enumerate()
        .map(|(index, rule)| {
            let kind = match rule.kind.as_str() {
                "Match" | "Final" => "FINAL",
                "DomainSuffix" => "DOMAIN-SUFFIX",
                "Domain" => "DOMAIN",
                "DomainKeyword" => "DOMAIN-KEYWORD",
                "DomainWildcard" => "DOMAIN-WILDCARD",
                "IPCIDR" => "IP-CIDR",
                "IPCIDR6" => "IP6-CIDR",
                "GeoIP" => "GEOIP",
                "IPASN" => "IP-ASN",
                "DstPort" => "DST-PORT",
                other => other,
            };
            json!({
                "index": index,
                "kind": kind,
                "value": if kind == "FINAL" { "" } else { rule.payload.as_str() },
                "policy": rule.proxy,
                "policyLabel": if rule.proxy == "__MANIS_GLOBAL__" { "全局" } else { rule.proxy.as_str() },
                "enabled": !rule.extra.disabled.unwrap_or(false),
            })
        })
        .collect();
    let policies: Vec<_> = snapshot
        .policy_groups()
        .into_iter()
        .map(|group| group.name)
        .filter(|name| name != "__MANIS_GLOBAL__")
        .collect();

    json!({
        "rules": rules,
        "kinds": [
            "DOMAIN-SUFFIX", "DOMAIN", "DOMAIN-KEYWORD", "DOMAIN-WILDCARD",
            "IP-CIDR", "IP6-CIDR", "GEOIP", "IP-ASN", "DST-PORT", "FINAL"
        ],
        "policies": policies,
        "warnings": [],
        "summary": format!("{} 条运行时规则", snapshot.rules.len()),
    })
}

/// Recompiles the saved desktop profile and reloads the live controller.
///
/// This is the desktop equivalent of Apply: it makes newly added/removed
/// sources, toggles and saved rules visible to the running Mihomo. The previous
/// generated YAML is restored if the reload is rejected.
///
/// # Errors
/// Returns a profile, render, controller or rollback error.
pub fn nsh_apply_profile() -> Result<(), String> {
    #[cfg(unix)]
    {
        let data_dir = crate::brand::data_dir().ok_or_else(|| "找不到数据目录".to_owned())?;
        let config_dir = crate::brand::config_dir().ok_or_else(|| "找不到配置目录".to_owned())?;
        mihomo::sync_single_node_provider_files(&config_dir, &data_dir)
            .map_err(|error| error.to_string())?;
        let profile =
            mihomo::compile_saved_profile(&config_dir, None).map_err(|error| error.to_string())?;
        let client = live_client().ok_or_else(|| "桌面内核未运行".to_owned())?;
        let snapshot = client.fetch_snapshot().map_err(|error| error.to_string())?;
        let rendered =
            manis_profile::render_mihomo_yaml_with_tun(&profile, snapshot.runtime.tun.enable)
                .map_err(|error| error.to_string())?;
        let previous = std::fs::read_to_string(data_dir.join("mihomo/manis-generated.yaml")).ok();
        if let Err(error) = client.reload_config_payload(&rendered) {
            if let Some(previous) = previous.as_deref() {
                let _ = client.reload_config_payload(previous);
            }
            return Err(error.to_string());
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        Err("桌面控制器只在 Unix 上接入".to_owned())
    }
}

/// Adds a subscription URL to the desktop store.
///
/// # Errors
/// Returns a store/validation error for an invalid URL or failed write.
pub fn nsh_add_subscription(name: &str, url: &str) -> Result<(), String> {
    let directory = crate::brand::config_dir().ok_or_else(|| "找不到配置目录".to_owned())?;
    mihomo::save_subscription_source_with_options_in(
        &directory,
        url.trim(),
        name.trim(),
        mihomo::RemoteSourceRefreshInterval::Manual,
        true,
    )
    .map(|_| ())
    .map_err(|error| error.to_string())
}

/// Adds a single VLESS/share-link node to the desktop store.
///
/// # Errors
/// Returns a store/validation error for an invalid link or failed write.
pub fn nsh_add_node(link: &str) -> Result<(), String> {
    let directory = crate::brand::config_dir().ok_or_else(|| "找不到配置目录".to_owned())?;
    let name = link
        .split_once('#')
        .map_or("手动节点", |(_, name)| name)
        .trim();
    mihomo::save_single_node_source_with_options_in(&directory, link.trim(), name, true)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Enables or disables one saved desktop subscription by list index.
///
/// # Errors
/// Returns a store error when the index is stale or the write fails.
pub fn nsh_toggle_subscription(index: usize, enabled: bool) -> Result<(), String> {
    let directory = crate::brand::config_dir().ok_or_else(|| "找不到配置目录".to_owned())?;
    let sources =
        mihomo::load_subscription_sources_in(&directory).map_err(|error| error.to_string())?;
    let source = sources
        .get(index)
        .ok_or_else(|| "订阅已变化，请刷新".to_owned())?;
    mihomo::update_subscription_source_enabled_in(&directory, &source.id, enabled)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Deletes one saved desktop subscription by list index.
///
/// # Errors
/// Returns a store error when the index is stale or the write fails.
pub fn nsh_delete_subscription(index: usize) -> Result<(), String> {
    let directory = crate::brand::config_dir().ok_or_else(|| "找不到配置目录".to_owned())?;
    let sources =
        mihomo::load_subscription_sources_in(&directory).map_err(|error| error.to_string())?;
    let source = sources
        .get(index)
        .ok_or_else(|| "订阅已变化，请刷新".to_owned())?;
    mihomo::remove_subscription_source_in(&directory, &source.id)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Enables or disables one saved desktop manual node by list index.
///
/// # Errors
/// Returns a store error when the index is stale or the write fails.
pub fn nsh_toggle_node(index: usize, enabled: bool) -> Result<(), String> {
    let directory = crate::brand::config_dir().ok_or_else(|| "找不到配置目录".to_owned())?;
    let nodes =
        mihomo::load_single_node_sources_in(&directory).map_err(|error| error.to_string())?;
    let node = nodes
        .get(index)
        .ok_or_else(|| "节点已变化，请刷新".to_owned())?;
    mihomo::update_single_node_source_enabled_in(&directory, &node.id, enabled)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Deletes one saved desktop manual node by list index.
///
/// # Errors
/// Returns a store error when the index is stale or the write fails.
pub fn nsh_delete_node(index: usize) -> Result<(), String> {
    let directory = crate::brand::config_dir().ok_or_else(|| "找不到配置目录".to_owned())?;
    let nodes =
        mihomo::load_single_node_sources_in(&directory).map_err(|error| error.to_string())?;
    let node = nodes
        .get(index)
        .ok_or_else(|| "节点已变化，请刷新".to_owned())?;
    mihomo::remove_single_node_source_in(&directory, &node.id)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Last lines of the desktop kernel log for the NSH settings page.
#[must_use]
pub fn nsh_logs() -> Vec<String> {
    let Some(path) = crate::brand::data_dir().map(|dir| dir.join("mihomo/manis-core.log")) else {
        return vec!["还没有日志".to_owned()];
    };
    let Ok(contents) = std::fs::read_to_string(path) else {
        return vec!["还没有日志".to_owned()];
    };
    let mut lines: Vec<String> = contents.lines().map(str::to_owned).collect();
    if lines.len() > 300 {
        lines.drain(..lines.len() - 300);
    }
    lines
}

fn mask_url(url: &str) -> String {
    let scheme = url
        .split_once("://")
        .map(|(scheme, _)| scheme)
        .unwrap_or("");
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let host = rest
        .split('/')
        .next()
        .unwrap_or(rest)
        .split('?')
        .next()
        .unwrap_or(rest);
    if scheme.is_empty() {
        host.to_owned()
    } else {
        format!("{scheme}://{host}/…")
    }
}

#[cfg(all(test, not(windows)))]
#[path = "app/tests.rs"]
mod tests;
