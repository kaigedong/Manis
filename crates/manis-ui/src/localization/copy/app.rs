use crate::localization::LocalizedText;

mod format;
pub(crate) use format::*;

pub(crate) const ACTIVITY: LocalizedText = LocalizedText::new("Activity", "活动");
pub(crate) const ADD_A_SOURCE_OR_CREATE_A_POLICY_GROUP_TO_CHOOSE: LocalizedText =
    LocalizedText::new(
        "Add a source or create a policy group to choose how traffic should be routed.",
        "添加来源或新建策略组，开始设置流量出口。",
    );
pub(crate) const ANOTHER_GROUP_IS_BEING_TESTED_WAIT_FOR_IT_TO_FINISH: LocalizedText =
    LocalizedText::new(
        "Another group is being tested; wait for it to finish",
        "已有分组正在测速，请等待完成后再试",
    );
pub(crate) const AUTOMATIC_SELECTION: LocalizedText =
    LocalizedText::new("Automatic selection", "自动选择");
pub(crate) const AUTOMATIC_SYSTEM_PROXY_RECOVERY_FAILED: LocalizedText = LocalizedText::new(
    " · automatic system proxy recovery failed: ",
    " · 系统代理自动恢复失败：",
);
pub(crate) const AUTO_SELECT: LocalizedText = LocalizedText::new("Auto select", "自动选择");
pub(crate) const CANDIDATE_GROUP: LocalizedText =
    LocalizedText::new("Candidate / group", "候选节点 / 分组");
pub(crate) const CHANGE_PROXY_MODE: LocalizedText =
    LocalizedText::new("Change proxy mode", "切换代理模式");
pub(crate) const CHANGE_ROUTING_MODE: LocalizedText =
    LocalizedText::new("Change routing mode", "切换路由模式");
pub(crate) const CONFIGURATION: LocalizedText = LocalizedText::new("configuration", "配置");
pub(crate) const CONFIGURATION_OPENED: LocalizedText =
    LocalizedText::new("Configuration opened", "已打开配置");
pub(crate) const CONNECTING: LocalizedText = LocalizedText::new("connecting", "连接中");
pub(crate) const CONNECTION_FAILED: LocalizedText =
    LocalizedText::new("connection failed", "连接失败");
pub(crate) const CONNECT_BEFORE_CHANGING_PROXY_MODE: LocalizedText = LocalizedText::new(
    "Connect before changing proxy mode:",
    "请先连接后再切换代理模式：",
);
pub(crate) const CONNECT_FIRST: LocalizedText = LocalizedText::new("connect first", "需先连接");
pub(crate) const CONNECT_TO_THE_KERNEL_BEFORE_CHANGING_ROUTING_MODE: LocalizedText =
    LocalizedText::new(
        "Connect to the kernel before changing routing mode",
        "请先连接内核，再切换路由模式",
    );
pub(crate) const COULD_NOT_REMOVE_SUBSCRIPTION: LocalizedText =
    LocalizedText::new("Could not remove subscription: ", "移除订阅失败：");
pub(crate) const COULD_NOT_SAVE_SUBSCRIPTION: LocalizedText =
    LocalizedText::new("Could not save subscription", "订阅保存失败");
pub(crate) const COULD_NOT_SAVE_SUBSCRIPTION_2: LocalizedText =
    LocalizedText::new("Could not save subscription: ", "订阅保存失败：");
pub(crate) const COULD_NOT_SAVE_THE_GLOBAL_NODE: LocalizedText =
    LocalizedText::new("Could not save the global node: ", "无法保存全局节点：");
pub(crate) const COULD_NOT_SAVE_NODE_SOURCE_EXPANSION: LocalizedText = LocalizedText::new(
    "Could not save the node source expansion state",
    "无法保存节点来源展开状态",
);
pub(crate) const COULD_NOT_SAVE_THE_POLICY_SELECTION: LocalizedText = LocalizedText::new(
    "Could not save the policy selection: ",
    "无法保存策略组选择：",
);
pub(crate) const CURRENT: LocalizedText = LocalizedText::new("Current", "当前出口");
pub(crate) const DARK: LocalizedText = LocalizedText::new("Dark", "深色");
pub(crate) const DARK_THEME_ENABLED: LocalizedText =
    LocalizedText::new("Dark theme enabled", "已切换到深色主题");
pub(crate) const DELETE_POLICY_GROUP: LocalizedText =
    LocalizedText::new("Delete policy group", "删除策略组");
