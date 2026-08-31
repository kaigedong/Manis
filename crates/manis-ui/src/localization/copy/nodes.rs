use crate::localization::LocalizedText;

mod format;
pub(crate) use format::*;

pub(crate) const INTERVAL_1_MINUTE: LocalizedText = LocalizedText::new("1 min", "1 分钟");
pub(crate) const INTERVAL_5_MINUTES: LocalizedText = LocalizedText::new("5 min", "5 分钟");
pub(crate) const INTERVAL_10_MINUTES: LocalizedText = LocalizedText::new("10 min", "10 分钟");
pub(crate) const INTERVAL_30_MINUTES: LocalizedText = LocalizedText::new("30 min", "30 分钟");

pub(crate) const ALL: LocalizedText = LocalizedText::new("All", "全部");
pub(crate) const ALL_NODES: LocalizedText = LocalizedText::new("All nodes", "全部节点");
pub(crate) const APPLYING_CHANGES: LocalizedText =
    LocalizedText::new("applying changes", "正在应用更改");
pub(crate) const AS_GLOBAL_EXIT: LocalizedText =
    LocalizedText::new("as global exit", "作为全局出口");
pub(crate) const AUTOMATIC_CHECK_INTERVAL_IS_INVALID: LocalizedText =
    LocalizedText::new("Automatic check interval is invalid", "自动检查间隔无效");
pub(crate) const AVAILABLE: LocalizedText = LocalizedText::new("Available", "可用");
pub(crate) const A_GROUP_TEST_IS_ALREADY_RUNNING_WAIT_FOR_IT_TO: LocalizedText = LocalizedText::new(
    "A group test is already running. Wait for it to finish.",
    "已有分组正在测速，请等待完成后再试",
);
pub(crate) const A_POLICY_GROUP_WITH_THIS_NAME_ALREADY_EXISTS_CHOOSE_ANOTHER: LocalizedText =
    LocalizedText::new(
        "A policy group with this name already exists. Choose another name.",
        "已有同名策略组，请换一个名称",
    );
pub(crate) const BASIC_INFORMATION: LocalizedText =
    LocalizedText::new("Basic information", "基本信息");
pub(crate) const BOLT: LocalizedText = LocalizedText::new("Bolt", "闪电");
pub(crate) const BUILT_IN: LocalizedText = LocalizedText::new("Built-in", "内置");
pub(crate) const CANDIDATES: LocalizedText = LocalizedText::new("Candidates", "候选节点");
pub(crate) const COMPASS: LocalizedText = LocalizedText::new("Compass", "罗盘");
pub(crate) const COULD_NOT_DETERMINE_WHERE_TO_SAVE_POLICY_GROUPS: LocalizedText =
    LocalizedText::new(
        "Could not determine where to save policy groups",
        "无法确定策略组保存位置",
    );
pub(crate) const CREATING_POLICY_GROUP: LocalizedText =
    LocalizedText::new("Creating policy group", "正在创建策略组");
pub(crate) const DONE: LocalizedText = LocalizedText::new("Done", "完成");
pub(crate) const EDITING_GROUP: LocalizedText = LocalizedText::new("Editing group", "正在编辑分组");
pub(crate) const ENTER_THE_NODE_NAME_TO_MATCH: LocalizedText =
    LocalizedText::new("Enter the node name to match", "请填写要匹配的节点名称");
pub(crate) const FAILED_TO_DELETE_POLICY_GROUP: LocalizedText =
    LocalizedText::new("Failed to delete policy group", "策略组删除失败");
pub(crate) const FAILED_TO_SAVE_POLICY_GROUP: LocalizedText =
    LocalizedText::new("Failed to save policy group", "策略组保存失败");
pub(crate) const FILTER_NODES_BY: LocalizedText = LocalizedText::new("Filter nodes by", "筛选节点");
pub(crate) const FINISH_SELECTING_CANDIDATES: LocalizedText =
    LocalizedText::new("Finish selecting candidates", "完成选择候选项");
pub(crate) const FIRST_LETTER: LocalizedText = LocalizedText::new("First letter", "首字圆标");
pub(crate) const GLOBE: LocalizedText = LocalizedText::new("Globe", "地球");
pub(crate) const GO_TO_CONFIGURATION_TO_IMPORT_A_SUBSCRIPTION: LocalizedText = LocalizedText::new(
    "Go to Configuration to import a subscription",
    "前往配置导入订阅",
);
pub(crate) const GROUP_DELETED: LocalizedText = LocalizedText::new("Group deleted", "分组已删除");
pub(crate) const GROUP_NAME_CANNOT_BE_EMPTY_OR_CONTAIN_NEWLINES_CONTROL_CHARACTERS: LocalizedText =
    LocalizedText::new(
        "Group name cannot be empty or contain newlines/control characters",
        "策略组名称不能为空，也不能包含换行或控制字符",
    );
