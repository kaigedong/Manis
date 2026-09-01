mod dialog;
mod form;
use gpui::AppContext as _;
use gpui_component::WindowExt as _;

use super::{
    ActionRole, AnyElement, Button, Context, ControlSize, Dialog, Disableable, Div, FluentBuilder,
    Focusable, InteractiveElement, IntoElement, Language, LogLevel, MAX_MANUAL_RULE_INPUT_BYTES,
    ManisApp, ManualRulePopover, Message, ParentElement, Radius, Role, Space, Stateful,
    StatefulInteractiveElement, Styled, SubscriptionTextInput, TextInputSpec, TextRole, Theme,
    Window, WindowSizeClass, copy, dialog_footer_surface, dialog_header_surface, div, field_label,
    manual_rule_error_label, manual_rule_kind_detail, manual_rule_placeholder, px, record_event,
    style_action_button, surface_dialog,
};
impl ManisApp {
    pub(in crate::app) fn open_manual_rule_editor(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.manual_rule_editor_state.is_open() {
            if let Some(condition) = self.manual_rule_conditions.first() {
                condition.input.focus_handle(cx).focus(window, cx);
            }
            return;
        }
        self.manual_rule_editor_state = crate::app::ManualRuleEditorState::Creating;
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

    pub(in crate::app) fn open_manual_rule_editor_for_edit(
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
        self.manual_rule_editor_state = crate::app::ManualRuleEditorState::Editing(index);
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

    pub(in crate::app) fn open_manual_rule_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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

    pub(in crate::app) fn reset_manual_rule_editor_state(&mut self) {
        self.manual_rule_editor_state = crate::app::ManualRuleEditorState::Closed;
        self.manual_rule_popover = None;
        self.manual_rule_error = None;
    }

    pub(in crate::app) fn close_manual_rule_editor(&mut self, cx: &mut Context<Self>) {
        self.reset_manual_rule_editor_state();
        cx.notify();
    }

    pub(in crate::app) fn ensure_manual_rule_input(
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
                crate::app::ManualRuleConditionEditor { kind, input }
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

    pub(in crate::app) fn manual_rule_targets(&self) -> Vec<String> {
        let mut targets = self
            .managed_policies
            .groups
            .iter()
            .map(|group| group.name.clone())
            .collect::<Vec<_>>();
        targets.extend(["DIRECT".to_owned(), "REJECT".to_owned()]);
        targets
    }

    pub(in crate::app) fn set_manual_rule_kind(
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

    pub(in crate::app) fn add_manual_rule_condition(&mut self, cx: &mut Context<Self>) {
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

    pub(in crate::app) fn remove_manual_rule_condition(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) {
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

    pub(in crate::app) fn apply_manual_rule_edit(
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

    pub(in crate::app) fn submit_manual_rule(&mut self, cx: &mut Context<Self>) -> bool {
        if self.configuration_transfer.active {
            return false;
        }
        if self.manual_rule_editor_state == crate::app::ManualRuleEditorState::Closed {
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

    pub(in crate::app) fn remove_manual_rule(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.configuration_transfer.active {
            return;
        }
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

    pub(in crate::app) fn set_manual_rule_enabled(
        &mut self,
        index: usize,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        if self.configuration_transfer.active {
            return;
        }
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
}
