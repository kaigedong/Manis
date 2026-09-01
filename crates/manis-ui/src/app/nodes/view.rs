use super::{
    Context, CountNoun, Div, FluentBuilder, ImportedSubscriptionState, InteractiveElement,
    Language, ManagedPolicyIcon, ManisApp, Message, NodeSourceGroup, NodeWorkspaceView,
    ParentElement, Space, Stateful, StatefulInteractiveElement, Styled, TextRole, Theme,
    WindowSizeClass, copy, div, px, subscription_provider_refs,
};

#[cfg(feature = "snapshot-fixtures")]
use super::{
    LoadedProvider, LoadedProviderNode, ManagedPolicyGroup, ManagedPolicyStrategy, PrimaryWorkspace,
};

impl ManisApp {
    #[cfg(feature = "snapshot-fixtures")]
    #[doc(hidden)]
    pub fn show_merged_nodes_fixture(&mut self, expanded: bool, cx: &mut Context<Self>) {
        if !self.runtime.is_fixture() {
            return;
        }
        self.primary_workspace = PrimaryWorkspace::Nodes;
        self.source_providers = vec![LoadedProvider {
            name: "测试订阅".to_owned(),
            vehicle_type: Some("HTTP".to_owned()),
            nodes: (1..=50)
                .map(|index| LoadedProviderNode {
                    name: format!("测试节点 {index:02}"),
                    protocol: "Trojan".to_owned(),
                    latency_label: Some(format!("{} ms", 40 + index)),
                    alive: Some(true),
                })
                .collect(),
        }];
        self.node_workspace.replace_collapsed_groups(["mihomo:0"]);
        self.managed_policies.groups = ["手动选择", "自动选择", "备用策略"]
            .into_iter()
            .enumerate()
            .map(|(index, name)| {
                let mut group = ManagedPolicyGroup::new(&format!("policy-{}", index + 1), name)
                    .expect("valid fixture policy");
                if index == 1 {
                    group.strategy = ManagedPolicyStrategy::LowestLatency;
                }
                group
            })
            .collect();
        self.managed_policies
            .node_selections
            .set_policy_target("自动选择", "测试节点 02")
            .expect("valid fixture selection");
        self.expanded_policy_group = expanded.then(|| manis_core::PolicyGroupId::new("policy-2"));
        cx.notify();
    }