pub(crate) const GROUP_SAVED: LocalizedText = LocalizedText::new("Group saved", "分组已保存");
pub(crate) const ICON: LocalizedText = LocalizedText::new("Icon", "图标");
pub(crate) const IMPORT_A_SUBSCRIPTION_OR_ADD_A_VLESS_NODE_NODES_WILL: LocalizedText =
    LocalizedText::new(
        "Import a subscription or add a VLESS node; nodes will then appear here automatically.",
        "导入订阅或添加 VLESS 节点后，节点会自动出现在这里。",
    );
pub(crate) const IMPORT_NODES_OR_CREATE_ANOTHER_POLICY_GROUP_BEFORE_MAKING_A: LocalizedText =
    LocalizedText::new(
        "Import nodes or create another policy group before making a selection.",
        "请先导入节点或创建其他策略组，再进行选择。",
    );
pub(crate) const INDIVIDUALLY_ADDED_VLESS_NODES_PRIVATE_LOCAL_STORAGE: LocalizedText =
    LocalizedText::new(
        "Individually added VLESS nodes · private local storage",
        "单独添加的 VLESS 节点 · 私有本机存储",
    );
pub(crate) const LOADING: LocalizedText = LocalizedText::new("Loading…", "读取中…");
pub(crate) const MANAGE_SUBSCRIPTION_SOURCES: LocalizedText =
    LocalizedText::new("Manage subscription sources", "管理订阅来源");
pub(crate) const MANIS_IS_LOADING_NODES_FROM_YOUR_SAVED_SUBSCRIPTIONS: LocalizedText =
    LocalizedText::new(
        "Manis is loading nodes from your saved subscriptions.",
        "正在从已保存的订阅中载入节点。",
    );
pub(crate) const MIHOMO_SOURCE: LocalizedText = LocalizedText::new("Mihomo source", "Mihomo 来源");
pub(crate) const NAME_CONTAINS: LocalizedText = LocalizedText::new("Name contains", "名称包含");
pub(crate) const NEW_POLICY_GROUP: LocalizedText =
    LocalizedText::new("New policy group", "新建策略组");
pub(crate) const NODE: LocalizedText = LocalizedText::new("Node", "节点");
pub(crate) const NODES_ARE_TEMPORARILY_UNAVAILABLE: LocalizedText =
    LocalizedText::new("Nodes are temporarily unavailable", "暂时无法读取节点");
pub(crate) const NODE_FILTER: LocalizedText = LocalizedText::new("Node filter", "节点筛选");
pub(crate) const NODE_NAME_CONTAINS: LocalizedText =
    LocalizedText::new("Node name contains", "节点名称包含");
pub(crate) const NODE_SCOPE: LocalizedText = LocalizedText::new("Node scope", "节点范围");
pub(crate) const NODE_SOURCE: LocalizedText = LocalizedText::new("node source", "节点来源");
pub(crate) const NODE_SOURCE_EXPANDED_STATE_UPDATED: LocalizedText = LocalizedText::new(
    "Node source expanded state updated",
    "已更新节点来源展开状态",
);
pub(crate) const NOT_LOADED: LocalizedText = LocalizedText::new("Not loaded", "尚未读取");
pub(crate) const NO_NODES_FROM_THIS_SOURCE_MATCH_THE_CURRENT_FILTER: LocalizedText =
    LocalizedText::new(
        "No nodes from this source match the current filter.",
        "这个来源中没有符合当前筛选的节点。",
    );
pub(crate) const POLICY_EDITING_CANCELLED: LocalizedText =
    LocalizedText::new("Policy editing cancelled", "已取消编辑策略");
pub(crate) const POLICY_GROUP_NAME: LocalizedText =
    LocalizedText::new("Policy group name", "策略组名称");
pub(crate) const PROTOCOL: LocalizedText = LocalizedText::new("Protocol", "协议");
pub(crate) const REFRESH_NODE_HEALTH: LocalizedText =
    LocalizedText::new("Refresh node health", "刷新节点健康状态");
pub(crate) const REMOVING: LocalizedText = LocalizedText::new("Removing", "正在移除");
pub(crate) const RESTORES_AFTER_RESTART: LocalizedText =
    LocalizedText::new("Restores after restart", "重启后自动恢复");
pub(crate) const RESTORING: LocalizedText = LocalizedText::new("Restoring", "正在恢复");
pub(crate) const RESTORING_NODES: LocalizedText =
    LocalizedText::new("Restoring nodes", "正在恢复节点");
pub(crate) const RETEST_INTERVAL: LocalizedText = LocalizedText::new("Retest interval", "重测间隔");
pub(crate) const ROUTING_RULES_POINT_TO_THIS_POLICY_THE_POLICY_CHOOSES_ONE: LocalizedText =
    LocalizedText::new(
        "Routing rules point to this policy; the policy chooses one exit from this node scope.",
        "分流规则会指向这个策略组；策略组再从这里配置的节点范围中选择出口。",
    );
