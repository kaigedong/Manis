use std::collections::{BTreeMap, BTreeSet};

use gpui::{
    AnyElement, AnyWindowHandle, Context, Div, Focusable, FontWeight, ParentElement, Role,
    Stateful, Styled, Toggled, Window, div, prelude::*, px,
};
use gpui_component::{
    Disableable, IconName, Selectable as _, WindowExt,
    button::{Button, ButtonVariant, ButtonVariants},
    checkbox::Checkbox,
    collapsible::Collapsible,
    dialog::Dialog,
    radio::Radio,
};
use manis_core::{
    ManagedPolicyGroup, ManagedPolicyIcon, ManagedPolicyStrategy, NodeAvailabilityFilter,
    NodeIdentity, PolicyCandidateKind, PolicyCandidateMatcher, PolicyNode, PrimaryWorkspace,
    ProxyId, WindowSizeClass,
};

use super::{
    GroupBenchmarkNodeState, GroupBenchmarkState, ImportedSubscriptionState, ManagedPolicyDraft,
    ManagedPolicyMutationState, ManisApp, PolicyCandidateMatcherKind, PolicyEditorPopover,
};
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
}

#[derive(Clone, Copy)]
struct NodeWorkspaceView {
    filter: NodeAvailabilityFilter,
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

struct SourceGroupPresentation {
    visible_count: usize,
    collapsed: bool,
    benchmark_key: String,
    benchmark: GroupBenchmarkState,
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
            .chain(
                self.saved_nodes
                    .iter()
                    .map(|node| ProxyDelayTarget::direct(node.name.clone())),
            )
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
}

const MAX_GROUP_BENCHMARK_NODES: usize = 512;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct NodeCounts {
    total: usize,
    available: usize,
    unavailable: usize,
    untested: usize,
}

impl NodeCounts {
    #[cfg(test)]
    fn from_providers(providers: &[LoadedProvider]) -> Self {
        let mut counts = Self::default();
        for provider in providers {
            counts.add_provider(provider);
        }
        counts
    }

    fn from_provider_refs(providers: &[&LoadedProvider]) -> Self {
        let mut counts = Self::default();
        for provider in providers {
            counts.add_provider(provider);
        }
        counts
    }

    fn from_groups(groups: &[NodeSourceGroup<'_>]) -> Self {
        let mut counts = Self::default();
        for group in groups {
            for provider in &group.providers {
                counts.add_provider(provider);
            }
            counts.total += group.saved_nodes.len();
            counts.untested += group.saved_nodes.len();
        }
        counts
    }

    fn add_provider(&mut self, provider: &LoadedProvider) {
        for node in &provider.nodes {
            self.total += 1;
            match node.alive {
                Some(true) => self.available += 1,
                Some(false) => self.unavailable += 1,
                None => self.untested += 1,
            }
        }
    }

    fn count_for(self, filter: NodeAvailabilityFilter) -> usize {
        match filter {
            NodeAvailabilityFilter::All => self.total,
            NodeAvailabilityFilter::Available => self.available,
            NodeAvailabilityFilter::Unavailable => self.unavailable,
            NodeAvailabilityFilter::Untested => self.untested,
        }
    }
}

impl ManisApp {
    fn managed_policy_icon_label(icon: ManagedPolicyIcon, language: Language) -> &'static str {
        match icon {
            ManagedPolicyIcon::None => language.localized(copy::nodes::FIRST_LETTER),
            ManagedPolicyIcon::Bolt => language.localized(copy::nodes::BOLT),
            ManagedPolicyIcon::Globe => language.localized(copy::nodes::GLOBE),
            ManagedPolicyIcon::Shield => language.localized(copy::nodes::SHIELD),
            ManagedPolicyIcon::Compass => language.localized(copy::nodes::COMPASS),
        }
    }

    fn availability_filter_label(
        filter: NodeAvailabilityFilter,
        language: Language,
    ) -> &'static str {
        match filter {
            NodeAvailabilityFilter::All => language.localized(copy::nodes::ALL),
            NodeAvailabilityFilter::Available => language.localized(copy::nodes::AVAILABLE),
            NodeAvailabilityFilter::Unavailable => language.localized(copy::nodes::UNAVAILABLE),
            NodeAvailabilityFilter::Untested => language.localized(copy::nodes::UNTESTED),
        }
    }

    fn source_count_label(count: usize, language: Language) -> String {
        language.count(CountNoun::Source, count)
    }

    fn node_count_label(count: usize, language: Language) -> String {
        language.count(CountNoun::Node, count)
    }

    fn success_fraction_label(succeeded: usize, total: usize, language: Language) -> String {
        copy::nodes::success_fraction(language, succeeded, total)
    }

    fn group_limit_label(count: usize, language: Language) -> String {
        copy::nodes::group_limit(language, count)
    }

    fn single_test_limit_label(limit: usize, language: Language) -> String {
        copy::nodes::single_test_limit(language, limit)
    }

    pub(super) fn node_workspace(
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
        let counts = NodeCounts::from_groups(&groups);
        let filter = self.node_workspace.filter;
        let loading = self.imported_subscriptions.iter().any(|subscription| {
            subscription.enabled
                && matches!(
                    subscription.state,
                    ImportedSubscriptionState::Pending(_)
                        | ImportedSubscriptionState::Refreshing(_)
                )
        });
        let refreshing = loading
            || (!has_local_sources
                && matches!(
                    self.controller,
                    crate::mihomo::ControllerState::Connecting { .. }
                ));
        let enabled_subscriptions = self
            .imported_subscriptions
            .iter()
            .filter(|subscription| subscription.enabled)
            .collect::<Vec<_>>();
        let unavailable = !enabled_subscriptions.is_empty()
            && enabled_subscriptions.iter().all(|subscription| {
                matches!(
                    subscription.state,
                    ImportedSubscriptionState::Unavailable(_, _)
                        | ImportedSubscriptionState::StoreError(_)
                )
            });
        let view = NodeWorkspaceView {
            filter,
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
            .child(self.node_workspace_header(
                groups.len(),
                counts,
                refreshing,
                !matches!(self.controller, super::ControllerState::Connected { .. }),
                view,
                cx,
            ))
            .child(self.node_workspace_body(&groups, loading, unavailable, view, cx))
    }

    fn node_source_groups(
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
                                language.localized(copy::nodes::UNAVAILABLE_2)
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

    fn node_workspace_header(
        &self,
        source_count: usize,
        counts: NodeCounts,
        refreshing: bool,
        show_connection: bool,
        view: NodeWorkspaceView,
        cx: &mut Context<Self>,
    ) -> Div {
        let NodeWorkspaceView {
            filter,
            compact,
            language,
            theme,
        } = view;
        let actions = div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(Space::Sm.px())
            .when(show_connection, |actions| {
                actions.child(self.connection_button(theme, cx))
            })
            .child(Self::node_refresh_button(refreshing, language, theme, cx))
            .child(Self::node_configuration_link(language, theme, cx));

        div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .gap(Space::Md.px())
            .px(if compact { px(12.0) } else { px(24.0) })
            .py(Space::Lg.px())
            .border_b_1()
            .border_color(theme.outline_subtle)
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(Space::Md.px())
                    .child(
                        div()
                            .flex()
                            .items_baseline()
                            .gap(Space::Md.px())
                            .child(
                                div()
                                    .text_size(TextRole::PageTitle.size())
                                    .line_height(TextRole::PageTitle.line_height())
                                    .font_weight(TextRole::PageTitle.weight())
                                    .text_color(theme.text_primary)
                                    .child(language.message(Message::Nodes)),
                            )
                            .child(
                                div()
                                    .text_size(TextRole::Metadata.size())
                                    .text_color(theme.text_secondary)
                                    .child(Self::source_count_label(source_count, language)),
                            ),
                    )
                    .child(actions),
            )
            .child(Self::node_filter_bar(counts, filter, compact, language, cx))
    }

    fn node_workspace_body(
        &self,
        groups: &[NodeSourceGroup<'_>],
        loading: bool,
        unavailable: bool,
        view: NodeWorkspaceView,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let NodeWorkspaceView {
            filter,
            compact,
            language,
            theme,
        } = view;
        div()
            .id("nodes-scroll")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
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
                body.child(self.source_group_list(groups, filter, compact, language, theme, cx))
            })
    }

    pub(super) fn open_managed_policy_create(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_managed_policy_create(cx);
        Self::open_managed_policy_dialog(window, cx);
        if let Some(input) = self.inputs.policy_group_name.as_ref() {
            input.focus_handle(cx).focus(window, cx);
        }
    }

    pub(super) fn open_managed_policy_settings(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_managed_policy_edit(id, cx);
        if self.managed_policies.draft.is_none() {
            return;
        }
        Self::open_managed_policy_dialog(window, cx);
        if let Some(input) = self.inputs.policy_group_name.as_ref() {
            input.focus_handle(cx).focus(window, cx);
        }
    }

