use gpui::{Context, Div, FontWeight, ParentElement, Role, Stateful, Styled, div, prelude::*, px};
use relay_core::{ConfigurationSection, WindowSizeClass};

use super::RelayApp;
use crate::{
    diagnostics::{UiEvent, trace_ui},
    theme::Theme,
};

const RULE_COUNT: usize = 2;

impl RelayApp {
    pub(super) fn configuration_workspace(
        &self,
        theme: Theme,
        size_class: WindowSizeClass,
        cx: &mut Context<Self>,
    ) -> Div {
        let compact = size_class == WindowSizeClass::Compact;
        let wide = size_class == WindowSizeClass::Wide;
        let mut tabs = div().flex().gap_1();
        for (section, label) in [
            (ConfigurationSection::Sources, "订阅源"),
            (ConfigurationSection::Groups, "策略组"),
            (ConfigurationSection::Rules, "规则顺序"),
        ] {
            tabs = tabs.child(self.configuration_tab(section, label, theme, cx));
        }

        let content = if wide {
            div()
                .id("configuration-wide")
                .flex_1()
                .min_h(px(0.0))
                .p_4()
                .flex()
                .items_start()
                .gap_3()
                .child(self.source_panel(theme, cx).w(px(270.0)).flex_shrink_0())
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .flex()
                        .flex_col()
                        .gap_3()
                        .child(Self::group_panel(theme, cx))
                        .child(self.rule_panel(theme, cx)),
                )
                .child(self.route_probe(theme, false).w(px(300.0)).flex_shrink_0())
        } else {
            let active = match self.configuration.section {
                ConfigurationSection::Sources => self.source_panel(theme, cx),
                ConfigurationSection::Groups => Self::group_panel(theme, cx),
                ConfigurationSection::Rules => self.rule_panel(theme, cx),
            };
            div()
                .id("configuration-scroll")
                .flex_1()
                .min_h(px(0.0))
                .overflow_y_scroll()
                .p(if compact { px(12.0) } else { px(16.0) })
                .pb_8()
                .flex()
                .flex_col()
                .gap_3()
                .child(self.configuration_flow_strip(theme, cx))
                .child(active)
                .child(self.route_probe(theme, true))
        };

