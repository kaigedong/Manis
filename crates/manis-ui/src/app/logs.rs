use std::cell::Cell;

use gpui::{
    ClipboardItem, Div, FontWeight, ParentElement, Rgba, Stateful, Styled, div, prelude::*, px,
};
use gpui_component::{
    IconName, Selectable as _, Sizable as _,
    button::{Button, ButtonVariant, ButtonVariants},
};
use manis_core::WindowSizeClass;

use super::ManisApp;
use crate::{
    components::{
        ActionRole, StatusTone, action_button, card_surface, empty_state, status_badge,
        style_action_button,
    },
    diagnostics::{UiLogEntry, recent_ui_logs},
    localization::{Language, Message, copy},
    mihomo::KernelLogEntry,
    theme::{ControlSize, MONO_FAMILY, Radius, Space, TextRole, Theme},
};

impl ManisApp {
    pub(super) fn logs_workspace(
        &self,
        theme: Theme,
        size_class: WindowSizeClass,
        cx: &mut gpui::Context<Self>,
    ) -> Div {
        let compact = size_class == WindowSizeClass::Compact;
        let query = logs_search_query(self, cx);
        let normalized_query = query.to_lowercase();
        let filter = log_level_filter();
        let logs = recent_ui_logs()
            .into_iter()
            .filter(|entry| ui_log_matches(entry, &normalized_query, filter))
            .collect::<Vec<_>>();
        let kernel_logs = self
            .kernel_logs
            .iter()
            .filter(|entry| kernel_log_matches(entry, &normalized_query, filter))
            .collect::<Vec<_>>();
        let language = self.language();
        let rows = logs_rows(logs, kernel_logs, query.is_empty(), language, theme);

        let refresh_action = action_button(
            "refresh-logs",
            language.message(Message::RefreshData),
            ActionRole::Secondary,
            ControlSize::Compact,
        )
        .icon(IconName::RefreshCw)
        .accessibility_label(language.localized(copy::logs::REFRESH_LOG_DATA))
        .on_click(cx.listener(|this, _, _, cx| this.connect_mihomo(cx)));
        let copy_action = style_action_button(
            Button::new("copy-logs").icon(IconName::Copy),
            ActionRole::Secondary,
            ControlSize::Icon,
        )
        .accessibility_label(logs_copy_label(language))
        .on_click(cx.listener(|this, _, _, cx| this.copy_logs_to_clipboard(cx)));
        let header = div()
            .flex_shrink_0()
            .px(if compact {
                Space::Lg.px()
            } else {
                Space::Xl.px()
            })
            .py(Space::Md.px())
            .flex()
            .flex_wrap()
            .items_center()
            .gap(Space::Sm.px())
            .when_some(self.inputs.logs_search.clone(), |header, input| {
                header.child(
                    div()
                        .flex_1()
                        .min_w(px(180.0))
                        .when(compact, gpui::Styled::w_full)
                        .child(input),
                )
            })
            .child(log_level_segments(filter, language, theme, cx))
            .child(div().flex_shrink_0().child(status_badge(
                logs_summary(language),
                StatusTone::Neutral,
                theme,
            )))
            .child(copy_action)
            .child(refresh_action);

        div()
            .flex_1()
            .h_full()
            .min_w_0()
            .flex()
            .flex_col()
            .bg(theme.surface_base)
            .child(header)
            .child(rows)
    }

    /// Copies the entries the current search and level filter leave visible.
    fn copy_logs_to_clipboard(&mut self, cx: &mut gpui::Context<Self>) {
        let normalized_query = logs_search_query(self, cx).to_lowercase();
        let filter = log_level_filter();
        let mut lines = Vec::new();
        for entry in self
            .kernel_logs
            .iter()
            .filter(|entry| kernel_log_matches(entry, &normalized_query, filter))
        {
            lines.push(format!(
                "{} {} {}",
                format_log_time(entry.timestamp_ms),
                entry.level.to_uppercase(),
                entry.payload
            ));
        }
        for entry in recent_ui_logs()
            .into_iter()
            .filter(|entry| ui_log_matches(entry, &normalized_query, filter))
        {
            lines.push(format!(
                "{} {} {}",
                format_log_time(entry.timestamp_ms),
                entry.level.to_uppercase(),
                entry.event
            ));
        }
        cx.write_to_clipboard(ClipboardItem::new_string(lines.join("\n")));
        logs_copied(self.language()).clone_into(&mut self.status);
        cx.notify();
    }
}

