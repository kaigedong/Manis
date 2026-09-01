use super::{
    ControlSize, ControllerState, FluentBuilder, InteractiveElement, LiveStreamPhase, ManisApp,
    Message, ParentElement, Role, Space, StatefulInteractiveElement, StatusBar, StatusTone, Styled,
    TextRole, Theme, WindowSizeClass, controller_status_label, copy, div, px, status_badge,
    status_bar_values,
};

impl ManisApp {
    pub(in crate::app) fn live_status_issue(&self) -> Option<String> {
        if !matches!(self.controller, ControllerState::Connected { .. }) {
            return None;
        }
        let language = self.language();
        let is_issue = |phase: &LiveStreamPhase| {
            !matches!(
                phase,
                LiveStreamPhase::Waiting | LiveStreamPhase::Connecting | LiveStreamPhase::Live
            )
        };
        if self.live_status.activity == self.live_status.logs
            && is_issue(&self.live_status.activity)
        {
            return Some(copy::app::live_stream_phase(
                language,
                &self.live_status.activity,
            ));
        }
        let issues = [
            (Message::NetworkActivity, &self.live_status.activity),
            (Message::Logs, &self.live_status.logs),
        ]
        .into_iter()
        .filter(|(_, phase)| is_issue(phase))
        .map(|(source, phase)| {
            format!(
                "{}：{}",
                language.message(source),
                copy::app::live_stream_phase(language, phase)
            )
        })
        .collect::<Vec<_>>();
        (!issues.is_empty()).then(|| issues.join(" · "))
    }

    pub(in crate::app) fn status_bar(
        &self,
        theme: Theme,
        size_class: WindowSizeClass,
    ) -> StatusBar {
        let compact = size_class == WindowSizeClass::Compact;
        let language = self.language();
        let kernel_name = self.runtime.kind().display_name();
        let source = controller_status_label(&self.controller, kernel_name, language);
        let mut values = status_bar_values(&self.controller, language, theme);
        let issue = self.live_status_issue();
        if issue.is_some() {
            values.dot = theme.status_warning;
            values.tone = StatusTone::Warning;
        }
        let status = issue.clone().unwrap_or_else(|| self.status.clone());
        let tooltip = status.clone();

        let left = div()
            .flex()
            .items_center()
            .gap(Space::Md.px())
            .min_w_0()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(Space::Sm.px())
                    .flex_none()
                    .child(div().size(px(8.0)).rounded_full().bg(values.dot))
                    .when(!compact || issue.is_none(), |identity| {
                        identity.child(status_badge(source, values.tone, theme))
                    }),
            )
            .when(issue.is_none(), |left| {
                left.child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_size(TextRole::Data.size())
                        .line_height(TextRole::Data.line_height())
                        .font_weight(TextRole::Data.weight())
                        .text_color(theme.text_secondary)
                        .child(values.endpoint),
                )
            })
            .child(
                div()
                    .id("runtime-status-message")
                    .role(Role::Status)
                    .aria_label(status.clone())
                    .min_w_0()
                    .truncate()
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .text_color(if issue.is_some() {
                        theme.status_warning
                    } else {
                        theme.text_tertiary
                    })
                    .tooltip(move |window, cx| {
                        gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
                    })
                    .child(status),
            );
        let right = div()
            .flex()
            .items_center()
            .gap(Space::Md.px())
            .text_size(TextRole::Data.size())
            .line_height(TextRole::Data.line_height())
            .font_weight(TextRole::Data.weight())
            .text_color(theme.text_secondary)
            .when(!compact || issue.is_none(), |right| {
                right.child(values.download).child(values.upload)
            });

        StatusBar::new()
            .h(ControlSize::Standard.height())
            .flex_shrink_0()
            .py_0()
            .px(Space::Md.px())
            .border_t_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_chrome)
            .text_size(TextRole::Metadata.size())
            .line_height(TextRole::Metadata.line_height())
            .text_color(theme.text_secondary)
            .left(left)
            .right(right)
    }
}