    fn open_managed_policy_dialog(window: &mut Window, cx: &mut Context<Self>) {
        let app = cx.entity();
        window.open_dialog(cx, move |dialog, window, cx| {
            app.update(cx, |this, cx| {
                let theme = this.theme();
                this.ensure_policy_group_inputs(theme, window, cx);
                this.set_policy_group_editor_enabled(
                    !this.managed_policies.mutation_state.is_busy(),
                    cx,
                );
                let compact = WindowSizeClass::for_width(window.viewport_size().width.as_f32())
                    == WindowSizeClass::Compact;
                this.managed_policy_editor_modal(
                    dialog,
                    compact,
                    this.language(),
                    theme,
                    window,
                    cx,
                )
            })
        });
        cx.notify();
    }

    fn set_policy_group_editor_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        for input in [
            self.inputs.policy_group_name.as_ref(),
            self.inputs.policy_group_filter.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            input.update(cx, |input, cx| input.set_enabled(enabled, cx));
        }
    }

    fn close_managed_policy_editor(&mut self, cx: &mut Context<Self>) {
        self.managed_policies.draft = None;
        self.managed_policies.editor_popover = None;
        self.language()
            .localized(copy::nodes::POLICY_EDITING_CANCELLED)
            .clone_into(&mut self.status);
        cx.notify();
    }

    fn managed_policy_editor_modal(
        &self,
        dialog: Dialog,
        compact: bool,
        language: Language,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Dialog {
        let viewport = window.viewport_size();
        let width = (viewport.width.as_f32() - 32.0).clamp(320.0, 780.0);
        let max_height = (viewport.height.as_f32() - 32.0).max(360.0);
        let margin_top = ((viewport.height.as_f32() - 640.0) / 2.0).max(16.0);
        let app = cx.entity();
        let busy = self.managed_policies.mutation_state.is_busy();
        let (title, body, footer) = if let Some(draft) = self.managed_policies.draft.as_ref() {
            (
                Self::managed_policy_editor_title(draft, language, theme),
                self.policy_editor_form(draft, compact, false, language, theme, cx)
                    .into_any_element(),
                self.managed_policy_editor_footer(draft, language, theme, cx),
            )
        } else {
            (
                Self::managed_policy_editor_completed_title(language, theme),
                div()
                    .p_5()
                    .text_size(TextRole::Body.size())
                    .line_height(TextRole::Body.line_height())
                    .text_color(theme.text_secondary)
                    .child(self.status.clone())
                    .into_any_element(),
                Self::managed_policy_editor_completed_footer(language, theme, cx),
            )
        };

        surface_dialog(dialog, theme)
            .width(px(width))
            .max_h(px(max_height))
            .margin_top(px(margin_top))
            .overlay(true)
            .overlay_closable(!busy)
            .keyboard(!busy)
            .close_button(false)
            .title(title)
            .child(body)
            .footer(footer)
            .on_close(move |_, _, cx| {
                app.update(cx, |this, cx| {
                    if !this.managed_policies.mutation_state.is_busy() {
                        this.managed_policies.draft = None;
                        this.managed_policies.editor_popover = None;
                        cx.notify();
                    }
                });
            })
    }

    fn managed_policy_editor_title(
        draft: &ManagedPolicyDraft,
        language: Language,
        theme: Theme,
    ) -> Div {
        let title = if draft.editing_id.is_some() {
            language.localized(copy::common::EDIT_POLICY_GROUP)
        } else {
            language.localized(copy::nodes::NEW_POLICY_GROUP)
        };
        dialog_header_surface(theme).child(
            div()
                .text_size(TextRole::SectionTitle.size())
                .line_height(TextRole::SectionTitle.line_height())
                .font_weight(TextRole::SectionTitle.weight())
                .text_color(theme.text_primary)
                .child(title),
        )
    }

    fn managed_policy_editor_completed_title(language: Language, theme: Theme) -> Div {
        dialog_header_surface(theme)
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .text_size(TextRole::SectionTitle.size())
                    .line_height(TextRole::SectionTitle.line_height())
                    .font_weight(TextRole::SectionTitle.weight())
                    .child(language.localized(copy::nodes::GROUP_SAVED)),
            )
            .child(status_badge(
                language.localized(copy::nodes::DONE),
                crate::components::StatusTone::Success,
                theme,
            ))
    }

    fn managed_policy_editor_footer(
        &self,
        draft: &ManagedPolicyDraft,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let editing_id = draft.editing_id.clone();
        let busy = self.managed_policies.mutation_state.is_busy();
        dialog_footer_surface(theme)
            .flex_col()
            .items_stretch()
            .gap_3()
            .child(
                div()
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .text_color(theme.text_secondary)
                    .child(self.status.clone()),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap_2()
                    .when_some(editing_id.clone(), |actions, remove_id| {
                        actions.child(
                            style_action_button(
                                Button::new("remove-managed-policy-dialog")
                                    .label(language.localized(copy::app::DELETE_POLICY_GROUP))
                                    .disabled(busy),
                                ActionRole::Danger,
                                ControlSize::Standard,
                            )
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    if !busy {
                                        this.remove_managed_policy_from_dialog(
                                            &remove_id, window, cx,
                                        );
                                    }
                                },
                            )),
                        )
                    })
                    .child(
                        style_action_button(
                            Button::new("cancel-managed-policy-dialog")
                                .label(language.message(Message::Cancel))
                                .disabled(busy),
                            ActionRole::Secondary,
                            ControlSize::Standard,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            if !this.managed_policies.mutation_state.is_busy() {
                                this.close_managed_policy_editor(cx);
                                window.close_dialog(cx);
                            }
                        })),
                    )
                    .child(
                        style_action_button(
                            Button::new("save-managed-policy-dialog")
                                .label(if busy {
                                    language.localized(copy::nodes::APPLYING_CHANGES)
                                } else if editing_id.is_some() {
                                    language.message(Message::SaveChanges)
                                } else {
                                    language.message(Message::AddPolicyGroup)
                                })
                                .loading(busy)
                                .disabled(busy),
                            ActionRole::Primary,
                            ControlSize::Standard,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            if !this.managed_policies.mutation_state.is_busy() {
                                this.save_managed_policy_from_dialog(window, cx);
                            }
                        })),
                    ),
            )
    }

    fn managed_policy_editor_completed_footer(
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        dialog_footer_surface(theme).child(
            style_action_button(
                Button::new("close-managed-policy-dialog")
                    .label(language.localized(copy::nodes::DONE)),
                ActionRole::Primary,
                ControlSize::Standard,
            )
            .on_click(cx.listener(|_, _, window, cx| {
                window.close_dialog(cx);
            })),
        )
    }

    pub(super) fn policy_editor_form(
        &self,
        draft: &ManagedPolicyDraft,
        compact: bool,
        embedded: bool,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let popover_width = if compact { 280.0 } else { 300.0 };
        let basics = self.policy_editor_basics(draft, language, theme, popover_width, cx);
        let nodes = self.policy_editor_candidates(draft, language, theme, popover_width, cx);

        div()
            .id("policy-editor-scroll")
            .when(!embedded, |form| form.flex_1().overflow_y_scroll())
            .px(if embedded {
                px(0.0)
            } else if compact {
                px(16.0)
            } else {
                px(28.0)
            })
            .py(if embedded { px(0.0) } else { px(24.0) })
            .child(
                div()
                    .w_full()
                    .max_w(px(760.0))
                    .mx_auto()
                    .child(Self::policy_editor_section_label(
                        language.localized(copy::nodes::BASIC_INFORMATION),
                        theme,
                    ))
                    .child(basics)
                    .child(
                        Self::policy_editor_section_label(
                            language.localized(copy::nodes::CANDIDATES),
                            theme,
                        )
                        .mt_6(),
                    )
                    .child(nodes)
                    .child(
                        div()
                            .mt_3()
                            .px_2()
                            .text_size(TextRole::Body.size())
                            .line_height(TextRole::Body.line_height())
                            .text_color(theme.text_tertiary)
                            .child(language.localized(copy::nodes::ROUTING_RULES_POINT_TO_THIS_POLICY_THE_POLICY_CHOOSES_ONE)),
                    ),
            )
    }

    fn policy_editor_basics(
        &self,
        draft: &ManagedPolicyDraft,
        language: Language,
        theme: Theme,
        popover_width: f32,
        cx: &mut Context<Self>,
    ) -> Div {
        let strategy = match draft.strategy {
            ManagedPolicyStrategy::Manual => "static".to_owned(),
            ManagedPolicyStrategy::LowestLatency => "url-latency-benchmark".to_owned(),
        };
        let busy = self.managed_policies.mutation_state.is_busy();
        let policy_name = self
            .inputs
            .policy_group_name
            .as_ref()
            .map_or_else(String::new, |input| input.read(cx).value().to_owned());
        div()
            .rounded(Radius::Pane.px())
            .overflow_hidden()
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_low)
            .child(Self::policy_editor_popup_row(
                "policy-editor-type",
                language.localized(copy::nodes::TYPE),
                strategy,
                None,
                theme,
                PolicyEditorPopup::new(
                    PolicyEditorPopover::Strategy,
                    self.managed_policies.editor_popover == Some(PolicyEditorPopover::Strategy),
                    Self::policy_strategy_menu(draft, language, theme, cx),
                    popover_width,
                    220.0,
                )
                .disabled(busy),
                cx,
            ))
            .child(Self::policy_editor_input_row(
                language.localized(copy::nodes::POLICY_GROUP_NAME),
                true,
                self.inputs.policy_group_name.clone(),
                true,
                theme,
            ))
            .child(Self::policy_editor_popup_row(
                "policy-editor-icon",
                language.localized(copy::nodes::ICON),
                Self::managed_policy_icon_label(draft.icon, language).to_owned(),
                Some(Self::policy_icon_visual(
                    draft.icon,
                    &policy_name,
                    28.0,
                    theme,
                )),
                theme,
                PolicyEditorPopup::new(
                    PolicyEditorPopover::Icon,
                    self.managed_policies.editor_popover == Some(PolicyEditorPopover::Icon),
                    Self::policy_icon_menu(draft, language, theme, cx),
                    popover_width,
                    320.0,
                )
                .with_divider(false)
                .disabled(busy),
                cx,
            ))
    }

    fn policy_editor_candidates(
        &self,
        draft: &ManagedPolicyDraft,
        language: Language,
        theme: Theme,
        popover_width: f32,
        cx: &mut Context<Self>,
    ) -> Div {
        let matcher = match draft.matcher_kind {
            PolicyCandidateMatcherKind::All => {
                language.localized(copy::nodes::ALL_NODES).to_owned()
            }
            PolicyCandidateMatcherKind::NameContains => {
                language.localized(copy::nodes::NAME_CONTAINS).to_owned()
            }
            PolicyCandidateMatcherKind::Explicit => language
                .localized(copy::nodes::SELECT_NODES_OR_GROUPS)
                .to_owned(),
        };
        let has_details = draft.matcher_kind != PolicyCandidateMatcherKind::All
            || draft.strategy == ManagedPolicyStrategy::LowestLatency;
        let busy = self.managed_policies.mutation_state.is_busy();
        let mut nodes = div()
            .rounded(Radius::Pane.px())
            .overflow_hidden()
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_low)
            .child(Self::policy_editor_popup_row(
                "policy-editor-candidate-mode",
                language.localized(copy::nodes::NODE_SCOPE),
                matcher,
                None,
                theme,
                PolicyEditorPopup::new(
                    PolicyEditorPopover::CandidateMode,
                    self.managed_policies.editor_popover
                        == Some(PolicyEditorPopover::CandidateMode),
                    Self::policy_candidate_mode_menu(draft, language, theme, cx),
                    popover_width,
                    280.0,
                )
                .with_divider(has_details)
                .disabled(busy),
                cx,
            ));
        if draft.matcher_kind == PolicyCandidateMatcherKind::NameContains {
            nodes = nodes.child(Self::policy_editor_input_row(
                language.localized(copy::nodes::NODE_NAME_CONTAINS),
                false,
                self.inputs.policy_group_filter.clone(),
                draft.strategy == ManagedPolicyStrategy::LowestLatency,
                theme,
            ));
        }
        if draft.matcher_kind == PolicyCandidateMatcherKind::Explicit {
            nodes = nodes.child(self.policy_editor_selected_candidates(
                draft,
                language,
                theme,
                popover_width,
                cx,
            ));
        }
        if draft.strategy == ManagedPolicyStrategy::LowestLatency {
            nodes = nodes.child(self.policy_editor_interval_row(
                draft,
                language,
                theme,
                popover_width,
                cx,
            ));
            nodes = nodes.child(self.policy_editor_tolerance_row(
                draft,
                language,
                theme,
                popover_width,
                cx,
            ));
        }
        nodes
    }

    fn policy_editor_selected_candidates(
        &self,
        draft: &ManagedPolicyDraft,
        language: Language,
        theme: Theme,
        popover_width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = copy::nodes::selected_count(language, draft.explicit_members.len());
        Self::policy_editor_popup_row(
            "policy-editor-selected-nodes",
            language.localized(copy::nodes::SELECTED_CANDIDATES),
            selected,
            None,
            theme,
            PolicyEditorPopup::new(
                PolicyEditorPopover::CandidateNodes,
                self.managed_policies.editor_popover == Some(PolicyEditorPopover::CandidateNodes),
                self.policy_candidate_menu(draft, language, theme, cx),
                popover_width.max(480.0),
                420.0,
            )
            .with_divider(draft.strategy == ManagedPolicyStrategy::LowestLatency)
            .disabled(self.managed_policies.mutation_state.is_busy()),
            cx,
        )
    }

    fn policy_editor_tolerance_row(
        &self,
        draft: &ManagedPolicyDraft,
        language: Language,
        theme: Theme,
        popover_width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let row = Self::policy_editor_popup_row(
            "policy-editor-tolerance",
            language.localized(copy::nodes::SWITCH_TOLERANCE),
            format!("{} ms", draft.switch_tolerance_ms),
            None,
            theme,
            PolicyEditorPopup::new(
                PolicyEditorPopover::Tolerance,
                self.managed_policies.editor_popover == Some(PolicyEditorPopover::Tolerance),
                Self::policy_tolerance_menu(draft, theme, cx),
                popover_width,
                320.0,
            )
            .with_divider(false)
            .disabled(self.managed_policies.mutation_state.is_busy()),
            cx,
        );
        div()
            .child(row)
            .child(
                div()
                    .px_4()
                    .pb_3()
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .text_color(theme.text_secondary)
                    .child(language.localized(copy::nodes::SWITCH_TOLERANCE_HELP)),
            )
            .into_any_element()
    }

    fn policy_editor_interval_row(
        &self,
        draft: &ManagedPolicyDraft,
        language: Language,
        theme: Theme,
        popover_width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let interval = copy::nodes::interval(language, draft.test_interval_secs);
        Self::policy_editor_popup_row(
            "policy-editor-interval",
            language.localized(copy::nodes::RETEST_INTERVAL),
            interval,
            None,
            theme,
            PolicyEditorPopup::new(
                PolicyEditorPopover::Interval,
                self.managed_policies.editor_popover == Some(PolicyEditorPopover::Interval),
                Self::policy_interval_menu(draft, language, theme, cx),
                popover_width,
                320.0,
            )
            .with_divider(true)
            .disabled(self.managed_policies.mutation_state.is_busy()),
            cx,
        )
    }

    fn policy_editor_section_label(label: &'static str, theme: Theme) -> Div {
        div()
            .mb_2()
            .px_2()
            .text_size(TextRole::Label.size())
            .line_height(TextRole::Label.line_height())
            .font_weight(TextRole::Label.weight())
            .text_color(theme.text_secondary)
            .child(label)
    }

    fn policy_editor_popup_row(
        id: &'static str,
        label: &'static str,
        value: String,
        value_icon: Option<Div>,
        theme: Theme,
        popup: PolicyEditorPopup,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let PolicyEditorPopup {
            kind,
            open,
            content,
            width,
            max_height,
            show_divider,
            disabled,
        } = popup;
        let app = cx.entity();
        let trigger = style_action_button(
            Button::new(id)
                .accessibility_label(format!("{label}: {value}"))
                .disabled(disabled)
                .dropdown_caret(true),
            ActionRole::Secondary,
            ControlSize::Standard,
        )
        .w_full()
        .child(
            div()
                .w_full()
                .min_w(px(0.0))
                .flex()
                .items_center()
                .gap_2()
                .when_some(value_icon, ParentElement::child)
                .child(
                    div()
                        .flex_1()
                        .overflow_x_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(TextRole::Label.size())
                        .line_height(TextRole::Label.line_height())
                        .font_weight(TextRole::Label.weight())
                        .text_color(theme.text_secondary)
                        .child(value),
                ),
        );
        let popover = crate::components::anchored_popover(
            format!("{id}-popover"),
            trigger,
            content,
            width,
            max_height,
        )
        .open(open)
        .on_open_change(move |open, _, cx| {
            app.update(cx, |this, cx| {
                if this.managed_policies.mutation_state.is_busy() {
                    this.managed_policies.editor_popover = None;
                    cx.notify();
                    return;
                }
                this.managed_policies.editor_popover = open.then_some(kind);
                cx.notify();
            });
        });

        div()
            .min_h(px(64.0))
            .px_4()
            .border_color(theme.outline_subtle)
            .when(show_divider, gpui::Styled::border_b_1)
            .flex()
            .items_center()
            .gap_4()
            .child(
                div()
                    .flex_1()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(label),
            )
            .child(div().w(px(300.0)).max_w(px(300.0)).child(popover))
            .into_any_element()
    }

    fn policy_editor_input_row(
        label: &'static str,
        required: bool,
        input: Option<gpui::Entity<crate::subscription_input::SubscriptionTextInput>>,
        show_divider: bool,
        theme: Theme,
    ) -> Div {
        div()
            .min_h(px(82.0))
            .px_4()
            .py_3()
            .border_color(theme.outline_subtle)
            .when(show_divider, gpui::Styled::border_b_1)
            .child(
                div()
                    .mb_2()
                    .flex()
                    .gap_1()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(label)
                    .when(required, |label| {
                        label.child(div().text_color(theme.status_error).child("*"))
                    }),
            )
            .when_some(input, ParentElement::child)
    }

    fn policy_choice_row(
        id: String,
        title: impl Into<gpui::SharedString>,
        selected: bool,
        theme: Theme,
        listener: impl Fn(&bool, &mut gpui::Window, &mut gpui::App) + 'static,
    ) -> Radio {
        let title = title.into();
        Radio::new(id)
            .label(title)
            .checked(selected)
            .tab_stop(true)
            .cursor_pointer()
            .min_h(px(44.0))
            .px_3()
            .flex()
            .items_center()
            .border_b_1()
            .border_color(theme.outline_subtle)
            .hover(|style| style.bg(theme.button_hover))
            .active(|style| style.bg(theme.button_active))
            .on_click(listener)
    }

    fn policy_icon_choice_row(
        id: String,
        icon: ManagedPolicyIcon,
        title: impl Into<gpui::SharedString>,
        selected: bool,
        theme: Theme,
        listener: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
    ) -> Stateful<Div> {
        let title = title.into();
        div()
            .id(id)
            .role(Role::RadioButton)
            .aria_toggled(if selected {
                Toggled::True
            } else {
                Toggled::False
            })
            .tab_stop(true)
            .focusable()
            .cursor_pointer()
            .min_h(px(50.0))
            .px_3()
            .border_b_1()
            .border_color(theme.outline_subtle)
            .hover(|style| style.bg(theme.button_hover))
            .active(|style| style.bg(theme.button_active))
            .flex()
            .items_center()
            .gap_3()
            .child(Self::policy_icon_visual(icon, "A", 30.0, theme))
            .child(div().flex_1().font_weight(FontWeight::MEDIUM).child(title))
            .child(
                div()
                    .size(px(16.0))
                    .rounded_full()
                    .border_2()
                    .border_color(if selected {
                        theme.action_primary
                    } else {
                        theme.outline_strong
                    })
                    .when(selected, |radio| radio.bg(theme.action_primary)),
            )
            .on_click(listener)
    }

    fn policy_strategy_menu(
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

    fn policy_icon_menu(
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

    fn policy_candidate_mode_menu(
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

    fn policy_candidate_menu(
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

    fn policy_candidate_menu_header(
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

    fn policy_tolerance_menu(
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

    fn policy_interval_menu(
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

    fn node_inventory(&self) -> Vec<NodeIdentity> {
        let has_local_sources =
            !self.imported_subscriptions.is_empty() || !self.saved_single_nodes.is_empty();
        let mut inventory = Vec::new();
        let mut seen = BTreeSet::new();
        for group in self.node_source_groups(has_local_sources, self.language()) {
            for provider in group.providers {
                for node in &provider.nodes {
                    if let Ok(identity) = NodeIdentity::new(&group.id, &node.name)
                        && seen.insert(identity.clone())
                    {
                        inventory.push(identity);
                    }
                }
            }
            for node in group.saved_nodes {
                if let Ok(identity) = NodeIdentity::new(&group.id, &node.name)
                    && seen.insert(identity.clone())
                {
                    inventory.push(identity);
                }
            }
        }
        inventory
    }

    fn policy_candidate_inventory(&self) -> Vec<NodeIdentity> {
        let mut inventory = ["PROXY", "DIRECT", "REJECT"]
            .into_iter()
            .filter_map(|name| NodeIdentity::new("builtin", name).ok())
            .collect::<Vec<_>>();
        inventory.extend(self.node_inventory());
        let editing_id = self
            .managed_policies
            .draft
            .as_ref()
            .and_then(|draft| draft.editing_id.as_deref());
        for group in &self.managed_policies.groups {
            if editing_id != Some(group.id.as_str())
                && let Ok(identity) =
                    NodeIdentity::new(&format!("policy:{}", group.id), &group.name)
            {
                inventory.push(identity);
            }
        }
        inventory
    }

    fn managed_policy_candidate_count(&self, group: &ManagedPolicyGroup) -> usize {
        self.node_inventory()
            .iter()
            .filter(|node| group.matches(&node.source_id, &node.node_name))
            .count()
    }

    pub(super) fn managed_policy_candidate_names(&self, group: &ManagedPolicyGroup) -> Vec<String> {
        self.managed_policy_candidate_nodes(group)
            .into_iter()
            .map(|node| node.name)
            .collect()
    }

    pub(super) fn managed_policy_candidate_nodes(
        &self,
        group: &ManagedPolicyGroup,
    ) -> Vec<PolicyNode> {
        let language = self.language();
        let has_local_sources =
            !self.imported_subscriptions.is_empty() || !self.saved_single_nodes.is_empty();
        let mut candidates = Vec::new();
        let mut seen = BTreeSet::new();
        for source in self.node_source_groups(has_local_sources, language) {
            for provider in source.providers {
                for node in &provider.nodes {
                    if group.matches(&source.id, &node.name) && seen.insert(node.name.clone()) {
                        candidates.push(PolicyNode {
                            id: ProxyId::new(format!("{}:{}", source.id, node.name)),
                            name: node.name.clone(),
                            kind: PolicyCandidateKind::Node,
                            provider: Some(source.name.clone()),
                            detail: node.protocol.clone(),
                            latency_ms: node
                                .latency_label
                                .as_deref()
                                .and_then(|label| label.strip_suffix(" ms"))
                                .and_then(|delay| delay.parse().ok()),
                            alive: node.alive,
                        });
                    }
                }
            }
            for node in source.saved_nodes {
                if group.matches(&source.id, &node.name) && seen.insert(node.name.clone()) {
                    candidates.push(PolicyNode {
                        id: ProxyId::new(format!("{}:{}", source.id, node.name)),
                        name: node.name.clone(),
                        kind: PolicyCandidateKind::Node,
                        provider: Some(source.name.clone()),
                        detail: node.protocol.to_owned(),
                        latency_ms: None,
                        alive: None,
                    });
                }
            }
        }
        if matches!(group.matcher, PolicyCandidateMatcher::Explicit(_)) {
            for name in ["PROXY", "DIRECT", "REJECT"] {
                let is_proxy = name == "PROXY";
                let runtime_name = if is_proxy {
                    manis_profile::MANIS_GLOBAL_GROUP_NAME
                } else {
                    name
                };
                if group.matches("builtin", name) && seen.insert(runtime_name.to_owned()) {
                    candidates.push(PolicyNode {
                        id: ProxyId::new(format!("builtin:{name}")),
                        name: runtime_name.to_owned(),
                        kind: if is_proxy {
                            PolicyCandidateKind::PolicyGroup
                        } else {
                            PolicyCandidateKind::Node
                        },
                        provider: Some(language.localized(copy::nodes::BUILT_IN).to_owned()),
                        detail: if is_proxy {
                            language
                                .localized(copy::nodes::FOLLOW_HOME_SELECTION)
                                .to_owned()
                        } else {
                            name.to_owned()
                        },
                        latency_ms: None,
                        alive: None,
                    });
                }
            }
            for policy in &self.managed_policies.groups {
                let source_id = format!("policy:{}", policy.id);
                if policy.id != group.id
                    && group.matches(&source_id, &policy.name)
                    && seen.insert(policy.name.clone())
                {
                    candidates.push(PolicyNode {
                        id: ProxyId::new(source_id),
                        name: policy.name.clone(),
                        kind: PolicyCandidateKind::PolicyGroup,
                        provider: None,
                        detail: language
                            .localized(match policy.strategy {
                                ManagedPolicyStrategy::Manual => copy::app::MANUAL_SELECTION,
                                ManagedPolicyStrategy::LowestLatency => {
                                    copy::app::AUTOMATIC_SELECTION
                                }
                            })
                            .to_owned(),
                        latency_ms: None,
                        alive: None,
                    });
                }
            }
        }
        candidates
    }

    fn clear_managed_policy_benchmarks(
        &mut self,
        id: &str,
        previous_name: Option<&str>,
        next_name: Option<&str>,
    ) {
        self.managed_policies
            .benchmarks
            .remove(&Self::managed_policy_benchmark_key(id));
        for name in previous_name.into_iter().chain(next_name) {
            self.managed_policies
                .benchmarks
                .remove(&format!("policy:{name}"));
        }
    }

    fn start_source_group_benchmark(
        &mut self,
        id: &str,
        name: &str,
        targets: Vec<ProxyDelayTarget>,
        cx: &mut Context<Self>,
    ) {
        let key = Self::source_group_benchmark_key(id);
        if matches!(
            self.managed_policies.benchmarks.get(&key),
            Some(GroupBenchmarkState::Running { .. })
        ) {
            return;
        }
        if targets.is_empty() {
            self.language()
                .localized(copy::nodes::THIS_SOURCE_HAS_NO_NODES_TO_TEST)
                .clone_into(&mut self.status);
            cx.notify();
            return;
        }
        if targets.len() > MAX_GROUP_BENCHMARK_NODES {
            let language = self.language();
            format!(
                "{}; {}",
                Self::group_limit_label(targets.len(), language),
                Self::single_test_limit_label(MAX_GROUP_BENCHMARK_NODES, language)
            )
            .clone_into(&mut self.status);
            cx.notify();
            return;
        }
        let Some(generation) = self.begin_group_benchmark(key.clone()) else {
            self.language()
                .localized(copy::nodes::A_GROUP_TEST_IS_ALREADY_RUNNING_WAIT_FOR_IT_TO)
                .clone_into(&mut self.status);
            cx.notify();
            return;
        };
        let language = self.language();
        self.status = format!(
            "{} “{name}” · {}",
            language.localized(copy::nodes::TESTING_SOURCE),
            Self::node_count_label(targets.len(), language)
        );
        trace_ui(UiEvent::GroupBenchmarkStarted);

        let runtime = self.runtime.clone();
        let progress =
            std::sync::Arc::new(std::sync::Mutex::new(std::collections::VecDeque::new()));
        self.poll_group_benchmark_progress(generation, key.clone(), progress.clone(), cx);
        let total = targets.len();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    runtime.test_proxy_delay_targets_with_progress(
                        &targets,
                        move |node_name, delay| {
                            if let Ok(mut updates) = progress.lock() {
                                updates.push_back((node_name.to_owned(), delay));
                            }
                        },
                    )
                })
                .await;
            this.update(cx, |this, cx| {
                this.finish_source_group_benchmark(&key, generation, total, result, cx);
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn finish_source_group_benchmark(
        &mut self,
        key: &str,
        generation: u64,
        total: usize,
        result: Result<BTreeMap<String, u16>, mihomo::LoadError>,
        cx: &mut Context<Self>,
    ) {
        let language = self.language();
        if self.managed_policies.active_benchmark_generation != Some(generation) {
            return;
        }
        self.managed_policies.active_benchmark_generation = None;
        let Some(state) = self.managed_policies.benchmarks.get_mut(key) else {
            cx.notify();
            return;
        };
        let failure = result.as_ref().err().map(ToString::to_string);
        let accepted = match result {
            Ok(delays) => state.complete(generation, total, delays),
            Err(_error) => state.fail(generation),
        };
        if !accepted {
            return;
        }
        match state {
            GroupBenchmarkState::Complete { summary, .. } => {
                trace_ui(UiEvent::GroupBenchmarkSucceeded);
                self.status = format!(
                    "{}: {}",
                    language.localized(copy::nodes::SOURCE_TEST_COMPLETED),
                    Self::success_fraction_label(summary.succeeded, summary.total, language)
                );
            }
            GroupBenchmarkState::Failed { .. } => {
                trace_ui(UiEvent::GroupBenchmarkFailed);
                self.status = format!(
                    "{}：{}",
                    language.localized(copy::nodes::SOURCE_TEST_FAILED),
                    failure.as_deref().unwrap_or_else(|| {
                        language.localized(copy::common::MIHOMO_DID_NOT_RETURN_A_RESULT)
                    })
                );
            }
            _ => return,
        }
        self.persist_group_benchmarks();
        cx.notify();
    }

    pub(super) fn start_managed_policy_create(&mut self, cx: &mut Context<Self>) {
        self.managed_policies.editor_popover = None;
        self.managed_policies.draft = Some(ManagedPolicyDraft {
            editing_id: None,
            icon: ManagedPolicyIcon::None,
            strategy: ManagedPolicyStrategy::Manual,
            test_interval_secs: 600,
            switch_tolerance_ms: ManagedPolicyGroup::DEFAULT_SWITCH_TOLERANCE_MS,
            matcher_kind: PolicyCandidateMatcherKind::All,
            explicit_members: BTreeSet::new(),
        });
        if let Some(input) = self.inputs.policy_group_name.as_ref() {
            input.update(
                cx,
                crate::subscription_input::SubscriptionTextInput::clear_without_event,
            );
        }
        if let Some(input) = self.inputs.policy_group_filter.as_ref() {
            input.update(
                cx,
                crate::subscription_input::SubscriptionTextInput::clear_without_event,
            );
        }
        self.language()
            .localized(copy::nodes::CREATING_POLICY_GROUP)
            .clone_into(&mut self.status);
        cx.notify();
    }

    pub(super) fn start_managed_policy_edit(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(group) = self
            .managed_policies
            .groups
            .iter()
            .find(|group| group.id == id)
            .cloned()
        else {
            return;
        };
        let (matcher_kind, filter, explicit_members) = match &group.matcher {
            PolicyCandidateMatcher::All => (PolicyCandidateMatcherKind::All, "", BTreeSet::new()),
            PolicyCandidateMatcher::NameContains(value) => (
                PolicyCandidateMatcherKind::NameContains,
                value.as_str(),
                BTreeSet::new(),
            ),
            PolicyCandidateMatcher::Explicit(members) => {
                (PolicyCandidateMatcherKind::Explicit, "", members.clone())
            }
        };
        self.managed_policies.editor_popover = None;
        self.managed_policies.draft = Some(ManagedPolicyDraft {
            editing_id: Some(group.id),
            icon: group.icon,
            strategy: group.strategy,
            test_interval_secs: group.test_interval_secs,
            switch_tolerance_ms: group.switch_tolerance_ms,
            matcher_kind,
            explicit_members,
        });
        if let Some(input) = self.inputs.policy_group_name.as_ref() {
            input.update(cx, |input, cx| {
                input.set_value_without_event(group.name.clone(), cx);
            });
        }
        if let Some(input) = self.inputs.policy_group_filter.as_ref() {
            input.update(cx, |input, cx| {
                input.set_value_without_event(filter.to_owned(), cx);
            });
        }
        let language = self.language();
        self.status = format!(
            "{} “{}”",
            language.localized(copy::nodes::EDITING_GROUP),
            group.name
        );
        cx.notify();
    }

    fn build_managed_policy(
        &self,
        draft: ManagedPolicyDraft,
        name: &str,
        filter: &str,
    ) -> Result<ManagedPolicyGroup, ManagedPolicyDraftError> {
        let id = draft
            .editing_id
            .clone()
            .unwrap_or_else(mihomo::new_managed_policy_id);
        let mut group =
            ManagedPolicyGroup::new(&id, name).map_err(|_| ManagedPolicyDraftError::InvalidName)?;
        if self
            .managed_policies
            .groups
            .iter()
            .any(|existing| existing.id != id && existing.name == name)
        {
            return Err(ManagedPolicyDraftError::DuplicateName);
        }
        if matches!(
            name,
            manis_profile::MANIS_GLOBAL_GROUP_NAME | "GLOBAL" | "DIRECT" | "REJECT"
        ) {
            return Err(ManagedPolicyDraftError::ReservedName);
        }
        group.icon = draft.icon;
        group.strategy = draft.strategy;
        group.switch_tolerance_ms = draft.switch_tolerance_ms;
        group
            .set_test_interval_secs(draft.test_interval_secs)
            .map_err(|_| ManagedPolicyDraftError::InvalidInterval)?;
        let matcher = match draft.matcher_kind {
            PolicyCandidateMatcherKind::All => PolicyCandidateMatcher::All,
            PolicyCandidateMatcherKind::NameContains => {
                PolicyCandidateMatcher::name_contains(filter)
                    .map_err(|_| ManagedPolicyDraftError::MissingFilter)?
            }
            PolicyCandidateMatcherKind::Explicit if draft.explicit_members.is_empty() => {
                return Err(ManagedPolicyDraftError::MissingExplicitMember);
            }
            PolicyCandidateMatcherKind::Explicit => {
                PolicyCandidateMatcher::Explicit(draft.explicit_members)
            }
        };
        let explicit = matches!(matcher, PolicyCandidateMatcher::Explicit(_));
        group
            .set_matcher(matcher)
            .map_err(|_| ManagedPolicyDraftError::NoCandidates)?;
        if !explicit && self.managed_policy_candidate_count(&group) == 0 {
            return Err(ManagedPolicyDraftError::NoCandidates);
        }
        let mut proposed = self.managed_policies.groups.clone();
        if let Some(existing) = proposed.iter_mut().find(|existing| existing.id == group.id) {
            existing.clone_from(&group);
        } else {
            proposed.push(group.clone());
        }
        mihomo::validate_managed_policy_references(&proposed)
            .map_err(|error| ManagedPolicyDraftError::InvalidReferences(error.to_string()))?;
        Ok(group)
    }

    fn save_managed_policy_from_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.save_managed_policy_with_dialog(Some(window.window_handle()), cx);
    }

    fn save_managed_policy_with_dialog(
        &mut self,
        dialog_window: Option<AnyWindowHandle>,
        cx: &mut Context<Self>,
    ) {
        if self.configuration_transfer.active {
            return;
        }
        if self.managed_policies.mutation_state.is_busy() {
            return;
        }
        let Some(draft) = self.managed_policies.draft.clone() else {
            return;
        };
        let name = self
            .inputs
            .policy_group_name
            .as_ref()
            .map(|input| input.read(cx).value().trim().to_owned())
            .unwrap_or_default();
        let filter = self
            .inputs
            .policy_group_filter
            .as_ref()
            .map(|input| input.read(cx).value().trim().to_owned())
            .unwrap_or_default();
        let language = self.language();
        let group = match self.build_managed_policy(draft, &name, &filter) {
            Ok(group) => group,
            Err(error) => {
                self.status = error.message(language);
                cx.notify();
                return;
            }
        };
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            language
                .localized(copy::nodes::COULD_NOT_DETERMINE_WHERE_TO_SAVE_POLICY_GROUPS)
                .clone_into(&mut self.status);
            cx.notify();
            return;
        };
        let group_name = group.name.clone();
        self.status = format!(
            "{} “{}”; {}",
            language.localized(copy::nodes::GROUP_SAVED),
            group_name,
            language.localized(copy::nodes::APPLYING_CHANGES)
        );
        self.managed_policies.mutation_state = ManagedPolicyMutationState::Saving;
        let runtime = self.runtime.clone();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    super::mutate_saved_sources(&runtime, &store_dir, || {
                        mihomo::save_managed_policy_in(&store_dir, &group).map(|()| group)
                    })
                })
                .await;
            let close = this
                .update(cx, |this, cx| this.finish_managed_policy_save(result, cx))
                .unwrap_or(false);
            if close && let Some(dialog_window) = dialog_window {
                let _ = cx.update_window(dialog_window, |_, window, cx| {
                    window.close_dialog(cx);
                });
            }
        })
        .detach();
        cx.notify();
    }

    fn finish_managed_policy_save(
        &mut self,
        result: Result<super::SourceMutation<ManagedPolicyGroup>, SubscriptionStoreError>,
        cx: &mut Context<Self>,
    ) -> bool {
        self.managed_policies.mutation_state = ManagedPolicyMutationState::Idle;
        let mut saved = false;
        match result {
            Ok(transaction) => {
                let language = self.language();
                transaction.apply.reconcile_proxy_mode(&mut self.proxy_mode);
                if let Some(group) = transaction.value {
                    let previous_name = self
                        .managed_policies
                        .groups
                        .iter()
                        .find(|existing| existing.id == group.id)
                        .map(|existing| existing.name.clone());
                    self.clear_managed_policy_benchmarks(
                        &group.id,
                        previous_name.as_deref(),
                        Some(&group.name),
                    );
                    if let Some(existing) = self
                        .managed_policies
                        .groups
                        .iter_mut()
                        .find(|existing| existing.id == group.id)
                    {
                        existing.clone_from(&group);
                    } else {
                        self.managed_policies.groups.push(group.clone());
                        self.managed_policies
                            .groups
                            .sort_by(|left, right| left.id.cmp(&right.id));
                    }
                    self.persist_group_benchmarks();
                    self.managed_policies.runtime_states.remove(&group.id);
                    self.managed_policies.draft = None;
                    self.managed_policies.editor_popover = None;
                    saved = true;
                    self.status = format!(
                        "{} “{}”{}",
                        language.localized(copy::nodes::GROUP_SAVED),
                        group.name,
                        transaction.apply.status_suffix(language)
                    );
                } else {
                    self.status = format!(
                        "{}{}",
                        language.localized(copy::nodes::FAILED_TO_SAVE_POLICY_GROUP),
                        transaction.apply.status_suffix_after_rollback_attempt(
                            language,
                            transaction.rollback_error.as_ref(),
                        )
                    );
                }
            }
            Err(error) => {
                self.status = format!(
                    "{}: {}",
                    self.language()
                        .localized(copy::nodes::FAILED_TO_SAVE_POLICY_GROUP),
                    copy::configuration::subscription_store_error(self.language(), error)
                );
            }
        }
        if saved {
            self.refresh_after_managed_policy_change(cx);
        }
        cx.notify();
        saved
    }

    fn refresh_after_managed_policy_change(&mut self, cx: &mut Context<Self>) {
        // The runtime catalog describes the old configuration. Render the committed local
        // groups immediately, including edits/removals, while a fresh snapshot is fetched.
        self.catalog = None;
        if matches!(self.controller, mihomo::ControllerState::Connected { .. }) {
            self.connect_mihomo(cx);
        }
    }

    fn remove_managed_policy_from_dialog(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.remove_managed_policy_with_dialog(id, Some(window.window_handle()), cx);
    }

    fn remove_managed_policy_with_dialog(
        &mut self,
        id: &str,
        dialog_window: Option<AnyWindowHandle>,
        cx: &mut Context<Self>,
    ) {
        if self.configuration_transfer.active {
            return;
        }
        if self.managed_policies.mutation_state.is_busy() {
            return;
        }
        let reference = format!("policy:{id}");
        if self.managed_policies.groups.iter().any(|group| {
            matches!(
                &group.matcher,
                PolicyCandidateMatcher::Explicit(members)
                    if members.iter().any(|member| member.source_id == reference)
            )
        }) {
            self.language()
                .localized(copy::nodes::THIS_POLICY_GROUP_IS_USED_BY_ANOTHER_POLICY_GROUP_AND)
                .clone_into(&mut self.status);
            cx.notify();
            return;
        }
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            return;
        };
        let Some(index) = self
            .managed_policies
            .groups
            .iter()
            .position(|group| group.id == id)
        else {
            return;
        };
        let language = self.language();
        let group = self.managed_policies.groups[index].clone();
        let remove_id = id.to_owned();
        self.status = format!(
            "{} “{}”; {}",
            language.localized(copy::nodes::GROUP_DELETED),
            group.name,
            language.localized(copy::nodes::APPLYING_CHANGES)
        );
        self.managed_policies.mutation_state = ManagedPolicyMutationState::Removing;
        let runtime = self.runtime.clone();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    super::mutate_saved_sources(&runtime, &store_dir, || {
                        mihomo::remove_managed_policy_in(&store_dir, &remove_id)
                            .map(|()| (remove_id, group))
                    })
                })
                .await;
            let close = this
                .update(cx, |this, cx| {
                    this.finish_managed_policy_removal(result, cx)
                })
                .unwrap_or(false);
            if close && let Some(dialog_window) = dialog_window {
                let _ = cx.update_window(dialog_window, |_, window, cx| {
                    window.close_dialog(cx);
                });
            }
        })
        .detach();
        cx.notify();
    }

    fn finish_managed_policy_removal(
        &mut self,
        result: Result<super::SourceMutation<(String, ManagedPolicyGroup)>, SubscriptionStoreError>,
        cx: &mut Context<Self>,
    ) -> bool {
        self.managed_policies.mutation_state = ManagedPolicyMutationState::Idle;
        let mut removed = false;
        match result {
            Ok(transaction) if transaction.value.is_some() => {
                self.finish_successful_managed_policy_removal(transaction);
                removed = true;
            }
            Ok(transaction) => {
                let language = self.language();
                self.status = format!(
                    "{}{}",
                    language.localized(copy::nodes::FAILED_TO_DELETE_POLICY_GROUP),
                    transaction.apply.status_suffix_after_rollback_attempt(
                        language,
                        transaction.rollback_error.as_ref(),
                    )
                );
            }
            Err(error) => {
                self.status = format!(
                    "{}: {}",
                    self.language()
                        .localized(copy::nodes::FAILED_TO_DELETE_POLICY_GROUP),
                    copy::configuration::subscription_store_error(self.language(), error)
                );
            }
        }
        if removed {
            self.refresh_after_managed_policy_change(cx);
        }
        cx.notify();
        removed
    }

    fn finish_successful_managed_policy_removal(
        &mut self,
        mut transaction: super::SourceMutation<(String, ManagedPolicyGroup)>,
    ) {
        let (deleted_id, group) = transaction
            .value
            .take()
            .expect("checked committed mutation");
        let language = self.language();
        transaction.apply.reconcile_proxy_mode(&mut self.proxy_mode);
        self.managed_policies
            .groups
            .retain(|candidate| candidate.id != deleted_id);
        self.clear_managed_policy_benchmarks(&deleted_id, Some(&group.name), None);
        self.persist_group_benchmarks();
        self.managed_policies.runtime_states.remove(&deleted_id);
        if self
            .managed_policies
            .draft
            .as_ref()
            .and_then(|draft| draft.editing_id.as_deref())
            == Some(deleted_id.as_str())
        {
            self.managed_policies.draft = None;
            self.managed_policies.editor_popover = None;
        }
        self.status = format!(
            "{} “{}”{}",
            language.localized(copy::nodes::GROUP_DELETED),
            group.name,
            transaction.apply.status_suffix(language)
        );
    }

    fn node_configuration_link(
        language: Language,
        _theme: Theme,
        cx: &mut Context<Self>,
    ) -> Button {
        action_button(
            "nodes-open-configuration",
            language.message(Message::ManageSources),
            ActionRole::Secondary,
            ControlSize::Compact,
        )
        .accessibility_label(language.localized(copy::nodes::MANAGE_SUBSCRIPTION_SOURCES))
        .on_click(cx.listener(|this, _, _, cx| {
            this.primary_workspace = PrimaryWorkspace::Configuration;
            this.language()
                .localized(copy::nodes::SUBSCRIPTION_SOURCE_CONFIGURATION_OPENED)
                .clone_into(&mut this.status);
            this.scroll_to_configuration_section(super::ConfigurationSection::ProxySources, cx);
        }))
    }

    fn node_refresh_button(
        refreshing: bool,
        language: Language,
        _theme: Theme,
        cx: &mut Context<Self>,
    ) -> Button {
        action_button(
            "nodes-refresh",
            if refreshing {
                language.localized(copy::nodes::LOADING)
            } else {
                language.message(Message::RefreshNodes)
            },
            ActionRole::Secondary,
            ControlSize::Compact,
        )
        .icon(IconName::Redo2)
        .loading(refreshing)
        .accessibility_label(language.localized(copy::nodes::REFRESH_NODE_HEALTH))
        .tab_stop(!refreshing)
        .on_click(cx.listener(move |this, _, _, cx| {
            if refreshing {
                return;
            }
            if !this.imported_subscriptions.is_empty() {
                for subscription in this
                    .imported_subscriptions
                    .iter_mut()
                    .filter(|subscription| subscription.enabled)
                {
                    let kind = super::source_kind(&subscription.source);
                    subscription.state = ImportedSubscriptionState::Pending(kind);
                }
                this.restore_imported_subscriptions(cx);
            } else if !this.saved_single_nodes.is_empty() {
                this.language()
                    .localized(copy::nodes::SAVED_NODES_DO_NOT_NEED_TO_BE_DOWNLOADED_AGAIN)
                    .clone_into(&mut this.status);
                cx.notify();
            } else {
                this.connect_mihomo(cx);
            }
        }))
    }

    fn node_filter_bar(
        counts: NodeCounts,
        selected: NodeAvailabilityFilter,
        compact: bool,
        language: Language,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let filters = [
            NodeAvailabilityFilter::All,
            NodeAvailabilityFilter::Available,
            NodeAvailabilityFilter::Unavailable,
            NodeAvailabilityFilter::Untested,
        ];
        div()
            .id("node-filter-bar")
            .flex()
            .items_center()
            .gap_2()
            .when(compact, gpui::StatefulInteractiveElement::overflow_x_scroll)
            .children(filters.into_iter().map(|filter| {
                let label = Self::availability_filter_label(filter, language);
                let active = selected == filter;
                action_button(
                    format!("node-filter-{label}"),
                    format!("{label} {}", counts.count_for(filter)),
                    if active {
                        ActionRole::Primary
                    } else {
                        ActionRole::Secondary
                    },
                    ControlSize::Compact,
                )
                .selected(active)
                .toggled(active)
                .accessibility_label(format!(
                    "{} {label}",
                    language.localized(copy::nodes::FILTER_NODES_BY)
                ))
                .flex_shrink_0()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.node_workspace.select_filter(filter);
                    let language = this.language();
                    this.status = format!(
                        "{}: {}",
                        language.localized(copy::nodes::NODE_FILTER),
                        Self::availability_filter_label(filter, language)
                    );
                    cx.notify();
                }))
            }))
    }

    fn source_group_list(
        &self,
        groups: &[NodeSourceGroup<'_>],
        filter: NodeAvailabilityFilter,
        compact: bool,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut list = div().flex().flex_col().gap_3();
        for group in groups {
            list = list.child(self.source_group(group, filter, compact, language, theme, cx));
        }
        list
    }

    fn source_group(
        &self,
        group: &NodeSourceGroup<'_>,
        filter: NodeAvailabilityFilter,
        compact: bool,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let presentation = self.source_group_presentation(group, filter, language);
        let header = Self::source_group_header(group, &presentation, compact, language, theme, cx);

        let content = if presentation.visible_count == 0 {
            div()
                .px_4()
                .py_3()
                .border_t_1()
                .border_color(theme.outline_subtle)
                .bg(theme.surface_base)
                .text_size(TextRole::Body.size())
                .line_height(TextRole::Body.line_height())
                .text_color(theme.text_secondary)
                .child(
                    language
                        .localized(copy::nodes::NO_NODES_FROM_THIS_SOURCE_MATCH_THE_CURRENT_FILTER),
                )
                .into_any_element()
        } else {
            self.source_group_table(
                group,
                &presentation.benchmark,
                NodeWorkspaceView {
                    filter,
                    compact,
                    language,
                    theme,
                },
                cx,
            )
            .into_any_element()
        };

        div()
            .rounded(Radius::Pane.px())
            .border_1()
            .border_color(theme.outline_subtle)
            .overflow_hidden()
            .child(
                Collapsible::new()
                    .open(!presentation.collapsed)
                    .child(header)
                    .content(content),
            )
    }

    fn source_group_presentation(
        &self,
        group: &NodeSourceGroup<'_>,
        filter: NodeAvailabilityFilter,
        language: Language,
    ) -> SourceGroupPresentation {
        let mut counts = NodeCounts::from_provider_refs(&group.providers);
        counts.total += group.saved_nodes.len();
        counts.untested += group.saved_nodes.len();
        let benchmark_key = Self::source_group_benchmark_key(&group.id);
        let benchmark = self
            .managed_policies
            .benchmarks
            .get(&benchmark_key)
            .cloned()
            .unwrap_or_default();
        let detail = match &benchmark {
            GroupBenchmarkState::Idle => group.detail.clone(),
            GroupBenchmarkState::Running { .. } => format!(
                "{} · {}",
                group.detail,
                language.localized(copy::nodes::TESTING)
            ),
            GroupBenchmarkState::Complete { summary, .. } => format!(
                "{} · {} {}",
                group.detail,
                language.localized(copy::nodes::TEST),
                Self::success_fraction_label(summary.succeeded, summary.total, language)
            ),
            GroupBenchmarkState::Failed { .. } => format!(
                "{} · {}",
                group.detail,
                language.localized(copy::nodes::TEST_FAILED)
            ),
        };
        SourceGroupPresentation {
            visible_count: counts.count_for(filter),
            collapsed: self.node_workspace.is_group_collapsed(&group.id),
            benchmark_key,
            benchmark,
            detail,
            total_nodes: counts.total,
        }
    }

    fn source_group_header(
        group: &NodeSourceGroup<'_>,
        presentation: &SourceGroupPresentation,
        compact: bool,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let action = if presentation.collapsed {
            language.localized(copy::common::EXPAND)
        } else {
            language.localized(copy::common::COLLAPSE)
        };
        let trigger_group_id = group.id.clone();
        let trigger = Button::new(format!("source-group-header-{}", group.id))
            .accessibility_label(format!(
                "{} {} {}",
                action,
                language.localized(copy::nodes::NODE_SOURCE),
                group.name
            ))
            .with_variant(ButtonVariant::Text)
            .h_full()
            .flex_1()
            .px_0()
            .text_color(theme.text_primary)
            .child(Self::source_group_header_content(
                group,
                presentation,
                language,
                theme,
            ))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.node_workspace.toggle_group(&trigger_group_id);
                this.persist_node_workspace();
                this.language()
                    .localized(copy::nodes::NODE_SOURCE_EXPANDED_STATE_UPDATED)
                    .clone_into(&mut this.status);
                cx.notify();
            }));
        let benchmarking = presentation.benchmark.is_running();
        let benchmark_id = group.id.clone();
        let benchmark_name = group.name.clone();
        let delay_targets = group.delay_targets();
        div()
            .id(format!("source-group-surface-{}", group.id))
            .min_h(px(58.0))
            .px(if compact { px(12.0) } else { px(16.0) })
            .py_3()
            .flex()
            .items_center()
            .gap_3()
            .rounded_tl(Radius::Pane.px())
            .rounded_tr(Radius::Pane.px())
            .when(presentation.collapsed, |header| {
                header
                    .rounded_bl(Radius::Pane.px())
                    .rounded_br(Radius::Pane.px())
            })
            .bg(theme.surface_low)
            .hover(move |header| header.bg(theme.action_soft))
            .child(Self::group_benchmark_icon(
                &presentation.benchmark_key,
                benchmarking,
                language,
                theme,
                cx.listener(move |this, _, _, cx| {
                    if !benchmarking {
                        this.start_source_group_benchmark(
                            &benchmark_id,
                            &benchmark_name,
                            delay_targets.clone(),
                            cx,
                        );
                    }
                }),
            ))
            .child(trigger)
    }

    fn source_group_header_content(
        group: &NodeSourceGroup<'_>,
        presentation: &SourceGroupPresentation,
        language: Language,
        theme: Theme,
    ) -> Div {
        div()
            .min_w(px(0.0))
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap_3()
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .child(
                        div()
                            .overflow_x_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(group.name.clone()),
                    )
                    .child(
                        div()
                            .mt(Space::Xs.px())
                            .text_size(TextRole::Metadata.size())
                            .line_height(TextRole::Metadata.line_height())
                            .text_color(theme.text_tertiary)
                            .child(presentation.detail.clone()),
                    ),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .text_size(TextRole::Metadata.size())
                            .line_height(TextRole::Metadata.line_height())
                            .text_color(theme.text_secondary)
                            .child(Self::node_count_label(presentation.total_nodes, language)),
                    )
                    .child(crate::components::disclosure_icon(
                        !presentation.collapsed,
                        theme,
                    )),
            )
    }

    fn source_group_table(
        &self,
        group: &NodeSourceGroup<'_>,
        benchmark: &GroupBenchmarkState,
        view: NodeWorkspaceView,
        cx: &mut Context<Self>,
    ) -> Div {
        let NodeWorkspaceView {
            filter,
            compact,
            language,
            theme,
        } = view;
        let mut table = div();
        if !compact {
            table = table.child(Self::node_table_header(language, theme));
        }

        for (provider_index, provider) in group.providers.iter().enumerate() {
            for (node_index, node) in provider.nodes.iter().enumerate() {
                if !filter.includes(node.alive) {
                    continue;
                }
                table = table.child(self.workspace_node_row(
                    node,
                    benchmark,
                    WorkspaceNodeRowContext {
                        row_id: format!("node-row-{}-{provider_index}-{node_index}", group.id),
                        source_id: group.id.clone(),
                        compact,
                        language,
                        theme,
                    },
                    cx,
                ));
            }
        }
        for (node_index, node) in group.saved_nodes.iter().enumerate() {
            if !filter.includes(None) {
                continue;
            }
            let loaded = LoadedProviderNode {
                name: node.name.clone(),
                protocol: node.protocol.to_owned(),
                latency_label: None,
                alive: None,
            };
            table = table.child(self.workspace_node_row(
                &loaded,
                benchmark,
                WorkspaceNodeRowContext {
                    row_id: format!(
                        "node-row-{}-{}-{node_index}",
                        group.id,
                        group.providers.len()
                    ),
                    source_id: group.id.clone(),
                    compact,
                    language,
                    theme,
                },
                cx,
            ));
        }
        table
    }

    fn node_table_header(language: Language, theme: Theme) -> Div {
        div()
            .h(ControlSize::Compact.height())
            .px_4()
            .flex()
            .items_center()
            .gap_3()
            .bg(theme.surface_low)
            .text_size(TextRole::Metadata.size())
            .line_height(TextRole::Metadata.line_height())
            .font_weight(TextRole::Metadata.weight())
            .text_color(theme.text_tertiary)
            .child(div().flex_1().child(language.localized(copy::nodes::NODE)))
            .child(
                div()
                    .w(px(100.0))
                    .child(language.localized(copy::nodes::PROTOCOL)),
            )
            .child(
                div()
                    .w(px(72.0))
                    .text_align(gpui::TextAlign::Right)
                    .child(language.localized(copy::common::LATENCY)),
            )
    }

    fn workspace_node_row(
        &self,
        node: &LoadedProviderNode,
        benchmark: &GroupBenchmarkState,
        context: WorkspaceNodeRowContext,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let WorkspaceNodeRowContext {
            row_id,
            source_id,
            compact,
            language,
            theme,
        } = context;
        let latency = benchmark.node_state(&node.name);
        let idle_latency = node.latency_label.clone().unwrap_or_else(|| "—".to_owned());
        let spinner_id = format!("{row_id}-latency");
        let global_identity = NodeIdentity::new(&source_id, &node.name).ok();
        let global_runtime_selected = self.runtime_global_target() == Some(node.name.as_str());
        let global_selected = global_identity.as_ref().is_some_and(|identity| {
            self.global_target_identity()
                .map_or(global_runtime_selected, |selected| selected == identity)
        });
        let selection_locked = self.global_selection_busy.is_some();
        let selected_name = node.name.clone();
        let row_body = if compact {
            Self::compact_node_row_content(
                node,
                latency,
                idle_latency,
                &spinner_id,
                language,
                theme,
            )
        } else {
            Self::wide_node_row_content(node, latency, idle_latency, &spinner_id, language, theme)
        };
        div()
            .id(row_id)
            .min_h(if compact { px(64.0) } else { px(52.0) })
            .px(if compact { px(12.0) } else { px(16.0) })
            .py_2()
            .flex()
            .items_center()
            .gap_3()
            .border_t_1()
            .border_color(theme.outline_subtle)
            .bg(if global_selected {
                theme.action_soft
            } else {
                theme.surface_base
            })
            .child(row_body)
            .when_some(global_identity, |row, selected_identity| {
                row.role(Role::RadioButton)
                    .aria_label(format!(
                        "{} {selected_name} {}",
                        language.localized(copy::nodes::SELECT),
                        language.localized(copy::nodes::AS_GLOBAL_EXIT)
                    ))
                    .aria_toggled(if global_selected {
                        Toggled::True
                    } else {
                        Toggled::False
                    })
                    .tab_stop(!selection_locked)
                    .focusable()
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !selection_locked {
                            this.select_global_node(selected_identity.clone(), cx);
                        }
                    }))
            })
    }

    fn compact_node_row_content(
        node: &LoadedProviderNode,
        latency: GroupBenchmarkNodeState,
        idle_latency: String,
        spinner_id: &str,
        language: Language,
        theme: Theme,
    ) -> Div {
        div()
            .min_w(px(0.0))
            .flex_1()
            .flex()
            .items_center()
            .justify_between()
            .gap_3()
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(node.name.clone()),
                    )
                    .child(
                        div()
                            .mt_1()
                            .text_size(TextRole::Metadata.size())
                            .line_height(TextRole::Metadata.line_height())
                            .text_color(theme.text_tertiary)
                            .child(node.protocol.clone()),
                    ),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .min_w(px(48.0))
                    .text_align(gpui::TextAlign::Right)
                    .child(
                        div()
                            .min_h(px(18.0))
                            .flex()
                            .items_center()
                            .justify_end()
                            .child(Self::benchmark_latency_content(
                                latency,
                                idle_latency,
                                spinner_id,
                                language,
                                theme,
                            )),
                    ),
            )
    }

    fn wide_node_row_content(
        node: &LoadedProviderNode,
        latency: GroupBenchmarkNodeState,
        idle_latency: String,
        spinner_id: &str,
        language: Language,
        theme: Theme,
    ) -> Div {
        div()
            .min_w(px(0.0))
            .flex_1()
            .flex()
            .items_center()
            .gap_3()
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(node.name.clone()),
            )
            .child(
                div()
                    .w(px(100.0))
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .text_color(theme.text_tertiary)
                    .child(node.protocol.clone()),
            )
            .child(
                div()
                    .w(px(72.0))
                    .min_h(px(18.0))
                    .flex()
                    .items_center()
                    .justify_end()
                    .child(Self::benchmark_latency_content(
                        latency,
                        idle_latency,
                        spinner_id,
                        language,
                        theme,
                    )),
            )
    }

    fn node_empty_state(
        compact: bool,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let action = action_button(
            "nodes-empty-import",
            language.message(Message::ImportSubscription),
            ActionRole::Primary,
            ControlSize::Standard,
        )
        .accessibility_label(
            language.localized(copy::nodes::GO_TO_CONFIGURATION_TO_IMPORT_A_SUBSCRIPTION),
        )
        .w(px(180.0))
        .on_click(cx.listener(|this, _, _, cx| {
            this.primary_workspace = PrimaryWorkspace::Configuration;
            this.language()
                .localized(copy::nodes::SUBSCRIPTION_SOURCE_CONFIGURATION_OPENED)
                .clone_into(&mut this.status);
            this.scroll_to_configuration_section(super::ConfigurationSection::ProxySources, cx);
        }))
        .into_any_element();

        div()
            .min_h(px(if compact { 260.0 } else { 320.0 }))
            .flex()
            .items_center()
            .child(empty_state(
                language.message(Message::NoNodes),
                language
                    .localized(copy::nodes::IMPORT_A_SUBSCRIPTION_OR_ADD_A_VLESS_NODE_NODES_WILL),
                Some(action),
                theme,
            ))
    }

    fn node_message_panel(title: &'static str, copy: &'static str, theme: Theme) -> Div {
        empty_state(title, copy, None, theme)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::{NodeCounts, NodeSourceGroup, subscription_provider_refs};
    use crate::app::{
        GroupBenchmarkNodeState, GroupBenchmarkState, GroupBenchmarkSummary,
        ManagedPolicyRuntimeState,
    };
    use crate::mihomo::{LoadedProvider, LoadedProviderNode, ProxyDelayTarget};

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
                    target: String::new(),
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
                Ok(SourceMutation {
                    value: Some(group.clone()),
                    apply: SourceRuntimeApply::MetadataOnly,
                    rollback_error: None,
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
                    target: String::new(),
                    nodes: Vec::new(),
                    rules_total: 0,
                    rules: Vec::new(),
                },
                Vec::new(),
            ));
            app.start_managed_policy_create(cx);
            assert!(!app.finish_managed_policy_save(
                Ok(SourceMutation {
                    value: None,
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
                Ok(SourceMutation {
                    value: Some(group.clone()),
                    apply: SourceRuntimeApply::MetadataOnly,
                    rollback_error: None,
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
                    target: String::new(),
                    nodes: Vec::new(),
                    rules_total: 0,
                    rules: Vec::new(),
                },
                Vec::new(),
            ));
            assert!(app.finish_managed_policy_removal(
                Ok(SourceMutation {
                    value: Some((group.id.clone(), group)),
                    apply: SourceRuntimeApply::MetadataOnly,
                    rollback_error: None,
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
    fn counts_node_availability_across_providers() {
        let providers = vec![LoadedProvider {
            name: "fixture".to_owned(),
            vehicle_type: None,
            nodes: vec![
                node(Some(true)),
                node(Some(true)),
                node(Some(false)),
                node(None),
            ],
        }];

        assert_eq!(
            NodeCounts::from_providers(&providers),
            NodeCounts {
                total: 4,
                available: 2,
                unavailable: 1,
                untested: 1,
            }
        );
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
        };

        assert_eq!(
            group.delay_targets(),
            vec![ProxyDelayTarget::provider("Subscription 1", "HK 03")]
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
        assert!(!GroupBenchmarkState::Failed { generation: 1 }.is_running());
    }

    #[test]
    fn managed_policy_runtime_rejects_unknown_selection() {
        let mut state = ManagedPolicyRuntimeState::Ready {
            generation: 4,
            current: Some("Tokyo".to_owned()),
            candidates: BTreeSet::from(["Tokyo".to_owned(), "Singapore".to_owned()]),
        };
        assert!(!state.begin_selection(5, "Unknown"));
        assert!(state.begin_selection(5, "Singapore"));
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