/// The level filter a log stream is narrowed to.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum LogLevelFilter {
    #[default]
    All,
    Error,
    Warn,
    Info,
    Debug,
}

impl LogLevelFilter {
    const ALL: [Self; 5] = [Self::All, Self::Error, Self::Warn, Self::Info, Self::Debug];

    const fn slot(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
        }
    }

    fn label(self, language: Language) -> &'static str {
        match (language, self) {
            (Language::SimplifiedChinese, Self::All) => "全部",
            (Language::English, Self::All) => "All",
            (_, Self::Error) => "ERROR",
            (_, Self::Warn) => "WARN",
            (_, Self::Info) => "INFO",
            (_, Self::Debug) => "DEBUG",
        }
    }

    const fn accepts(self, level: LogLevel) -> bool {
        match self {
            Self::All => true,
            Self::Error => matches!(level, LogLevel::Error),
            Self::Warn => matches!(level, LogLevel::Warn),
            Self::Info => matches!(level, LogLevel::Info),
            // TRACE is finer than DEBUG, so the debug filter keeps it visible.
            Self::Debug => matches!(level, LogLevel::Debug | LogLevel::Trace),
        }
    }
}

// A level filter is a view preference the workspace owns no state for, so it is kept beside the
// workspace's own thread of control rather than on the application.
thread_local! {
    static LOG_LEVEL_FILTER: Cell<LogLevelFilter> = const { Cell::new(LogLevelFilter::All) };
}

fn log_level_filter() -> LogLevelFilter {
    LOG_LEVEL_FILTER.with(Cell::get)
}

fn set_log_level_filter(filter: LogLevelFilter) {
    LOG_LEVEL_FILTER.with(|cell| cell.set(filter));
}

/// The severity a log level names, independent of how the kernel spells it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
    Other,
}

impl LogLevel {
    fn classify(level: &str) -> Self {
        match level.to_ascii_lowercase().as_str() {
            "error" | "fatal" | "panic" => Self::Error,
            "warn" | "warning" => Self::Warn,
            "info" => Self::Info,
            "debug" => Self::Debug,
            "trace" => Self::Trace,
            _ => Self::Other,
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Error => "ERROR",
            Self::Warn => "WARN",
            Self::Info => "INFO",
            Self::Debug => "DEBUG",
            Self::Trace => "TRACE",
            Self::Other => "",
        }
    }

    /// The tag's colour pair: status soft fills for the two warning tones, a tint of the level
    /// colour everywhere else, so no level borrows a colour it does not own.
    fn fill(self, theme: Theme) -> (Rgba, Rgba) {
        match self {
            Self::Error => (theme.status_error, theme.status_error_soft),
            Self::Warn => (theme.status_warning, theme.status_warning_soft),
            Self::Info => (theme.log_info, tinted(theme, theme.log_info)),
            Self::Debug => (theme.log_debug, tinted(theme, theme.log_debug)),
            Self::Trace => (theme.log_trace, tinted(theme, theme.log_trace)),
            Self::Other => (theme.text_secondary, theme.surface_low),
        }
    }
}

fn tinted(theme: Theme, color: Rgba) -> Rgba {
    theme.surface_base.blend(color.opacity(0.1))
}

fn logs_search_query(app: &ManisApp, cx: &gpui::App) -> String {
    app.inputs
        .logs_search
        .as_ref()
        .map(|input| input.read(cx).value().trim().to_owned())
        .unwrap_or_default()
}

