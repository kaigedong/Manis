use gpui::{
    Context, Div, Entity, FontWeight, IntoElement, ParentElement, Render, Role, Stateful, Styled,
    Subscription, Toggled, Window, div, prelude::*, px,
};
use relay_core::{
    CompactNavigation, ConfigurationWorkspaceState, PolicyCatalog, PolicyGroup, PolicyNode,
    PolicyWorkspaceState, PrimaryWorkspace, ProxyId, WindowSizeClass,
};
use relay_mihomo::ObservedRouteEvidence;

use crate::{
    demo,
    diagnostics::{UiEvent, trace_ui},
    mihomo::{
        self, ControllerRuntime, ControllerState, LoadedProvider, LoadedSnapshot,
        SubscriptionPreviewError,
    },
    subscription::{SourceKind, SubscriptionInputError, SubscriptionPreview},
    subscription_input::{SubscriptionInputChanged, SubscriptionTextInput},
    theme::Theme,
};

mod configuration;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
enum SubscriptionFeedback {
    #[default]
    Idle,
    Loading(SourceKind),
    Valid(SubscriptionPreview),
    InvalidInput(SubscriptionInputError),
    PreviewFailed(SubscriptionPreviewError),
}

pub struct RelayApp {
    primary_workspace: PrimaryWorkspace,
    configuration: ConfigurationWorkspaceState,
    workspace: PolicyWorkspaceState,
    catalog: PolicyCatalog,
    runtime: ControllerRuntime,
    controller: ControllerState,
    observed_routes: Vec<ObservedRouteEvidence>,
    source_providers: Vec<LoadedProvider>,
    subscription_preview_providers: Vec<LoadedProvider>,
    subscription_preview_generation: u64,
    proxy_enabled: bool,
    inspector_open: bool,
    dark: bool,
    status: String,
    subscription_input: Option<Entity<SubscriptionTextInput>>,
    subscription_feedback: SubscriptionFeedback,
    subscription_input_events: Option<Subscription>,
}

impl RelayApp {
    #[must_use]
    pub fn new() -> Self {
        Self::with_runtime(mihomo::configured_runtime())
    }

    #[must_use]
    pub fn with_controller(endpoint: impl Into<String>) -> Self {
        Self::with_runtime(ControllerRuntime::External {
            endpoint: endpoint.into(),
        })
    }

