impl ManisApp {
    fn open_manual_rule_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.manual_rule_editor_state.is_open() {
            if let Some(condition) = self.manual_rule_conditions.first() {
                condition.input.focus_handle(cx).focus(window, cx);
            }
            return;
        }
        self.manual_rule_editor_state = super::ManualRuleEditorState::Creating;
        self.manual_rule_popover = None;
        self.manual_rule_error = None;
        self.manual_rule_condition_count = 1;
        for condition in &self.manual_rule_conditions {
            condition
                .input
                .update(cx, SubscriptionTextInput::clear_without_event);
        }
        for (index, condition) in self.manual_rule_conditions.iter_mut().enumerate() {
            condition.kind = if index == 1 {
                crate::manual_rule::ManualRuleKind::DstPort
            } else {
                crate::manual_rule::ManualRuleKind::default()
            };
        }
        if let Some(target) = self.manual_rule_targets().first() {
            self.manual_rule_target.clone_from(target);
        }
        self.open_manual_rule_dialog(window, cx);
    }

    fn open_manual_rule_editor_for_edit(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.manual_rule_editor_state.is_open() {
            return;
        }
        let Some(rule) = self.manual_rules.get(index).cloned() else {
            return;
        };
        self.manual_rule_editor_state = super::ManualRuleEditorState::Editing(index);
        self.manual_rule_popover = None;
        self.manual_rule_error = None;
        self.manual_rule_condition_count = if rule.is_final() {
            1
        } else {
            rule.conditions().len()
        };
        for (condition_index, editor) in self.manual_rule_conditions.iter_mut().enumerate() {
            if rule.is_final() && condition_index == 0 {
                editor.kind = crate::manual_rule::ManualRuleKind::Final;
                editor
                    .input
                    .update(cx, SubscriptionTextInput::clear_without_event);
            } else if let Some(condition) = rule.conditions().get(condition_index) {
                editor.kind = condition.kind();
                editor.input.update(cx, |input, cx| {
                    input.set_value_without_event(condition.parameter().to_owned(), cx);
                });
            } else {
                editor
                    .input
                    .update(cx, SubscriptionTextInput::clear_without_event);
            }
        }
        let targets = self.manual_rule_targets();
        self.manual_rule_target = if targets.iter().any(|target| target == rule.target()) {
            rule.target().to_owned()
        } else if rule.target() == "Proxy" {
            self.managed_policies
                .groups
                .first()
                .map_or_else(|| "DIRECT".to_owned(), |group| group.name.clone())
        } else {
            targets
                .first()
                .cloned()
                .unwrap_or_else(|| "DIRECT".to_owned())
        };
        self.open_manual_rule_dialog(window, cx);
    }

    fn open_manual_rule_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let app = cx.entity();
        window.open_dialog(cx, move |dialog, window, cx| {
            app.update(cx, |this, cx| {
                let width = window.viewport_size().width.as_f32();
                let size_class = WindowSizeClass::for_width(width);
                let theme = this.theme();
                this.ensure_manual_rule_input(theme, window, cx);
                this.manual_rule_editor_modal(
                    dialog,
                    theme,
                    this.language(),
                    size_class == WindowSizeClass::Compact,
                    window,
                    cx,
                )
            })
        });
        if let Some(condition) = self.manual_rule_conditions.first() {
            condition.input.focus_handle(cx).focus(window, cx);
        }
        cx.notify();
    }

    fn reset_manual_rule_editor_state(&mut self) {
        self.manual_rule_editor_state = super::ManualRuleEditorState::Closed;
        self.manual_rule_popover = None;
        self.manual_rule_error = None;
    }

    fn close_manual_rule_editor(&mut self, cx: &mut Context<Self>) {
        self.reset_manual_rule_editor_state();
        cx.notify();
    }

    pub(super) fn ensure_manual_rule_input(
        &mut self,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self
            .manual_rule_targets()
            .contains(&self.manual_rule_target)
        {
            self.manual_rule_target = self.manual_rule_targets().remove(0);
        }
        if !self.manual_rule_conditions.is_empty() {
            for condition in &self.manual_rule_conditions {
                let placeholder = manual_rule_placeholder(condition.kind, self.language());
                condition.input.update(cx, |input, cx| {
                    input.set_theme(theme, self.dark, cx);
                    input.set_placeholder(placeholder, cx);
                });
            }
            return;
        }
        self.manual_rule_conditions = (0..crate::manual_rule::MAX_CONDITIONS)
            .map(|index| {
                let kind = if index == 1 {
                    crate::manual_rule::ManualRuleKind::DstPort
                } else {
                    crate::manual_rule::ManualRuleKind::default()
                };
                let placeholder = manual_rule_placeholder(kind, self.language());
                let input = cx.new(|cx| {
                    SubscriptionTextInput::new_field(
                        TextInputSpec::new(
                            format!("manual-rule-parameter-{index}"),
                            placeholder,
                            MAX_MANUAL_RULE_INPUT_BYTES,
                            theme,
                            self.dark,
                        ),
                        window,
                        cx,
                    )
                });
                super::ManualRuleConditionEditor { kind, input }
            })
            .collect();
        let Some(store_dir) = self.subscription_store_dir.as_ref() else {
            return;
        };
        match crate::manual_rule::load_manual_rules_in(store_dir) {
            Ok(rules) => {
                self.manual_rules = rules;
                self.sync_routing_rule_group_order();
            }
            Err(error) => {
                self.status = format!(
                    "{}{}",
                    self.language()
                        .localized(copy::configuration::COULD_NOT_READ_MANUAL_RULES),
                    copy::configuration::manual_rule_store_error(self.language(), error)
                );
            }
        }
        cx.notify();
    }

    fn manual_rule_targets(&self) -> Vec<String> {
        let mut targets = self
            .managed_policies
            .groups
            .iter()
            .map(|group| group.name.clone())
            .collect::<Vec<_>>();
        targets.extend(["DIRECT".to_owned(), "REJECT".to_owned()]);
        targets
    }

    fn set_manual_rule_kind(
        &mut self,
        condition_index: usize,
        kind: crate::manual_rule::ManualRuleKind,
        cx: &mut Context<Self>,
    ) {
        if !kind.supported_by(self.runtime.kind()) {
            self.manual_rule_error = Some(crate::manual_rule::ManualRuleError::UnsupportedByKernel);
            self.manual_rule_popover = None;
            cx.notify();
            return;
        }
        let Some(condition) = self.manual_rule_conditions.get_mut(condition_index) else {
            return;
        };
        condition.kind = kind;
        if kind == crate::manual_rule::ManualRuleKind::Final {
            self.manual_rule_condition_count = 1;
            for condition in &self.manual_rule_conditions {
                condition
                    .input
                    .update(cx, SubscriptionTextInput::clear_without_event);
            }
        }
        self.manual_rule_error = None;
        self.manual_rule_popover = None;
        let placeholder = manual_rule_placeholder(kind, self.language());
        if let Some(condition) = self.manual_rule_conditions.get(condition_index) {
            condition
                .input
                .update(cx, |input, cx| input.set_placeholder(placeholder, cx));
        }
        cx.notify();
    }

    fn add_manual_rule_condition(&mut self, cx: &mut Context<Self>) {
        if self.manual_rule_condition_count >= crate::manual_rule::MAX_CONDITIONS
            || self
                .manual_rule_conditions
                .first()
                .map(|condition| condition.kind)
                == Some(crate::manual_rule::ManualRuleKind::Final)
        {
            return;
        }
        let index = self.manual_rule_condition_count;
        self.manual_rule_condition_count += 1;
        self.manual_rule_popover = None;
        self.manual_rule_error = None;
        if let Some(condition) = self.manual_rule_conditions.get(index) {
            condition
                .input
                .update(cx, SubscriptionTextInput::clear_without_event);
        }
        cx.notify();
    }

    fn remove_manual_rule_condition(&mut self, index: usize, cx: &mut Context<Self>) {
        if index == 0 || index >= self.manual_rule_condition_count {
            return;
        }
        for current in index..self.manual_rule_condition_count - 1 {
            let next_kind = self.manual_rule_conditions[current + 1].kind;
            let value = self.manual_rule_conditions[current + 1]
                .input
                .read(cx)
                .value()
                .to_owned();
            self.manual_rule_conditions[current].kind = next_kind;
            self.manual_rule_conditions[current]
                .input
                .update(cx, |input, cx| input.set_value_without_event(value, cx));
        }
        self.manual_rule_condition_count -= 1;
        if let Some(condition) = self
            .manual_rule_conditions
            .get(self.manual_rule_condition_count)
        {
            condition
                .input
                .update(cx, SubscriptionTextInput::clear_without_event);
        }
        self.manual_rule_popover = None;
        self.manual_rule_error = None;
        cx.notify();
    }

    fn apply_manual_rule_edit(
        &mut self,
        editing_index: Option<usize>,
        rule: crate::manual_rule::ManualRule,
    ) -> Result<(), crate::manual_rule::ManualRuleEditError> {
        if let Some(index) = editing_index {
            crate::manual_rule::replace_manual_rule(&mut self.manual_rules, index, rule)?;
        } else {
            if rule.is_final()
                && self
                    .manual_rules
                    .iter()
                    .any(crate::manual_rule::ManualRule::is_final)
            {
                return Err(crate::manual_rule::ManualRuleEditError::FinalAlreadyExists);
            }
            if self
                .manual_rules
                .iter()
                .any(|existing| existing.same_definition(&rule))
            {
                return Err(crate::manual_rule::ManualRuleEditError::Duplicate);
            }
            self.manual_rules.push(rule);
        }
        Ok(())
    }

    fn submit_manual_rule(&mut self, cx: &mut Context<Self>) -> bool {
        if self.manual_rule_editor_state == super::ManualRuleEditorState::Closed {
            return false;
        }
        if self.manual_rule_conditions[..self.manual_rule_condition_count]
            .iter()
            .any(|condition| !condition.kind.supported_by(self.runtime.kind()))
        {
            self.manual_rule_error = Some(crate::manual_rule::ManualRuleError::UnsupportedByKernel);
            cx.notify();
            return false;
        }
        let conditions = self.manual_rule_conditions[..self.manual_rule_condition_count]
            .iter()
            .map(|condition| (condition.kind, condition.input.read(cx).value().to_owned()))
            .collect::<Vec<_>>();
        let condition_count = conditions.len();
        let rule = match crate::manual_rule::ManualRule::parse_conditions(
            conditions,
            &self.manual_rule_target,
        ) {
            Ok(rule) => rule,
            Err(error) => {
                self.manual_rule_error = Some(error);
                cx.notify();
                return false;
            }
        };
        let editing_index = self.manual_rule_editor_state.editing_index();
        let previous_rules = self.manual_rules.clone();
        match self.apply_manual_rule_edit(editing_index, rule) {
            Ok(()) => {}
            Err(crate::manual_rule::ManualRuleEditError::Duplicate) => {
                self.manual_rule_error = Some(crate::manual_rule::ManualRuleError::Duplicate);
                cx.notify();
                return false;
            }
            Err(crate::manual_rule::ManualRuleEditError::FinalAlreadyExists) => {
                self.manual_rule_error =
                    Some(crate::manual_rule::ManualRuleError::FinalAlreadyExists);
                cx.notify();
                return false;
            }
            Err(crate::manual_rule::ManualRuleEditError::Missing) => {
                self.reset_manual_rule_editor_state();
                cx.notify();
                return false;
            }
        }
        if let Some(index) = self
            .manual_rules
            .iter()
            .position(crate::manual_rule::ManualRule::is_final)
        {
            let final_rule = self.manual_rules.remove(index);
            self.manual_rules.push(final_rule);
        }
        let completion = self
            .language()
            .localized(copy::configuration::MANUAL_RULES_UPDATED)
            .to_owned();
        if !self.persist_manual_rules(completion, previous_rules.clone(), cx) {
            self.manual_rules = previous_rules;
            return false;
        }
        self.manual_rule_error = None;
        self.reset_manual_rule_editor_state();
        record_event(
            LogLevel::Info,
            if editing_index.is_some() {
                "routing.manual_rule.updated"
            } else {
                "routing.manual_rule.added"
            },
            format!(
                "conditions={} target={} total={}",
                condition_count,
                self.manual_rule_target,
                self.manual_rules.len()
            ),
        );
        true
    }

    fn remove_manual_rule(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.manual_rules.len() {
            return;
        }
        let previous_rules = self.manual_rules.clone();
        let removed = self.manual_rules.remove(index);
        let completion = self
            .language()
            .localized(copy::configuration::MANUAL_RULE_REMOVED)
            .to_owned();
        if !self.persist_manual_rules(completion, previous_rules.clone(), cx) {
            self.manual_rules = previous_rules;
            return;
        }
        self.manual_rule_error = None;
        record_event(
            LogLevel::Info,
            "routing.manual_rule.removed",
            format!(
                "conditions={} total={}",
                removed.conditions().len(),
                self.manual_rules.len()
            ),
        );
    }

    fn set_manual_rule_enabled(&mut self, index: usize, enabled: bool, cx: &mut Context<Self>) {
        let previous_rules = self.manual_rules.clone();
        let Some(rule) = self.manual_rules.get_mut(index) else {
            return;
        };
        if rule.is_enabled() == enabled {
            return;
        }
        rule.set_enabled(enabled);
        let completion = self
            .language()
            .localized(if enabled {
                copy::configuration::MANUAL_RULE_ENABLED
            } else {
                copy::configuration::MANUAL_RULE_DISABLED
            })
            .to_owned();
        if !self.persist_manual_rules(completion, previous_rules.clone(), cx) {
            self.manual_rules = previous_rules;
            return;
        }
        record_event(
            LogLevel::Info,
            if enabled {
                "routing.manual_rule.enabled"
            } else {
                "routing.manual_rule.disabled"
            },
            format!("index={index} total={}", self.manual_rules.len()),
        );
        cx.notify();
    }

    fn persist_manual_rules(
        &mut self,
        completion: String,
        previous_rules: Vec<crate::manual_rule::ManualRule>,
        cx: &mut Context<Self>,
    ) -> bool {
        let language = self.language();
        if self.routing_apply_state.is_busy() {
            language
                .message(Message::RoutingApplyBusy)
                .clone_into(&mut self.status);
            cx.notify();
            return false;
        }
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            language
                .message(Message::ManualRulesLocationUnavailable)
                .clone_into(&mut self.status);
            cx.notify();
            return false;
        };
        let store_snapshot = match mihomo::SubscriptionStoreSnapshot::capture(&store_dir) {
            Ok(snapshot) => snapshot,
            Err(_error) => {
                language
                    .message(Message::StoreTransactionUnavailable)
                    .clone_into(&mut self.status);
                cx.notify();
                return false;
            }
        };
        let previous_order = self.rule_sources.group_order.clone();
        self.sync_routing_rule_group_order();
        if mihomo::save_routing_rule_group_order_in(&store_dir, &self.rule_sources.group_order)
            .is_err()
        {
            self.rule_sources.group_order = previous_order;
            let _ = store_snapshot.restore(&store_dir);
            language
                .message(Message::RuleGroupOrderSaveFailed)
                .clone_into(&mut self.status);
            cx.notify();
            return false;
        }
        if let Err(error) = crate::manual_rule::save_manual_rules_in(&store_dir, &self.manual_rules)
        {
            let _ = store_snapshot.clone().restore(&store_dir);
            self.status = format!(
                "{}{}",
                language.message(Message::ManualRulesSaveFailed),
                copy::configuration::manual_rule_store_error(language, error)
            );
            cx.notify();
            return false;
        }
        self.start_routing_runtime_apply(
            store_dir,
            completion,
            super::RoutingApplyRollback {
                manual_rules: previous_rules,
                group_order: previous_order,
                store_snapshot,
            },
            cx,
        );
        true
    }

    fn start_routing_runtime_apply(
        &mut self,
        store_dir: std::path::PathBuf,
        completion: String,
        rollback: super::RoutingApplyRollback,
        cx: &mut Context<Self>,
    ) {
        let started = self.routing_apply_state.begin();
        debug_assert!(started, "routing apply must be idle before spawning");
        if !started {
            return;
        }
        self.status = format!(
            "{} · {}",
            completion,
            self.language().message(Message::ApplyingChanges)
        );
        let runtime = self.runtime.clone();
        let executor = cx.background_executor().clone();
        let disk_rollback = rollback.clone();
        cx.spawn(async move |this, cx| {
            let (apply, rollback_error) = executor
                .spawn(async move {
                    let apply =
                        SourceRuntimeApply::from_result(runtime.apply_saved_sources(&store_dir));
                    let rollback_error = if apply.requires_source_rollback() {
                        disk_rollback.store_snapshot.restore(&store_dir).err()
                    } else {
                        None
                    };
                    (apply, rollback_error)
                })
                .await;
            this.update(cx, |this, cx| {
                this.routing_apply_state.finish();
                if apply.requires_source_rollback() {
                    this.manual_rules = rollback.manual_rules;
                    this.rule_sources.group_order = rollback.group_order;
                }
                apply.reconcile_proxy_mode(&mut this.proxy_mode);
                this.status = if let Some(rollback_error) = rollback_error {
                    format!(
                        "{}{} · {}{}",
                        completion,
                        apply.status_suffix(this.language()),
                        this.language().localized(
                            copy::configuration::COULD_NOT_RESTORE_THE_PREVIOUS_SAVED_RULES
                        ),
                        copy::configuration::subscription_store_error(
                            this.language(),
                            rollback_error,
                        )
                    )
                } else {
                    format!(
                        "{}{}",
                        completion,
                        if apply.requires_source_rollback() {
                            apply.status_suffix_after_source_rollback(this.language())
                        } else {
                            apply.status_suffix(this.language())
                        }
                    )
                };
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn sync_routing_rule_group_order(&mut self) {
        self.rule_sources.group_order = mihomo::normalized_routing_rule_group_order(
            &self.rule_sources.group_order,
            !self.manual_rules.is_empty(),
            &self.rule_sources.sources,
        );
    }

    fn move_routing_rule_group(&mut self, group_id: &str, direction: i8, cx: &mut Context<Self>) {
        if self.routing_apply_state.is_busy() {
            self.language()
                .message(Message::RoutingApplyBusy)
                .clone_into(&mut self.status);
            cx.notify();
            return;
        }
        self.sync_routing_rule_group_order();
        let previous = self.rule_sources.group_order.clone();
        if !mihomo::move_routing_rule_group(&mut self.rule_sources.group_order, group_id, direction)
        {
            return;
        }
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            self.rule_sources.group_order = previous;
            return;
        };
        let store_snapshot = match mihomo::SubscriptionStoreSnapshot::capture(&store_dir) {
            Ok(snapshot) => snapshot,
            Err(_error) => {
                self.rule_sources.group_order = previous;
                self.language()
                    .message(Message::StoreTransactionUnavailable)
                    .clone_into(&mut self.status);
                cx.notify();
                return;
            }
        };
        if mihomo::save_routing_rule_group_order_in(&store_dir, &self.rule_sources.group_order)
            .is_err()
        {
            self.rule_sources.group_order = previous;
            self.language()
                .message(Message::RuleGroupOrderSaveFailed)
                .clone_into(&mut self.status);
            cx.notify();
            return;
        }
        let language = self.language();
        let completion = if direction < 0 {
            language.localized(copy::configuration::RULE_GROUP_MOVED_UP)
        } else {
            language.localized(copy::configuration::RULE_GROUP_MOVED_DOWN)
        }
        .to_owned();
        self.start_routing_runtime_apply(
            store_dir,
            completion,
            super::RoutingApplyRollback {
                manual_rules: self.manual_rules.clone(),
                group_order: previous,
                store_snapshot,
            },
            cx,
        );
    }

    fn manual_rule_kind_menu(
        &self,
        condition_index: usize,
        selected_kind: crate::manual_rule::ManualRuleKind,
        theme: Theme,
        language: Language,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let kernel = self.runtime.kind();
        let editing_index = self.manual_rule_editor_state.editing_index();
        let final_available = !self
            .manual_rules
            .iter()
            .enumerate()
            .any(|(index, rule)| rule.is_final() && Some(index) != editing_index);
        let mut choices = div().id("manual-rule-kind-choices");
        for kind in crate::manual_rule::ManualRuleKind::ALL {
            if kind == crate::manual_rule::ManualRuleKind::Final && condition_index > 0 {
                continue;
            }
            let supported = kind.supported_by(kernel)
                && (kind != crate::manual_rule::ManualRuleKind::Final || final_available);
            let selected = selected_kind == kind;
            let detail = if supported {
                manual_rule_kind_detail(kind, language)
            } else if kind == crate::manual_rule::ManualRuleKind::Final {
                language.localized(copy::configuration::ALREADY_CONFIGURED)
            } else if kind == crate::manual_rule::ManualRuleKind::UserAgent {
                language.localized(copy::configuration::NO_EXACT_KERNEL_EQUIVALENT)
            } else {
                language.localized(copy::configuration::AVAILABLE_WITH_MIHOMO)
            };
            choices = choices.child(
                div()
                    .id(format!(
                        "manual-rule-kind-{condition_index}-{}",
                        kind.storage_key()
                    ))
                    .role(Role::Button)
                    .aria_label(kind.display_label())
                    .tab_stop(supported)
                    .focusable()
                    .when(supported, gpui::Styled::cursor_pointer)
                    .min_h(px(36.0))
                    .px(Space::Md.px())
                    .py(Space::Xs.px())
                    .border_b_1()
                    .border_color(theme.outline_subtle)
                    .bg(if selected {
                        theme.action_soft
                    } else {
                        theme.surface_high
                    })
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .text_size(TextRole::Label.size())
                            .line_height(TextRole::Label.line_height())
                            .font_weight(TextRole::Label.weight())
                            .text_color(if supported {
                                theme.text_primary
                            } else {
                                theme.text_tertiary
                            })
                            .child(kind.display_label()),
                    )
                    .child(
                        div()
                            .text_size(TextRole::Metadata.size())
                            .line_height(TextRole::Metadata.line_height())
                            .text_color(theme.text_tertiary)
                            .child(detail),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if supported {
                            this.set_manual_rule_kind(condition_index, kind, cx);
                        }
                    })),
            );
        }
        choices
    }

    fn manual_rule_target_menu(&self, theme: Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        let mut choices = div().id("manual-rule-target-choices");
        for target in self.manual_rule_targets() {
            let selected = self.manual_rule_target == target;
            let row_target = target.clone();
            choices = choices.child(
                div()
                    .id(format!("manual-rule-target-{target}"))
                    .role(Role::Button)
                    .aria_label(format!("Target {target}"))
                    .tab_stop(true)
                    .focusable()
                    .cursor_pointer()
                    .min_h(ControlSize::Standard.min_pointer_target())
                    .px(Space::Md.px())
                    .py(Space::Sm.px())
                    .border_b_1()
                    .border_color(theme.outline_subtle)
                    .bg(if selected {
                        theme.action_soft
                    } else {
                        theme.surface_high
                    })
                    .text_size(TextRole::Label.size())
                    .line_height(TextRole::Label.line_height())
                    .font_weight(TextRole::Label.weight())
                    .text_color(if selected {
                        theme.action_primary
                    } else {
                        theme.text_primary
                    })
                    .child(target)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.manual_rule_target.clone_from(&row_target);
                        this.manual_rule_error = None;
                        this.manual_rule_popover = None;
                        cx.notify();
                    })),
            );
        }
        choices
    }

    fn manual_rule_select(
        id: &str,
        label: &'static str,
        value: String,
        menu: impl gpui::IntoElement,
        open: bool,
        width: f32,
        on_open_change: impl Fn(&bool, &mut gpui::Window, &mut gpui::App) + 'static,
    ) -> AnyElement {
        let trigger = Button::new(id.to_owned())
            .accessibility_label(label)
            .dropdown_caret(true)
            .with_variant(ButtonVariant::Default)
            .with_size(ControlSize::Standard.component_size())
            .h(ControlSize::Standard.height())
            .w_full()
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .overflow_x_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(TextRole::Label.size())
                    .line_height(TextRole::Label.line_height())
                    .font_weight(TextRole::Label.weight())
                    .child(value),
            );

        crate::components::anchored_popover(format!("{id}-popover"), trigger, menu, width, 360.0)
            .open(open)
            .on_open_change(on_open_change)
            .into_any_element()
    }

    fn manual_rule_condition_editor(
        &self,
        condition_index: usize,
        kind: crate::manual_rule::ManualRuleKind,
        theme: Theme,
        language: Language,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let input = self
            .manual_rule_conditions
            .get(condition_index)
            .expect("manual rule condition input is initialized")
            .input
            .clone();
        let kind_width = if compact { 260.0 } else { 240.0 };
        let kind_popover = ManualRulePopover::Kind(condition_index);
        let kind_open = self.manual_rule_popover == Some(kind_popover);
        let kind_menu = self.manual_rule_kind_menu(condition_index, kind, theme, language, cx);
        let select_id = format!("manual-rule-kind-select-{condition_index}");
        let label = language.localized(copy::configuration::CHOOSE_CONDITION_TYPE);
        let app = cx.entity();
        let kind_select = Self::manual_rule_select(
            &select_id,
            label,
            kind.display_label().to_owned(),
            kind_menu,
            kind_open,
            kind_width,
            move |open, _, cx| {
                app.update(cx, |this, cx| {
                    this.manual_rule_popover = open.then_some(kind_popover);
                    cx.notify();
                });
            },
        );
        let is_final = kind == crate::manual_rule::ManualRuleKind::Final;
        let mut row = div()
            .mt_3()
            .child(div().child(field_label(
                if condition_index == 0 {
                    language.localized(copy::configuration::CONDITION_1).to_owned()
                } else {
                    copy::configuration::condition_title(language, condition_index + 1)
                },
                theme,
            )))
            .child(
                div()
                    .flex()
                    .when(compact, gpui::Styled::flex_col)
                    .items_stretch()
                    .gap_2()
                    .child(
                        div()
                            .when(compact, gpui::Styled::w_full)
                            .when(!compact, |item| item.w(px(220.0)))
                            .flex_shrink_0()
                            .child(kind_select),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .when(!is_final, |field| field.child(input))
                            .when(is_final, |field| {
                                field.child(
                                    div()
                                        .h(ControlSize::Standard.height())
                                        .px(Space::Md.px())
                                        .flex()
                                        .items_center()
                                        .rounded(Radius::Control.px())
                                        .bg(theme.surface_low)
                                        .text_size(TextRole::Body.size())
                                        .line_height(TextRole::Body.line_height())
                                        .text_color(theme.text_secondary)
                                        .child(language.localized(copy::configuration::MATCHES_ONLY_AFTER_EVERY_RULE_ABOVE_MISSES)),
                                )
                            }),
                    ),
            );
        if condition_index > 0 {
            row = row.child(
                Button::new(format!("remove-manual-rule-condition-{condition_index}"))
                    .accessibility_label(
                        language.localized(copy::configuration::REMOVE_THIS_CONDITION),
                    )
                    .label(language.localized(copy::configuration::REMOVE_CONDITION))
                    .text()
                    .with_size(ControlSize::Compact.component_size())
                    .h(ControlSize::Compact.height())
                    .mt(Space::Sm.px())
                    .cursor_pointer()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.status_error)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.remove_manual_rule_condition(condition_index, cx);
                    })),
            );
        }
        row
    }

    pub(super) fn manual_rule_editor_modal(
        &self,
        dialog: Dialog,
        theme: Theme,
        language: Language,
        compact: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Dialog {
        let target_width = if compact { 260.0 } else { 240.0 };
        let target_open = self.manual_rule_popover == Some(ManualRulePopover::Target);
        let target_menu = self.manual_rule_target_menu(theme, cx);
        let app = cx.entity();
        let target = Self::manual_rule_select(
            "manual-rule-target-select",
            language.localized(copy::configuration::CHOOSE_TARGET_POLICY),
            self.manual_rule_target.clone(),
            target_menu,
            target_open,
            target_width,
            move |open, _, cx| {
                app.update(cx, |this, cx| {
                    this.manual_rule_popover = open.then_some(ManualRulePopover::Target);
                    cx.notify();
                });
            },
        );

        let editing = self.manual_rule_editor_state.editing_index().is_some();
        let final_selected = self
            .manual_rule_conditions
            .first()
            .map(|condition| condition.kind)
            == Some(crate::manual_rule::ManualRuleKind::Final);
        let conditions =
            self.manual_rule_editor_conditions(final_selected, compact, theme, language, cx);
        let body = self.manual_rule_editor_body(conditions, target, theme, language);
        let footer = self.manual_rule_editor_footer(editing, theme, language, cx);

        let viewport = window.viewport_size();
        let dialog_width = (viewport.width.as_f32() - 32.0).clamp(280.0, 720.0);
        let estimated_height = if compact {
            520.0
        } else {
            match self.manual_rule_condition_count {
                0 | 1 => 368.0,
                2 => 458.0,
                3 => 548.0,
                _ => 638.0,
            }
        };
        let margin_top = ((viewport.height.as_f32() - estimated_height) / 2.0).max(16.0);
        let app = cx.entity();

        dialog
            .width(px(dialog_width))
            .max_h(px((viewport.height.as_f32() - 32.0).max(320.0)))
            .margin_top(px(margin_top))
            .overlay(true)
            .overlay_closable(true)
            .keyboard(true)
            .close_button(false)
            .p_0()
            .rounded_md()
            .bg(theme.surface_high)
            .overflow_hidden()
            .title(Self::manual_rule_editor_title(
                editing,
                final_selected,
                theme,
                language,
            ))
            .child(body)
            .footer(footer)
            .on_close(move |_, _, cx| {
                app.update(cx, ManisApp::close_manual_rule_editor);
            })
    }

    fn manual_rule_editor_conditions(
        &self,
        final_selected: bool,
        compact: bool,
        theme: Theme,
        language: Language,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut conditions = div();
        for index in 0..self.manual_rule_condition_count {
            conditions = conditions.child(self.manual_rule_condition_editor(
                index,
                self.manual_rule_conditions[index].kind,
                theme,
                language,
                compact,
                cx,
            ));
        }
        if final_selected || self.manual_rule_condition_count >= crate::manual_rule::MAX_CONDITIONS
        {
            return conditions;
        }
        conditions.child(
            Button::new("add-manual-rule-condition")
                .accessibility_label(language.localized(copy::configuration::ADD_AN_AND_CONDITION))
                .label(language.localized(copy::configuration::ADD_AND_CONDITION))
                .with_variant(ButtonVariant::Default)
                .with_size(ControlSize::Standard.component_size())
                .h(ControlSize::Standard.height())
                .mt(Space::Md.px())
                .px(Space::Md.px())
                .py(Space::Sm.px())
                .cursor_pointer()
                .border_color(theme.outline_strong)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.action_primary)
                .on_click(cx.listener(|this, _, _, cx| this.add_manual_rule_condition(cx))),
        )
    }

    fn manual_rule_editor_body(
        &self,
        conditions: Div,
        target: AnyElement,
        theme: Theme,
        language: Language,
    ) -> Stateful<Div> {
        div()
            .id("manual-rule-modal-body")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .px_5()
            .py_4()
            .child(conditions)
            .child(
                div()
                    .mt_4()
                    .child(field_label(
                        language.localized(copy::configuration::POLICY_GROUP_AFTER_MATCH),
                        theme,
                    ))
                    .child(target),
            )
            .when_some(self.manual_rule_error, |body, error| {
                body.child(
                    div()
                        .mt_3()
                        .p_3()
                        .rounded_md()
                        .bg(theme.surface_low)
                        .text_size(TextRole::Body.size())
                        .line_height(TextRole::Body.line_height())
                        .text_color(theme.status_error)
                        .child(manual_rule_error_label(error, language)),
                )
            })
    }

    fn manual_rule_editor_footer(
        &self,
        editing: bool,
        theme: Theme,
        language: Language,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .flex_shrink_0()
            .px_5()
            .py_3()
            .border_t_1()
            .border_color(theme.outline_subtle)
            .flex()
            .items_center()
            .justify_end()
            .gap_2()
            .child(
                style_action_button(
                    Button::new("cancel-manual-rule").label(language.message(Message::Cancel)),
                    ActionRole::Secondary,
                    ControlSize::Standard,
                )
                .px(Space::Lg.px())
                .cursor_pointer()
                .on_click(cx.listener(|this, _, window, cx| {
                    this.close_manual_rule_editor(cx);
                    window.close_dialog(cx);
                })),
            )
            .child(
                style_action_button(
                    Button::new("save-manual-rule")
                        .label(if editing {
                            language.message(Message::SaveChanges)
                        } else {
                            language.message(Message::AddRule)
                        })
                        .disabled(self.routing_apply_state.is_busy()),
                    ActionRole::Primary,
                    ControlSize::Standard,
                )
                .px(Space::Lg.px())
                .cursor_pointer()
                .bg(theme.action_primary)
                .text_color(theme.action_on_primary)
                .on_click(cx.listener(|this, _, window, cx| {
                    if this.submit_manual_rule(cx) {
                        window.close_dialog(cx);
                    }
                })),
            )
    }

    fn manual_rule_editor_title(
        editing: bool,
        final_selected: bool,
        theme: Theme,
        language: Language,
    ) -> Stateful<Div> {
        div()
            .id("manual-rule-modal-header")
            .flex_shrink_0()
            .px_5()
            .py_4()
            .border_b_1()
            .border_color(theme.outline_subtle)
            .child(
                div()
                    .text_size(px(17.0))
                    .font_weight(TextRole::SectionTitle.weight())
                    .child(if editing {
                        language.localized(copy::configuration::EDIT_ROUTING_RULE)
                    } else {
                        language.localized(copy::configuration::ADD_ROUTING_RULE)
                    }),
            )
            .child(
                div()
                    .mt_1()
                    .text_size(TextRole::Metadata.size())
                    .text_color(theme.text_secondary)
                    .child(if final_selected {
                        language.localized(copy::configuration::FINAL_IS_ALWAYS_EVALUATED_LAST_AND_HANDLES_UNMATCHED_TRAFFIC)
                    } else {
                        language.localized(copy::configuration::ALL_CONDITIONS_MUST_MATCH_GROUP_ORDER_DETERMINES_RULE_PRIORITY)
                    }),
            )
    }

    fn manual_routing_rule_row(
        &self,
        order: usize,
        index: usize,
        rule: &crate::manual_rule::ManualRule,
        theme: Theme,
        language: Language,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let enabled = rule.is_enabled();
        let target = self.effective_rule_target(rule.target(), language);
        let matchers = Self::manual_rule_matchers(rule, enabled, theme, language);
        let edit_label = copy::configuration::manual_rule_accessibility(language, order);
        let row = div()
            .id(format!("manual-routing-rule-{index}"))
            .mt_1()
            .min_h(px(44.0))
            .px(Space::Md.px())
            .py(Space::Sm.px())
            .rounded(Radius::Row.px())
            .bg(if enabled {
                theme.surface_low
            } else {
                theme.surface_base
            })
            .flex()
            .items_center()
            .gap(Space::Md.px())
            .cursor_pointer()
            .role(Role::Button)
            .tab_stop(true)
            .focusable()
            .aria_label(edit_label)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_manual_rule_editor_for_edit(index, window, cx);
            }))
            .on_key_down(cx.listener(move |this, event, window, cx| {
                let Some(action) = manual_rule_keyboard_action(event) else {
                    return;
                };
                cx.stop_propagation();
                match action {
                    ManualRuleKeyboardAction::Edit => {
                        this.open_manual_rule_editor_for_edit(index, window, cx);
                    }
                    ManualRuleKeyboardAction::Toggle => {
                        this.set_manual_rule_enabled(index, !enabled, cx);
                    }
                    ManualRuleKeyboardAction::Delete => {
                        this.remove_manual_rule(index, cx);
                    }
                }
            }))
            .child(
                div()
                    .w(px(42.0))
                    .flex_shrink_0()
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .text_color(theme.text_tertiary)
                    .child(format!("#{order:03}")),
            )
            .child(matchers)
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(TextRole::Data.size())
                    .line_height(TextRole::Data.line_height())
                    .font_weight(TextRole::Data.weight())
                    .text_color(if enabled {
                        theme.action_primary
                    } else {
                        theme.text_tertiary
                    })
                    .child(target),
            )
            .when(!enabled, |row| {
                row.child(
                    div()
                        .flex_shrink_0()
                        .px_2()
                        .py_1()
                        .rounded(Radius::Control.px())
                        .bg(theme.surface_chrome)
                        .text_size(TextRole::Metadata.size())
                        .line_height(TextRole::Metadata.line_height())
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.text_tertiary)
                        .child(language.localized(copy::configuration::DISABLED)),
                )
            });
        Self::manual_rule_context_menu(row, index, enabled, language, cx.entity())
    }

    fn manual_rule_context_menu(
        row: Stateful<Div>,
        index: usize,
        enabled: bool,
        language: Language,
        app: Entity<Self>,
    ) -> AnyElement {
        let toggle_label = if enabled {
            language.message(Message::Disable)
        } else {
            language.message(Message::Enable)
        };
        row.context_menu(move |menu, _, _| {
            let toggle_app = app.clone();
            let delete_app = app.clone();
            menu.item(PopupMenuItem::new(toggle_label).on_click(move |_, _, cx| {
                toggle_app.update(cx, |this, cx| {
                    this.set_manual_rule_enabled(index, !enabled, cx);
                });
            }))
            .separator()
            .item(
                PopupMenuItem::new(language.message(Message::Delete)).on_click(move |_, _, cx| {
                    delete_app.update(cx, |this, cx| {
                        this.remove_manual_rule(index, cx);
                    });
                }),
            )
        })
        .into_any_element()
    }

    fn manual_rule_matchers(
        rule: &crate::manual_rule::ManualRule,
        enabled: bool,
        theme: Theme,
        language: Language,
    ) -> Div {
        let primary_text = if enabled {
            theme.text_primary
        } else {
            theme.text_tertiary
        };
        let secondary_text = if enabled {
            theme.text_secondary
        } else {
            theme.text_tertiary
        };
        if rule.is_final() {
            return div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .items_center()
                .gap(Space::Sm.px())
                .child(
                    div()
                        .px(Space::Sm.px())
                        .py(Space::Xs.px())
                        .rounded(Radius::Control.px())
                        .bg(theme.surface_high)
                        .text_size(TextRole::Label.size())
                        .line_height(TextRole::Label.line_height())
                        .font_weight(TextRole::Label.weight())
                        .text_color(primary_text)
                        .child("FINAL"),
                )
                .child(
                    div()
                        .text_size(TextRole::Metadata.size())
                        .line_height(TextRole::Metadata.line_height())
                        .text_color(secondary_text)
                        .child(language.localized(copy::configuration::FALLBACK_ALWAYS_LAST)),
                );
        }
        let mut matchers = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_wrap()
            .items_center()
            .gap_1();
        for (condition_index, condition) in rule.conditions().iter().enumerate() {
            if condition_index > 0 {
                matchers = matchers.child(
                    div()
                        .mx_1()
                        .text_size(TextRole::Metadata.size())
                        .line_height(TextRole::Metadata.line_height())
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.text_tertiary)
                        .child(language.localized(copy::configuration::AND)),
                );
            }
            matchers = matchers.child(
                div()
                    .flex()
                    .items_center()
                    .gap(Space::Sm.px())
                    .px(Space::Sm.px())
                    .py(Space::Xs.px())
                    .rounded(Radius::Control.px())
                    .bg(theme.surface_high)
                    .child(
                        div()
                            .text_size(TextRole::Metadata.size())
                            .line_height(TextRole::Metadata.line_height())
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(secondary_text)
                            .child(condition.kind().display_label()),
                    )
                    .child(
                        div()
                            .text_size(TextRole::Data.size())
                            .line_height(TextRole::Data.line_height())
                            .text_color(primary_text)
                            .child(condition.parameter().to_owned()),
                    ),
            );
        }
        matchers
    }

    fn rule_group_order_controls(
        &self,
        group_id: &str,
        group_name: &str,
        position: (usize, usize),
        theme: Theme,
        language: Language,
        cx: &mut Context<Self>,
    ) -> Div {
        let (position, group_count) = position;
        let up_id = group_id.to_owned();
        let down_id = group_id.to_owned();
        div()
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(Space::Xs.px())
            .child(
                Button::new(format!("move-rule-group-up-{group_id}"))
                    .accessibility_label(copy::configuration::move_rule_group(
                        language, group_name, true,
                    ))
                    .icon(IconName::ArrowUp)
                    .text()
                    .with_size(ControlSize::Icon.component_size())
                    .text_color(theme.text_secondary)
                    .disabled(position == 0 || self.routing_apply_state.is_busy())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.move_routing_rule_group(&up_id, -1, cx);
                    })),
            )
            .child(
                Button::new(format!("move-rule-group-down-{group_id}"))
                    .accessibility_label(copy::configuration::move_rule_group(
                        language, group_name, false,
                    ))
                    .icon(IconName::ArrowDown)
                    .text()
                    .with_size(ControlSize::Icon.component_size())
                    .text_color(theme.text_secondary)
                    .disabled(position + 1 >= group_count || self.routing_apply_state.is_busy())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.move_routing_rule_group(&down_id, 1, cx);
                    })),
            )
    }

    fn active_rules_panel(
        &self,
        theme: Theme,
        language: Language,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let remote_count = self
            .rule_sources
            .sources
            .iter()
            .filter(|source| source.enabled)
            .map(|source| source.rule_count)
            .sum::<usize>();
        let disabled_remote_count = self
            .rule_sources
            .sources
            .iter()
            .filter(|source| !source.enabled)
            .map(|source| source.rule_count)
            .sum::<usize>();
        let enabled_manual_count = self
            .manual_rules
            .iter()
            .filter(|rule| rule.is_enabled())
            .count();
        let disabled_manual_count = self.manual_rules.len() - enabled_manual_count;
        let active_count = enabled_manual_count + remote_count;
        let disabled_count = disabled_manual_count + disabled_remote_count;
        let group_order = mihomo::normalized_routing_rule_group_order(
            &self.rule_sources.group_order,
            !self.manual_rules.is_empty(),
            &self.rule_sources.sources,
        );
        let mut list = Self::active_rules_panel_shell(
            active_count,
            disabled_count,
            compact,
            language,
            theme,
            cx,
        );
        let mut rule_order = 1;
        for (position, group_id) in group_order.iter().enumerate() {
            if group_id == mihomo::MANUAL_ROUTING_RULE_GROUP_ID {
                list = list.child(self.manual_rule_group(
                    disabled_manual_count,
                    &mut rule_order,
                    RuleGroupRenderContext {
                        position,
                        group_count: group_order.len(),
                        compact,
                        language,
                        theme,
                    },
                    cx,
                ));
            } else if let Some((source_index, source)) = self
                .rule_sources
                .sources
                .iter()
                .enumerate()
                .find(|(_, source)| source.id == *group_id)
            {
                list = list.child(self.remote_rule_group(
                    source_index,
                    source,
                    &mut rule_order,
                    RuleGroupRenderContext {
                        position,
                        group_count: group_order.len(),
                        compact,
                        language,
                        theme,
                    },
                    cx,
                ));
            }
        }
        if group_order.is_empty() {
            list = list.child(
                empty_state(
                    language.localized(copy::configuration::NO_ROUTING_RULES_YET),
                    language.localized(copy::configuration::ADD_RULES_TO_SEND_MATCHING_CONNECTIONS_THROUGH_A_POLICY_GROUP),
                    None,
                    theme,
                )
                .mt(Space::Lg.px()),
            );
        }
        list
    }

    fn active_rules_panel_shell(
        active_count: usize,
        disabled_count: usize,
        compact: bool,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let summary =
            copy::configuration::active_rule_summary(language, active_count, disabled_count);
        div()
            .id("active-routing-rules")
            .w_full()
            .flex_1()
            .min_w(px(0.0))
            .p(if compact {
                Space::Md.px()
            } else {
                Space::Lg.px()
            })
            .rounded(Radius::Pane.px())
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_high)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(Space::Md.px())
                    .child(section_heading(
                        language.localized(copy::configuration::ACTIVE_RULES),
                        language.localized(
                            copy::configuration::GROUPS_MATCH_FROM_TOP_TO_BOTTOM_USE_THE_ARROWS_TO,
                        ),
                        None,
                        theme,
                    ))
                    .child(
                        div()
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .gap(Space::Sm.px())
                            .child(status_badge(summary, StatusTone::Route, theme))
                            .child(
                                action_button(
                                    "open-manual-rule-editor",
                                    language.message(Message::AddRule),
                                    ActionRole::Primary,
                                    ControlSize::Compact,
                                )
                                .cursor_pointer()
                                .bg(theme.action_primary)
                                .text_color(theme.action_on_primary)
                                .on_click(cx.listener(
                                    |this, _, window, cx| {
                                        this.open_manual_rule_editor(window, cx);
                                    },
                                )),
                            ),
                    ),
            )
    }

    fn manual_rule_group(
        &self,
        disabled_count: usize,
        rule_order: &mut usize,
        view: RuleGroupRenderContext,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let RuleGroupRenderContext {
            compact,
            language,
            theme,
            ..
        } = view;
        let group_name = language.localized(copy::common::MANUAL_RULES);
        let detail = copy::configuration::manual_group_detail(
            language,
            self.manual_rules.len(),
            disabled_count,
        );
        let title = self.rule_group_title(
            mihomo::MANUAL_ROUTING_RULE_GROUP_ID,
            group_name,
            detail,
            None,
            view,
            cx,
        );
        let mut rules = Self::rule_group_rows(compact, theme);
        for (index, rule) in self.manual_rules.iter().enumerate() {
            rules = rules.child(self.manual_routing_rule_row(
                *rule_order,
                index,
                rule,
                theme,
                language,
                cx,
            ));
            *rule_order += 1;
        }
        let open = rule_group_is_open(&self.node_workspace, MANUAL_RULES_EXPANSION_KEY);
        Accordion::new("routing-manual-rules")
            .bordered(false)
            .with_size(Size::Large)
            .mt(Space::Lg.px())
            .rounded(Radius::Pane.px())
            .overflow_hidden()
            .border_1()
            .border_color(theme.outline_subtle)
            .item(|item| {
                item.open(open)
                    .title_style(accordion_title_style(compact, theme))
                    .content_style(accordion_content_style())
                    .title(title)
                    .child(rules)
            })
            .on_toggle_click(cx.listener(|this, open_indices: &[usize], _, cx| {
                Self::sync_rule_group_open(
                    this,
                    MANUAL_RULES_EXPANSION_KEY,
                    open_indices.contains(&0),
                    cx,
                );
            }))
            .into_any_element()
    }

    fn remote_rule_group(
        &self,
        source_index: usize,
        source: &mihomo::StoredQxRuleSource,
        rule_order: &mut usize,
        view: RuleGroupRenderContext,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let RuleGroupRenderContext {
            compact,
            language,
            theme,
            ..
        } = view;
        let parsed = QxRuleList::parse(&source.content);
        let target = self.effective_rule_target(source.target_policy.as_str(), language);
        let name = source.source.subscription_name().unwrap_or_else(|| {
            copy::configuration::numbered_rule_source(language, source_index + 1)
        });
        let detail = Self::remote_rule_group_detail(source, parsed.rules.len(), &target, language);
        let target_select = self.qx_rule_source_target_select(
            source,
            source.enabled && !self.source_refresh_busy(),
            theme,
            cx,
        );
        let title = self.rule_group_title(&source.id, &name, detail, Some(target_select), view, cx);
        let mut rules = Self::rule_group_rows(compact, theme);
        for rule in parsed.rules {
            rules = rules.child(Self::routing_rule_row(
                *rule_order,
                Self::qx_rule_kind_label(rule.kind),
                &rule.value,
                &target,
                theme,
            ));
            *rule_order += 1;
        }
        let expansion_key = rule_source_expansion_key(&source.id);
        let toggle_key = expansion_key.clone();
        let open = rule_group_is_open(&self.node_workspace, &expansion_key);
        Accordion::new(format!("routing-rule-source-{}", source.id))
            .bordered(false)
            .with_size(Size::Large)
            .mt(Space::Lg.px())
            .rounded(Radius::Pane.px())
            .overflow_hidden()
            .border_1()
            .border_color(theme.outline_subtle)
            .item(|item| {
                item.open(open)
                    .title_style(accordion_title_style(compact, theme))
                    .content_style(accordion_content_style())
                    .title(title)
                    .child(rules)
            })
            .on_toggle_click(cx.listener(move |this, open_indices: &[usize], _, cx| {
                Self::sync_rule_group_open(this, &toggle_key, open_indices.contains(&0), cx);
            }))
            .into_any_element()
    }

    fn rule_group_title(
        &self,
        group_id: &str,
        name: &str,
        detail: String,
        middle: Option<AnyElement>,
        view: RuleGroupRenderContext,
        cx: &mut Context<Self>,
    ) -> Div {
        let RuleGroupRenderContext {
            position,
            group_count,
            language,
            theme,
            ..
        } = view;
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(Space::Sm.px())
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(
                        div()
                            .text_size(TextRole::Label.size())
                            .font_weight(TextRole::Label.weight())
                            .child(name.to_owned()),
                    )
                    .child(
                        div()
                            .mt(Space::Xs.px())
                            .overflow_x_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(TextRole::Metadata.size())
                            .text_color(theme.text_tertiary)
                            .child(detail),
                    ),
            )
            .when_some(middle, ParentElement::child)
            .child(self.rule_group_order_controls(
                group_id,
                name,
                (position, group_count),
                theme,
                language,
                cx,
            ))
    }

    fn remote_rule_group_detail(
        source: &mihomo::StoredQxRuleSource,
        rule_count: usize,
        target: &str,
        language: Language,
    ) -> String {
        let update = source_update_label(
            source.last_successful_update_unix_secs,
            mihomo::current_unix_secs(),
            language,
        );
        copy::configuration::remote_group_detail(
            language,
            rule_count,
            source.enabled,
            target,
            &update,
        )
    }

    fn rule_group_rows(compact: bool, theme: Theme) -> Div {
        div()
            .px(if compact {
                Space::Sm.px()
            } else {
                Space::Md.px()
            })
            .pb(Space::Md.px())
            .border_t_1()
            .border_color(theme.outline_subtle)
    }

    fn sync_rule_group_open(this: &mut Self, key: &str, open: bool, cx: &mut Context<Self>) {
        let should_collapse = !open;
        if this.node_workspace.is_group_collapsed(key) != should_collapse {
            this.node_workspace.toggle_group(key);
            this.persist_node_workspace();
            cx.notify();
        }
    }

    fn qx_rule_kind_label(kind: QxRuleKind) -> &'static str {
        match kind {
            QxRuleKind::Domain => "DOMAIN",
            QxRuleKind::DomainKeyword => "DOMAIN-KEYWORD",
            QxRuleKind::DomainSuffix => "DOMAIN-SUFFIX",
        }
    }

    fn routing_rule_row(
        order: usize,
        kind: &'static str,
        value: &str,
        target: &str,
        theme: Theme,
    ) -> Div {
        div()
            .mt_1()
            .min_h(px(44.0))
            .px(Space::Md.px())
            .py(Space::Sm.px())
            .rounded(Radius::Row.px())
            .bg(theme.surface_low)
            .flex()
            .items_center()
            .gap(Space::Md.px())
            .child(
                div()
                    .w(px(42.0))
                    .flex_shrink_0()
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .text_color(theme.text_tertiary)
                    .child(format!("#{order:03}")),
            )
            .child(
                div()
                    .w(px(124.0))
                    .flex_shrink_0()
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text_secondary)
                    .child(kind),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_x_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(TextRole::Data.size())
                    .line_height(TextRole::Data.line_height())
                    .child(value.to_owned()),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(TextRole::Data.size())
                    .line_height(TextRole::Data.line_height())
                    .font_weight(TextRole::Data.weight())
                    .text_color(theme.action_primary)
                    .child(target.to_owned()),
            )
    }

    fn qx_rule_import_feedback(&self, theme: Theme, language: Language) -> Div {
        let (message, color) = match &self.rule_sources.feedback {
            QxRuleImportFeedback::Idle => (
                language
                    .localized(copy::configuration::HTTPS_ONLY_UP_TO_1_MIB_INVALID_LINES_ARE_COUNTED)
                    .to_owned(),
                theme.text_secondary,
            ),
            QxRuleImportFeedback::Importing => (
                language
                    .localized(copy::configuration::SECURELY_DOWNLOADING_PARSING_AND_WRITING_LOCALLY)
                    .to_owned(),
                theme.action_primary,
            ),
            QxRuleImportFeedback::Imported {
                rule_count,
                diagnostic_count,
            } => (
                copy::configuration::imported_rules(
                    language,
                    *rule_count,
                    *diagnostic_count,
                ),
                theme.status_success,
            ),
            QxRuleImportFeedback::AlreadyExists {
                rule_count,
                target_policy,
                ..
            } => (
                copy::configuration::duplicate_rule_source(
                    language,
                    *rule_count,
                    target_policy,
                ),
                theme.status_warning,
            ),
            QxRuleImportFeedback::InvalidDocument => (
                language
                    .localized(copy::configuration::FILE_DOWNLOADED_BUT_NO_RECOGNIZABLE_QX_DOMAIN_RULES_WERE_FOUND)
                    .to_owned(),
                theme.status_error,
            ),
            QxRuleImportFeedback::DownloadFailed(error) => (
                copy::configuration::rule_download_error(language, *error).to_owned(),
                theme.status_error,
            ),
            QxRuleImportFeedback::StoreFailed(error) => (
                copy::configuration::subscription_store_error(language, *error).to_owned(),
                theme.status_error,
            ),
        };
        div()
            .mt_2()
            .text_size(TextRole::Body.size())
            .line_height(TextRole::Body.line_height())
            .text_color(color)
            .child(message)
    }

    fn qx_rule_targets(&self) -> Vec<String> {
        let mut targets = self
            .managed_policies
            .groups
            .iter()
            .map(|group| group.name.clone())
            .collect::<Vec<_>>();
        targets.push("DIRECT".to_owned());
        targets
    }

    fn effective_rule_target(&self, target: &str, language: Language) -> String {
        if target != "Proxy"
            || self
                .managed_policies
                .groups
                .iter()
                .any(|group| group.name == target)
        {
            return target.to_owned();
        }
        self.managed_policies.groups.first().map_or_else(
            || {
                language
                    .localized(copy::configuration::GLOBAL_EXIT)
                    .to_owned()
            },
            |group| group.name.clone(),
        )
    }

}
