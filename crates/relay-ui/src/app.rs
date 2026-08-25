use gpui::{
    Context, Div, FontWeight, IntoElement, ParentElement, Render, Role, Stateful, Styled, Toggled,
    Window, div, prelude::*, px,
};
use relay_core::{CompactNavigation, PolicyGroupId, PolicyWorkspaceState, WindowSizeClass};

use crate::{
    demo::{DemoNode, DemoPolicy, node, policies, policy},
    theme::Theme,
};

pub struct RelayApp {
    workspace: PolicyWorkspaceState,
    proxy_enabled: bool,
    inspector_open: bool,
    dark: bool,
    status: String,
}

impl RelayApp {
    #[must_use]
    pub fn new() -> Self {
        Self {
            workspace: PolicyWorkspaceState::demo(),
            proxy_enabled: true,
            inspector_open: false,
            dark: false,
            status: "演示数据 · 尚未连接 Mihomo".to_owned(),
        }
    }

    fn theme(&self) -> Theme {
        if self.dark {
            Theme::dark()
        } else {
            Theme::light()
        }
    }

    fn selected_policy(&self) -> &'static DemoPolicy {
        policy(
            self.workspace
                .selected_group
                .unwrap_or(PolicyGroupId("streaming")),
        )
    }

    fn selected_node(&self) -> DemoNode {
        let policy = self.selected_policy();
        node(
            policy,
            self.workspace.selected_node.unwrap_or(policy.nodes[0].id),
        )
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
                            "已切换到深色主题"
                        } else {
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
                            "演示：系统代理已开启"
                        } else {
                            "演示：系统代理已关闭"
                        }
                        .clone_into(&mut this.status);
                        cx.notify();
                    })),
            )
    }

    fn navigation(theme: Theme, size_class: WindowSizeClass) -> Div {
        let labels = ["概览", "策略组", "规则", "连接", "配置", "日志"];
        let show_labels = size_class == WindowSizeClass::Wide;
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
            .children(labels.into_iter().map(|label| {
                let selected = label == "策略组";
                div()
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
            }))
            .child(div().flex_1())
            .child(
                div()
                    .p_2()
                    .text_size(px(11.0))
                    .text_color(theme.text_tertiary)
                    .child(if show_labels {
                        "Mihomo 未连接 · 演示"
                    } else {
                        "演示"
                    }),
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
        for item in policies() {
            let selected = self.workspace.selected_group == Some(item.id);
            let item_id = item.id;
            rows = rows.child(
                div()
                    .id(format!("policy-{}", item.id.0))
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
                                    .child(format!("{}  {}", item.name, item.rules_count)),
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
                        this.workspace.select_group(item_id);
                        this.status = format!("已打开策略组“{}” · 演示数据", policy(item_id).name);
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
                            .child(Self::small_button("new-policy", "＋ 新建", theme)),
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

    fn node_row(
        item: DemoNode,
        selected: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let node_id = item.id;
        div()
            .id(format!("node-{}", item.id.0))
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
                    .child(item.provider),
            )
            .child(
                div()
                    .w(px(64.0))
                    .text_color(theme.status_success)
                    .child(format!("{} ms", item.latency_ms)),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.workspace.select_node(node_id);
                this.status = format!("已切换到 {} · 尚未写入 Mihomo", item.name);
                cx.notify();
            }))
    }

    #[allow(clippy::too_many_lines)]
    fn detail(&self, theme: Theme, compact: bool, cx: &mut Context<Self>) -> Div {
        let selected_policy = self.selected_policy();
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
        for item in selected_policy.nodes {
            body = body.child(Self::node_row(
                *item,
                item.id == selected_node.id,
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
                        .child(format!("{} 条，按顺序匹配", selected_policy.rules_count)),
                ),
        );
        for rule in selected_policy.rules {
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
                                            .child(selected_policy.name),
                                    )
                                    .child(div().mt_1().text_color(theme.text_secondary).child(
                                        format!(
                                            "{} · {} 个节点 · {} 条规则",
                                            selected_policy.kind,
                                            selected_policy.nodes.len(),
                                            selected_policy.rules_count
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
        let selected_policy = self.selected_policy();
        let selected_node = self.selected_node();
        let domain = if selected_policy.id == PolicyGroupId("search") {
            "openai.com"
        } else {
            "youtube.com"
        };
        let rule_index = selected_policy.rules.first().map_or(18, |rule| rule.index);

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
                            .child(Self::signal_stage("02", "交给策略组", selected_policy.name.to_owned(), format!("{} · 当前选择固定节点", selected_policy.kind), false, theme))
                            .child(Self::signal_stage("03", "最终出口", selected_node.name.to_owned(), format!("{} ms · {}", selected_node.latency_ms, selected_node.provider), false, theme)),
                    )
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
                    .child(div().size(px(8.0)).rounded_full().bg(theme.route_trace))
                    .child("Mihomo 未连接"),
            )
            .child("配置：演示配置")
            .child(self.status.clone())
            .child(div().flex_1())
            .child("↓ 3.4 MB/s")
            .child("↑ 812 KB/s")
    }
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
        let compact = size_class == WindowSizeClass::Compact;
        let show_groups =
            !compact || self.workspace.compact_navigation == CompactNavigation::GroupList;
        let show_detail =
            !compact || self.workspace.compact_navigation == CompactNavigation::GroupDetail;
        let overlay_inspector = size_class != WindowSizeClass::Wide;
        let show_inspector = size_class == WindowSizeClass::Wide || self.inspector_open;

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
                    .child(Self::navigation(theme, size_class))
                    .when(show_groups, |main| {
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
                    .when(show_detail, |main| {
                        main.child(self.detail(theme, compact, cx))
                    })
                    .when(show_inspector, |main| {
                        main.child(self.inspector(theme, overlay_inspector, cx))
                    }),
            )
            .child(self.status_bar(theme))
    }
}
