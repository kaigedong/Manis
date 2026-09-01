use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowSizeClass {
    Compact,
    Medium,
    Wide,
}

impl WindowSizeClass {
    #[must_use]
    pub fn for_width(width: f32) -> Self {
        if width >= 1_280.0 {
            Self::Wide
        } else if width >= 900.0 {
            Self::Medium
        } else {
            Self::Compact
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompactNavigation {
    GroupList,
    GroupDetail,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PrimaryWorkspace {
    #[default]
    Nodes,
    RoutingRules,
    Activity,
    Logs,
    Configuration,
}

impl PrimaryWorkspace {
    #[must_use]
    pub const fn navigation_order() -> &'static [Self; 5] {
        &[
            Self::Nodes,
            Self::RoutingRules,
            Self::Activity,
            Self::Logs,
            Self::Configuration,
        ]
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ProxyMode {
    #[default]
    Off,
    System,
    Tun,
}

impl ProxyMode {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Off => "关闭代理",
            Self::System => "系统代理",
            Self::Tun => "TUN 代理",
        }
    }

    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Off => Self::System,
            Self::System => Self::Tun,
            Self::Tun => Self::Off,
        }
    }

    /// Returns the mode that a checkable control should apply when `selected` is clicked.
    ///
    /// Selecting the already active mode clears it, which keeps a checkbox-style tray menu
    /// honest: the check mark is removed and routing falls back to no proxy.
    #[must_use]
    pub const fn toggled(self, selected: Self) -> Self {
        if matches!(
            (self, selected),
            (Self::Off, Self::Off) | (Self::System, Self::System) | (Self::Tun, Self::Tun)
        ) {
            Self::Off
        } else {
            selected
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RoutingMode {
    Direct,
    Global,
    #[default]
    Rule,
}

impl RoutingMode {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Direct => "直连",
            Self::Global => "全局",
            Self::Rule => "规则",
        }
    }

    #[must_use]
    pub const fn wire_value(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Global => "global",
            Self::Rule => "rule",
        }
    }

    #[must_use]
    pub fn parse_wire_value(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "direct" => Some(Self::Direct),
            "global" => Some(Self::Global),
            "rule" => Some(Self::Rule),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct NodeWorkspaceState {
    collapsed_groups: BTreeSet<String>,
}

impl NodeWorkspaceState {
    pub fn toggle_group(&mut self, group_id: &str) {
        if group_id.is_empty() {
            return;
        }
        if !self.collapsed_groups.remove(group_id) {
            self.collapsed_groups.insert(group_id.to_owned());
        }
    }

    #[must_use]
    pub fn is_group_collapsed(&self, group_id: &str) -> bool {
        self.collapsed_groups.contains(group_id)
    }

    pub fn replace_collapsed_groups<'a>(&mut self, group_ids: impl IntoIterator<Item = &'a str>) {
        self.collapsed_groups = group_ids
            .into_iter()
            .filter(|group_id| !group_id.is_empty())
            .map(str::to_owned)
            .collect();
    }

    pub fn collapsed_group_ids(&self) -> impl Iterator<Item = &str> {
        self.collapsed_groups.iter().map(String::as_str)
    }
}