    pub(in crate::app) fn managed_policy_icon_label(
        icon: ManagedPolicyIcon,
        language: Language,
    ) -> &'static str {
        match icon {
            ManagedPolicyIcon::None => language.localized(copy::nodes::FIRST_LETTER),
            ManagedPolicyIcon::Bolt => language.localized(copy::nodes::BOLT),
            ManagedPolicyIcon::Globe => language.localized(copy::nodes::GLOBE),
            ManagedPolicyIcon::Shield => language.localized(copy::nodes::SHIELD),
            ManagedPolicyIcon::Compass => language.localized(copy::nodes::COMPASS),
        }
    }

    pub(in crate::app) fn node_count_label(count: usize, language: Language) -> String {
        language.count(CountNoun::Node, count)
    }

    pub(in crate::app) fn success_fraction_label(
        succeeded: usize,
        total: usize,
        language: Language,
    ) -> String {
        copy::nodes::success_fraction(language, succeeded, total)
    }

    pub(in crate::app) fn group_limit_label(count: usize, language: Language) -> String {
        copy::nodes::group_limit(language, count)
    }

    pub(in crate::app) fn single_test_limit_label(limit: usize, language: Language) -> String {
        copy::nodes::single_test_limit(language, limit)
    }

    pub(in crate::app) fn node_workspace(
        &self,
        theme: Theme,
        size_class: WindowSizeClass,
        cx: &mut Context<Self>,
    ) -> Div {
        let compact = size_class == WindowSizeClass::Compact;
        let has_local_sources = self
            .imported_subscriptions
            .iter()
            .any(|subscription| subscription.enabled)
            || !self.saved_single_nodes.is_empty();
        let language = self.language();
        let groups = self.node_source_groups(has_local_sources, language);
        let loading = self.imported_subscriptions.iter().any(|subscription| {
            subscription.enabled
                && matches!(
                    subscription.state,
                    ImportedSubscriptionState::Pending(_)
                        | ImportedSubscriptionState::Refreshing(_)
                )
        });
        let (enabled_count, unavailable_count) = self
            .imported_subscriptions
            .iter()
            .filter(|subscription| subscription.enabled)
            .fold((0, 0), |(enabled, unavailable), subscription| {
                (
                    enabled + 1,
                    unavailable
                        + usize::from(matches!(
                            subscription.state,
                            ImportedSubscriptionState::Unavailable(_, _)
                                | ImportedSubscriptionState::StoreError(_)
                        )),
                )
            });
        let unavailable = enabled_count > 0 && unavailable_count == enabled_count;
        let view = NodeWorkspaceView {
            compact,
            language,
            theme,
        };
        div()
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .flex()
            .flex_col()
            .bg(theme.surface_base)
            .child(self.node_workspace_header(view, cx))
            .child(
                div()
                    .id("nodes-scroll")
                    .debug_selector(|| "nodes-scroll".to_owned())
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .child(self.node_workspace_body(&groups, loading, unavailable, view, cx))
                    .child(self.policy_section(theme, compact, cx)),
            )
    }

    pub(super) fn node_source_groups(
        &self,
        has_local_sources: bool,
        language: Language,
    ) -> Vec<NodeSourceGroup<'_>> {
        if has_local_sources {
            let mut groups: Vec<_> = self
                .imported_subscriptions
                .iter()
                .filter(|subscription| subscription.enabled)
                .enumerate()
                .map(|(index, subscription)| {
                    let name = subscription.name.clone();
                    let runtime_provider_name = format!("Subscription {}", index + 1);
                    let providers = subscription_provider_refs(
                        &subscription.providers,
                        &self.source_providers,
                        &runtime_provider_name,
                    );
                    let using_runtime_cache =
                        subscription.providers.is_empty() && !providers.is_empty();
                    let provider_count = providers.len();
                    let transport = if subscription.source.is_https() {
                        language.localized(copy::common::HTTPS_SUBSCRIPTION)
                    } else {
                        language.localized(copy::common::HTTP_SUBSCRIPTION)
                    };
                    let state = if using_runtime_cache {
                        language.localized(copy::nodes::USING_MIHOMO_CACHE)
                    } else {
                        match subscription.state {
                            ImportedSubscriptionState::Pending(_)
                            | ImportedSubscriptionState::Refreshing(_) => {
                                language.localized(copy::nodes::RESTORING)
                            }
                            ImportedSubscriptionState::Ready(_) => {
                                language.localized(copy::nodes::RESTORES_AFTER_RESTART)
                            }
                            ImportedSubscriptionState::Unavailable(_, _)
                            | ImportedSubscriptionState::StoreError(_) => {
                                language.localized(copy::nodes::SOURCE_UNAVAILABLE)
                            }
                            ImportedSubscriptionState::Removing(_) => {
                                language.localized(copy::nodes::REMOVING)
                            }
                            ImportedSubscriptionState::None => {
                                language.localized(copy::nodes::NOT_LOADED)
                            }
                        }
                    };
                    NodeSourceGroup {
                        id: format!("subscription:{}", subscription.id),
                        name,
                        detail: format!("{transport} · {state}"),
                        providers,
                        runtime_provider_names: vec![runtime_provider_name; provider_count],
                        saved_nodes: Vec::new(),
                    }
                })
                .collect();
            if self.saved_single_nodes.iter().any(|saved| saved.enabled) {
                groups.push(NodeSourceGroup {
                    id: "saved".to_owned(),
                    name: language.localized(copy::common::SAVED).to_owned(),
                    detail: language
                        .localized(
                            copy::nodes::INDIVIDUALLY_ADDED_VLESS_NODES_PRIVATE_LOCAL_STORAGE,
                        )
                        .to_owned(),
                    providers: Vec::new(),
                    runtime_provider_names: Vec::new(),
                    saved_nodes: self
                        .saved_single_nodes
                        .iter()
                        .filter(|saved| saved.enabled)
                        .map(|saved| saved.source.preview())
                        .collect(),
                });
            }
            return groups;
        }

        self.source_providers
            .iter()
            .enumerate()
            .map(|(index, provider)| NodeSourceGroup {
                id: format!("mihomo:{index}"),
                name: provider.name.clone(),
                detail: provider.vehicle_type.as_ref().map_or_else(
                    || language.localized(copy::nodes::MIHOMO_SOURCE).to_owned(),
                    |vehicle| {
                        format!(
                            "{} · {vehicle}",
                            language.localized(copy::nodes::MIHOMO_SOURCE)
                        )
                    },
                ),
                providers: vec![provider],
                runtime_provider_names: vec![provider.name.clone()],
                saved_nodes: Vec::new(),
            })
            .collect()
    }

    pub(super) fn node_workspace_header(
        &self,
        view: NodeWorkspaceView,
        cx: &mut Context<Self>,
    ) -> Div {
        let NodeWorkspaceView {
            compact,
            language,
            theme,
        } = view;
        div()
            .flex_shrink_0()
            .px(if compact { px(12.0) } else { px(24.0) })
            .py(Space::Lg.px())
            .border_b_1()
            .border_color(theme.outline_subtle)
            .flex()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap(Space::Md.px())
            .child(
                div()
                    .text_size(TextRole::PageTitle.size())
                    .line_height(TextRole::PageTitle.line_height())
                    .font_weight(TextRole::PageTitle.weight())
                    .text_color(theme.text_primary)
                    .child(language.message(Message::Nodes)),
            )
            .when(
                !matches!(
                    self.controller,
                    crate::mihomo::ControllerState::Connected { .. }
                ),
                |header| header.child(self.connection_button(theme, cx)),
            )
    }

    pub(super) fn node_workspace_body(
        &self,
        groups: &[NodeSourceGroup<'_>],
        loading: bool,
        unavailable: bool,
        view: NodeWorkspaceView,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let NodeWorkspaceView {
            compact,
            language,
            theme,
        } = view;
        div()
        .id("node-sources-section")
        .debug_selector(|| "node-sources-section".to_owned())
        .flex_shrink_0()
        .px(if compact { px(12.0) } else { px(24.0) })
        .py_4()
        .when(loading && groups.is_empty(), |body| {
            body.child(Self::node_message_panel(
                language.localized(copy::nodes::RESTORING_NODES),
                language.localized(copy::nodes::MANIS_IS_LOADING_NODES_FROM_YOUR_SAVED_SUBSCRIPTIONS),
                theme,
            ))
        })
        .when(unavailable && groups.is_empty(), |body| {
            body.child(Self::node_message_panel(
                language.localized(copy::nodes::NODES_ARE_TEMPORARILY_UNAVAILABLE),
                language.localized(copy::nodes::SUBSCRIPTIONS_REMAIN_STORED_LOCALLY_CHECK_SOURCE_DETAILS_IN_CONFIGURATION),
                theme,
            ))
        })
        .when(!loading && !unavailable && groups.is_empty(), |body| {
            body.child(Self::node_empty_state(compact, language, theme, cx))
        })
        .when(!groups.is_empty(), |body| {
            body.child(self.source_group_list(groups, compact, language, theme, cx))
        })
    }
}
