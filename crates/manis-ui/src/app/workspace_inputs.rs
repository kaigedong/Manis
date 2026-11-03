use gpui::{Entity, Subscription};

use crate::subscription_input::SubscriptionTextInput;

#[derive(Default)]
pub(in crate::app) struct WorkspaceInputs {
    pub(in crate::app) qx_rule: Option<Entity<SubscriptionTextInput>>,
    pub(in crate::app) qx_rule_events: Option<Subscription>,
    pub(in crate::app) qx_rule_name: Option<Entity<SubscriptionTextInput>>,
    pub(in crate::app) qx_rule_name_events: Option<Subscription>,
    pub(in crate::app) policy_group_name: Option<Entity<SubscriptionTextInput>>,
    pub(in crate::app) policy_group_filter: Option<Entity<SubscriptionTextInput>>,
    pub(in crate::app) activity_search: Option<Entity<SubscriptionTextInput>>,
    pub(in crate::app) activity_search_events: Option<Subscription>,
    pub(in crate::app) logs_search: Option<Entity<SubscriptionTextInput>>,
    pub(in crate::app) logs_search_events: Option<Subscription>,
}
