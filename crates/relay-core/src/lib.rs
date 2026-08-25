use std::collections::BTreeMap;

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

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct PolicyGroupId(pub &'static str);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ProxyId(pub &'static str);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompactNavigation {
    GroupList,
    GroupDetail,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RouteEvidence {
    Predicted {
        domain: String,
        rule: &'static str,
        policy: PolicyGroupId,
        proxy: ProxyId,
    },
    Observed {
        domain: String,
        rule: String,
        policy: PolicyGroupId,
        chain: Vec<ProxyId>,
    },
    NeedsConnection {
        domain: String,
        reason: &'static str,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicyWorkspaceState {
    pub size_class: WindowSizeClass,
    pub selected_group: Option<PolicyGroupId>,
    pub selected_node: Option<ProxyId>,
    pub compact_navigation: CompactNavigation,
    selections: BTreeMap<PolicyGroupId, ProxyId>,
}

impl Default for PolicyWorkspaceState {
    fn default() -> Self {
        Self {
            size_class: WindowSizeClass::Wide,
            selected_group: None,
            selected_node: None,
            compact_navigation: CompactNavigation::GroupList,
            selections: BTreeMap::new(),
        }
    }
}

impl PolicyWorkspaceState {
    #[must_use]
    pub fn demo() -> Self {
        let streaming = PolicyGroupId("streaming");
        let hk_01 = ProxyId("hk-01");
        let search = PolicyGroupId("search");
        let sg_02 = ProxyId("sg-02");
        let selections = BTreeMap::from([(streaming, hk_01), (search, sg_02)]);

        Self {
            selected_group: Some(streaming),
            selected_node: Some(hk_01),
            selections,
            ..Self::default()
        }
    }

    pub fn resize(&mut self, width: f32) {
        self.size_class = WindowSizeClass::for_width(width);
    }

    pub fn select_group(&mut self, group: PolicyGroupId) {
        self.selected_group = Some(group);
        self.selected_node = self.selections.get(&group).copied();
        if self.size_class == WindowSizeClass::Compact {
            self.compact_navigation = CompactNavigation::GroupDetail;
        }
    }

    pub fn select_node(&mut self, proxy: ProxyId) {
        if let Some(group) = self.selected_group {
            self.selections.insert(group, proxy);
            self.selected_node = Some(proxy);
        }
    }

    pub fn navigate_back(&mut self) {
        self.compact_navigation = CompactNavigation::GroupList;
    }

    #[must_use]
    pub fn predict(&self, domain: &str) -> RouteEvidence {
        if domain == "process-dependent.example" {
            return RouteEvidence::NeedsConnection {
                domain: domain.to_owned(),
                reason: "该规则依赖进程信息，需要实际连接才能确认",
            };
        }

        let (policy, fallback_proxy) =
            if domain.ends_with("youtube.com") || domain.ends_with("netflix.com") {
                (PolicyGroupId("streaming"), ProxyId("hk-01"))
            } else if domain.ends_with("openai.com") || domain.ends_with("google.com") {
                (PolicyGroupId("search"), ProxyId("sg-02"))
            } else {
                return RouteEvidence::NeedsConnection {
                    domain: domain.to_owned(),
                    reason: "缺少可确定的域名规则，需要实际连接才能确认",
                };
            };

        RouteEvidence::Predicted {
            domain: domain.to_owned(),
            rule: "DOMAIN-SUFFIX",
            policy,
            proxy: self
                .selections
                .get(&policy)
                .copied()
                .unwrap_or(fallback_proxy),
        }
    }
}
