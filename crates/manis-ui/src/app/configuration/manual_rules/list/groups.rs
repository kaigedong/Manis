use gpui::SharedString;
use gpui_component::{Disableable, IconName, switch::Switch};

use crate::components::{
    ActionRole, StatusTone, card_surface, row_action_button, status_badge, style_action_button,
};
use crate::theme::{ControlSize, MONO_FAMILY};

use super::{
    AnyElement, Button, ButtonVariant, ButtonVariants, Collapsible, Context, Div, FluentBuilder,
    FontWeight, InteractiveElement, IntoElement, Language, MANUAL_RULES_EXPANSION_KEY, ManisApp,
    Message, ParentElement, QxRuleKind, QxRuleList, Radius, RuleGroupRenderContext, RuleRowScope,
    Space, Stateful, Styled, TextRole, Theme, copy, div, empty_state, mihomo, px,
    rule_group_is_open, rule_source_expansion_key, source_update_label,
};

/// Everything a priority-pipeline card header needs beyond the group's own rules.
struct RuleGroupCardView<'a> {
    /// The expansion key and element-id suffix. Unique per group.
    key: &'a str,
    /// The persisted routing group id that a drop or a priority action reorders.
    group_id: &'a str,
    name: &'a str,
    detail: String,
    /// The one policy this group routes to, when every rule in it agrees.
    target: Option<String>,
    /// Group-specific controls, placed between the title and the priority actions.
    controls: Option<AnyElement>,
    /// The group's enable switch. `None` for a group with no group-level state.
    enable_switch: Option<AnyElement>,
}

impl ManisApp {
    pub(in crate::app) fn active_rules_summary(&self, language: Language) -> String {
        let remote_count = self
            .rule_sources
            .sources
            .iter()
            .filter(|source| source.enabled)
            .map(|source| source.rule_count)
            .sum::<usize>();
        let disabled_remote_count = self
            .rule_sources
            .sources
            .iter()
            .filter(|source| !source.enabled)
            .map(|source| source.rule_count)
            .sum::<usize>();
        let enabled_manual_count = self
            .manual_rules
            .iter()
            .filter(|rule| rule.is_enabled())
            .count();
        let disabled_manual_count = self.manual_rules.len() - enabled_manual_count;
        // A disabled group contributes nothing, so all of its rules count as disabled.
        let (enabled_local_count, disabled_local_count) =
            self.local_rule_groups
                .iter()
                .fold((0, 0), |(enabled, disabled), group| {
                    let group_enabled = group.rules.iter().filter(|rule| rule.is_enabled()).count();
                    if group.enabled {
                        (
                            enabled + group_enabled,
                            disabled + group.rules.len() - group_enabled,
                        )
                    } else {
                        (enabled, disabled + group.rules.len())
                    }
                });
        let active_count = enabled_manual_count + remote_count + enabled_local_count;
        let disabled_count = disabled_manual_count + disabled_remote_count + disabled_local_count;
        copy::configuration::active_rule_summary(language, active_count, disabled_count)
    }

