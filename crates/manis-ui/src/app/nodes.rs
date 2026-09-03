use std::collections::{BTreeMap, BTreeSet};

use gpui::{
    AnyElement, AnyWindowHandle, AppContext, Context, Div, Focusable, FontWeight, ParentElement,
    Role, Stateful, Styled, Toggled, Window, div,
    prelude::{FluentBuilder, InteractiveElement, IntoElement, StatefulInteractiveElement},
    px,
};
use gpui_component::{
    Disableable, WindowExt,
    button::{Button, ButtonVariant, ButtonVariants},
    checkbox::Checkbox,
    collapsible::Collapsible,
    dialog::Dialog,
    radio::Radio,
};
use manis_core::{
    KernelKind, ManagedPolicyGroup, ManagedPolicyIcon, ManagedPolicyStrategy, NodeIdentity,
    PolicyCandidateKind, PolicyCandidateMatcher, PolicyNode, PrimaryWorkspace, ProxyId,
    WindowSizeClass,
};

use super::{ImportedSubscriptionState, ManisApp};
use crate::{
    components::{
        ActionRole, action_button, dialog_footer_surface, dialog_header_surface, empty_state,
        status_badge, style_action_button, surface_dialog,
    },
    diagnostics::{UiEvent, trace_ui},
    localization::{CountNoun, Language, Message, copy},
    mihomo::{self, LoadedProvider, LoadedProviderNode, ProxyDelayTarget, SubscriptionStoreError},
    subscription::SourceNodePreview,
    theme::{ControlSize, Radius, Space, TextRole, Theme},
};

struct NodeSourceGroup<'a> {
    id: String,
    name: String,
    detail: String,
    providers: Vec<&'a LoadedProvider>,
    runtime_provider_names: Vec<String>,
    saved_nodes: Vec<&'a SourceNodePreview>,
    saved_nodes_use_runtime_providers: bool,
}

#[derive(Clone, Copy)]
struct NodeWorkspaceView {
    compact: bool,
    language: Language,
    theme: Theme,
}

struct WorkspaceNodeRowContext {
    row_id: String,
    source_id: String,
    compact: bool,
    language: Language,
    theme: Theme,
}

struct SourceGroupPresentation<'a> {
    collapsed: bool,
    benchmark_key: String,
    benchmark: &'a GroupBenchmarkState,
    detail: String,
    total_nodes: usize,
}

enum ManagedPolicyDraftError {
    InvalidName,
    DuplicateName,
    ReservedName,
    InvalidInterval,
    MissingFilter,
    MissingExplicitMember,
    NoCandidates,
    InvalidReferences(String),
}

impl ManagedPolicyDraftError {
    fn message(self, language: Language) -> String {
        match self {
            Self::InvalidName => language
                .localized(
                    copy::nodes::GROUP_NAME_CANNOT_BE_EMPTY_OR_CONTAIN_NEWLINES_CONTROL_CHARACTERS,
                )
                .to_owned(),
            Self::DuplicateName => language
                .localized(copy::nodes::A_POLICY_GROUP_WITH_THIS_NAME_ALREADY_EXISTS_CHOOSE_ANOTHER)
                .to_owned(),
            Self::ReservedName => language
                .localized(copy::nodes::THIS_NAME_IS_RESERVED_BY_THE_PROXY_KERNEL)
                .to_owned(),
            Self::InvalidInterval => language
                .localized(copy::nodes::AUTOMATIC_CHECK_INTERVAL_IS_INVALID)
                .to_owned(),
            Self::MissingFilter => language
                .localized(copy::nodes::ENTER_THE_NODE_NAME_TO_MATCH)
                .to_owned(),
            Self::MissingExplicitMember => language
                .localized(copy::nodes::SELECT_AT_LEAST_ONE_NODE_OR_POLICY_GROUP)
                .to_owned(),
            Self::NoCandidates => language
                .localized(copy::nodes::THE_CURRENT_RULE_DOES_NOT_MATCH_ANY_IMPORTED_NODES)
                .to_owned(),
            Self::InvalidReferences(message) => message,
        }
    }
}

struct PolicyEditorPopup {
    kind: PolicyEditorPopover,
    open: bool,
    content: AnyElement,
    width: f32,
    max_height: f32,
    show_divider: bool,
    disabled: bool,
}