        let header = Self::configuration_header(theme, compact, wide, tabs);
        div()
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .flex()
            .flex_col()
            .bg(theme.surface_base)
            .child(header)
            .child(content)
    }

    fn configuration_header(theme: Theme, compact: bool, wide: bool, tabs: Div) -> Div {
        div()
            .px(if compact { px(12.0) } else { px(20.0) })
            .py_3()
            .flex_shrink_0()
            .border_b_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_high)
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .flex_1()
                            .child(
                                div()
                                    .text_size(px(19.0))
                                    .font_weight(FontWeight::BOLD)
                                    .child("Operate · 配置工作区"),
                            )
                            .when(!compact, |header| {
                                header.child(div().mt_1().text_color(theme.text_secondary).child(
                                    "订阅源 → 策略组 → 有序规则；当前仅预览，不写入运行中的 Mihomo",
                                ))
                            }),
                    )
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .rounded_sm()
                            .bg(theme.action_soft)
                            .text_size(px(10.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.action_primary)
                            .child("安全预览"),
                    ),
            )
            .when(wide, |header| header.child(div().mt_3().child(tabs)))
    }

    fn configuration_flow_strip(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let mut strip = div().flex().gap_2();
        for (section, index, label, detail) in [
            (ConfigurationSection::Sources, "01", "订阅源", "安全来源"),
            (ConfigurationSection::Groups, "02", "策略组", "3 个出口"),
            (ConfigurationSection::Rules, "03", "规则", "2 条有序"),
        ] {
            strip =
                strip.child(self.configuration_flow_step(section, index, label, detail, theme, cx));
        }
        strip
    }

    #[allow(clippy::too_many_arguments)]
    fn configuration_flow_step(
        &self,
        section: ConfigurationSection,
        index: &'static str,
        label: &'static str,
        detail: &'static str,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let selected = self.configuration.section == section;
        div()
            .id(format!("configuration-step-{index}"))
            .role(Role::Button)
            .tab_stop(true)
            .focusable()
            .cursor_pointer()
            .min_w(px(0.0))
            .flex_1()
            .p_3()
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
                    .text_size(px(10.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(if selected {
                        theme.action_primary
                    } else {
                        theme.text_tertiary
                    })
                    .child(format!("{index} · {label}")),
            )
            .child(
                div()
                    .mt_1()
                    .text_size(px(11.0))
                    .text_color(theme.text_secondary)
                    .child(detail),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                let event = match section {
                    ConfigurationSection::Sources => {
                        this.configuration.open_source_diagnostics();
                        "已展开订阅源安全诊断 · 未读取凭据".clone_into(&mut this.status);
                        UiEvent::SourceDiagnosticsOpened
                    }
                    ConfigurationSection::Groups => {
                        this.configuration.select_section(section);
                        this.status = format!("配置预览 · {label}");
                        UiEvent::ConfigurationGroupsOpened
                    }
                    ConfigurationSection::Rules => {
                        this.configuration.select_section(section);
                        this.status = format!("配置预览 · {label}");
                        UiEvent::ConfigurationRulesOpened
                    }
                };
                trace_ui(event);
                cx.notify();
            }))
    }

    fn configuration_tab(
        &self,
        section: ConfigurationSection,
        label: &'static str,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let selected = self.configuration.section == section;
        div()
            .id(format!("configuration-tab-{label}"))
            .role(Role::Button)
            .tab_stop(true)
            .focusable()
            .cursor_pointer()
            .h(px(32.0))
            .px_3()
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
            .text_color(if selected {
                theme.action_primary
            } else {
                theme.text_secondary
            })
            .font_weight(if selected {
                FontWeight::SEMIBOLD
            } else {
                FontWeight::NORMAL
            })
            .flex()
            .items_center()
            .child(label)
            .on_click(cx.listener(move |this, _, _, cx| {
                let event = match section {
                    ConfigurationSection::Sources => {
                        this.configuration.open_source_diagnostics();
                        "已展开订阅源安全诊断 · 未读取凭据".clone_into(&mut this.status);
                        UiEvent::SourceDiagnosticsOpened
                    }
                    ConfigurationSection::Groups => {
                        this.configuration.select_section(section);
                        this.status = format!("配置预览 · {label}");
                        UiEvent::ConfigurationGroupsOpened
                    }
                    ConfigurationSection::Rules => {
                        this.configuration.select_section(section);
                        this.status = format!("配置预览 · {label}");
                        UiEvent::ConfigurationRulesOpened
                    }
                };
                trace_ui(event);
                cx.notify();
            }))
    }

    fn source_panel(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let source = self.runtime.profile_source();
        let diagnostics_open = self.configuration.source_diagnostics_open;
        let panel = div()
            .id("configuration-source")
            .role(Role::Button)
            .aria_label(if diagnostics_open {
                "收起订阅源安全诊断"
            } else {
                "查看订阅源安全诊断"
            })
            .tab_stop(true)
            .focusable()
            .cursor_pointer()
            .min_h(px(190.0))
            .p_4()
            .rounded_md()
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_high)
            .child(
                div()
                    .text_size(px(11.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text_tertiary)
                    .child("01 · 订阅源"),
            )
            .child(
                div()
                    .mt_4()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .size(px(12.0))
                            .rounded_full()
                            .border_2()
                            .border_color(theme.action_primary),
                    )
                    .child(
                        div()
                            .text_size(px(16.0))
                            .font_weight(FontWeight::BOLD)
                            .child(source.label()),
                    ),
            )
            .child(
                div()
                    .mt_2()
                    .text_color(theme.text_secondary)
                    .child(source.detail()),
            )
            .child(
                div()
                    .mt_5()
                    .pt_3()
                    .border_t_1()
                    .border_color(theme.outline_subtle)
                    .text_size(px(11.0))
                    .text_color(theme.text_tertiary)
                    .child("刷新周期 24 小时 · 健康检查 10 分钟"),
            )
            .when(diagnostics_open, |panel| {
                panel.child(Self::source_diagnostics(theme, source.label()))
            })
            .child(
                div()
                    .mt_2()
                    .px_2()
                    .py_1()
                    .rounded_sm()
                    .bg(theme.surface_low)
                    .text_size(px(10.0))
                    .text_color(theme.text_secondary)
                    .child(if diagnostics_open {
                        "订阅 URL、token 与本机路径均不显示 · 点击收起"
                    } else {
                        "订阅 URL、token 与本机路径均不显示 · 点击查看诊断"
                    }),
            )
            .on_click(cx.listener(|this, _, _, cx| {
                let was_open = this.configuration.source_diagnostics_open;
                this.configuration.toggle_source_diagnostics();
                if was_open {
                    "已收起订阅源安全诊断".clone_into(&mut this.status);
                    trace_ui(UiEvent::SourceDiagnosticsClosed);
                } else {
                    "已展开订阅源安全诊断 · 未读取凭据".clone_into(&mut this.status);
                    trace_ui(UiEvent::SourceDiagnosticsOpened);
                }
                cx.notify();
            }));
        div().w_full().child(panel)
    }

    fn source_diagnostics(theme: Theme, source_label: &'static str) -> Div {
        let mut diagnostics = div()
            .mt_3()
            .pt_3()
            .border_t_1()
            .border_color(theme.outline_subtle)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.action_primary)
                    .child(div().size(px(7.0)).rounded_full().bg(theme.action_primary))
                    .child("安全诊断已展开"),
            );
        for detail in [
            format!("来源模式 · {source_label}"),
            "凭据状态 · 已隐藏，未进入界面状态".to_owned(),
            "当前能力 · 只读预览，未启用配置写入".to_owned(),
            "调试日志 · 仅记录事件名，不记录订阅值".to_owned(),
        ] {
            diagnostics = diagnostics.child(
                div()
                    .mt_1()
                    .text_size(px(11.0))
                    .text_color(theme.text_secondary)
                    .child(detail),
            );
        }
        diagnostics
    }

    fn group_panel(theme: Theme, cx: &mut Context<Self>) -> Div {
        div()
            .p_4()
            .rounded_md()
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_high)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(11.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text_tertiary)
                            .child("02 · 策略组"),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme.text_tertiary)
                            .child("QX 风格预设"),
                    ),
            )
            .child(Self::group_row(
                "Auto",
                "URL Test",
                "订阅节点 · 自动优选",
                false,
                theme,
                cx,
            ))
            .child(Self::group_row(
                "Proxy",
                "Select",
                "Auto / DIRECT / 订阅节点",
                true,
                theme,
                cx,
            ))
            .child(Self::group_row(
                "DIRECT",
                "Builtin",
                "内置直连出口",
                false,
                theme,
                cx,
            ))
    }

    fn group_row(
        name: &'static str,
        kind: &'static str,
        detail: &'static str,
        primary: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        div()
            .id(format!("configuration-group-{name}"))
            .role(Role::Button)
            .tab_stop(true)
            .focusable()
            .cursor_pointer()
            .mt_2()
            .min_h(px(54.0))
            .px_3()
            .rounded_md()
            .border_1()
            .border_color(if primary {
                theme.action_primary
            } else {
                theme.outline_subtle
            })
            .bg(if primary {
                theme.action_soft
            } else {
                theme.surface_low
            })
            .flex()
            .items_center()
            .gap_3()
            .child(div().w(px(3.0)).h(px(30.0)).rounded_full().bg(if primary {
                theme.action_primary
            } else {
                theme.outline_strong
            }))
            .child(
                div()
                    .flex_1()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(name))
                    .child(
                        div()
                            .mt_1()
                            .text_size(px(11.0))
                            .text_color(theme.text_secondary)
                            .child(detail),
                    ),
            )
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(theme.text_tertiary)
                    .child(kind),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.configuration
                    .select_section(ConfigurationSection::Groups);
                trace_ui(UiEvent::PolicyPreviewOpened);
                this.status = format!("策略组预览 · {name} · 尚未写入 Mihomo");
                cx.notify();
            }))
    }

    fn rule_panel(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        div()
            .p_4()
            .rounded_md()
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_high)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(11.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text_tertiary)
                            .child("03 · 规则顺序"),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme.text_tertiary)
                            .child("从上到下，首条命中"),
                    ),
            )
            .child(self.rule_row(0, "GEOIP", "CN · no-resolve", "DIRECT", theme, cx))
            .child(self.rule_row(1, "MATCH", "其余流量", "Proxy", theme, cx))
    }

    #[allow(clippy::too_many_arguments)]
    fn rule_row(
        &self,
        index: usize,
        kind: &'static str,
        payload: &'static str,
        policy: &'static str,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let selected = self.configuration.selected_rule == index;
        div()
            .id(format!("configuration-rule-{index}"))
            .role(Role::Button)
            .tab_stop(true)
            .focusable()
            .cursor_pointer()
            .mt_2()
            .min_h(px(50.0))
            .px_3()
            .rounded_md()
            .border_1()
            .border_color(if selected {
                theme.route_trace
            } else {
                theme.outline_subtle
            })
            .bg(theme.surface_low)
            .flex()
            .items_center()
            .gap_3()
            .child(
                div()
                    .w(px(30.0))
                    .text_color(theme.text_tertiary)
                    .child(format!("#{:02}", index + 1)),
            )
            .child(
                div()
                    .flex_1()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(kind))
                    .child(
                        div()
                            .mt_1()
                            .text_size(px(11.0))
                            .text_color(theme.text_secondary)
                            .child(payload),
                    ),
            )
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(if selected {
                        theme.route_trace
                    } else {
                        theme.text_primary
                    })
                    .child(policy),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.configuration.select_rule(index, RULE_COUNT);
                trace_ui(UiEvent::RulePreviewOpened);
                this.status = format!("规则 #{:02} → {policy} · 配置预览", index + 1);
                cx.notify();
            }))
    }

    fn route_probe(&self, theme: Theme, condensed: bool) -> Div {
        let (rule, policy, exit) = if self.configuration.selected_rule == 0 {
            ("#01 · GEOIP, CN", "DIRECT", "内置直连")
        } else {
            ("#02 · MATCH", "Proxy", "Auto / 订阅节点")
        };
        let stages = if condensed {
            div()
                .mt_3()
                .flex()
                .items_center()
                .gap_2()
                .child(Self::probe_chip("规则", rule, theme))
                .child(
                    div()
                        .w(px(22.0))
                        .h(px(2.0))
                        .flex_shrink_0()
                        .bg(theme.route_trace),
                )
                .child(Self::probe_chip("策略", policy, theme))
                .child(
                    div()
                        .w(px(22.0))
                        .h(px(2.0))
                        .flex_shrink_0()
                        .bg(theme.route_trace),
                )
                .child(Self::probe_chip("出口", exit, theme))
        } else {
            div()
                .child(Self::probe_stage("规则", rule, true, theme))
                .child(Self::probe_stage("策略", policy, true, theme))
                .child(Self::probe_stage("出口", exit, false, theme))
        };

        div()
            .p_4()
            .rounded_md()
            .border_1()
            .border_color(theme.route_trace)
            .bg(theme.surface_high)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(11.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.route_trace)
                            .child("ROUTE PROBE"),
                    )
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .rounded_sm()
                            .bg(theme.route_soft)
                            .text_size(px(10.0))
                            .text_color(theme.route_trace)
                            .child("本地配置预览"),
                    ),
            )
            .child(stages)
            .child(
                div()
                    .mt_3()
                    .pt_3()
                    .border_t_1()
                    .border_color(theme.outline_subtle)
                    .text_size(px(11.0))
                    .text_color(theme.text_secondary)
                    .child("铜色只表示依赖路径；这不是 Mihomo 的实时命中结果。"),
            )
    }

    fn probe_chip(label: &'static str, value: &'static str, theme: Theme) -> Div {
        div()
            .min_w(px(0.0))
            .flex_1()
            .p_2()
            .rounded_md()
            .bg(theme.route_soft)
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(theme.route_trace)
                    .child(label),
            )
            .child(div().mt_1().font_weight(FontWeight::SEMIBOLD).child(value))
    }

    fn probe_stage(label: &'static str, value: &'static str, tail: bool, theme: Theme) -> Div {
        div()
            .mt_3()
            .flex()
            .gap_3()
            .child(
                div()
                    .w(px(18.0))
                    .flex()
                    .flex_col()
                    .items_center()
                    .child(div().size(px(10.0)).rounded_full().bg(theme.route_trace))
                    .when(tail, |rail| {
                        rail.child(div().mt_1().w(px(2.0)).h(px(28.0)).bg(theme.route_trace))
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme.text_tertiary)
                            .child(label),
                    )
                    .child(div().mt_1().font_weight(FontWeight::SEMIBOLD).child(value)),
            )
    }
}