    /// The pipeline reads top to bottom: every rule group, then the terminal fallback node.
    pub(in crate::app) fn active_rules_panel(
        &self,
        theme: Theme,
        language: Language,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let disabled_manual_count = self
            .manual_rules
            .iter()
            .filter(|rule| !rule.is_enabled())
            .count();
        let group_order = mihomo::normalized_routing_rule_group_order(
            &self.rule_sources.group_order,
            !self.manual_rules.is_empty(),
            &self.rule_sources.sources,
            &self.local_rule_groups,
        );
        let mut list = div()
            .id("active-routing-rules")
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(10.0));
        let mut rule_order = 1;
        for (position, group_id) in group_order.iter().enumerate() {
            let view = RuleGroupRenderContext {
                position,
                group_count: group_order.len(),
                compact,
                language,
                theme,
            };
            if group_id == mihomo::MANUAL_ROUTING_RULE_GROUP_ID {
                list = list.child(self.manual_rule_group(
                    disabled_manual_count,
                    &mut rule_order,
                    view,
                    cx,
                ));
            } else if let Some((source_index, source)) = self
                .rule_sources
                .sources
                .iter()
                .enumerate()
                .find(|(_, source)| source.id == *group_id)
            {
                list = list.child(self.remote_rule_group(
                    source_index,
                    source,
                    &mut rule_order,
                    view,
                    cx,
                ));
            } else if let Some((group_index, group)) = self
                .local_rule_groups
                .iter()
                .enumerate()
                .find(|(_, group)| group.id == *group_id)
            {
                list = list.child(self.local_rule_group(
                    group_index,
                    group,
                    &mut rule_order,
                    view,
                    cx,
                ));
            }
        }
        if group_order.is_empty() {
            list = list.child(
                empty_state(
                    language.localized(copy::configuration::NO_ROUTING_RULES_YET),
                    language.localized(copy::configuration::ADD_RULES_TO_SEND_MATCHING_CONNECTIONS_THROUGH_A_POLICY_GROUP),
                    None,
                    theme,
                )
                .mt(Space::Lg.px()),
            );
        } else {
            list = list.child(fallback_card(language, theme));
        }
        list
    }

    fn manual_rule_group(
        &self,
        disabled_count: usize,
        rule_order: &mut usize,
        view: RuleGroupRenderContext,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let RuleGroupRenderContext {
            compact,
            language,
            theme,
            ..
        } = view;
        let group_name = language.localized(copy::common::MANUAL_RULES);
        let detail = copy::configuration::manual_group_detail(
            language,
            self.manual_rules.len(),
            disabled_count,
        );
        let mut rules = Self::rule_group_rows(compact, theme);
        for (index, rule) in self.manual_rules.iter().enumerate() {
            rules = rules.child(self.manual_routing_rule_row(
                *rule_order,
                index,
                rule,
                RuleRowScope::Manual,
                theme,
                language,
                cx,
            ));
            *rule_order += 1;
        }
        self.rule_group_card(
            RuleGroupCardView {
                key: MANUAL_RULES_EXPANSION_KEY,
                group_id: mihomo::MANUAL_ROUTING_RULE_GROUP_ID,
                name: group_name,
                detail,
                // Manual rules may route to different policies, so the header only claims a
                // target when every enabled rule in the group shares one.
                target: self.manual_group_target(language),
                controls: None,
                // Individual manual rules own their enabled state; the group has none.
                enable_switch: None,
            },
            rules,
            view,
            cx,
        )
    }

    fn remote_rule_group(
        &self,
        source_index: usize,
        source: &mihomo::StoredQxRuleSource,
        rule_order: &mut usize,
        view: RuleGroupRenderContext,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let RuleGroupRenderContext {
            compact,
            language,
            theme,
            ..
        } = view;
        let parsed = QxRuleList::parse(&source.content);
        let target = self.effective_rule_target(source.target_policy.as_str(), language);
        let name = Self::qx_rule_source_name(source, source_index, language);
        let detail = Self::remote_rule_group_detail(source, parsed.rules.len(), language);
        let interactive = !self.source_refresh_busy();
        let target_select =
            self.qx_rule_source_target_select(source, source.enabled && interactive, theme, cx);
        let enable_switch = rule_group_enable_switch(
            format!("rule-group-remote-enabled-{}", source.id),
            format!("qx-rule-source-enabled-{}", source.id),
            name.clone(),
            source.enabled,
            interactive,
            cx,
            {
                let source_id = source.id.clone();
                move |app, checked, cx| {
                    app.set_qx_rule_source_enabled(source_id.clone(), checked, cx);
                }
            },
        );
        let mut rules = Self::rule_group_rows(compact, theme);
        for rule in parsed.rules {
            rules = rules.child(Self::routing_rule_row(
                *rule_order,
                Self::qx_rule_kind_label(rule.kind),
                &rule.value,
                &target,
                theme,
            ));
            *rule_order += 1;
        }
        let expansion_key = rule_source_expansion_key(&source.id);
        self.rule_group_card(
            RuleGroupCardView {
                key: &expansion_key,
                group_id: &source.id,
                name: &name,
                detail,
                target: Some(target),
                controls: Some(target_select),
                enable_switch: Some(enable_switch),
            },
            rules,
            view,
            cx,
        )
    }

    /// A local group: a name, one target policy and its own rules, no URL anywhere.
    fn local_rule_group(
        &self,
        group_index: usize,
        group: &mihomo::LocalRoutingRuleGroup,
        rule_order: &mut usize,
        view: RuleGroupRenderContext,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let RuleGroupRenderContext {
            compact,
            language,
            theme,
            ..
        } = view;
        let disabled_count = group.rules.iter().filter(|rule| !rule.is_enabled()).count();
        let target = self.effective_rule_target(group.target_policy.as_str(), language);
        let detail = copy::configuration::local_group_detail(
            language,
            group.rules.len(),
            disabled_count,
            group.enabled,
        );
        let controls = self.local_rule_group_header_controls(group_index, cx);
        let interactive =
            !self.routing_apply_state.is_busy() && !self.configuration_transfer.active;
        let enable_switch = rule_group_enable_switch(
            format!("rule-group-local-enabled-{}", group.id),
            format!("local-rule-group-enabled-{}", group.id),
            group.name.clone(),
            group.enabled,
            interactive,
            cx,
            move |app, checked, cx| {
                app.set_local_rule_group_enabled(group_index, checked, cx);
            },
        );
        let mut rules = Self::rule_group_rows(compact, theme);
        for (index, rule) in group.rules.iter().enumerate() {
            rules = rules.child(self.manual_routing_rule_row(
                *rule_order,
                index,
                rule,
                RuleRowScope::Local(group_index),
                theme,
                language,
                cx,
            ));
            *rule_order += 1;
        }
        let add_rule = self.local_rule_group_add_rule(group_index, language, cx);
        let expansion_key = rule_source_expansion_key(&group.id);
        self.rule_group_card(
            RuleGroupCardView {
                key: &expansion_key,
                group_id: &group.id,
                name: &group.name,
                detail,
                target: Some(target),
                controls: Some(controls),
                enable_switch: Some(enable_switch),
            },
            rules.child(add_rule),
            view,
            cx,
        )
    }

    /// The target policy shown on the manual group, when all of its enabled rules share one.
    fn manual_group_target(&self, language: Language) -> Option<String> {
        let mut targets: Vec<String> = Vec::new();
        for rule in self.manual_rules.iter().filter(|rule| rule.is_enabled()) {
            let target = self.effective_rule_target(rule.target(), language);
            if !targets.contains(&target) {
                targets.push(target);
            }
        }
        match targets.as_slice() {
            [single] => Some(single.clone()),
            _ => None,
        }
    }

    fn local_rule_group_header_controls(
        &self,
        group_index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let language = self.language();
        let busy = self.routing_apply_state.is_busy() || self.configuration_transfer.active;
        let controls_enabled = !busy;
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(Space::Sm.px())
            .child(
                row_action_button(
                    format!("local-rule-group-edit-{group_index}"),
                    language.localized(copy::configuration::EDIT_LOCAL_RULE_GROUP),
                    ActionRole::Secondary,
                    ControlSize::Compact,
                )
                .debug_selector(move || format!("local-rule-group-edit-{group_index}"))
                .disabled(!controls_enabled)
                .when(!controls_enabled, gpui::Styled::cursor_default)
                .on_click(cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    this.open_local_rule_group_editor_for_edit(group_index, window, cx);
                })),
            )
            .child(
                row_action_button(
                    format!("local-rule-group-remove-{group_index}"),
                    language.message(Message::Delete),
                    ActionRole::Danger,
                    ControlSize::Compact,
                )
                .debug_selector(move || format!("local-rule-group-remove-{group_index}"))
                .disabled(!controls_enabled)
                .when(!controls_enabled, gpui::Styled::cursor_default)
                .on_click(cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    this.open_local_rule_group_delete(group_index, window, cx);
                })),
            )
            .into_any_element()
    }

    fn rule_group_card(
        &self,
        view: RuleGroupCardView<'_>,
        rules: Div,
        context: RuleGroupRenderContext,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let RuleGroupRenderContext {
            position,
            group_count,
            compact,
            language,
            theme,
        } = context;
        let RuleGroupCardView {
            key,
            group_id,
            name,
            detail,
            target,
            controls,
            enable_switch,
        } = view;
        let open = rule_group_is_open(&self.node_workspace, key);
        let action = language.localized(if open {
            copy::common::COLLAPSE
        } else {
            copy::common::EXPAND
        });
        let toggle_key = key.to_owned();
        let title = rule_group_identity(name, detail, target, theme);
        let header = Button::new(format!("rule-group-toggle-{key}"))
            .map(crate::components::primary_button_interaction)
            .debug_selector({
                let key = key.to_owned();
                move || format!("rule-group-toggle-{key}")
            })
            .with_variant(ButtonVariant::Ghost)
            .accessibility_label(format!("{action} {name}"))
            .cursor_pointer()
            .toggled(open)
            .w_full()
            .h_auto()
            .px(if compact { px(12.0) } else { px(14.0) })
            .py(px(12.0))
            // The card beneath owns the fill; the header stays a transparent hit area.
            .bg(gpui::rgba(0x0000_0000))
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .child(order_badge(format!("{:02}", position + 1), theme))
                    .child(self.rule_group_drag_handle(
                        group_id,
                        name,
                        (position, group_count),
                        theme,
                        language,
                        cx,
                    ))
                    .child(title)
                    .when_some(controls, ParentElement::child)
                    .child(self.rule_group_priority_actions(
                        group_id,
                        (position, group_count),
                        language,
                        cx,
                    ))
                    .when_some(enable_switch, ParentElement::child)
                    .child(crate::components::disclosure_icon(open, theme)),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                Self::sync_rule_group_open(this, &toggle_key, !open, cx);
            }));
        let card = card_surface(theme)
            .id(format!("routing-rule-group-{key}"))
            .flex_shrink_0()
            .overflow_hidden()
            .child(Collapsible::new().open(open).child(header).content(rules));
        self.rule_group_drop_target(div().w_full().child(card), group_id, theme, cx)
    }

    /// Up/down priority actions. Each is disabled at its end of the pipeline and moves exactly
    /// what a drag would move.
    fn rule_group_priority_actions(
        &self,
        group_id: &str,
        (position, group_count): (usize, usize),
        language: Language,
        cx: &mut Context<Self>,
    ) -> Div {
        let interactive =
            !self.routing_apply_state.is_busy() && !self.configuration_transfer.active;
        let mut actions = div().flex_shrink_0().flex().items_center().gap(px(2.0));
        for (direction, enabled, icon, label) in [
            (
                mihomo::MoveDirection::Up,
                interactive && position > 0,
                IconName::ArrowUp,
                priority_action_label(language, mihomo::MoveDirection::Up),
            ),
            (
                mihomo::MoveDirection::Down,
                interactive && position + 1 < group_count,
                IconName::ArrowDown,
                priority_action_label(language, mihomo::MoveDirection::Down),
            ),
        ] {
            let slot = match direction {
                mihomo::MoveDirection::Up => "up",
                mihomo::MoveDirection::Down => "down",
            };
            let selector = format!("rule-group-priority-{slot}-{group_id}");
            let target_id = group_id.to_owned();
            actions = actions.child(
                style_action_button(
                    Button::new(selector.clone()).icon(icon),
                    ActionRole::Secondary,
                    ControlSize::Icon,
                )
                .debug_selector(move || selector.clone())
                .accessibility_label(label)
                .disabled(!enabled)
                .when(!enabled, gpui::Styled::cursor_default)
                .when(!enabled, |button| button.opacity(0.35))
                .block_mouse_except_scroll()
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    if enabled {
                        this.move_routing_rule_group(&target_id, direction, cx);
                    }
                })),
            );
        }
        actions
    }

    pub(in crate::app) fn sync_rule_group_open(
        this: &mut Self,
        key: &str,
        open: bool,
        cx: &mut Context<Self>,
    ) {
        let should_collapse = !open;
        if this.node_workspace.is_group_collapsed(key) != should_collapse {
            this.node_workspace.toggle_group(key);
            this.persist_node_workspace();
            cx.notify();
        }
    }

    pub(in crate::app) fn qx_rule_kind_label(kind: QxRuleKind) -> &'static str {
        match kind {
            QxRuleKind::Domain => "DOMAIN",
            QxRuleKind::DomainKeyword => "DOMAIN-KEYWORD",
            QxRuleKind::DomainSuffix => "DOMAIN-SUFFIX",
        }
    }

    pub(in crate::app) fn remote_rule_group_detail(
        source: &mihomo::StoredQxRuleSource,
        rule_count: usize,
        language: Language,
    ) -> String {
        let update = source_update_label(
            source.last_successful_update_unix_secs,
            mihomo::current_unix_secs(),
            language,
        );
        copy::configuration::remote_group_detail(language, rule_count, source.enabled, &update)
    }

    pub(in crate::app) fn rule_group_rows(compact: bool, theme: Theme) -> Div {
        div()
            .px(if compact {
                Space::Sm.px()
            } else {
                Space::Md.px()
            })
            .pb(Space::Md.px())
            .border_t_1()
            .border_color(theme.outline_subtle)
    }

    /// One imported rule inside an expanded remote group.
    pub(in crate::app) fn routing_rule_row(
        order: usize,
        kind: &'static str,
        value: &str,
        target: &str,
        theme: Theme,
    ) -> Div {
        div()
            .min_h(px(44.0))
            .px(Space::Md.px())
            .py(Space::Sm.px())
            .border_b_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_base)
            .flex()
            .items_center()
            .gap(Space::Md.px())
            .child(
                div()
                    .w(px(42.0))
                    .flex_shrink_0()
                    .font_family(MONO_FAMILY)
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .text_color(theme.text_tertiary)
                    .child(format!("#{order:03}")),
            )
            .child(
                div()
                    .w(px(124.0))
                    .flex_shrink_0()
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text_secondary)
                    .child(kind),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_x_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .font_family(MONO_FAMILY)
                    .text_size(TextRole::Data.size())
                    .line_height(TextRole::Data.line_height())
                    .child(value.to_owned()),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(TextRole::Data.size())
                    .line_height(TextRole::Data.line_height())
                    .font_weight(TextRole::Data.weight())
                    .text_color(theme.route_trace)
                    .child(format!("→ {target}")),
            )
    }
}

