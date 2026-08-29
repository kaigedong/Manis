#[derive(Clone, Copy)]
struct PolicyGroupIconView<'a> {
    id: &'a str,
    icon: ManagedPolicyIcon,
    policy_name: &'a str,
    benchmarkable: bool,
    running: bool,
    language: Language,
    theme: Theme,
}

impl ManisApp {
    fn persist_node_workspace(&mut self) {
        let Some(store_dir) = self.subscription_store_dir.as_ref() else {
            return;
        };
        if let Err(error) =
            mihomo::save_collapsed_groups_in(store_dir, self.node_workspace.collapsed_group_ids())
        {
            self.source_store_error = Some(error);
            self.language()
                .localized(copy::app::COULD_NOT_SAVE_NODE_SOURCE_EXPANSION)
                .clone_into(&mut self.status);
        }
    }

    fn theme(&self) -> Theme {
        if self.dark {
            Theme::dark()
        } else {
            Theme::light()
        }
    }

    fn policy_groups(&self) -> impl Iterator<Item = &PolicyGroup> {
        self.catalog.iter().flat_map(PolicyCatalog::iter)
    }

    fn selected_policy(&self) -> Option<&PolicyGroup> {
        self.catalog
            .as_ref()
            .map(|catalog| catalog.select(self.workspace.selected_group.as_ref()))
    }

    fn policy_group_benchmarkable(group: &PolicyGroup) -> bool {
        !group.nodes.is_empty()
    }

    fn source_group_benchmark_key(id: &str) -> String {
        format!("source:{id}")
    }

    fn managed_policy_benchmark_key(id: &str) -> String {
        format!("user:{id}")
    }

    fn policy_group_benchmark_key(id: &manis_core::PolicyGroupId) -> String {
        format!("policy:{}", id.as_str())
    }

    fn begin_group_benchmark(&mut self, key: String) -> Option<u64> {
        if self.managed_policies.active_benchmark_generation.is_some() {
            return None;
        }
        self.managed_policies.benchmark_generation =
            self.managed_policies.benchmark_generation.wrapping_add(1);
        let generation = self.managed_policies.benchmark_generation;
        self.managed_policies
            .benchmarks
            .insert(key, GroupBenchmarkState::running(generation));
        self.managed_policies.active_benchmark_generation = Some(generation);
        Some(generation)
    }

