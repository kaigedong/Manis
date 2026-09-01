use super::{
    AnyElement, Button, ButtonVariant, ButtonVariants, Collapsible, Context, Div, FluentBuilder,
    FontWeight, InteractiveElement, IntoElement, Language, MANUAL_RULES_EXPANSION_KEY, ManisApp,
    ParentElement, QxRuleKind, QxRuleList, Radius, RuleGroupRenderContext, Space, Stateful, Styled,
    TextRole, Theme, copy, div, empty_state, mihomo, px, rule_group_is_open,
    rule_source_expansion_key, source_update_label,
};

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
        let active_count = enabled_manual_count + remote_count;
        let disabled_count = disabled_manual_count + disabled_remote_count;
        copy::configuration::active_rule_summary(language, active_count, disabled_count)
    }
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
        );
        let mut list = div()
            .id("active-routing-rules")
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(Space::Sm.px());
        let mut rule_order = 1;
        for (position, group_id) in group_order.iter().enumerate() {
            if group_id == mihomo::MANUAL_ROUTING_RULE_GROUP_ID {
                list = list.child(self.manual_rule_group(
                    disabled_manual_count,
                    &mut rule_order,
                    RuleGroupRenderContext {
                        position,
                        group_count: group_order.len(),
                        compact,
                        language,
                        theme,
                    },
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
                    RuleGroupRenderContext {
                        position,
                        group_count: group_order.len(),
                        compact,
                        language,
                        theme,
                    },
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
        let title = self.rule_group_title(
            mihomo::MANUAL_ROUTING_RULE_GROUP_ID,
            group_name,
            detail,
            None,
            view,
            cx,
        );
        let mut rules = Self::rule_group_rows(compact, theme);
        for (index, rule) in self.manual_rules.iter().enumerate() {
            rules = rules.child(self.manual_routing_rule_row(
                *rule_order,
                index,
                rule,
                theme,
                language,
                cx,
            ));
            *rule_order += 1;
        }
        self.rule_group_card(
            (MANUAL_RULES_EXPANSION_KEY, group_name),
            title,
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
        let detail = Self::remote_rule_group_detail(source, parsed.rules.len(), &target, language);
        let target_select = self.qx_rule_source_target_select(
            source,
            source.enabled && !self.source_refresh_busy(),
            theme,
            cx,
        );
        let title = self.rule_group_title(&source.id, &name, detail, Some(target_select), view, cx);
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
        self.rule_group_card((&expansion_key, &name), title, rules, view, cx)
    }

    fn rule_group_card(
        &self,
        (key, name): (&str, &str),
        title: Div,
        rules: Div,
        view: RuleGroupRenderContext,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let RuleGroupRenderContext {
            compact,
            language,
            theme,
            ..
        } = view;
        let open = rule_group_is_open(&self.node_workspace, key);
        let action = language.localized(if open {
            copy::common::COLLAPSE
        } else {
            copy::common::EXPAND
        });
        let toggle_key = key.to_owned();
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
            .rounded_tl(Radius::Pane.px())
            .rounded_tr(Radius::Pane.px())
            .when(!open, |header| {
                header
                    .rounded_bl(Radius::Pane.px())
                    .rounded_br(Radius::Pane.px())
            })
            .px(if compact { px(12.0) } else { px(16.0) })
            .py_3()
            .bg(theme.surface_low)
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(div().flex_1().min_w(px(0.0)).child(title))
                    .child(crate::components::disclosure_icon(open, theme)),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                Self::sync_rule_group_open(this, &toggle_key, !open, cx);
            }));
        div()
            .id(format!("routing-rule-group-{key}"))
            .flex_shrink_0()
            .rounded(Radius::Pane.px())
            .overflow_hidden()
            .border_1()
            .border_color(theme.outline_subtle)
            .child(Collapsible::new().open(open).child(header).content(rules))
            .into_any_element()
    }

    fn rule_group_title(
        &self,
        group_id: &str,
        name: &str,
        detail: String,
        middle: Option<AnyElement>,
        view: RuleGroupRenderContext,
        cx: &mut Context<Self>,
    ) -> Div {
        let RuleGroupRenderContext {
            position,
            group_count,
            language,
            theme,
            ..
        } = view;
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(Space::Sm.px())
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(
                        div()
                            .text_size(TextRole::Label.size())
                            .font_weight(TextRole::Label.weight())
                            .child(name.to_owned()),
                    )
                    .child(
                        div()
                            .mt(Space::Xs.px())
                            .overflow_x_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(TextRole::Metadata.size())
                            .text_color(theme.text_tertiary)
                            .child(detail),
                    ),
            )
            .when_some(middle, ParentElement::child)
            .child(self.rule_group_order_controls(
                group_id,
                name,
                (position, group_count),
                theme,
                language,
                cx,
            ))
    }

    pub(in crate::app) fn remote_rule_group_detail(
        source: &mihomo::StoredQxRuleSource,
        rule_count: usize,
        target: &str,
        language: Language,
    ) -> String {
        let update = source_update_label(
            source.last_successful_update_unix_secs,
            mihomo::current_unix_secs(),
            language,
        );
        copy::configuration::remote_group_detail(
            language,
            rule_count,
            source.enabled,
            target,
            &update,
        )
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
                    .text_color(theme.action_primary)
                    .child(target.to_owned()),
            )
    }
}