/// The card header's identity column: the group name and its target pill, above the detail line.
fn rule_group_identity(name: &str, detail: String, target: Option<String>, theme: Theme) -> Div {
    div()
        .flex_1()
        .min_w(px(0.0))
        .child(
            div()
                .flex()
                .items_center()
                .gap(Space::Sm.px())
                .min_w(px(0.0))
                .child(
                    div()
                        .min_w(px(0.0))
                        .overflow_x_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(TextRole::Label.size())
                        .line_height(TextRole::Label.line_height())
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.text_primary)
                        .child(name.to_owned()),
                )
                .when_some(target, |title, target| {
                    title.child(div().flex_shrink_0().child(status_badge(
                        format!("→ {target}"),
                        StatusTone::Route,
                        theme,
                    )))
                }),
        )
        .child(
            div()
                .mt(px(2.0))
                .overflow_x_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(TextRole::Metadata.size())
                .line_height(TextRole::Metadata.line_height())
                .text_color(theme.text_tertiary)
                .child(detail),
        )
}

/// The pipeline ordinal. Monospaced so reordering never shifts the column.
fn order_badge(text: impl Into<SharedString>, theme: Theme) -> Div {
    div()
        .flex_shrink_0()
        .w(px(26.0))
        .h(px(24.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(Radius::Badge.px())
        .bg(theme.surface_low)
        .font_family(MONO_FAMILY)
        .text_size(px(11.5))
        .line_height(px(16.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme.text_secondary)
        .child(text.into())
}

/// The pipeline terminator: unmatched traffic always leaves through the global exit.
fn fallback_card(language: Language, theme: Theme) -> Stateful<Div> {
    div()
        .id("routing-rule-fallback")
        .debug_selector(|| "routing-rule-fallback".to_owned())
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_between()
        .gap(Space::Md.px())
        .px(Space::Lg.px())
        .py(px(11.0))
        .rounded(Radius::Card.px())
        .border_1()
        .border_color(theme.outline_strong)
        .bg(theme.surface_low)
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(10.0))
                .min_w(px(0.0))
                .child(order_badge("END", theme))
                .child(
                    div()
                        .min_w(px(0.0))
                        .child(
                            div()
                                .text_size(TextRole::Label.size())
                                .line_height(TextRole::Label.line_height())
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme.text_primary)
                                .child(fallback_title(language)),
                        )
                        .child(
                            div()
                                .text_size(TextRole::Metadata.size())
                                .line_height(TextRole::Metadata.line_height())
                                .text_color(theme.text_tertiary)
                                .child(language.localized(
                                    copy::configuration::FALLBACK_FOR_TRAFFIC_NOT_MATCHED_ABOVE,
                                )),
                        ),
                ),
        )
        .child(status_badge(
            format!("→ {}", language.localized(copy::configuration::GLOBAL_EXIT)),
            StatusTone::Route,
            theme,
        ))
}