fn log_level_segments(
    current: LogLevelFilter,
    language: Language,
    theme: Theme,
    cx: &mut gpui::Context<ManisApp>,
) -> Div {
    let selected_fill = if theme.surface_elevated == theme.surface_base {
        theme.surface_high
    } else {
        theme.surface_elevated
    };
    let mut segments = div().flex().items_center().gap(px(2.0)).h_full();
    for filter in LogLevelFilter::ALL {
        let selected = filter == current;
        let label = filter.label(language);
        segments = segments.child(
            Button::new(format!("log-level-filter-{}", filter.slot()))
                .map(crate::components::primary_button_interaction)
                .debug_selector(move || format!("log-level-filter-{}", filter.slot()))
                .accessibility_label(label)
                .label(label)
                .selected(selected)
                .with_variant(ButtonVariant::Ghost)
                .with_size(gpui_component::Size::Small)
                .h_full()
                .px(px(9.0))
                .rounded(px(6.0))
                .text_size(TextRole::Metadata.size())
                .line_height(TextRole::Metadata.line_height())
                .font_weight(if selected {
                    FontWeight::SEMIBOLD
                } else {
                    TextRole::Label.weight()
                })
                .when(selected, |button| {
                    button
                        .bg(selected_fill)
                        .text_color(theme.text_primary)
                        .cursor_pointer()
                })
                .when(!selected, |button| {
                    button.text_color(theme.text_secondary).cursor_pointer()
                })
                .on_click(cx.listener(move |_, _, _, cx| {
                    cx.stop_propagation();
                    set_log_level_filter(filter);
                    cx.notify();
                })),
        );
    }
    div()
        .flex_shrink_0()
        .h(px(28.0))
        .p(px(2.0))
        .rounded(Radius::Control.px())
        .border_1()
        .border_color(theme.outline_subtle)
        .bg(theme.surface_low)
        .flex()
        .items_center()
        .child(segments)
}

fn logs_rows(
    logs: Vec<UiLogEntry>,
    kernel_logs: Vec<&KernelLogEntry>,
    no_query: bool,
    language: Language,
    theme: Theme,
) -> Stateful<Div> {
    let rows = div()
        .id("logs-scroll")
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .p(Space::Lg.px());
    if logs.is_empty() && kernel_logs.is_empty() {
        let filtered = !no_query || log_level_filter() != LogLevelFilter::All;
        return rows.child(if filtered {
            logs_empty_state(language, false, theme)
        } else {
            logs_empty_state(language, true, theme)
        });
    }
    let mut card = card_surface(theme).overflow_hidden().flex().flex_col();
    let total = kernel_logs.len() + logs.len();
    let mut index = 0;
    for entry in kernel_logs.into_iter().rev() {
        card = card.child(kernel_log_row(entry, index + 1 == total, theme));
        index += 1;
    }
    for entry in logs.into_iter().rev() {
        let reference = entry.operation_id.map_or_else(
            || format!("#{:04}", entry.sequence),
            |operation| format!("#{:04} · OP-{operation:04}", entry.sequence),
        );
        card = card.child(ui_log_row(entry, reference, index + 1 == total, theme));
        index += 1;
    }
    rows.child(card)
}

fn logs_empty_state(language: Language, no_logs_yet: bool, theme: Theme) -> Div {
    let (title, detail) = if no_logs_yet {
        (
            language.message(Message::NoLogs),
            language
                .localized(copy::logs::LOGS_WILL_APPEAR_HERE_AFTER_MIHOMO_STARTS_OR_MANIS_PERFORMS),
        )
    } else {
        (
            language.message(Message::NoFilterMatches),
            language
                .localized(copy::logs::CLEAR_THE_FILTER_OR_SEARCH_BY_OPERATION_ERROR_MESSAGE_OR),
        )
    };
    empty_state(title, detail, None, theme)
}

fn kernel_log_row(entry: &KernelLogEntry, last: bool, theme: Theme) -> Div {
    log_row(
        format!("K#{:04}", entry.sequence),
        &entry.level,
        div()
            .font_family(MONO_FAMILY)
            .text_size(TextRole::Data.size())
            .line_height(TextRole::Data.line_height())
            .text_color(theme.text_primary)
            .child(entry.payload.clone()),
        format_log_time(entry.timestamp_ms),
        last,
        theme,
    )
}

fn ui_log_row(entry: UiLogEntry, reference: String, last: bool, theme: Theme) -> Div {
    log_row(
        reference,
        &entry.level,
        div()
            .flex()
            .flex_col()
            .gap(Space::Xs.px())
            .font_family(MONO_FAMILY)
            .text_size(TextRole::Data.size())
            .line_height(TextRole::Data.line_height())
            .text_color(theme.text_primary)
            .child(entry.event)
            .when_some(entry.detail, |row, detail| {
                row.child(
                    div()
                        .font_family(MONO_FAMILY)
                        .text_size(TextRole::Metadata.size())
                        .line_height(TextRole::Metadata.line_height())
                        .text_color(theme.text_secondary)
                        .child(detail),
                )
            }),
        format_log_time(entry.timestamp_ms),
        last,
        theme,
    )
}