    fn with_runtime(runtime: ControllerRuntime) -> Self {
        let status = runtime.initial_status();
        Self {
            primary_workspace: PrimaryWorkspace::default(),
            configuration: ConfigurationWorkspaceState::default(),
            workspace: PolicyWorkspaceState::demo(),
            catalog: demo::catalog(),
            runtime,
            controller: ControllerState::Demo,
            observed_routes: Vec::new(),
            source_providers: Vec::new(),
            subscription_preview_providers: Vec::new(),
            subscription_preview_generation: 0,
            proxy_enabled: true,
            inspector_open: false,
            dark: false,
            status,
            subscription_input: None,
            subscription_feedback: SubscriptionFeedback::Idle,
            subscription_input_events: None,
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
                this.subscription_preview_providers.clear();
                this.subscription_preview_generation =
                    this.subscription_preview_generation.wrapping_add(1);
                cx.notify();
            }
        });
        self.subscription_input = Some(input);
        self.subscription_input_events = Some(events);
    }

    fn preview_remote_subscription(
        &mut self,
        input: String,
        preview: SubscriptionPreview,
        cx: &mut Context<Self>,
    ) {
        self.subscription_preview_generation = self.subscription_preview_generation.wrapping_add(1);
        let generation = self.subscription_preview_generation;
        self.subscription_preview_providers.clear();
        self.subscription_feedback = SubscriptionFeedback::Loading(preview.kind);
        "正在隔离的 Mihomo 中下载并解析订阅节点".clone_into(&mut self.status);
        trace_ui(UiEvent::SourcePreviewStarted);

        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move { mihomo::preview_subscription(&input) })
                .await;
            this.update(cx, |this, cx| {
                if this.subscription_preview_generation != generation {
                    return;
                }
                match result {
                    Ok(providers) => {
                        let node_count: usize =
                            providers.iter().map(|provider| provider.nodes.len()).sum();
                        let provider_count = providers.len();
                        this.subscription_preview_providers = providers;
                        this.subscription_feedback = SubscriptionFeedback::Valid(preview);
                        this.status = format!(
                            "订阅预览完成 · {provider_count} 个来源 · {node_count} 个节点 · 尚未保存"
                        );
                        trace_ui(UiEvent::SourcePreviewSucceeded);
                    }
                    Err(error) => {
                        this.subscription_feedback = SubscriptionFeedback::PreviewFailed(error);
                        this.status = format!("订阅预览失败：{error}");
                        trace_ui(UiEvent::SourcePreviewFailed);
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
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

    fn selected_node(&self) -> PolicyNode {
        let policy = self.selected_policy();
        self.workspace
            .selected_node
            .as_ref()
            .and_then(|selected| policy.nodes.iter().find(|node| node.id == *selected))
            .or_else(|| policy.nodes.first())
            .cloned()
            .unwrap_or_else(|| PolicyNode {
                id: ProxyId::new("unavailable"),
                name: "暂无可用节点".to_owned(),
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
                    Ok(result) => this.apply_mihomo_snapshot(result.endpoint, result.snapshot),
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

    #[allow(clippy::too_many_lines)]
    fn chrome(&self, theme: Theme, size_class: WindowSizeClass, cx: &mut Context<Self>) -> Div {
        let compact = size_class == WindowSizeClass::Compact;
        let proxy_label = match (compact, self.proxy_enabled) {
            (true, true) => "代理 · 开",
            (true, false) => "代理 · 关",
            (false, true) => "系统代理 · 开",
            (false, false) => "系统代理 · 关",
        };
        let theme_label = match (compact, self.dark) {
            (true, true) => "浅",
            (true, false) => "深",
            (false, true) => "浅色",
            (false, false) => "深色",
        };

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
            .child(
                div()
                    .id("system-proxy")
                    .role(Role::Switch)
                    .aria_label("系统代理")
                    .aria_toggled(if self.proxy_enabled {
                        Toggled::True
                    } else {
                        Toggled::False
                    })
                    .tab_stop(true)
                    .focusable()
                    .cursor_pointer()
                    .h(px(34.0))
                    .px_3()
                    .rounded_md()
                    .border_1()
                    .border_color(if self.proxy_enabled {
                        theme.action_primary
                    } else {
                        theme.outline_subtle
                    })
                    .bg(if self.proxy_enabled {
                        theme.action_primary
                    } else {
                        theme.surface_high
                    })
                    .text_color(if self.proxy_enabled {
                        theme.action_on_primary
                    } else {
                        theme.text_primary
                    })
                    .flex()
                    .items_center()
                    .child(proxy_label)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.proxy_enabled = !this.proxy_enabled;
                        if this.proxy_enabled {
                            trace_ui(UiEvent::SystemProxyEnabled);
                            "演示：系统代理已开启"
                        } else {
                            trace_ui(UiEvent::SystemProxyDisabled);
                            "演示：系统代理已关闭"
                        }
                        .clone_into(&mut this.status);
                        cx.notify();
                    })),
            )
    }

    fn navigation(&self, theme: Theme, size_class: WindowSizeClass, cx: &mut Context<Self>) -> Div {
        let entries = [
            ("策略组", PrimaryWorkspace::Policies),
            ("配置", PrimaryWorkspace::Configuration),
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
            .children(entries.into_iter().map(|(label, workspace)| {
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
                    .child(if show_labels { label } else { &label[..3] })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.primary_workspace = workspace;
                        this.status = match workspace {
                            PrimaryWorkspace::Policies => {
                                trace_ui(UiEvent::WorkspacePoliciesOpened);
                                "已打开策略组工作区".to_owned()
                            }
                            PrimaryWorkspace::Configuration => {
                                trace_ui(UiEvent::WorkspaceConfigurationOpened);
                                "已打开安全配置预览".to_owned()
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
                    .child(div().w(px(3.0)).h(px(44.0)).rounded_full().bg(if selected {
                        theme.route_trace
                    } else {
                        theme.outline_strong
                    }))
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
                                    .child(item.kind),
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

    fn node_row(
        item: PolicyNode,
        selected: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let node_id = item.id.clone();
        let node_name = item.name.clone();
        let provider = item
            .provider
            .clone()
            .unwrap_or_else(|| "内置节点".to_owned());
        let latency = item
            .latency_ms
            .map_or_else(|| "—".to_owned(), |latency| format!("{latency} ms"));
        div()
            .id(format!("node-{}", item.id.as_str()))
            .role(Role::RadioButton)
            .aria_toggled(if selected {
                Toggled::True
            } else {
                Toggled::False
            })
            .tab_stop(true)
            .focusable()
            .cursor_pointer()
            .min_h(px(64.0))
            .px_3()
            .flex()
            .items_center()
            .gap_3()
            .rounded_md()
            .border_1()
            .border_color(if selected {
                theme.action_primary
            } else {
                theme.outline_subtle
            })
            .bg(if selected {
                theme.action_soft
            } else {
                theme.surface_high
            })
            .child(
                div()
                    .size(px(18.0))
                    .rounded_full()
                    .border_2()
                    .border_color(if selected {
                        theme.action_primary
                    } else {
                        theme.outline_strong
                    })
                    .when(selected, |dot| dot.bg(theme.action_primary)),
            )
            .child(
                div()
                    .flex_1()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(item.name))
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
                    .text_color(theme.text_secondary)
                    .child(provider),
            )
            .child(
                div()
                    .w(px(64.0))
                    .text_color(theme.status_success)
                    .child(latency),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.workspace.select_node(node_id.clone());
                trace_ui(UiEvent::PolicyPreviewOpened);
                this.status = format!("已选择 {node_name} · 只读模式未写入 Mihomo");
                cx.notify();
            }))
    }

    #[allow(clippy::too_many_lines)]
    fn detail(&self, theme: Theme, compact: bool, cx: &mut Context<Self>) -> Div {
        let selected_policy = self.selected_policy().clone();
        let selected_node = self.selected_node();
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
                .child(
                    div()
                        .text_color(theme.text_secondary)
                        .child("选择此策略当前使用的出口节点"),
                )
                .child(Self::small_button("add-node", "＋ 添加节点", theme)),
        );
        body = body.child(
            div()
                .mt_2()
                .px_3()
                .flex()
                .text_size(px(11.0))
                .text_color(theme.text_tertiary)
                .child(div().flex_1().child("节点"))
                .child(div().w(px(100.0)).child("来源"))
                .child(div().w(px(64.0)).child("延迟")),
        );
        for item in selected_policy.nodes.iter().cloned() {
            let selected = item.id == selected_node.id;
            body = body.child(Self::node_row(item, selected, theme, cx));
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
                                            selected_policy.kind,
                                            selected_policy.nodes.len(),
                                            selected_policy.rules_count()
                                        ),
                                    )),
                            )
                            .child(Self::small_button("latency-test", "测速", theme))
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
                            .child(Self::signal_stage("02", "交给策略组", selected_policy.name.clone(), format!("{} · 当前选择固定节点", selected_policy.kind), false, theme))
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
        let compact = size_class == WindowSizeClass::Compact;
        let show_groups =
            !compact || self.workspace.compact_navigation == CompactNavigation::GroupList;
        let show_detail =
            !compact || self.workspace.compact_navigation == CompactNavigation::GroupDetail;
        let overlay_inspector = size_class != WindowSizeClass::Wide;
        let show_inspector = size_class == WindowSizeClass::Wide || self.inspector_open;
        let policies_active = self.primary_workspace == PrimaryWorkspace::Policies;

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
                    .when(!policies_active, |main| {
                        main.child(self.configuration_workspace(theme, size_class, cx))
                    })
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
