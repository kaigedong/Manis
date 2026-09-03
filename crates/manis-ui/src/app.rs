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
mod workspace_inputs;

#[cfg(not(test))]
use crate::core_update;
use configuration::{
    ImportQxRuleError, ManualRuleConditionEditor, ManualRuleEditorState, ManualRulePopover,
    ProxySourceEditorState, QxRuleEditorPopover, QxRuleImportFeedback, QxRuleImportResult,
    QxRuleSourceRefreshState, RuleSourceState,
};
use nodes::{
    GroupBenchmarkNodeState, GroupBenchmarkProgressQueue, GroupBenchmarkState,
    GroupBenchmarkSummary, ManagedPolicyRuntimeState, ManagedPolicyState, PolicyBenchmarkRun,
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
use runtime_lifecycle::{AppUpdateState, KernelSwitchState, MihomoCoreUpdateState};
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

pub struct ManisApp {
    localizer: Localizer,
    primary_workspace: PrimaryWorkspace,
    configuration_section: ConfigurationSection,
    configuration_scroll: gpui::ScrollHandle,
    configuration_navigation_scroll: gpui::ScrollHandle,
    configuration_add_section: Option<ConfigurationSection>,
    configuration_transfer: configuration::ConfigurationTransfer,
    node_workspace: NodeWorkspaceState,
    workspace: PolicyWorkspaceState,
    expanded_policy_group: Option<PolicyGroupId>,
    catalog: Option<PolicyCatalog>,
    runtime: KernelRuntime,
    kernel_switch_state: KernelSwitchState,
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
        Self::start_app_update_polling(cx);
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
        Self {
            localizer,
            primary_workspace: PrimaryWorkspace::default(),
            configuration_section: ConfigurationSection::default(),
            configuration_scroll: gpui::ScrollHandle::new(),
            configuration_navigation_scroll: gpui::ScrollHandle::new(),
            configuration_add_section: None,
            configuration_transfer: configuration::ConfigurationTransfer::default(),
            node_workspace,
            workspace: PolicyWorkspaceState::default(),
            expanded_policy_group: None,
            catalog: None,
            runtime,
            kernel_switch_state: KernelSwitchState::Idle,
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
            Self::Busy => language.localized(copy::app::SWITCHING_STATUS),
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
    match requested {
        ProxyMode::Off | ProxyMode::System => None,
        ProxyMode::Tun => match tun {
            TunSupport::Supported => None,
            TunSupport::KernelUnsupported => Some(ProxyModeBlock::KernelHasNoTun),
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
            .children(gpui_component::Root::render_sheet_layer(window, cx))
            .child(self.status_bar(theme, size_class))
    }
}

#[cfg(all(test, not(windows)))]
#[path = "app/tests.rs"]
mod tests;