/// One log line: mono time, a fixed-width level tag, the body, then the entry's own reference.
fn log_row(
    reference: String,
    level: &str,
    body: Div,
    timestamp: String,
    last: bool,
    theme: Theme,
) -> Div {
    div()
        .flex_shrink_0()
        .flex()
        .items_start()
        .gap(Space::Md.px())
        .px(Space::Lg.px())
        .py(Space::Sm.px())
        .when(!last, |row| {
            row.border_b_1().border_color(theme.outline_subtle)
        })
        .child(log_timestamp(timestamp, theme))
        .child(log_level_tag(level, theme))
        .child(div().flex_1().min_w(px(0.0)).child(body))
        .child(log_reference(reference, theme))
}

fn log_reference(reference: String, theme: Theme) -> Div {
    div()
        .w(px(88.0))
        .flex_shrink_0()
        .font_family(MONO_FAMILY)
        .text_size(TextRole::Metadata.size())
        .line_height(TextRole::Metadata.line_height())
        .text_color(theme.text_tertiary)
        .child(reference)
}

/// A 48px level tag: one width for every level, so the body column never shifts.
fn log_level_tag(level: &str, theme: Theme) -> Div {
    let severity = LogLevel::classify(level);
    let (foreground, background) = severity.fill(theme);
    let label: gpui::SharedString = match severity.name() {
        "" => level.to_uppercase().into(),
        name => name.into(),
    };
    div()
        .w(px(48.0))
        .h(px(18.0))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .overflow_hidden()
        .rounded(Radius::Badge.px() - px(1.0))
        .bg(background)
        .font_family(MONO_FAMILY)
        .text_size(px(10.5))
        .line_height(px(14.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(foreground)
        .child(label)
}

fn log_timestamp(timestamp: String, theme: Theme) -> Div {
    div()
        .w(px(74.0))
        .flex_shrink_0()
        .font_family(MONO_FAMILY)
        .text_size(TextRole::Metadata.size())
        .line_height(TextRole::Metadata.line_height())
        .text_color(theme.text_tertiary)
        .child(timestamp)
}

fn ui_log_matches(entry: &UiLogEntry, normalized_query: &str, filter: LogLevelFilter) -> bool {
    if !filter.accepts(LogLevel::classify(&entry.level)) {
        return false;
    }
    if normalized_query.is_empty() {
        return true;
    }
    let operation = entry
        .operation_id
        .map(|operation| format!("op-{operation:04}"));
    [
        Some(entry.event.as_str()),
        Some(entry.level.as_str()),
        entry.detail.as_deref(),
        operation.as_deref(),
    ]
    .into_iter()
    .flatten()
    .any(|value| value.to_lowercase().contains(normalized_query))
        || entry.sequence.to_string().contains(normalized_query)
}

fn kernel_log_matches(
    entry: &KernelLogEntry,
    normalized_query: &str,
    filter: LogLevelFilter,
) -> bool {
    if !filter.accepts(LogLevel::classify(&entry.level)) {
        return false;
    }
    if normalized_query.is_empty() {
        return true;
    }
    entry.level.to_lowercase().contains(normalized_query)
        || entry.payload.to_lowercase().contains(normalized_query)
        || format!("k#{:04}", entry.sequence).contains(normalized_query)
}

fn logs_summary(language: Language) -> String {
    copy::logs::summary(language)
}

fn logs_copied(language: Language) -> &'static str {
    match language {
        Language::English => "Visible logs copied.",
        Language::SimplifiedChinese => "已复制当前可见日志。",
    }
}

fn logs_copy_label(language: Language) -> &'static str {
    match language {
        Language::English => "Copy the visible logs",
        Language::SimplifiedChinese => "复制当前可见的日志",
    }
}

fn format_log_time(timestamp_ms: u128) -> String {
    // `Local` silently falls back to UTC when the system zone is unreadable, so there is no
    // failure to report here — only an offset, which is zero in that case.
    format_local_time(
        timestamp_ms,
        chrono::Local::now().offset().local_minus_utc(),
    )
}