pub(crate) const DISCONNECTED: LocalizedText = LocalizedText::new("disconnected", "未连接");
pub(crate) const DOWNLOADING_AND_VERIFYING_THE_STABLE_MIHOMO_RELEASE: LocalizedText =
    LocalizedText::new(
        "Downloading and verifying the stable Mihomo release…",
        "正在下载并校验 Mihomo 稳定版…",
    );
pub(crate) const ENABLED: LocalizedText = LocalizedText::new(" enabled", "已生效");
pub(crate) const ENABLING: LocalizedText = LocalizedText::new("Enabling…", "启用中…");
pub(crate) const FAILED_TO_CHANGE_PROXY_MODE: LocalizedText =
    LocalizedText::new("Failed to change proxy mode: ", "代理模式切换失败：");
pub(crate) const FAILED_TO_CHANGE_ROUTING_MODE: LocalizedText =
    LocalizedText::new("Failed to change routing mode: ", "路由模式切换失败：");
pub(crate) const FALLBACK: LocalizedText = LocalizedText::new("Fallback", "故障转移");
pub(crate) const FILTER_BY_TARGET_PROCESS_RULE_OR_ROUTE: LocalizedText = LocalizedText::new(
    "Filter by target, process, rule, or route",
    "筛选目标、进程、规则或路径",
);
pub(crate) const G: LocalizedText = LocalizedText::new("G", "组");
pub(crate) const GLOBAL: LocalizedText = LocalizedText::new("Global", "全局");
pub(crate) const GLOBAL_MODE_ENABLED_CHOOSE_THE_GLOBAL_EXIT_ON_THE_NODES: LocalizedText =
    LocalizedText::new(
        "Global mode enabled; choose the global exit on the Nodes page",
        "全局模式已生效；请在节点页选择全局出口",
    );
pub(crate) const GROUPS: LocalizedText = LocalizedText::new("Groups", "组");
pub(crate) const IMPORTED_SUBSCRIPTION_REMOVAL_FAILED: LocalizedText =
    LocalizedText::new("Imported subscription removal failed", "导入订阅移除失败");
pub(crate) const IMPORTED_SUBSCRIPTION_REMOVED: LocalizedText =
    LocalizedText::new("Imported subscription removed", "已移除导入订阅");
pub(crate) const KERNEL_HAS_NO_TUN: LocalizedText =
    LocalizedText::new("kernel has no TUN", "当前内核无 TUN");
pub(crate) const LATENCY_TEST_FAILED_THIS_POLICY_GROUP_RETURNED_NO_DELAY_DATA: LocalizedText =
    LocalizedText::new(
        "Latency test failed · this policy group returned no delay data",
        "测速失败 · 当前策略组未返回延迟，请检查 Mihomo 连接后重试",
    );
pub(crate) const LIGHT: LocalizedText = LocalizedText::new("Light", "浅色");
pub(crate) const LIGHT_THEME_ENABLED: LocalizedText =
    LocalizedText::new("Light theme enabled", "已切换到浅色主题");
pub(crate) const LOADING_POLICY_GROUPS: LocalizedText =
    LocalizedText::new("Loading policy groups…", "正在读取策略组…");
pub(crate) const LOAD_BALANCE: LocalizedText = LocalizedText::new("Load balance", "负载均衡");
pub(crate) const LOCAL_CONFIGURATION: LocalizedText =
    LocalizedText::new("Local configuration", "本地配置");
pub(crate) const LOCAL_CONTROLLER: LocalizedText =
    LocalizedText::new("Local controller", "本地控制器");
pub(crate) const LOGS_OPENED: LocalizedText = LocalizedText::new("Logs opened", "已打开日志");
pub(crate) const MANIS_IS_LOADING_YOUR_CURRENT_GROUPS_AND_SELECTED_NODES: LocalizedText =
    LocalizedText::new(
        "Manis is loading your current groups and selected nodes.",
        "正在载入当前策略组和已选节点。",
    );
pub(crate) const MANUAL: LocalizedText = LocalizedText::new("Manual", "手动选择");
pub(crate) const MANUAL_SELECTION: LocalizedText =
    LocalizedText::new("Manual selection", "手动选择");
pub(crate) const MIHOMO_COULD_NOT_BE_STARTED_CHECK_LOGS_FOR_DETAILS_THEN: LocalizedText =
    LocalizedText::new(
        "Mihomo could not be started. Check Logs for details, then try again.",
        "Mihomo 启动失败。请在“日志”中查看原因，然后重试。",
    );
pub(crate) const MIHOMO_UPDATE_FAILED_THE_PREVIOUS_CORE_WAS_RESTORED: LocalizedText =
    LocalizedText::new(
        "Mihomo update failed; the previous core was restored: ",
        "Mihomo 更新失败，已恢复原内核：",
    );
