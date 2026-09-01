use super::{
    ActionRole, BTreeMap, Checkbox, Context, ControlSize, Div, FluentBuilder, FontWeight,
    InteractiveElement, Language, ManagedPolicyDraft, ManagedPolicyIcon, ManagedPolicyStrategy,
    ManisApp, Message, ParentElement, PolicyCandidateMatcherKind, PolicyEditorPopover, Stateful,
    StatefulInteractiveElement, Styled, TextRole, Theme, action_button, copy, div, px,
};

impl ManisApp {
    pub(in crate::app) fn policy_strategy_menu(
        draft: &ManagedPolicyDraft,
        _language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let mut choices = div().id("policy-strategy-choices");
        for (strategy, technical) in [
            (ManagedPolicyStrategy::Manual, "static"),
            (
                ManagedPolicyStrategy::LowestLatency,
                "url-latency-benchmark",
            ),
        ] {
            let selected = draft.strategy == strategy;
            choices = choices.child(Self::policy_choice_row(
                format!("policy-group-strategy-{}", strategy.key()),
                technical,
                selected,
                theme,
                cx.listener(move |this, _, _, cx| {
                    if let Some(draft) = this.managed_policies.draft.as_mut() {
                        draft.strategy = strategy;
                    }
                    this.managed_policies.editor_popover = None;
                    cx.notify();
                }),
            ));
        }
        choices
    }

    pub(in crate::app) fn policy_icon_menu(
        draft: &ManagedPolicyDraft,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let mut choices = div().id("policy-icon-choices");
        for icon in [
            ManagedPolicyIcon::None,
            ManagedPolicyIcon::Bolt,
            ManagedPolicyIcon::Globe,
            ManagedPolicyIcon::Shield,
            ManagedPolicyIcon::Compass,
        ] {
            let selected = draft.icon == icon;
            choices = choices.child(Self::policy_icon_choice_row(
                format!("policy-group-icon-{}", icon.key()),
                icon,
                Self::managed_policy_icon_label(icon, language),
                selected,
                theme,
                cx.listener(move |this, _, _, cx| {
                    if let Some(draft) = this.managed_policies.draft.as_mut() {
                        draft.icon = icon;
                    }
                    this.managed_policies.editor_popover = None;
                    cx.notify();
                }),
            ));
        }
        choices
    }

    pub(in crate::app) fn policy_candidate_mode_menu(
        draft: &ManagedPolicyDraft,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let mut choices = div().id("policy-candidate-mode-choices");
        for (matcher, title) in [
            (
                PolicyCandidateMatcherKind::All,
                language.localized(copy::nodes::ALL_NODES),
            ),
            (
                PolicyCandidateMatcherKind::NameContains,
                language.localized(copy::nodes::NAME_CONTAINS),
            ),
            (
                PolicyCandidateMatcherKind::Explicit,
                language.localized(copy::nodes::SELECT_NODES_OR_GROUPS),
            ),
        ] {
            let selected = draft.matcher_kind == matcher;
            choices = choices.child(Self::policy_choice_row(
                format!("policy-group-matcher-{matcher:?}"),
                title,
                selected,
                theme,
                cx.listener(move |this, _, _, cx| {
                    if let Some(draft) = this.managed_policies.draft.as_mut() {
                        draft.matcher_kind = matcher;
                    }
                    this.managed_policies.editor_popover = (matcher
                        == PolicyCandidateMatcherKind::Explicit)
                        .then_some(PolicyEditorPopover::CandidateNodes);
                    cx.notify();
                }),
            ));
        }
        choices
    }