/// Renders a timestamp on the viewer's own wall clock.
fn format_local_time(timestamp_ms: u128, offset_seconds: i32) -> String {
    // Reducing to one day first keeps the value trivially in range for the signed arithmetic.
    let day_seconds = i64::try_from(timestamp_ms / 1_000 % 86_400).unwrap_or(0);
    // `rem_euclid` keeps a westward offset from underflowing into a negative clock.
    let seconds = (day_seconds + i64::from(offset_seconds)).rem_euclid(86_400);
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3_600,
        seconds % 3_600 / 60,
        seconds % 60
    )
}

#[cfg(test)]
mod tests {
    use crate::diagnostics::UiLogEntry;

    #[test]
    fn log_time_is_shown_in_the_local_offset() {
        // 01:02:03 UTC on each viewer's own wall clock.
        assert_eq!(super::format_local_time(3_723_000, 8 * 3_600), "09:02:03");
        assert_eq!(super::format_local_time(3_723_000, 0), "01:02:03");
        // Half-hour zones such as India must not be rounded away.
        assert_eq!(
            super::format_local_time(3_723_000, 5 * 3_600 + 1_800),
            "06:32:03"
        );
    }

    #[test]
    fn a_local_offset_may_move_the_clock_across_midnight() {
        // 02:00:00 UTC is the previous evening in New York.
        assert_eq!(super::format_local_time(7_200_000, -5 * 3_600), "21:00:00");
        // 23:00:00 UTC is the next morning in Shanghai.
        assert_eq!(super::format_local_time(82_800_000, 8 * 3_600), "07:00:00");
    }

    #[test]
    fn severity_drives_the_level_colour() {
        let theme = crate::theme::Theme::light();

        assert_eq!(
            super::LogLevel::classify("ERROR").fill(theme).0,
            theme.status_error
        );
        assert_eq!(
            super::LogLevel::classify("WARN").fill(theme).0,
            theme.status_warning
        );
        assert_eq!(
            super::LogLevel::classify("INFO").fill(theme).0,
            theme.log_info
        );
        assert_eq!(
            super::LogLevel::classify("DEBUG").fill(theme).0,
            theme.log_debug
        );
        assert_eq!(
            super::LogLevel::classify("TRACE").fill(theme).0,
            theme.log_trace
        );
    }

    #[test]
    fn kernel_level_spellings_map_to_the_same_colours() {
        let theme = crate::theme::Theme::dark();

        // Mihomo writes "warning" and lowercases everything.
        assert_eq!(
            super::LogLevel::classify("warning").fill(theme).0,
            theme.status_warning
        );
        assert_eq!(
            super::LogLevel::classify("warn").fill(theme).0,
            theme.status_warning
        );
        assert_eq!(
            super::LogLevel::classify("error").fill(theme).0,
            theme.status_error
        );
        assert_eq!(
            super::LogLevel::classify("fatal").fill(theme).0,
            theme.status_error
        );
        assert_eq!(
            super::LogLevel::classify("panic").fill(theme).0,
            theme.status_error
        );
        assert_eq!(
            super::LogLevel::classify("info").fill(theme).0,
            theme.log_info
        );
        assert_eq!(
            super::LogLevel::classify("debug").fill(theme).0,
            theme.log_debug
        );
        assert_eq!(
            super::LogLevel::classify("trace").fill(theme).0,
            theme.log_trace
        );
        // An unknown level must stay legible rather than borrow a severity colour.
        assert_eq!(
            super::LogLevel::classify("silly").fill(theme).0,
            theme.text_secondary
        );
    }

