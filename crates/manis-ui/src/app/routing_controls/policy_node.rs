use super::{
    BTreeSet, Context, ControllerRuntime, ControllerState, LogLevel, ManagedPolicyRuntimeState,
    ManagedPolicyStrategy, ManisApp, NodeIdentity, PolicyGroupId, PolicySelectionRequest, ProxyId,
    begin_operation, copy, mihomo, policy_target_is_selectable, record_operation,
};

impl ManisApp {
    pub(in crate::app) fn select_policy_node(
        &mut self,
        request: PolicySelectionRequest,
        cx: &mut Context<Self>,
    ) {
        if self.configuration_transfer.active {
            return;
        }
        let PolicySelectionRequest {
            group_id,
            group_name,
            node_id,
            node_name,
        } = request;
        let language = self.language();
        let operation = begin_operation(
            "policy.node.requested",
            format!("group={group_name} candidate_selected=true"),
        );
        if self.reject_policy_selection_request(&group_id, &group_name, &node_name, operation, cx) {
            return;
        }

        let Some(previous) = self.persist_policy_selection(
            &group_id,
            &group_name,
            &node_id,
            &node_name,
            operation,
            cx,
        ) else {
            return;
        };

        let can_apply_now = matches!(self.controller, ControllerState::Connected { .. })
            && matches!(&*self.runtime, ControllerRuntime::Managed { .. });
        if !can_apply_now {
            self.status = copy::app::deferred_policy_selection(
                language,
                &group_name,
                Self::policy_candidate_display_name(&node_name),
            );
            cx.notify();
            return;
        }

        if let Some((stored_group_id, candidates)) = self
            .managed_policies
            .groups
            .iter()
            .find(|group| group.name == group_name)
            .map(|group| {
                (
                    group.id.clone(),
                    self.managed_policy_candidate_names(group)
                        .into_iter()
                        .collect::<BTreeSet<_>>(),
                )
            })
        {
            self.managed_policies.runtime_generation =
                self.managed_policies.runtime_generation.wrapping_add(1);
            let generation = self.managed_policies.runtime_generation;
            let state = self
                .managed_policies
                .runtime_states
                .entry(stored_group_id)
                .or_default();
            if !state.begin_selection(generation, &node_name) {
                *state = ManagedPolicyRuntimeState::Selecting {
                    generation,
                    current: previous.policy_target(&group_name).map(str::to_owned),
                    candidates,
                    pending: node_name.clone(),
                };
            }
        }
        self.policy_selection_busy = Some(node_name.clone());
        self.status = copy::app::setting_policy_node(
            language,
            &group_name,
            Self::policy_candidate_display_name(&node_name),
        );
        let completion = PolicySelectionRequest {
            group_id,
            group_name: group_name.clone(),
            node_id,
            node_name: node_name.clone(),
        };
        let runtime = self.runtime.clone();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn({
                    let group_name = group_name.clone();
                    let node_name = node_name.clone();
                    async move { runtime.select_policy_candidate(&group_name, &node_name) }
                })
                .await;
            this.update(cx, |this, cx| {
                this.finish_policy_node_selection(completion, operation, result, cx);
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn reject_policy_selection_request(
        &mut self,
        group_id: &PolicyGroupId,
        group_name: &str,
        node_name: &str,
        operation: u64,
        cx: &mut Context<Self>,
    ) -> bool {
        let language = self.language();
        if self.policy_selection_busy.is_some() {
            record_operation(
                operation,
                LogLevel::Warn,
                "policy.node.ignored",
                "reason=selection_busy",
            );
            return true;
        }
        let stored_group = self
            .managed_policies
            .groups
            .iter()
            .find(|group| group.id == group_id.as_str() || group.name == group_name);
        if !matches!(self.controller, ControllerState::Connected { .. }) && stored_group.is_none() {
            record_operation(
                operation,
                LogLevel::Warn,
                "policy.node.rejected",
                "reason=runtime_policy_unavailable",
            );
            language
                .localized(copy::app::START_MIHOMO_BEFORE_SELECTING_A_NODE_FOR_THIS_POLICY_GROUP)
                .clone_into(&mut self.status);
            cx.notify();
            return true;
        }
        let catalog_allows = self
            .policy_groups()
            .find(|group| group.id == *group_id || group.name == group_name)
            .map(|group| {
                group.kind.allows_manual_selection()
                    && group.nodes.iter().any(|node| node.name == node_name)
            });
        let stored_group_allows = stored_group.map(|group| {
            group.strategy == ManagedPolicyStrategy::Manual
                && self
                    .managed_policy_candidate_names(group)
                    .iter()
                    .any(|candidate| candidate == node_name)
        });
        if policy_target_is_selectable(
            matches!(self.controller, ControllerState::Connected { .. }),
            catalog_allows,
            stored_group_allows,
        ) {
            return false;
        }
        record_operation(
            operation,
            LogLevel::Error,
            "policy.node.rejected",
            "reason=not_manual_candidate",
        );
        language
            .localized(copy::app::ONLY_A_CANDIDATE_INSIDE_A_MANUAL_POLICY_CAN_BE_SELECTED)
            .clone_into(&mut self.status);
        cx.notify();
        true
    }

    fn persist_policy_selection(
        &mut self,
        group_id: &PolicyGroupId,
        group_name: &str,
        node_id: &ProxyId,
        node_name: &str,
        operation: u64,
        cx: &mut Context<Self>,
    ) -> Option<mihomo::NodeSelectionPreferences> {
        let language = self.language();
        let previous = self.managed_policies.node_selections.clone();
        if let Err(error) = self
            .managed_policies
            .node_selections
            .set_policy_target(group_name, node_name)
        {
            record_operation(
                operation,
                LogLevel::Error,
                "policy.node.rejected",
                error.to_string(),
            );
            language
                .localized(copy::app::THIS_POLICY_SELECTION_CANNOT_BE_SAVED)
                .clone_into(&mut self.status);
            cx.notify();
            return None;
        }
        if let Some(directory) = self.subscription_store_dir.as_deref()
            && let Err(error) = mihomo::save_node_selection_preferences_in(
                directory,
                &self.managed_policies.node_selections,
            )
        {
            self.managed_policies.node_selections = previous;
            record_operation(
                operation,
                LogLevel::Error,
                "policy.node.persistence_failed",
                error.to_string(),
            );
            self.status = format!(
                "{}{error}",
                language.localized(copy::app::COULD_NOT_SAVE_THE_POLICY_SELECTION)
            );
            cx.notify();
            return None;
        }
        let catalog_selection = self
            .policy_groups()
            .find(|group| group.name == group_name)
            .and_then(|group| {
                group
                    .nodes
                    .iter()
                    .find(|node| node.name == node_name)
                    .map(|node| (group.id.clone(), node.id.clone()))
            });
        let catalog_updated = self.update_catalog_selector_target(group_name, node_name, operation);
        if catalog_updated && self.workspace.selected_group.as_ref() == Some(group_id) {
            self.workspace.select_node(node_id.clone());
        } else if catalog_updated
            && let Some((catalog_group_id, catalog_node_id)) = catalog_selection
            && self.workspace.selected_group.as_ref() == Some(&catalog_group_id)
        {
            self.workspace.select_node(catalog_node_id);
        }
        record_operation(
            operation,
            LogLevel::Info,
            "policy.node.saved",
            format!("group={group_name}"),
        );
        Some(previous)
    }

    fn finish_policy_node_selection(
        &mut self,
        request: PolicySelectionRequest,
        operation: u64,
        result: Result<mihomo::ManagedPolicyRuntimeSnapshot, mihomo::LoadError>,
        cx: &mut Context<Self>,
    ) {
        let PolicySelectionRequest {
            group_id,
            group_name,
            node_id,
            node_name,
        } = request;
        self.policy_selection_busy = None;
        match result {
            Ok(snapshot) => {
                let current = snapshot
                    .current
                    .clone()
                    .unwrap_or_else(|| node_name.clone());
                let catalog_updated =
                    self.update_catalog_selector_target(&group_name, &current, operation);
                if catalog_updated && self.workspace.selected_group.as_ref() == Some(&group_id) {
                    self.workspace.select_node(node_id);
                }
                if let Some(stored_group) = self
                    .managed_policies
                    .groups
                    .iter()
                    .find(|group| group.name == group_name)
                {
                    self.managed_policies.runtime_generation =
                        self.managed_policies.runtime_generation.wrapping_add(1);
                    self.managed_policies.runtime_states.insert(
                        stored_group.id.clone(),
                        ManagedPolicyRuntimeState::Ready {
                            generation: self.managed_policies.runtime_generation,
                            current: snapshot.current,
                            candidates: snapshot.candidates,
                        },
                    );
                }
                record_operation(
                    operation,
                    LogLevel::Info,
                    "policy.node.succeeded",
                    format!("group={group_name}"),
                );
                self.status = copy::app::policy_selection_applied(
                    self.language(),
                    &group_name,
                    Self::policy_candidate_display_name(&current),
                );
            }
            Err(error) => {
                record_operation(
                    operation,
                    LogLevel::Error,
                    "policy.node.failed",
                    error.to_string(),
                );
                self.status = copy::app::policy_selection_apply_failed(
                    self.language(),
                    &group_name,
                    Self::policy_candidate_display_name(&node_name),
                    &error.to_string(),
                );
            }
        }
        cx.notify();
    }

    fn update_catalog_selector_target(
        &mut self,
        group_name: &str,
        node_name: &str,
        operation: u64,
    ) -> bool {
        let Some(catalog) = self.catalog.as_mut() else {
            return true;
        };
        if catalog.apply_selector_target(group_name, node_name) {
            return true;
        }
        record_operation(
            operation,
            LogLevel::Error,
            "policy.node.catalog_sync_failed",
            format!("group={group_name}"),
        );
        false
    }

    pub(in crate::app) fn global_target_identity(&self) -> Option<&NodeIdentity> {
        self.managed_policies.node_selections.global()
    }

    pub(in crate::app) fn global_target(&self) -> Option<&str> {
        self.global_target_identity()
            .map(|identity| identity.node_name.as_str())
            .or_else(|| self.runtime_global_target())
    }

    pub(in crate::app) fn runtime_global_target(&self) -> Option<&str> {
        self.policy_groups()
            .find(|group| group.name.eq_ignore_ascii_case("GLOBAL"))
            .and_then(|group| group.target.as_deref())
    }
}