pub(crate) const N: LocalizedText = LocalizedText::new("N", "点");
pub(crate) const NETWORK_ACTIVITY_OPENED: LocalizedText =
    LocalizedText::new("Network activity opened", "已打开网络活动");
pub(crate) const NODES_OPENED: LocalizedText =
    LocalizedText::new("Nodes opened", "已打开节点工作区");
pub(crate) const NO_AVAILABLE_NODES: LocalizedText =
    LocalizedText::new("No available nodes", "暂无可用节点");
pub(crate) const NO_IMPORTED_NODES_CURRENTLY_MATCH_THIS_POLICY: LocalizedText = LocalizedText::new(
    "No imported nodes currently match this policy.",
    "当前没有已导入节点符合这个策略组。",
);
pub(crate) const NO_POLICY_GROUPS_YET: LocalizedText =
    LocalizedText::new("No policy groups yet", "还没有策略组");
pub(crate) const NO_RUNTIME_DATA: LocalizedText =
    LocalizedText::new("No runtime data", "无运行数据");
pub(crate) const OFF: LocalizedText = LocalizedText::new("Off", "关闭");
pub(crate) const ONLY_A_CANDIDATE_INSIDE_A_MANUAL_POLICY_CAN_BE_SELECTED: LocalizedText =
    LocalizedText::new(
        "Only a candidate inside a manual policy can be selected",
        "只能选择手动策略组中的候选节点",
    );
pub(crate) const POLICY_GROUPS_OPENED: LocalizedText =
    LocalizedText::new("Policy groups opened", "已打开策略组工作区");
pub(crate) const POLICY_GROUPS_UNAVAILABLE: LocalizedText =
    LocalizedText::new("Policy groups unavailable", "暂时无法读取策略组");
pub(crate) const POLICY_GROUP_BENCHMARK_FAILED: LocalizedText =
    LocalizedText::new("Policy group benchmark failed", "策略组测速失败");
pub(crate) const PREPARING_THE_MACOS_TUN_HELPER_AND_TRAFFIC_ROUTE: LocalizedText =
    LocalizedText::new(
        "Preparing the macOS TUN helper and traffic route…",
        "正在准备 macOS TUN 辅助服务与流量接管…",
    );
pub(crate) const PREPARING_TUN: LocalizedText = LocalizedText::new("Preparing TUN…", "准备 TUN…");
pub(crate) const PROXY: LocalizedText = LocalizedText::new("Proxy", "代理");
pub(crate) const READ_ONLY: LocalizedText = LocalizedText::new("Read-only", "只读");
pub(crate) const RECONNECT_TO_RESTART_THE_KERNEL: LocalizedText = LocalizedText::new(
    " · reconnect to restart the kernel",
    " · 重新连接即可重启内核",
);
pub(crate) const REMOVING_IMPORTED_SUBSCRIPTION: LocalizedText =
    LocalizedText::new("Removing imported subscription", "正在移除已导入订阅");
pub(crate) const RESTART_PREFERENCE_COULD_NOT_BE_SAVED: LocalizedText = LocalizedText::new(
    " · restart preference could not be saved",
    " · 但未能保存重启偏好",
);
pub(crate) const ROUTING: LocalizedText = LocalizedText::new("Routing", "路由");
pub(crate) const ROUTING_RULES_CHOOSE_POLICY_GROUPS_POLICIES_CHOOSE_EXITS: LocalizedText =
    LocalizedText::new(
        "Routing rules choose policy groups; policies choose exits.",
        "分流规则命中策略组，策略组再决定具体出口。",
    );
pub(crate) const ROUTING_RULES_OPENED: LocalizedText =
    LocalizedText::new("Routing rules opened", "已打开分流规则");
pub(crate) const RULES: LocalizedText = LocalizedText::new("Rules", "规则");
pub(crate) const SAVED_SOURCES_ARE_READY: LocalizedText =
    LocalizedText::new("Saved sources are ready", "已载入保存的来源");
pub(crate) const SAVED_SOURCES_ARE_READY_AND_MIHOMO_WAS_RESTARTED: LocalizedText =
    LocalizedText::new(
        "Saved sources are ready and Mihomo was restarted",
        "已载入保存的来源，Mihomo 已重新启动",
    );
pub(crate) const SAVED_SOURCES_WERE_LOADED_BUT_THE_CHANGES_COULD_NOT_BE: LocalizedText =
    LocalizedText::new(
        "Saved sources were loaded, but the changes could not be applied: ",
        "已载入保存的来源，但更改未能生效：",
    );