    #[test]
    fn log_levels_are_visually_distinct_in_both_themes() {
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            let colors = [
                theme.log_trace,
                theme.log_debug,
                theme.log_info,
                theme.status_warning,
                theme.status_error,
            ];
            for (index, color) in colors.iter().enumerate() {
                for other in &colors[index + 1..] {
                    assert_ne!(color, other);
                }
                assert_ne!(*color, theme.action_primary);
                assert_ne!(*color, theme.text_tertiary);
            }
        }
    }

    #[test]
    fn log_summary_uses_selected_language() {
        assert_eq!(
            super::logs_summary(crate::localization::Language::English),
            "Sensitive data hidden"
        );
        assert_eq!(
            super::logs_summary(crate::localization::Language::SimplifiedChinese),
            "敏感信息已隐藏"
        );
    }

    #[test]
    fn log_filter_matches_event_detail_level_and_operation_number() {
        let entry = UiLogEntry {
            sequence: 12,
            timestamp_ms: 0,
            level: "ERROR".to_owned(),
            operation_id: Some(5),
            event: "proxy.mode.failed".to_owned(),
            detail: Some("helper timed out".to_owned()),
        };

        for query in ["proxy", "ERROR", "timed out", "op-0005", "12", ""] {
            assert!(super::ui_log_matches(
                &entry,
                &query.to_lowercase(),
                super::LogLevelFilter::All
            ));
        }
        assert!(!super::ui_log_matches(
            &entry,
            "routing",
            super::LogLevelFilter::All
        ));
    }

    /// The level filter narrows by severity, and DEBUG keeps the finer TRACE lines visible.
    #[test]
    fn the_level_filter_narrows_by_severity() {
        let entries = [
            ("ERROR", super::LogLevelFilter::Error),
            ("warning", super::LogLevelFilter::Warn),
            ("info", super::LogLevelFilter::Info),
            ("debug", super::LogLevelFilter::Debug),
            ("trace", super::LogLevelFilter::Debug),
        ];
        for (level, filter) in entries {
            for candidate in super::LogLevelFilter::ALL {
                let kept = super::ui_log_matches(
                    &UiLogEntry {
                        sequence: 1,
                        timestamp_ms: 0,
                        level: level.to_owned(),
                        operation_id: None,
                        event: "kernel.started".to_owned(),
                        detail: None,
                    },
                    "",
                    candidate,
                );
                let expected = candidate == super::LogLevelFilter::All || candidate == filter;
                assert_eq!(kept, expected, "{level} under {candidate:?}");
            }
        }
    }

    /// Copying puts exactly the entries the filter leaves visible on the clipboard.
    #[gpui::test]
    fn copying_logs_writes_the_visible_entries_to_the_clipboard(cx: &mut gpui::TestAppContext) {
        use gpui::{AppContext as _, ClipboardItem};

        cx.update(crate::init);
        let mut app = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            let entity = cx.new(|_| super::ManisApp::with_fixture_controller("http://127.0.0.1:1"));
            app = Some(entity.clone());
            crate::root(entity, window, cx)
        });
        let app = app.expect("fixture app");
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.write_to_clipboard(ClipboardItem::new_string(String::new()));

        crate::diagnostics::record_event(
            crate::diagnostics::LogLevel::Info,
            "logs.copy.fixture",
            "visible detail",
        );
        super::set_log_level_filter(super::LogLevelFilter::Info);

        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            app.update(cx, super::ManisApp::copy_logs_to_clipboard);
        });

        let copied = cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .expect("clipboard text");
        assert!(copied.contains("logs.copy.fixture"), "{copied}");
        assert!(!copied.contains("workspace."), "{copied}");
        app.read_with(cx, |app, _| {
            // The notice follows the interface language, which the fixture resolves from the
            // host locale, so assert the pair rather than one fixed translation.
            assert_eq!(app.status, super::logs_copied(app.language()));
        });
        super::set_log_level_filter(super::LogLevelFilter::All);
    }

    #[test]
    fn the_level_filter_labels_match_the_design_copy() {
        for (filter, chinese, english) in [
            (super::LogLevelFilter::All, "全部", "All"),
            (super::LogLevelFilter::Error, "ERROR", "ERROR"),
            (super::LogLevelFilter::Warn, "WARN", "WARN"),
            (super::LogLevelFilter::Info, "INFO", "INFO"),
            (super::LogLevelFilter::Debug, "DEBUG", "DEBUG"),
        ] {
            assert_eq!(
                filter.label(crate::localization::Language::SimplifiedChinese),
                chinese
            );
            assert_eq!(
                filter.label(crate::localization::Language::English),
                english
            );
        }
    }

    #[test]
    fn the_level_tag_shows_one_canonical_label_per_severity() {
        assert_eq!(super::LogLevel::classify("warning").name(), "WARN");
        assert_eq!(super::LogLevel::classify("fatal").name(), "ERROR");
        assert_eq!(super::LogLevel::classify("silly").name(), "");
    }
}