impl PolicyEditorPopup {
    fn new(
        kind: PolicyEditorPopover,
        open: bool,
        content: impl gpui::IntoElement,
        width: f32,
        max_height: f32,
    ) -> Self {
        Self {
            kind,
            open,
            content: content.into_any_element(),
            width,
            max_height,
            show_divider: true,
            disabled: false,
        }
    }

    fn with_divider(mut self, show_divider: bool) -> Self {
        self.show_divider = show_divider;
        self
    }

    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

fn subscription_provider_refs<'a>(
    preview_providers: &'a [LoadedProvider],
    runtime_providers: &'a [LoadedProvider],
    runtime_provider_name: &str,
) -> Vec<&'a LoadedProvider> {
    if preview_providers.is_empty() {
        runtime_providers
            .iter()
            .filter(|provider| provider.name == runtime_provider_name)
            .collect()
    } else {
        preview_providers.iter().collect()
    }
}

impl NodeSourceGroup<'_> {
    fn delay_targets(&self) -> Vec<ProxyDelayTarget> {
        self.providers
            .iter()
            .enumerate()
            .flat_map(|(index, provider)| {
                let runtime_name = self
                    .runtime_provider_names
                    .get(index)
                    .unwrap_or(&provider.name);
                provider.nodes.iter().map(move |node| {
                    ProxyDelayTarget::provider(runtime_name.clone(), node.name.clone())
                })
            })
            .chain(self.saved_nodes.iter().enumerate().map(|(index, node)| {
                if self.saved_nodes_use_runtime_providers {
                    ProxyDelayTarget::provider(
                        format!("Single node {}", index + 1),
                        node.name.clone(),
                    )
                } else {
                    ProxyDelayTarget::direct(node.name.clone())
                }
            }))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
}

const MAX_GROUP_BENCHMARK_NODES: usize = 512;