    pub(in crate::app) fn policy_candidate_menu(
        &self,
        draft: &ManagedPolicyDraft,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let inventory = self.policy_candidate_inventory();
        let has_local_sources =
            !self.imported_subscriptions.is_empty() || !self.saved_single_nodes.is_empty();
        let source_labels = self
            .node_source_groups(has_local_sources, language)
            .into_iter()
            .map(|group| (group.id, group.name))
            .collect::<BTreeMap<_, _>>();
        let selected_count = draft.explicit_members.len();
        let mut list =
            div()
                .id("policy-group-member-picker")
                .child(Self::policy_candidate_menu_header(
                    selected_count,
                    language,
                    theme,
                    cx,
                ));
        if inventory.is_empty() {
            list = list.child(div().p_5().text_color(theme.text_secondary).child(
                language.localized(
                    copy::nodes::IMPORT_NODES_OR_CREATE_ANOTHER_POLICY_GROUP_BEFORE_MAKING_A,
                ),
            ));
        }
        for member in inventory {
            let is_proxy = member.source_id == "builtin" && member.node_name == "PROXY";
            let selected = draft.explicit_members.contains(&member);
            let member_for_click = member.clone();
            list = list.child(
                Checkbox::new(format!(
                    "policy-group-member-{}-{}",
                    member.source_id, member.node_name
                ))
                .label(if is_proxy {
                    "Proxy".to_owned()
                } else {
                    member.node_name.clone()
                })
                .map(crate::components::primary_button_interaction)
                .checked(selected)
                .tab_stop(true)
                .cursor_pointer()
                .min_h(px(58.0))
                .px_4()
                .flex()
                .items_center()
                .border_b_1()
                .border_color(theme.outline_subtle)
                .hover(|style| style.bg(theme.button_hover))
                .active(|style| style.bg(theme.button_active))
                .child(
                    div().flex().items_center().gap_3().child(
                        div()
                            .text_size(TextRole::Metadata.size())
                            .line_height(TextRole::Metadata.line_height())
                            .text_color(theme.text_tertiary)
                            .child(match member.source_id.as_str() {
                                "builtin" if is_proxy => language
                                    .localized(copy::nodes::FOLLOW_HOME_SELECTION)
                                    .to_owned(),
                                "builtin" => language.localized(copy::nodes::BUILT_IN).to_owned(),
                                source if source.starts_with("policy:") => {
                                    language.message(Message::PolicyGroup).to_owned()
                                }
                                source => source_labels
                                    .get(source)
                                    .cloned()
                                    .unwrap_or_else(|| source.to_owned()),
                            }),
                    ),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(draft) = this.managed_policies.draft.as_mut()
                        && !draft.explicit_members.remove(&member_for_click)
                    {
                        draft.explicit_members.insert(member_for_click.clone());
                    }
                    cx.notify();
                })),
            );
        }
        list
    }

    pub(in crate::app) fn policy_candidate_menu_header(
        selected_count: usize,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .h(px(48.0))
            .px_4()
            .border_b_1()
            .border_color(theme.outline_subtle)
            .flex()
            .items_center()
            .justify_between()
            .child(div().font_weight(FontWeight::SEMIBOLD).child(
                copy::nodes::candidate_selection_title(language, selected_count),
            ))
            .child(
                action_button(
                    "policy-editor-node-menu-done",
                    language.localized(copy::nodes::DONE),
                    ActionRole::Primary,
                    ControlSize::Compact,
                )
                .accessibility_label(language.localized(copy::nodes::FINISH_SELECTING_CANDIDATES))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.managed_policies.editor_popover = None;
                    cx.notify();
                })),
            )
    }

    pub(in crate::app) fn policy_tolerance_menu(
        draft: &ManagedPolicyDraft,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let mut choices = div().id("policy-tolerance-choices");
        let mut values = vec![0, 50, 100, 150, 200, 300, 500];
        if !values.contains(&draft.switch_tolerance_ms) {
            values.push(draft.switch_tolerance_ms);
            values.sort_unstable();
        }
        for milliseconds in values {
            choices = choices.child(Self::policy_choice_row(
                format!("policy-group-tolerance-{milliseconds}"),
                format!("{milliseconds} ms"),
                draft.switch_tolerance_ms == milliseconds,
                theme,
                cx.listener(move |this, _, _, cx| {
                    if let Some(draft) = this.managed_policies.draft.as_mut() {
                        draft.switch_tolerance_ms = milliseconds;
                    }
                    this.managed_policies.editor_popover = None;
                    cx.notify();
                }),
            ));
        }
        choices
    }

    pub(in crate::app) fn policy_interval_menu(
        draft: &ManagedPolicyDraft,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let mut choices = div().id("policy-interval-choices");
        for (seconds, label) in [
            (60, copy::nodes::INTERVAL_1_MINUTE),
            (300, copy::nodes::INTERVAL_5_MINUTES),
            (600, copy::nodes::INTERVAL_10_MINUTES),
            (1_800, copy::nodes::INTERVAL_30_MINUTES),
        ] {
            choices = choices.child(Self::policy_choice_row(
                format!("policy-group-interval-{seconds}"),
                language.localized(label),
                draft.test_interval_secs == seconds,
                theme,
                cx.listener(move |this, _, _, cx| {
                    if let Some(draft) = this.managed_policies.draft.as_mut() {
                        draft.test_interval_secs = seconds;
                    }
                    this.managed_policies.editor_popover = None;
                    cx.notify();
                }),
            ));
        }
        choices
    }
}