fn fallback_title(language: Language) -> &'static str {
    match language {
        Language::English => "FINAL · default fallback",
        Language::SimplifiedChinese => "FINAL · 默认兜底",
    }
}

fn priority_action_label(language: Language, direction: mihomo::MoveDirection) -> &'static str {
    match (language, direction) {
        (Language::English, mihomo::MoveDirection::Up) => "Move this group up in priority",
        (Language::English, mihomo::MoveDirection::Down) => "Move this group down in priority",
        (Language::SimplifiedChinese, mihomo::MoveDirection::Up) => "提高这个规则组的优先级",
        (Language::SimplifiedChinese, mihomo::MoveDirection::Down) => "降低这个规则组的优先级",
    }
}

/// A group-level enable switch. `debug_id` keeps the selector a debug test already relies on.
fn rule_group_enable_switch(
    id: String,
    debug_id: String,
    label: String,
    checked: bool,
    interactive: bool,
    cx: &mut Context<ManisApp>,
    on_toggle: impl Fn(&mut ManisApp, bool, &mut Context<ManisApp>) + 'static,
) -> AnyElement {
    let app = cx.entity();
    div()
        .flex_shrink_0()
        .flex()
        .items_center()
        .debug_selector(move || debug_id.clone())
        .child(
            Switch::new(id)
                .checked(checked)
                .disabled(!interactive)
                .accessibility_label(label)
                .on_change(move |checked, _, cx| {
                    cx.stop_propagation();
                    if interactive {
                        let checked = *checked;
                        app.update(cx, |this, cx| on_toggle(this, checked, cx));
                    }
                }),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use gpui::AppContext as _;

    use crate::manual_rule::{ManualRule, ManualRuleKind};

    /// The pipeline ends in the fallback node, and every group offers both priority actions.
    #[gpui::test]
    fn the_pipeline_renders_priority_actions_and_a_final_fallback(cx: &mut gpui::TestAppContext) {
        cx.update(crate::init);
        let mut app = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            let entity = cx.new(|_| {
                let mut app = super::ManisApp::with_fixture_controller("http://127.0.0.1:1");
                app.primary_workspace = manis_core::PrimaryWorkspace::RoutingRules;
                app.manual_rules = vec![
                    ManualRule::parse(ManualRuleKind::HostSuffix, "example.com", "DIRECT")
                        .expect("rule"),
                ];
                app
            });
            app = Some(entity.clone());
            crate::root(entity, window, cx)
        });
        let _app = app.expect("fixture app");
        cx.update(|window, cx| window.draw(cx).clear(cx));

        for selector in [
            "rule-group-toggle-routing-manual-rules",
            "rule-group-priority-up-manual",
            "rule-group-priority-down-manual",
            "routing-rule-fallback",
        ] {
            assert!(cx.debug_bounds(selector).is_some(), "{selector}");
        }
    }
}