pub(crate) const SEARCH_OPERATIONS_ERRORS_OR_LOG_LEVELS: LocalizedText = LocalizedText::new(
    "Search operations, errors, or log levels",
    "搜索操作、错误或日志级别",
);
pub(crate) const SOURCE: LocalizedText = LocalizedText::new("Source", "来源");
pub(crate) const START_MIHOMO_BEFORE_SELECTING_A_NODE_FOR_THIS_POLICY_GROUP: LocalizedText =
    LocalizedText::new(
        "Start Mihomo before selecting a node for this policy group.",
        "请先启动 Mihomo，再为此策略组选择节点。",
    );
pub(crate) const START_MIHOMO_BEFORE_TESTING_THIS_POLICY_GROUP: LocalizedText = LocalizedText::new(
    "Start Mihomo before testing this policy group",
    "请先启动 Mihomo，再测试此策略组",
);
pub(crate) const START_MIHOMO_TO_LOAD_YOUR_POLICY_GROUPS_AND_SELECTED_NODES: LocalizedText =
    LocalizedText::new(
        "Start Mihomo to load your policy groups and selected nodes.",
        "启动 Mihomo 后即可查看策略组和已选节点。",
    );
pub(crate) const SUBSCRIPTION_IMPORT_FAILED: LocalizedText =
    LocalizedText::new("Subscription import failed: ", "订阅导入失败：");
pub(crate) const SUBSCRIPTION_LOADED_BUT_ITS_UPDATE_TIME_COULD_NOT_BE_SAVED: LocalizedText =
    LocalizedText::new(
        "Subscription loaded, but its update time could not be saved: ",
        "订阅已读取，但更新时间保存失败：",
    );
pub(crate) const SUBSCRIPTION_UPDATE_FAILED: LocalizedText =
    LocalizedText::new("Subscription update failed", "订阅更新失败");
pub(crate) const SUBSCRIPTION_UPDATE_FAILED_2: LocalizedText =
    LocalizedText::new("Subscription update failed: ", "订阅更新失败：");
pub(crate) const SWITCHING: LocalizedText = LocalizedText::new("Switching…", "切换中…");
pub(crate) const SWITCHING_2: LocalizedText = LocalizedText::new("switching", "切换中");
pub(crate) const SWITCHING_TO: LocalizedText = LocalizedText::new("Switching to ", "正在切换到");
pub(crate) const SYSTEM: LocalizedText = LocalizedText::new("System", "系统代理");
#[cfg(not(test))]
pub(crate) const SYSTEM_PROXY_RECOVERY_NEEDS_ATTENTION: LocalizedText = LocalizedText::new(
    "System proxy recovery needs attention: ",
    "系统代理恢复需要处理：",
);
#[cfg(not(test))]
pub(crate) const SYSTEM_PROXY_RECOVERY_STATE_IS_UNAVAILABLE: LocalizedText = LocalizedText::new(
    "System proxy recovery state is unavailable",
    "系统代理恢复状态不可用",
);
pub(crate) const SYSTEM_PROXY_STATE_LOCK_WAS_DAMAGED: LocalizedText = LocalizedText::new(
    "system proxy state lock was damaged",
    "系统代理状态锁已损坏",
);
pub(crate) const SYSTEM_PROXY_WAS_RESTORED_RECONNECT_TO_RESTART_THE_KERNEL: LocalizedText =
    LocalizedText::new(
        " · system proxy was restored; reconnect to restart the kernel",
        " · 系统代理已恢复；重新连接即可重启内核",
    );
pub(crate) const TEST_FIXTURES_CANNOT_CHANGE_ROUTING_MODE: LocalizedText = LocalizedText::new(
    "Test fixtures cannot change routing mode",
    "测试快照不能切换路由模式",
);
pub(crate) const TEST_FIXTURES_CANNOT_ENABLE_TUN: LocalizedText = LocalizedText::new(
    "Test fixtures cannot enable TUN",
    "测试快照不能启用 TUN 模式",
);
pub(crate) const TEST_FIXTURE_IS_READ_ONLY: LocalizedText =
    LocalizedText::new("test fixture is read-only", "测试快照只读");
pub(crate) const THE_KERNEL_RETURNED_NO_GROUP_MEMBERS: LocalizedText =
    LocalizedText::new("The kernel returned no group members", "内核未返回组内节点");
