use super::{
    AnyWindowHandle, AppContext, BTreeSet, Context, ManagedPolicyDraft, ManagedPolicyDraftError,
    ManagedPolicyGroup, ManagedPolicyIcon, ManagedPolicyMutationState, ManagedPolicyStrategy,
    ManisApp, PolicyCandidateMatcher, PolicyCandidateMatcherKind, SubscriptionStoreError, Window,
    WindowExt, copy, mihomo,
};

impl ManisApp {
    pub(in crate::app) fn start_managed_policy_create(&mut self, cx: &mut Context<Self>) {
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

    pub(in crate::app) fn start_managed_policy_edit(&mut self, id: &str, cx: &mut Context<Self>) {
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

    pub(in crate::app) fn save_managed_policy_from_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.save_managed_policy_with_dialog(Some(window.window_handle()), cx);
    }

    pub(in crate::app) fn save_managed_policy_with_dialog(
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
                    crate::app::mutate_saved_sources(&runtime, &store_dir, |store_dir| {
                        mihomo::save_managed_policy_in(store_dir, &group).map(|()| group)
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

    pub(in crate::app) fn finish_managed_policy_save(
        &mut self,
        result: Result<crate::app::SourceMutation<ManagedPolicyGroup>, SubscriptionStoreError>,
        cx: &mut Context<Self>,
    ) -> bool {
        self.managed_policies.mutation_state = ManagedPolicyMutationState::Idle;
        let mut saved = false;
        match result {
            Ok(crate::app::SourceMutation::Committed {
                value: group,
                apply,
            }) => {
                let language = self.language();
                apply.reconcile_proxy_mode(&mut self.proxy_mode);
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
                    apply.status_suffix(language)
                );
            }
            Ok(crate::app::SourceMutation::RollbackAttempted {
                apply,
                rollback_error,
            }) => {
                let language = self.language();
                apply.reconcile_proxy_mode(&mut self.proxy_mode);
                self.status = format!(
                    "{}{}",
                    language.localized(copy::nodes::FAILED_TO_SAVE_POLICY_GROUP),
                    apply.status_suffix_after_rollback_attempt(language, rollback_error.as_ref())
                );
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

    pub(in crate::app) fn refresh_after_managed_policy_change(&mut self, cx: &mut Context<Self>) {
        // The runtime catalog describes the old configuration. Render the committed local
        // groups immediately, including edits/removals, while a fresh snapshot is fetched.
        self.catalog = None;
        if matches!(self.controller, mihomo::ControllerState::Connected { .. }) {
            self.connect_mihomo(cx);
        }
    }

    pub(in crate::app) fn remove_managed_policy_from_dialog(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.remove_managed_policy_with_dialog(id, Some(window.window_handle()), cx);
    }

    pub(in crate::app) fn remove_managed_policy_with_dialog(
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
                    crate::app::mutate_saved_sources(&runtime, &store_dir, |store_dir| {
                        mihomo::remove_managed_policy_in(store_dir, &remove_id)
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

    pub(in crate::app) fn finish_managed_policy_removal(
        &mut self,
        result: Result<
            crate::app::SourceMutation<(String, ManagedPolicyGroup)>,
            SubscriptionStoreError,
        >,
        cx: &mut Context<Self>,
    ) -> bool {
        self.managed_policies.mutation_state = ManagedPolicyMutationState::Idle;
        let mut removed = false;
        match result {
            Ok(crate::app::SourceMutation::Committed {
                value: (deleted_id, group),
                apply,
            }) => {
                self.finish_successful_managed_policy_removal(&deleted_id, &group, &apply);
                removed = true;
            }
            Ok(crate::app::SourceMutation::RollbackAttempted {
                apply,
                rollback_error,
            }) => {
                let language = self.language();
                self.status = format!(
                    "{}{}",
                    language.localized(copy::nodes::FAILED_TO_DELETE_POLICY_GROUP),
                    apply.status_suffix_after_rollback_attempt(language, rollback_error.as_ref())
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

    pub(in crate::app) fn finish_successful_managed_policy_removal(
        &mut self,
        deleted_id: &str,
        group: &ManagedPolicyGroup,
        apply: &crate::app::SourceRuntimeApply,
    ) {
        let language = self.language();
        apply.reconcile_proxy_mode(&mut self.proxy_mode);
        self.managed_policies
            .groups
            .retain(|candidate| candidate.id != deleted_id);
        self.clear_managed_policy_benchmarks(deleted_id, Some(&group.name), None);
        self.persist_group_benchmarks();
        self.managed_policies.runtime_states.remove(deleted_id);
        if self
            .managed_policies
            .draft
            .as_ref()
            .and_then(|draft| draft.editing_id.as_deref())
            == Some(deleted_id)
        {
            self.managed_policies.draft = None;
            self.managed_policies.editor_popover = None;
        }
        self.status = format!(
            "{} “{}”{}",
            language.localized(copy::nodes::GROUP_DELETED),
            group.name,
            apply.status_suffix(language)
        );
    }
}
