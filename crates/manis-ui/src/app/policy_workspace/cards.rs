use super::{
    Context, CountNoun, Div, InteractiveElement, IntoElement, ManisApp, Message, ParentElement,
    Space, Stateful, Styled, Theme, copy, div, page_heading, px,
};

mod list;
mod offline;

impl ManisApp {
    pub(in crate::app) fn policy_section(
        &self,
        theme: Theme,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let language = self.language();
        let mut rows = div().flex_shrink_0().flex().flex_col().gap(Space::Sm.px());
        let policy_count;
        if self.catalog.is_some() {
            // The core always carries an internal all-node selector for global-mode exits. The
            // source rows above already pick that exit, so listing it as a policy group would
            // show a group the user never created.
            let groups: Vec<_> = self
                .policy_groups()
                .filter(|group| group.name != manis_profile::MANIS_GLOBAL_GROUP_NAME)
                .collect();
            policy_count = groups.len();
            for item in groups {
                rows = rows.child(self.policy_list_card(item, language, theme, cx));
            }
        } else {
            policy_count = self.managed_policies.groups.len();
            for policy in &self.managed_policies.groups {
                rows = rows.child(self.offline_policy_card(policy, language, theme, cx));
            }
        }
        if policy_count == 0 {
            rows = rows.child(self.empty_policy_content(language, theme));
        }
        div()
            .id("node-policies-section")
            .debug_selector(|| "node-policies-section".to_owned())
            .flex_shrink_0()
            .min_w(px(0.0))
            .border_t_1()
            .border_color(theme.outline_subtle)
            .px(if compact { px(12.0) } else { px(24.0) })
            .py(Space::Lg.px())
            .flex()
            .flex_col()
            .gap(Space::Lg.px())
            .child(page_heading(
                language.message(Message::PolicyGroups),
                format!(
                    "{} · {}",
                    language.count(CountNoun::PolicyGroup, policy_count),
                    language.localized(
                        copy::app::ROUTING_RULES_CHOOSE_POLICY_GROUPS_POLICIES_CHOOSE_EXITS,
                    ),
                ),
                Some(
                    Self::managed_policy_add_button("add-policy-group-header", language, theme, cx)
                        .into_any_element(),
                ),
                theme,
            ))
            .child(rows)
    }
}
