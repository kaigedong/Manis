//! The Manis 2.0 policy group card.
//!
//! A policy group is a bento card: identity and strategy on the top row, the current exit in a
//! recessed row, and a member grid once expanded. The same shell renders runtime (catalog) groups
//! and locally saved groups so both paths keep one visual grammar.

use super::super::{
    AnyElement, Div, FluentBuilder, InteractiveElement, Language, ParentElement, Radius, Role,
    Space, Stateful, StatefulInteractiveElement, Styled, TextRole, Theme, Toggled, copy, div, px,
};
use crate::components::{
    StatusTone, card_surface, latency_pill, mono_text, primary_button_interaction, status_badge,
};

/// The parts of a policy card that differ between runtime and saved groups.
pub(super) struct PolicyCardContent<'a> {
    /// Debug selector for the card surface, e.g. `policy-card-<id>`.
    pub card_id: String,
    /// Debug selector and element id for the expand toggle.
    pub toggle_id: String,
    pub name: &'a str,
    pub strategy: String,
    /// Candidate count and other supporting text.
    pub meta: String,
    pub current_exit: String,
    pub current_latency: Option<u64>,
    /// Whether the group actually resolves to a candidate exit.
    pub has_exit: bool,
    pub expanded: bool,
    /// The group's identity and latency-test control, shown beside the name.
    pub identity_action: Option<AnyElement>,
    /// Settings and disclosure actions for this group.
    pub actions: Vec<AnyElement>,
    /// Member chips, already built.
    pub members: Vec<AnyElement>,
    /// Shown instead of the grid when the group has no candidates.
    pub members_empty: Option<AnyElement>,
    /// Benchmark progress or result, shown above the member grid.
    pub feedback: Option<AnyElement>,
    /// One member column per row on compact windows.
    pub compact: bool,
    pub language: Language,
    pub theme: Theme,
}

/// One policy candidate rendered as a selectable chip.
pub(super) struct PolicyMember<'a> {
    pub id: String,
    pub name: &'a str,
    pub latency: Option<u64>,
    pub selected: bool,
    pub selectable: bool,
    pub language: Language,
    pub theme: Theme,
}

/// A policy card. `on_toggle` runs on a click anywhere in the identity or exit rows.
pub(super) fn policy_card<F>(content: PolicyCardContent<'_>, on_toggle: F) -> Div
where
    F: Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
{
    let PolicyCardContent {
        card_id,
        toggle_id,
        name,
        strategy,
        meta,
        current_exit,
        current_latency,
        has_exit,
        expanded,
        identity_action,
        actions,
        members,
        members_empty,
        feedback,
        compact,
        language,
        theme,
    } = content;
    let action = if expanded {
        language.localized(copy::common::COLLAPSE)
    } else {
        language.localized(copy::common::EXPAND)
    };
    let aria_label = format!(
        "{action} {}",
        copy::app::policy_identity(language, &strategy, &current_exit)
    );
    let selector = card_id.clone();
    let toggle_selector = toggle_id.clone();
    let mut surface = card_surface(theme)
        .debug_selector(move || selector.clone())
        .flex_shrink_0()
        // The grid cell sets the row height; a shorter card grows to match its neighbour.
        .flex_1()
        .overflow_hidden();
    if expanded {
        // The accent border is the only state marker an expanded card needs.
        surface = surface.border_color(theme.action_primary);
    }
    surface
        .child(
            div()
                .id(toggle_id)
                .debug_selector(move || toggle_selector.clone())
                .role(Role::Button)
                .aria_label(aria_label)
                .tab_stop(true)
                .focusable()
                .map(primary_button_interaction)
                .cursor_pointer()
                .px(Space::Lg.px())
                .py(Space::Md.px())
                .flex()
                .flex_col()
                .gap(px(10.0))
                .on_click(on_toggle)
                .child(card_identity(
                    identity_action,
                    name,
                    strategy,
                    meta,
                    actions,
                    theme,
                ))
                .child(exit_row(current_exit, current_latency, has_exit, theme)),
        )
        .when(expanded, |card| {
            card.child(members_area(
                members,
                members_empty,
                feedback,
                compact,
                theme,
            ))
        })
}

/// The top row: identity medallion, group name, strategy pill and the group's own actions.
fn card_identity(
    identity_action: Option<AnyElement>,
    name: &str,
    strategy: String,
    meta: String,
    actions: Vec<AnyElement>,
    theme: Theme,
) -> Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(10.0))
        .child(
            div()
                .min_w(px(0.0))
                .flex_1()
                .flex()
                .items_center()
                .gap(Space::Sm.px())
                .when_some(identity_action, Div::child)
                .child(
                    div()
                        .min_w(px(0.0))
                        .flex_1()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(Space::Sm.px())
                                .child(
                                    div()
                                        .min_w(px(0.0))
                                        .overflow_x_hidden()
                                        .whitespace_nowrap()
                                        .text_ellipsis()
                                        .text_size(TextRole::Body.size())
                                        .line_height(TextRole::Body.line_height())
                                        .font_weight(TextRole::SectionTitle.weight())
                                        .text_color(theme.text_primary)
                                        .child(name.to_owned()),
                                )
                                .child(status_badge(strategy, StatusTone::Neutral, theme)),
                        )
                        .child(
                            div()
                                .mt(px(2.0))
                                .text_size(TextRole::Metadata.size())
                                .line_height(TextRole::Metadata.line_height())
                                .text_color(theme.text_tertiary)
                                .child(meta),
                        ),
                ),
        )
        .child(
            div()
                .flex_shrink_0()
                .flex()
                .items_center()
                .gap(Space::Sm.px())
                .children(actions),
        )
}