    fn poll_group_benchmark_progress(
        &mut self,
        generation: u64,
        key: String,
        updates: GroupBenchmarkProgressQueue,
        cx: &mut Context<Self>,
    ) {
        let drained = updates
            .lock()
            .map(|mut updates| updates.drain(..).collect::<Vec<_>>())
            .unwrap_or_default();
        let mut changed = false;
        if let Some(state) = self.managed_policies.benchmarks.get_mut(&key) {
            for (name, delay) in drained {
                changed |= state.record(generation, &name, delay);
            }
        }
        if changed {
            cx.notify();
        }
        if self.managed_policies.active_benchmark_generation != Some(generation) {
            return;
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(40))
                .await;
            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    this.poll_group_benchmark_progress(generation, key, updates, cx);
                });
            }
        })
        .detach();
    }

    fn policy_icon_visual(
        icon: ManagedPolicyIcon,
        policy_name: &str,
        size: f32,
        theme: Theme,
    ) -> Div {
        let glyph = match icon {
            ManagedPolicyIcon::None => Self::policy_initial_glyph(policy_name, theme),
            ManagedPolicyIcon::Bolt => Self::policy_bolt_glyph(theme),
            ManagedPolicyIcon::Globe => Self::policy_globe_glyph(theme),
            ManagedPolicyIcon::Shield => Self::policy_shield_glyph(theme),
            ManagedPolicyIcon::Compass => Self::policy_compass_glyph(theme),
        };
        div()
            .size(px(size))
            .rounded_full()
            .bg(theme.action_soft)
            .flex()
            .items_center()
            .justify_center()
            .child(glyph)
    }

    fn policy_initial_glyph(policy_name: &str, theme: Theme) -> Div {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .font_weight(FontWeight::BOLD)
            .text_color(theme.action_primary)
            .child(
                policy_name
                    .chars()
                    .next()
                    .map_or_else(|| "?".to_owned(), |character| character.to_string()),
            )
    }

    fn policy_bolt_glyph(theme: Theme) -> Div {
        let color = theme.action_primary;
        div()
            .relative()
            .size(px(20.0))
            .child(
                div()
                    .absolute()
                    .left(px(9.0))
                    .top(px(1.0))
                    .w(px(5.0))
                    .h(px(8.0))
                    .rounded_sm()
                    .bg(color),
            )
            .child(
                div()
                    .absolute()
                    .left(px(6.0))
                    .top(px(7.0))
                    .w(px(8.0))
                    .h(px(6.0))
                    .rounded_sm()
                    .bg(color),
            )
            .child(
                div()
                    .absolute()
                    .left(px(6.0))
                    .top(px(12.0))
                    .w(px(5.0))
                    .h(px(7.0))
                    .rounded_sm()
                    .bg(color),
            )
    }

    fn policy_globe_glyph(theme: Theme) -> Div {
        let color = theme.action_primary;
        div()
            .relative()
            .size(px(20.0))
            .rounded_full()
            .border_2()
            .border_color(color)
            .child(
                div()
                    .absolute()
                    .left(px(7.0))
                    .top(px(1.0))
                    .w(px(2.0))
                    .h(px(14.0))
                    .rounded_full()
                    .bg(color),
            )
            .child(
                div()
                    .absolute()
                    .left(px(1.0))
                    .top(px(7.0))
                    .w(px(14.0))
                    .h(px(2.0))
                    .rounded_full()
                    .bg(color),
            )
    }

    fn policy_shield_glyph(theme: Theme) -> Div {
        div()
            .size(px(19.0))
            .rounded_md()
            .border_2()
            .border_color(theme.action_primary)
            .flex()
            .items_center()
            .justify_center()
            .child(div().size(px(7.0)).rounded_full().bg(theme.action_primary))
    }

    fn policy_compass_glyph(theme: Theme) -> Div {
        div()
            .relative()
            .size(px(20.0))
            .rounded_full()
            .border_2()
            .border_color(theme.action_primary)
            .child(
                div()
                    .absolute()
                    .left(px(7.0))
                    .top(px(3.0))
                    .w(px(3.0))
                    .h(px(10.0))
                    .rounded_full()
                    .bg(theme.action_primary),
            )
            .child(
                div()
                    .absolute()
                    .left(px(5.0))
                    .top(px(7.0))
                    .size(px(7.0))
                    .rounded_full()
                    .border_2()
                    .border_color(theme.surface_high),
            )
    }

    fn policy_group_icon(
        view: PolicyGroupIconView<'_>,
        listener: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
    ) -> Stateful<Div> {
        let PolicyGroupIconView {
            id,
            icon,
            policy_name,
            benchmarkable,
            running,
            language,
            theme,
        } = view;
        div()
            .id(format!("policy-icon-{id}"))
            .size(px(38.0))
            .flex_shrink_0()
            .rounded_full()
            .when(benchmarkable, |avatar| {
                avatar
                    .role(Role::Button)
                    .aria_label(language.localized(if running {
                        copy::nodes::POLICY_BENCHMARK_IN_PROGRESS
                    } else {
                        copy::nodes::TEST_POLICY_CANDIDATE_LATENCY
                    }))
                    .tab_stop(!running)
                    .focusable()
                    .on_click(listener)
            })
            .when(benchmarkable && !running, gpui::Styled::cursor_pointer)
            .when(running, |avatar| {
                avatar
                    .bg(theme.action_soft)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(Self::benchmark_latency_spinner(
                        format!("{id}-policy-icon-spinner"),
                        theme,
                    ))
            })
            .when(!running, |avatar| {
                avatar.child(Self::policy_icon_visual(icon, policy_name, 38.0, theme))
            })
    }

    fn group_benchmark_icon(
        id: &str,
        running: bool,
        language: Language,
        theme: Theme,
        listener: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
    ) -> Stateful<Div> {
        let bar_color = if running {
            theme.action_primary
        } else {
            theme.text_secondary
        };
        div()
            .id(format!("group-benchmark-{id}"))
            .role(Role::Button)
            .aria_label(language.localized(if running {
                copy::nodes::GROUP_BENCHMARK_IN_PROGRESS
            } else {
                copy::nodes::TEST_GROUP_LATENCY
            }))
            .tab_stop(!running)
            .focusable()
            .size(px(30.0))
            .flex_shrink_0()
            .rounded_md()
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_high)
            .flex()
            .justify_center()
            .when(running, |button| {
                button.items_center().child(Self::benchmark_latency_spinner(
                    format!("{id}-button-spinner"),
                    theme,
                ))
            })
            .when(!running, |button| {
                button
                    .items_end()
                    .gap(px(2.0))
                    .pb(px(7.0))
                    .child(div().w(px(2.0)).h(px(5.0)).rounded_full().bg(bar_color))
                    .child(div().w(px(2.0)).h(px(9.0)).rounded_full().bg(bar_color))
                    .child(div().w(px(2.0)).h(px(13.0)).rounded_full().bg(bar_color))
            })
            .when(!running, gpui::Styled::cursor_pointer)
            .on_click(listener)
    }

    fn benchmark_latency_content(
        state: GroupBenchmarkNodeState,
        idle_label: String,
        spinner_id: &str,
        language: Language,
        theme: Theme,
    ) -> Div {
        let cell = div()
            .min_w(px(42.0))
            .flex()
            .items_center()
            .justify_end()
            .font_weight(FontWeight::SEMIBOLD)
            .text_size(TextRole::Data.size())
            .line_height(TextRole::Data.line_height());
        match state {
            GroupBenchmarkNodeState::Idle => cell.text_color(theme.text_tertiary).child(idle_label),
            GroupBenchmarkNodeState::Pending => cell.child(Self::benchmark_latency_spinner(
                spinner_id.to_owned(),
                theme,
            )),
            GroupBenchmarkNodeState::Measured(delay) => cell
                .text_color(theme.status_success)
                .child(format!("{delay} ms")),
            GroupBenchmarkNodeState::Failed => cell
                .text_color(theme.route_trace)
                .child(language.localized(copy::nodes::BENCHMARK_FAILED)),
        }
    }

    fn benchmark_latency_spinner(id: String, theme: Theme) -> impl IntoElement {
        div().id(id).size(px(14.0)).child(
            Spinner::new()
                .with_size(px(14.0))
                .color(theme.action_primary.into()),
        )
    }

    fn policy_benchmark_status(
        language: Language,
        kind: manis_core::PolicyGroupKind,
        current: Option<&str>,
        summary: GroupBenchmarkSummary,
    ) -> String {
        copy::app::policy_benchmark_complete(
            language,
            kind.is_automatic(),
            current,
            summary.succeeded,
            summary.total,
        )
    }

    fn policy_group_benchmark_feedback(
        language: Language,
        state: &GroupBenchmarkState,
        total: usize,
        theme: Theme,
    ) -> Option<Div> {
        let (label, color) = match state {
            GroupBenchmarkState::Idle => return None,
            GroupBenchmarkState::Running { results, .. } => (
                copy::app::benchmark_progress(language, results.len(), total),
                theme.action_primary,
            ),
            GroupBenchmarkState::Complete { summary, .. } => (
                copy::app::benchmark_complete(
                    language,
                    summary.succeeded,
                    summary.total,
                    summary.minimum_ms,
                    summary.average_ms,
                ),
                theme.status_success,
            ),
            GroupBenchmarkState::Failed { .. } => (
                language
                    .localized(
                        copy::app::LATENCY_TEST_FAILED_THIS_POLICY_GROUP_RETURNED_NO_DELAY_DATA,
                    )
                    .to_owned(),
                theme.route_trace,
            ),
        };
        Some(
            div()
                .mt(Space::Sm.px())
                .text_size(TextRole::Metadata.size())
                .line_height(TextRole::Metadata.line_height())
                .font_weight(TextRole::Label.weight())
                .text_color(color)
                .child(label),
        )
    }

    fn selected_node(&self) -> Option<PolicyNode> {
        let policy = self.selected_policy()?;
        Some(self.node_for_policy(policy))
    }

    fn node_for_policy(&self, policy: &PolicyGroup) -> PolicyNode {
        let selected = if policy.kind.allows_manual_selection() {
            self.workspace
                .selection_for(&policy.id)
                .and_then(|selected| policy.nodes.iter().find(|node| node.id == *selected))
                .or_else(|| policy.nodes.iter().find(|node| node.name == policy.target))
        } else {
            policy.nodes.iter().find(|node| node.name == policy.target)
        };
        selected
            .or_else(|| policy.nodes.first())
            .cloned()
            .unwrap_or_else(|| PolicyNode {
                id: ProxyId::new("unavailable"),
                name: self
                    .language()
                    .localized(copy::app::NO_AVAILABLE_NODES)
                    .to_owned(),
                kind: manis_core::PolicyCandidateKind::Node,
                provider: None,
                detail: self
                    .language()
                    .localized(copy::app::THE_KERNEL_RETURNED_NO_GROUP_MEMBERS)
                    .to_owned(),
                latency_ms: None,
                alive: None,
            })
    }

    fn policy_node_source_label(&self, node: &PolicyNode, language: Language) -> String {
        if node.kind == manis_core::PolicyCandidateKind::PolicyGroup {
            return language.message(Message::PolicyGroup).to_owned();
        }

        if let Some(index) = node
            .provider
            .as_deref()
            .and_then(managed_subscription_provider_index)
            && let Some(subscription) = self
                .imported_subscriptions
                .iter()
                .filter(|subscription| subscription.enabled)
                .nth(index)
        {
            return subscription.name.clone();
        }

        if let Some(provider) = node.provider.as_ref() {
            return provider.clone();
        }

        if self
            .saved_single_nodes
            .iter()
            .filter(|saved| saved.enabled)
            .any(|saved| saved.source.preview().name == node.name)
        {
            return language.localized(copy::common::SAVED).to_owned();
        }

        if let Some((_index, subscription)) = self
            .imported_subscriptions
            .iter()
            .enumerate()
            .filter(|(_, subscription)| subscription.enabled)
            .find(|(_, subscription)| {
                subscription.providers.iter().any(|provider| {
                    provider
                        .nodes
                        .iter()
                        .any(|candidate| candidate.name == node.name)
                })
            })
        {
            return subscription.name.clone();
        }

        language
            .localized(copy::app::LOCAL_CONFIGURATION)
            .to_owned()
    }
}