mod benchmark;
mod benchmark_state;
mod policy_editor;
mod source_groups;
mod view;
pub(in crate::app) use benchmark_state::{
    GroupBenchmarkNodeState, GroupBenchmarkProgressQueue, GroupBenchmarkState,
    GroupBenchmarkSummary, PolicyBenchmarkRun,
};
pub(in crate::app) use policy_editor::{
    ManagedPolicyRuntimeState, ManagedPolicyState, PolicyEditorPopover,
};

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::{NodeSourceGroup, subscription_provider_refs};
    use crate::app::{
        GroupBenchmarkNodeState, GroupBenchmarkState, GroupBenchmarkSummary,
        ManagedPolicyRuntimeState,
    };
    use crate::mihomo::{LoadedProvider, LoadedProviderNode, ProxyDelayTarget};

    fn merged_nodes_fixture() -> crate::app::ManisApp {
        use crate::app::ManisApp;
        use manis_core::ManagedPolicyGroup;
        let mut app = ManisApp::with_fixture_controller("http://127.0.0.1:1");
        app.source_providers = vec![LoadedProvider {
            name: "Fixture source".to_owned(),
            vehicle_type: None,
            nodes: (0..50)
                .map(|index| LoadedProviderNode {
                    name: format!("Node {index}"),
                    protocol: "Trojan".to_owned(),
                    latency_label: None,
                    alive: [Some(true), Some(false), None][index % 3],
                })
                .collect(),
        }];
        app.node_workspace.toggle_group("mihomo:0");
        app.managed_policies.groups = (1..=3)
            .map(|index| {
                ManagedPolicyGroup::new(&format!("policy-{index}"), &format!("Group {index}"))
                    .unwrap()
            })
            .collect();
        app
    }

    #[gpui::test]
    fn secondary_clicks_leave_homepage_controls_unchanged(cx: &mut gpui::TestAppContext) {
        use gpui::{AppContext as _, MouseButton, MouseDownEvent, MouseUpEvent};
        cx.update(crate::init);
        let mut app = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            let entity = cx.new(|_| merged_nodes_fixture());
            app = Some(entity.clone());
            crate::root(entity, window, cx)
        });
        let app = app.unwrap();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let before = app.read_with(cx, |app, _| {
            (
                app.status.clone(),
                app.proxy_mode,
                app.routing_mode,
                app.node_workspace.clone(),
            )
        });
        for selector in [
            "source-group-header-mihomo:0",
            "saved-policy-header-policy-1",
            "proxy-mode-System",
            "routing-mode-Global",
        ] {
            let bounds = cx.debug_bounds(selector).expect(selector);
            for button in [MouseButton::Right, MouseButton::Middle] {
                cx.simulate_event(MouseDownEvent {
                    button,
                    position: bounds.center(),
                    ..Default::default()
                });
                cx.simulate_event(MouseUpEvent {
                    button,
                    position: bounds.center(),
                    ..Default::default()
                });
                cx.update(|window, cx| {
                    assert!(
                        window.focused(cx).is_none(),
                        "{selector} must not take focus"
                    );
                    window.draw(cx).clear(cx);
                });
            }
        }
        app.read_with(cx, |app, _| {
            assert_eq!(
                before,
                (
                    app.status.clone(),
                    app.proxy_mode,
                    app.routing_mode,
                    app.node_workspace.clone()
                )
            );
            assert!(app.expanded_policy_group.is_none());
            assert!(app.proxy_mode_busy.is_none());
            assert!(app.routing_mode_busy.is_none());
        });
    }

    #[gpui::test]
    fn nodes_page_keeps_sources_and_expanding_policies_in_one_document(
        cx: &mut gpui::TestAppContext,
    ) {
        use gpui::{AppContext as _, ScrollDelta, ScrollWheelEvent, point, px};
        use manis_core::PolicyGroupId;

        cx.update(crate::init);
        let mut app = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            let entity = cx.new(|_| merged_nodes_fixture());
            app = Some(entity.clone());
            crate::root(entity, window, cx)
        });
        let app = app.unwrap();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let sources = cx
            .debug_bounds("node-sources-section")
            .expect("sources on homepage");
        let policies = cx
            .debug_bounds("node-policies-section")
            .expect("policies on homepage");
        assert!(policies.origin.y >= sources.bottom());
        let first = cx.debug_bounds("saved-policy-card-policy-1").unwrap();
        let third = cx.debug_bounds("saved-policy-card-policy-3").unwrap();
        cx.update(|window, cx| {
            app.update(cx, |app, cx| {
                app.expanded_policy_group = Some(PolicyGroupId::new("policy-2"));
                cx.notify();
            });
            window.draw(cx).clear(cx);
        });
        assert_eq!(
            cx.debug_bounds("saved-policy-card-policy-1").unwrap(),
            first
        );
        let expanded = cx.debug_bounds("saved-policy-card-policy-2").unwrap();
        let after = cx.debug_bounds("saved-policy-card-policy-3").unwrap();
        assert!(
            expanded.size.height > px(2500.0),
            "all 50 candidates retain row height"
        );
        assert!(
            after.origin.y >= expanded.bottom(),
            "following card must move down"
        );
        assert_eq!(
            after.size.height, third.size.height,
            "following card must not shrink"
        );
        let viewport = cx.debug_bounds("nodes-scroll").unwrap();
        cx.simulate_event(ScrollWheelEvent {
            position: viewport.center(),
            delta: ScrollDelta::Pixels(point(px(0.0), px(-10_000.0))),
            ..Default::default()
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let last = cx.debug_bounds("saved-policy-card-policy-3").unwrap();
        assert!(
            last.bottom() <= viewport.bottom(),
            "last group is reachable by scrolling"
        );
        assert!(last.origin.y >= viewport.origin.y);
        cx.simulate_event(ScrollWheelEvent {
            position: viewport.center(),
            delta: ScrollDelta::Pixels(point(px(0.0), px(10_000.0))),
            ..Default::default()
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!(cx.debug_bounds("navigation-Nodes").is_some());
        assert!(cx.debug_bounds("navigation-Policies").is_none());
        cx.update(|window, cx| {
            app.update(cx, |app, cx| {
                app.node_workspace.toggle_group("mihomo:0");
                cx.notify();
            });
            window.draw(cx).clear(cx);
        });
        for row in [
            "node-row-mihomo:0-0-0",
            "node-row-mihomo:0-0-1",
            "node-row-mihomo:0-0-2",
        ] {
            assert!(
                cx.debug_bounds(row).is_some(),
                "alive/failed/unknown nodes must all stay visible"
            );
        }
    }

    #[gpui::test]
    fn nodes_page_opens_policy_creation_and_settings_without_leaving_home(
        cx: &mut gpui::TestAppContext,
    ) {
        use crate::app::ManisApp;
        use gpui::{AppContext as _, Modifiers};
        use gpui_component::WindowExt as _;
        use manis_core::{ManagedPolicyGroup, PrimaryWorkspace};

        cx.update(crate::init);
        let mut app = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            let entity = cx.new(|_| {
                let mut app = ManisApp::with_fixture_controller("http://127.0.0.1:1");
                app.managed_policies.groups =
                    vec![ManagedPolicyGroup::new("policy-1", "Saved group").unwrap()];
                app
            });
            app = Some(entity.clone());
            crate::root(entity, window, cx)
        });
        let app = app.unwrap();
        for (selector, editing_id) in [
            ("policy-settings-policy-1", Some("policy-1")),
            ("add-policy-group-header", None),
        ] {
            cx.update(|window, cx| window.draw(cx).clear(cx));
            let button = cx
                .debug_bounds(selector)
                .expect("policy action on homepage");
            cx.simulate_click(button.center(), Modifiers::none());
            cx.update(|window, cx| {
                assert!(window.has_active_dialog(cx));
                app.read_with(cx, |app, _| {
                    assert_eq!(app.primary_workspace, PrimaryWorkspace::Nodes);
                    let draft = app
                        .managed_policies
                        .draft
                        .as_ref()
                        .expect("policy editor draft");
                    assert_eq!(draft.editing_id.as_deref(), editing_id);
                });
                window.close_dialog(cx);
            });
        }
    }

    #[test]
    fn proxy_candidate_keeps_its_identity_when_the_homepage_exit_changes()
    -> Result<(), Box<dyn std::error::Error>> {
        use crate::app::ManisApp;
        use manis_core::{
            ManagedPolicyGroup, NodeIdentity, PolicyCandidateKind, PolicyCandidateMatcher,
        };
        use manis_profile::MANIS_GLOBAL_GROUP_NAME;

        let mut app = ManisApp::with_fixture_controller("http://127.0.0.1:1");
        let proxy = NodeIdentity::new("builtin", "PROXY")?;
        assert!(app.policy_candidate_inventory().contains(&proxy));
        let mut group = ManagedPolicyGroup::new("policy-deadbeef", "Follow homepage")?;
        group.set_matcher(PolicyCandidateMatcher::Explicit(BTreeSet::from([
            proxy.clone()
        ])))?;
        for name in ["Tokyo", "Singapore"] {
            app.managed_policies
                .node_selections
                .set_global(NodeIdentity::new("saved", name)?);
            let candidates = app.managed_policy_candidate_nodes(&group);
            assert_eq!(candidates.len(), 1);
            assert_eq!(candidates[0].name, MANIS_GLOBAL_GROUP_NAME);
            assert_eq!(candidates[0].kind, PolicyCandidateKind::PolicyGroup);
            assert!(group.matches(&proxy.source_id, &proxy.node_name));
        }
        // The automatic all-nodes scope must not gain an extra selector candidate.
        group.set_matcher(PolicyCandidateMatcher::All)?;
        assert!(app.managed_policy_candidate_nodes(&group).is_empty());
        Ok(())
    }

    #[gpui::test]
    fn selecting_proxy_in_an_offline_policy_saves_the_dynamic_reference(
        cx: &mut gpui::TestAppContext,
    ) {
        use crate::app::{ManisApp, PolicySelectionRequest};
        use gpui::AppContext as _;
        use manis_core::{
            ManagedPolicyGroup, NodeIdentity, PolicyCandidateMatcher, PolicyGroupId, ProxyId,
        };
        use manis_profile::MANIS_GLOBAL_GROUP_NAME;

        let app = cx.new(|_| ManisApp::with_fixture_controller("http://127.0.0.1:1"));
        app.update(cx, |app, cx| {
            let mut group = ManagedPolicyGroup::new("policy-deadbeef", "Follow homepage").unwrap();
            group
                .set_matcher(PolicyCandidateMatcher::Explicit(BTreeSet::from([
                    NodeIdentity::new("builtin", "PROXY").unwrap(),
                ])))
                .unwrap();
            app.managed_policies.groups.push(group.clone());
            app.select_policy_node(
                PolicySelectionRequest {
                    group_id: PolicyGroupId::new(group.id),
                    group_name: group.name.clone(),
                    node_id: ProxyId::new("builtin:PROXY"),
                    node_name: MANIS_GLOBAL_GROUP_NAME.to_owned(),
                },
                cx,
            );
            assert_eq!(
                app.managed_policies
                    .node_selections
                    .policy_target(&group.name),
                Some(MANIS_GLOBAL_GROUP_NAME)
            );
        });
    }

    #[gpui::test]
    fn saving_policy_replaces_stale_catalog_before_background_refresh(
        cx: &mut gpui::TestAppContext,
    ) {
        use crate::app::{ManisApp, SourceMutation, SourceRuntimeApply};
        use crate::mihomo::ControllerState;
        use gpui::AppContext as _;
        use manis_core::{
            ManagedPolicyGroup, PolicyCatalog, PolicyGroup, PolicyGroupId, PolicyGroupKind,
        };

        let app = cx.new(|_| ManisApp::with_fixture_controller("http://127.0.0.1:1"));
        app.update(cx, |app, cx| {
            app.subscription_store_dir = None;
            app.catalog = Some(PolicyCatalog::from_primary(
                PolicyGroup {
                    id: PolicyGroupId::new("Old"),
                    name: "Old".to_owned(),
                    kind: PolicyGroupKind::Selector,
                    target: None,
                    nodes: Vec::new(),
                    rules_total: 0,
                    rules: Vec::new(),
                },
                Vec::new(),
            ));
            app.controller = ControllerState::Connected {
                endpoint: "http://127.0.0.1:1".to_owned(),
                version: "fixture".to_owned(),
                active_connections: 0,
                download_total: 0,
                upload_total: 0,
            };
            let group = ManagedPolicyGroup::new("group-new", "New").expect("group");
            assert!(app.finish_managed_policy_save(
                Ok(SourceMutation::Committed {
                    value: group.clone(),
                    apply: SourceRuntimeApply::MetadataOnly,
                }),
                cx
            ));
            assert!(
                app.catalog.is_none(),
                "render saved groups immediately instead of the old catalog"
            );
            assert_eq!(app.managed_policies.groups, vec![group]);
            assert!(matches!(app.controller, ControllerState::Connecting { .. }));
        });
    }

    #[gpui::test]
    fn failed_policy_save_keeps_catalog_and_draft(cx: &mut gpui::TestAppContext) {
        use crate::app::{ManisApp, SourceMutation, SourceRuntimeApply};
        use gpui::AppContext as _;
        use manis_core::{PolicyCatalog, PolicyGroup, PolicyGroupId, PolicyGroupKind};

        let app = cx.new(|_| ManisApp::with_fixture_controller("http://127.0.0.1:1"));
        app.update(cx, |app, cx| {
            app.catalog = Some(PolicyCatalog::from_primary(
                PolicyGroup {
                    id: PolicyGroupId::new("Old"),
                    name: "Old".to_owned(),
                    kind: PolicyGroupKind::Selector,
                    target: None,
                    nodes: Vec::new(),
                    rules_total: 0,
                    rules: Vec::new(),
                },
                Vec::new(),
            ));
            app.start_managed_policy_create(cx);
            assert!(!app.finish_managed_policy_save(
                Ok(SourceMutation::RollbackAttempted {
                    apply: SourceRuntimeApply::Failed("fixture validation failure".to_owned()),
                    rollback_error: None,
                }),
                cx
            ));
            assert!(app.catalog.is_some());
            assert!(app.managed_policies.groups.is_empty());
            assert!(app.managed_policies.draft.is_some());
        });
    }

    #[gpui::test]
    fn offline_policy_changes_never_start_the_kernel(cx: &mut gpui::TestAppContext) {
        use crate::app::{ManisApp, SourceMutation, SourceRuntimeApply};
        use crate::mihomo::ControllerState;
        use gpui::AppContext as _;
        use manis_core::{
            ManagedPolicyGroup, PolicyCatalog, PolicyGroup, PolicyGroupId, PolicyGroupKind,
        };

        let app = cx.new(|_| ManisApp::with_fixture_controller("http://127.0.0.1:1"));
        app.update(cx, |app, cx| {
            let mut group = ManagedPolicyGroup::new("group-edit", "Original").expect("group");
            app.managed_policies.groups.push(group.clone());
            group.name = "Renamed".to_owned();
            assert!(app.finish_managed_policy_save(
                Ok(SourceMutation::Committed {
                    value: group.clone(),
                    apply: SourceRuntimeApply::MetadataOnly,
                }),
                cx
            ));
            assert_eq!(app.managed_policies.groups, vec![group.clone()]);
            assert!(matches!(app.controller, ControllerState::Disconnected));

            app.catalog = Some(PolicyCatalog::from_primary(
                PolicyGroup {
                    id: PolicyGroupId::new("Renamed"),
                    name: "Renamed".to_owned(),
                    kind: PolicyGroupKind::Selector,
                    target: None,
                    nodes: Vec::new(),
                    rules_total: 0,
                    rules: Vec::new(),
                },
                Vec::new(),
            ));
            assert!(app.finish_managed_policy_removal(
                Ok(SourceMutation::Committed {
                    value: (group.id.clone(), group),
                    apply: SourceRuntimeApply::MetadataOnly,
                }),
                cx
            ));
            assert!(
                app.catalog.is_none(),
                "the deleted runtime row must disappear immediately"
            );
            assert!(app.managed_policies.groups.is_empty());
            assert!(matches!(app.controller, ControllerState::Disconnected));
        });
    }

    #[test]
    fn imported_group_delay_targets_use_the_runtime_provider_id() {
        let provider = LoadedProvider {
            name: "订阅预览".to_owned(),
            vehicle_type: Some("HTTP".to_owned()),
            nodes: vec![LoadedProviderNode {
                name: "HK 03".to_owned(),
                protocol: "Trojan".to_owned(),
                latency_label: None,
                alive: None,
            }],
        };
        let group = NodeSourceGroup {
            id: "subscription:fixture".to_owned(),
            name: "Fixture".to_owned(),
            detail: String::new(),
            providers: vec![&provider],
            runtime_provider_names: vec!["Subscription 1".to_owned()],
            saved_nodes: Vec::new(),
            saved_nodes_use_runtime_providers: false,
        };

        assert_eq!(
            group.delay_targets(),
            vec![ProxyDelayTarget::provider("Subscription 1", "HK 03")]
        );
    }

    #[test]
    fn saved_source_group_copy_does_not_assume_a_node_protocol() {
        let mut app = crate::app::ManisApp::with_fixture_controller("http://127.0.0.1:1");
        app.saved_single_nodes
            .push(crate::mihomo::StoredSingleNode {
                id: "saved-trojan".to_owned(),
                name: "Saved Trojan".to_owned(),
                source: crate::subscription::SingleNodeSource::parse(
                    "trojan://secret@example.com:443#Saved%20Trojan",
                )
                .expect("saved Trojan node"),
                enabled: true,
            });

        let groups = app.node_source_groups(true, crate::localization::Language::SimplifiedChinese);

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].detail, "单独添加的节点 · 私有本机存储");
        assert_eq!(groups[0].saved_nodes[0].protocol, "Trojan");
        assert_eq!(
            groups[0].delay_targets(),
            vec![ProxyDelayTarget::provider("Single node 1", "Saved Trojan")]
        );
    }

    #[test]
    fn imported_subscription_falls_back_to_the_matching_runtime_provider() {
        let cached = LoadedProvider {
            name: "Subscription 1".to_owned(),
            vehicle_type: Some("HTTP".to_owned()),
            nodes: vec![node(None)],
        };
        let unrelated = LoadedProvider {
            name: "Proxy".to_owned(),
            vehicle_type: Some("Compatible".to_owned()),
            nodes: vec![node(None)],
        };
        let runtime = vec![unrelated, cached.clone()];

        let restored = subscription_provider_refs(&[], &runtime, "Subscription 1");
        assert_eq!(restored, vec![&cached]);

        let preview = vec![LoadedProvider {
            name: "Fresh preview".to_owned(),
            vehicle_type: Some("HTTP".to_owned()),
            nodes: vec![node(Some(true))],
        }];
        let restored = subscription_provider_refs(&preview, &runtime, "Subscription 1");
        assert_eq!(restored, vec![&preview[0]]);
    }

    #[test]
    fn group_benchmark_summary_counts_failures_and_latency_range() {
        let summary = GroupBenchmarkSummary::from_delays(4, [80, 0, 42]);

        assert_eq!(summary.total, 4);
        assert_eq!(summary.succeeded, 2);
        assert_eq!(summary.failed, 2);
        assert_eq!(summary.minimum_ms, Some(42));
        assert_eq!(summary.maximum_ms, Some(80));
        assert_eq!(summary.average_ms, Some(61));
    }

    #[test]
    fn imported_node_latency_uses_running_and_failure_states_without_health_labels() {
        assert_eq!(
            GroupBenchmarkState::running(2).node_state("Saved Edge"),
            GroupBenchmarkNodeState::Pending,
        );
        assert_eq!(
            GroupBenchmarkState::Complete {
                generation: 2,
                summary: GroupBenchmarkSummary::default(),
                delays: BTreeMap::new(),
            }
            .node_state("Saved Edge"),
            GroupBenchmarkNodeState::Failed,
        );
        assert_eq!(
            GroupBenchmarkState::Complete {
                generation: 2,
                summary: GroupBenchmarkSummary::default(),
                delays: BTreeMap::from([("Saved Edge".to_owned(), 47)]),
            }
            .node_state("Saved Edge"),
            GroupBenchmarkNodeState::Measured(47),
        );
    }

    #[test]
    fn group_benchmark_state_ignores_a_stale_completion() {
        let mut state = GroupBenchmarkState::running(7);
        let outdated = BTreeMap::from([("Tokyo".to_owned(), 90)]);
        assert!(!state.complete(6, 2, outdated));
        assert_eq!(state, GroupBenchmarkState::running(7));

        let current = BTreeMap::from([("Tokyo".to_owned(), 55), ("Singapore".to_owned(), 75)]);
        assert!(state.complete(7, 2, current));
        assert!(matches!(
            &state,
            GroupBenchmarkState::Complete {
                summary: GroupBenchmarkSummary {
                    average_ms: Some(65),
                    ..
                },
                delays,
                ..
            } if delays.get("Tokyo") == Some(&55)
        ));
    }

    #[test]
    fn group_benchmark_exposes_each_node_result_before_completion() {
        let mut state = GroupBenchmarkState::running(7);

        assert_eq!(
            state.node_state("Tokyo"),
            crate::app::GroupBenchmarkNodeState::Pending
        );
        assert!(state.record(7, "Tokyo", Some(42)));
        assert_eq!(
            state.node_state("Tokyo"),
            crate::app::GroupBenchmarkNodeState::Measured(42)
        );
        assert!(state.record(7, "Singapore", None));
        assert_eq!(
            state.node_state("Singapore"),
            crate::app::GroupBenchmarkNodeState::Failed
        );
        assert!(!state.record(6, "Stale", Some(99)));
        assert_eq!(
            state.node_state("Stale"),
            crate::app::GroupBenchmarkNodeState::Pending
        );
    }

    #[test]
    fn benchmark_state_reports_running_only_for_active_variant() {
        assert!(GroupBenchmarkState::running(1).is_running());
        assert!(!GroupBenchmarkState::Idle.is_running());
        assert!(
            !GroupBenchmarkState::Failed {
                generation: 1,
                message: None
            }
            .is_running()
        );
    }

    #[test]
    fn managed_policy_runtime_rejects_unknown_selection() {
        let mut state = ManagedPolicyRuntimeState::Ready {
            generation: 4,
            current: Some("Tokyo".to_owned()),
            candidates: BTreeSet::from(["Tokyo".to_owned(), "Singapore".to_owned()]),
        };
        let ready = state.clone();
        assert!(!state.begin_selection(5, "Unknown"));
        assert_eq!(state, ready);
        assert!(state.begin_selection(5, "Singapore"));
        assert_eq!(
            state,
            ManagedPolicyRuntimeState::Selecting {
                generation: 5,
                current: Some("Tokyo".to_owned()),
                candidates: BTreeSet::from(["Tokyo".to_owned(), "Singapore".to_owned()]),
                pending: "Singapore".to_owned(),
            }
        );
        assert!(matches!(
            state,
            ManagedPolicyRuntimeState::Selecting {
                generation: 5,
                ref pending,
                ..
            } if pending == "Singapore"
        ));
    }

    fn node(alive: Option<bool>) -> LoadedProviderNode {
        LoadedProviderNode {
            name: "node".to_owned(),
            protocol: "SS".to_owned(),
            latency_label: None,
            alive,
        }
    }
}
