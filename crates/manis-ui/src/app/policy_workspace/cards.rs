use super::{
    AnyElement, Context, ControllerState, CountNoun, Div, InteractiveElement, IntoElement,
    ManisApp, Message, ParentElement, Space, Stateful, Styled, Theme, copy, div, px,
};
use crate::components::section_header;

mod card;
mod list;
mod offline;

impl ManisApp {
    /// Section one of the nodes page: the policy groups that pick exits.
    ///
    /// Cards are prioritised over the subscription list because they are the high-frequency
    /// control here, and the group's own actions live inside its card rather than in a page header.
    pub(in crate::app) fn policy_section(
        &self,
        theme: Theme,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let language = self.language();
        let mut cards: Vec<AnyElement> = Vec::new();
        if self.catalog.is_some() {
            // The core always carries an internal all-node selector for global-mode exits. The
            // source rows on this page already pick that exit, so listing it as a policy group
            // would show a group the user never created.
            let groups: Vec<_> = self
                .policy_groups()
                .filter(|group| group.name != manis_profile::MANIS_GLOBAL_GROUP_NAME)
                .collect();
            for item in groups {
                cards.push(
                    self.policy_list_card(item, language, theme, compact, cx)
                        .into_any_element(),
                );
            }
        } else {
            for policy in &self.managed_policies.groups {
                cards.push(
                    self.offline_policy_card(policy, language, theme, compact, cx)
                        .into_any_element(),
                );
            }
        }
        let policy_count = cards.len();
        let mut actions = div().flex().items_center().gap(Space::Sm.px());
        if !matches!(self.controller, ControllerState::Connected { .. }) {
            // Starting the kernel belongs to the routing work this section owns, not to a second
            // page header.
            actions = actions.child(self.connection_button(theme, cx));
        }
        actions = actions.child(Self::managed_policy_add_button(
            "add-policy-group-header",
            language,
            theme,
            cx,
        ));

        let mut section = div()
            .id("node-policies-section")
            .debug_selector(|| "node-policies-section".to_owned())
            .flex_shrink_0()
            .min_w(px(0.0))
            .px(if compact { px(12.0) } else { px(24.0) })
            .pt(Space::Xl.px())
            .pb(Space::Xl.px())
            .flex()
            .flex_col()
            .gap(Space::Md.px())
            .child(section_header(
                language.message(Message::PolicyGroups),
                format!(
                    "{} · {}",
                    language.count(CountNoun::PolicyGroup, policy_count),
                    language.localized(
                        copy::app::ROUTING_RULES_CHOOSE_POLICY_GROUPS_POLICIES_CHOOSE_EXITS,
                    ),
                ),
                Some(actions.into_any_element()),
                theme,
            ));
        if policy_count == 0 {
            section = section.child(self.empty_policy_content(language, theme));
        } else {
            section = section.child(card::policy_card_grid(cards, compact));
        }
        section
    }
}