pub(crate) const THE_LOCAL_CONFIGURATION_DIRECTORY_IS_UNAVAILABLE_THE_KERNEL_CANNOT_BE:
    LocalizedText = LocalizedText::new(
    "The local configuration directory is unavailable; the kernel cannot be changed",
    "无法确定本机配置目录，不能切换内核",
);
pub(crate) const THE_MANIS_DATA_DIRECTORY_IS_UNAVAILABLE_MIHOMO_CANNOT_BE_UPDATED: LocalizedText =
    LocalizedText::new(
        "The Manis data directory is unavailable; Mihomo cannot be updated",
        "无法确定 Manis 数据目录，不能更新 Mihomo",
    );
pub(crate) const THE_MANIS_MANAGED_KERNEL_STOPPED_UNEXPECTEDLY: LocalizedText = LocalizedText::new(
    "The Manis-managed kernel stopped unexpectedly",
    "Manis 托管内核已意外停止",
);
pub(crate) const THE_SUBSCRIPTION_STORAGE_LOCATION_IS_UNAVAILABLE: LocalizedText =
    LocalizedText::new(
        "The subscription storage location is unavailable",
        "无法确定订阅保存位置",
    );
pub(crate) const THIS_POLICY_GROUP_HAS_NO_TESTABLE_CANDIDATES: LocalizedText = LocalizedText::new(
    "This policy group has no testable candidates",
    "当前策略组没有可测速候选项",
);
pub(crate) const THIS_POLICY_HAS_NO_CANDIDATE_NODES: LocalizedText = LocalizedText::new(
    "This policy has no candidate nodes.",
    "这个策略组没有候选节点。",
);
pub(crate) const THIS_POLICY_SELECTION_CANNOT_BE_SAVED: LocalizedText = LocalizedText::new(
    "This policy selection cannot be saved",
    "无法保存这个策略组选择",
);
pub(crate) const THIS_RUNTIME_POLICY_IS_READ_ONLY_IN_MANIS: LocalizedText = LocalizedText::new(
    "This runtime policy is read-only in Manis",
    "这个运行时策略组在 Manis 中为只读",
);
pub(crate) const TOTAL: LocalizedText = LocalizedText::new("Total ", "累计");
#[cfg(not(test))]
pub(crate) const TUN_DNS_RECOVERY_NEEDS_ATTENTION: LocalizedText = LocalizedText::new(
    "TUN DNS recovery needs attention: ",
    "TUN DNS 恢复需要处理：",
);
#[cfg(not(test))]
pub(crate) const TUN_DNS_RECOVERY_STATE_IS_UNAVAILABLE: LocalizedText = LocalizedText::new(
    "TUN DNS recovery state is unavailable",
    "TUN DNS 恢复状态不可用",
);
pub(crate) const TUN_DNS_STATE_LOCK_WAS_DAMAGED: LocalizedText =
    LocalizedText::new("TUN DNS state lock was damaged", "TUN DNS 状态锁已损坏");
pub(crate) const TUN_IS_DISABLED_BUT_RESTORING_THE_ORIGINAL_DNS_FAILED_RECOVERY: LocalizedText =
    LocalizedText::new(
        "TUN is disabled, but restoring the original DNS failed; recovery will be retried",
        "TUN 已关闭，但恢复原 DNS 失败；Manis 将继续重试恢复",
    );
pub(crate) const TUN_IS_NOT_YET_AVAILABLE_FOR_THE_SING_BOX_ADAPTER: LocalizedText =
    LocalizedText::new(
        "TUN is not yet available for the sing-box adapter; use the system HTTP/SOCKS proxy",
        "当前 sing-box 适配器尚未开放 TUN；可使用系统 HTTP/SOCKS 代理",
    );
pub(crate) const TURNING_OFF: LocalizedText = LocalizedText::new("Turning off…", "关闭中…");
pub(crate) const TURN_OFF_THE_ACTIVE_PROXY_MODE_BEFORE_UPDATING_MIHOMO: LocalizedText =
    LocalizedText::new(
        "Turn off the active proxy mode before updating Mihomo",
        "请先关闭当前代理模式，再更新 Mihomo",
    );
pub(crate) const UNKNOWN_TYPE: LocalizedText = LocalizedText::new("Unknown type", "类型未知");
pub(crate) const UPDATING_SUBSCRIPTION_NODES: LocalizedText =
    LocalizedText::new("Updating subscription nodes", "正在更新订阅节点");
pub(crate) const VALIDATING: LocalizedText = LocalizedText::new("Validating", "正在校验并准备");
pub(crate) const VALIDATING_NODES_AND_IMPORTING_SUBSCRIPTION: LocalizedText = LocalizedText::new(
    "Validating nodes and importing subscription",
    "正在验证节点并导入订阅",
);