pub(crate) const SAVED_NODES_DO_NOT_NEED_TO_BE_DOWNLOADED_AGAIN: LocalizedText = LocalizedText::new(
    "Saved nodes do not need to be downloaded again",
    "已保存节点不需要重新下载",
);
pub(crate) const SELECT: LocalizedText = LocalizedText::new("Select", "选择");
pub(crate) const SELECTED_CANDIDATES: LocalizedText =
    LocalizedText::new("Selected candidates", "已选候选项");
pub(crate) const SELECT_AT_LEAST_ONE_NODE_OR_POLICY_GROUP: LocalizedText = LocalizedText::new(
    "Select at least one node or policy group",
    "请至少选择一个节点或策略组",
);
pub(crate) const SELECT_NODES_OR_GROUPS: LocalizedText =
    LocalizedText::new("Select nodes or groups", "选择节点或策略组");
pub(crate) const SHIELD: LocalizedText = LocalizedText::new("Shield", "盾牌");
pub(crate) const SOURCE_TEST_COMPLETED: LocalizedText =
    LocalizedText::new("Source test completed", "来源测速完成");
pub(crate) const SOURCE_TEST_FAILED: LocalizedText =
    LocalizedText::new("Source test failed", "来源测速失败");
pub(crate) const SUBSCRIPTIONS_REMAIN_STORED_LOCALLY_CHECK_SOURCE_DETAILS_IN_CONFIGURATION:
    LocalizedText = LocalizedText::new(
    "Subscriptions remain stored locally. Check source details in Configuration.",
    "订阅仍保存在本机。请前往配置页检查来源详情。",
);
pub(crate) const SUBSCRIPTION_SOURCE_CONFIGURATION_OPENED: LocalizedText = LocalizedText::new(
    "Subscription source configuration opened",
    "已打开订阅来源配置",
);
pub(crate) const TEST: LocalizedText = LocalizedText::new("test", "测速");
pub(crate) const TESTING: LocalizedText = LocalizedText::new("testing...", "正在测速…");
pub(crate) const TESTING_SOURCE: LocalizedText =
    LocalizedText::new("Testing source", "正在测试来源");
pub(crate) const TEST_FAILED: LocalizedText = LocalizedText::new("test failed", "测速失败");
pub(crate) const BENCHMARK_FAILED: LocalizedText = LocalizedText::new("Failed", "失败");
pub(crate) const GROUP_BENCHMARK_IN_PROGRESS: LocalizedText =
    LocalizedText::new("Group benchmark in progress", "分组测速中");
pub(crate) const POLICY_BENCHMARK_IN_PROGRESS: LocalizedText =
    LocalizedText::new("Policy benchmark in progress", "策略组测速中");
pub(crate) const TEST_GROUP_LATENCY: LocalizedText =
    LocalizedText::new("Test this group's latency", "测试该分组延迟");
pub(crate) const TEST_POLICY_CANDIDATE_LATENCY: LocalizedText =
    LocalizedText::new("Test policy candidate latency", "测试策略组候选项延迟");
pub(crate) const THE_CURRENT_RULE_DOES_NOT_MATCH_ANY_IMPORTED_NODES: LocalizedText =
    LocalizedText::new(
        "The current rule does not match any imported nodes",
        "当前规则没有匹配到任何已导入节点",
    );
pub(crate) const THIS_NAME_IS_RESERVED_BY_THE_PROXY_KERNEL: LocalizedText = LocalizedText::new(
    "This name is reserved by the proxy kernel",
    "该名称由代理内核保留",
);
pub(crate) const THIS_POLICY_GROUP_IS_USED_BY_ANOTHER_POLICY_GROUP_AND: LocalizedText =
    LocalizedText::new(
        "This policy group is used by another policy group and cannot be deleted",
        "该策略组正被其他策略组使用，无法删除",
    );
pub(crate) const THIS_SOURCE_HAS_NO_NODES_TO_TEST: LocalizedText =
    LocalizedText::new("This source has no nodes to test", "当前来源没有可测速节点");
pub(crate) const TYPE: LocalizedText = LocalizedText::new("Type", "类型");
pub(crate) const UNAVAILABLE: LocalizedText = LocalizedText::new("Unavailable", "不可用");
pub(crate) const UNAVAILABLE_2: LocalizedText = LocalizedText::new("Unavailable", "当前不可用");
pub(crate) const UNTESTED: LocalizedText = LocalizedText::new("Untested", "未测速");
pub(crate) const USING_MIHOMO_CACHE: LocalizedText =
    LocalizedText::new("Using Mihomo cache", "使用 Mihomo 缓存");