/// The recessed row that names the exit this group currently resolves to.
fn exit_row(
    current_exit: String,
    current_latency: Option<u64>,
    has_exit: bool,
    theme: Theme,
) -> Div {
    div()
        .px(px(10.0))
        .py(Space::Sm.px())
        .rounded(Radius::Control.px())
        .bg(theme.surface_low)
        .flex()
        .items_center()
        .justify_between()
        .gap(Space::Sm.px())
        .child(
            div()
                .min_w(px(0.0))
                .flex()
                .items_center()
                .gap(px(7.0))
                .child(
                    div()
                        .size(px(6.0))
                        .flex_shrink_0()
                        .rounded_full()
                        .bg(if has_exit {
                            theme.status_success
                        } else {
                            theme.text_tertiary
                        }),
                )
                .child(
                    div()
                        .min_w(px(0.0))
                        .overflow_x_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(TextRole::Label.size())
                        .line_height(TextRole::Label.line_height())
                        .font_weight(TextRole::SectionTitle.weight())
                        .text_color(theme.text_primary)
                        .child(current_exit),
                ),
        )
        .child(latency_pill(current_latency, theme))
}

/// The expanded member grid, separated from the exit row by a hairline.
fn members_area(
    members: Vec<AnyElement>,
    members_empty: Option<AnyElement>,
    feedback: Option<AnyElement>,
    compact: bool,
    theme: Theme,
) -> Div {
    div().px(Space::Lg.px()).pb(Space::Md.px()).child(
        div()
            .pt(px(10.0))
            .border_t_1()
            .border_color(theme.outline_subtle)
            .flex()
            .flex_col()
            .gap(px(6.0))
            .when_some(feedback, Div::child)
            .when_some(members_empty, Div::child)
            .children(member_rows(members, compact)),
    )
}

/// The member grid: two chips per row, one per row on compact windows.
fn member_rows(members: Vec<AnyElement>, compact: bool) -> Vec<Div> {
    let columns = if compact { 1 } else { 2 };
    let mut rows = Vec::new();
    let mut members = members.into_iter();
    loop {
        let row: Vec<_> = members.by_ref().take(columns).collect();
        if row.is_empty() {
            break;
        }
        let filled = row.len();
        rows.push(
            div()
                .flex()
                .items_stretch()
                .gap(px(6.0))
                .children(
                    row.into_iter()
                        .map(|chip| div().min_w(px(0.0)).flex_1().flex().flex_col().child(chip)),
                )
                .when(filled < columns, |row| {
                    // Grid parity: a lone chip keeps half the width instead of stretching.
                    row.child(div().flex_1().min_w(px(0.0)))
                }),
        );
    }
    rows
}

/// One candidate chip: name on the left, mono latency on the right, accent when selected.
pub(super) fn member_chip<F>(member: PolicyMember<'_>, on_select: F) -> Stateful<Div>
where
    F: Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
{
    let PolicyMember {
        id,
        name,
        latency,
        selected,
        selectable,
        language,
        theme,
    } = member;
    let selector = id.clone();
    div()
        .id(id)
        .debug_selector(move || selector.clone())
        .min_w(px(0.0))
        .min_h(px(32.0))
        .px(px(10.0))
        .py(px(7.0))
        .rounded(Radius::Control.px())
        .border_1()
        .border_color(if selected {
            theme.action_primary
        } else {
            theme.outline_subtle
        })
        .bg(if selected {
            theme.action_soft
        } else {
            theme.surface_base
        })
        .flex()
        .items_center()
        .justify_between()
        .gap(px(6.0))
        .child(
            div()
                .min_w(px(0.0))
                .overflow_x_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(TextRole::Label.size())
                .line_height(TextRole::Label.line_height())
                .font_weight(if selected {
                    TextRole::SectionTitle.weight()
                } else {
                    TextRole::Label.weight()
                })
                .text_color(if selected {
                    theme.action_primary
                } else {
                    theme.text_primary
                })
                .child(name.to_owned()),
        )
        .child(mono_text(
            latency.map_or_else(|| "—".to_owned(), |value| format!("{value} ms")),
            theme.latency_color(latency),
        ))
        .when(selectable, |chip| {
            chip.role(Role::RadioButton)
                .aria_label(format!(
                    "{} {name}",
                    language.localized(copy::nodes::SELECT)
                ))
                .aria_toggled(if selected {
                    Toggled::True
                } else {
                    Toggled::False
                })
                .tab_stop(true)
                .focusable()
                .map(primary_button_interaction)
                .cursor_pointer()
                .hover(move |chip| {
                    chip.bg(if selected {
                        theme.action_soft
                    } else {
                        theme.surface_high
                    })
                })
                .on_click(on_select)
        })
}

/// The policy grid: two columns on wide and medium windows, one on compact.
pub(super) fn policy_card_grid(cards: Vec<AnyElement>, compact: bool) -> Div {
    let columns = if compact { 1 } else { 2 };
    let mut grid = div().w_full().flex().flex_col().gap(Space::Md.px());
    let mut cards = cards.into_iter();
    loop {
        let row: Vec<_> = cards.by_ref().take(columns).collect();
        if row.is_empty() {
            break;
        }
        let filled = row.len();
        grid = grid.child(
            div()
                .flex()
                .items_stretch()
                .gap(Space::Md.px())
                .children(
                    row.into_iter()
                        .map(|card| div().min_w(px(0.0)).flex_1().flex().flex_col().child(card)),
                )
                .when(filled < columns, |row| {
                    row.child(div().flex_1().min_w(px(0.0)))
                }),
        );
    }
    grid
}
