use std::collections::BTreeMap;

use crate::{CompactNavigation, PolicyGroupId, ProxyId, WindowSizeClass};

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
        let streaming = PolicyGroupId::new("streaming");
        let hk_01 = ProxyId::new("hk-01");
        let search = PolicyGroupId::new("search");
        let sg_02 = ProxyId::new("sg-02");
        let selections = BTreeMap::from([
            (streaming.clone(), hk_01.clone()),
            (search.clone(), sg_02.clone()),
        ]);

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
        self.selected_node = self.selections.get(&group).cloned();
        self.selected_group = Some(group);
        if self.size_class == WindowSizeClass::Compact {
            self.compact_navigation = CompactNavigation::GroupDetail;
        }
    }

    pub fn select_node(&mut self, proxy: ProxyId) {
        if let Some(group) = &self.selected_group {
            self.selections.insert(group.clone(), proxy.clone());
            self.selected_node = Some(proxy);
        }
    }

    #[must_use]
    pub fn selection_for(&self, group: &PolicyGroupId) -> Option<&ProxyId> {
        self.selections.get(group)
    }

    pub fn navigate_back(&mut self) {
        self.compact_navigation = CompactNavigation::GroupList;
    }

    pub fn replace_source_selection(&mut self, group: PolicyGroupId, proxy: Option<ProxyId>) {
        self.selections.clear();
        if let Some(proxy) = &proxy {
            self.selections.insert(group.clone(), proxy.clone());
        }
        self.selected_group = Some(group);
        self.selected_node = proxy;
        self.compact_navigation = CompactNavigation::GroupList;
    }

    pub fn clear_source_selection(&mut self) {
        self.selections.clear();
        self.selected_group = None;
        self.selected_node = None;
        self.compact_navigation = CompactNavigation::GroupList;
    }
}
