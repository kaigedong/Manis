use crate::localization::LocalizedText;

mod format;
pub(crate) use format::*;

pub(crate) const MANUAL_RULE_ENABLED: LocalizedText =
    LocalizedText::new("Manual rule enabled", "手动规则已启用");
pub(crate) const MANUAL_RULE_DISABLED: LocalizedText =
    LocalizedText::new("Manual rule disabled", "手动规则已禁用");

pub(crate) const ADD_AND_CONDITION: LocalizedText =
    LocalizedText::new("+ Add AND condition", "+ 添加“并且”条件");
pub(crate) const ADD_AN_AND_CONDITION: LocalizedText =
    LocalizedText::new("Add an AND condition", "添加并且条件");
pub(crate) const ADD_A_REMOTE_QX_RULE_SET: LocalizedText =
    LocalizedText::new("Add a remote QX rule set.", "添加一个远程 QX 规则集。");
pub(crate) const ADD_A_SUBSCRIPTION_OR_A_SINGLE_NODE_SOURCE: LocalizedText = LocalizedText::new(
    "Add a subscription or a single-node source.",
    "添加订阅或单节点来源。",
);
pub(crate) const ADD_PROXY_SOURCE: LocalizedText =
    LocalizedText::new("Add proxy source", "添加代理来源");
pub(crate) const ADD_ROUTING_RULE: LocalizedText =
    LocalizedText::new("Add routing rule", "添加分流规则");
pub(crate) const ADD_RULES_TO_SEND_MATCHING_CONNECTIONS_THROUGH_A_POLICY_GROUP: LocalizedText =
    LocalizedText::new(
        "Add rules to send matching connections through a policy group. Rules are evaluated from top to bottom.",
        "添加规则，将匹配的连接交给指定策略组。规则会按从上到下的顺序生效。",
    );
pub(crate) const ADD_RULE_SOURCE: LocalizedText =
    LocalizedText::new("Add rule source", "添加规则来源");
pub(crate) const ADD_SOURCE: LocalizedText = LocalizedText::new("Add source", "添加来源");
pub(crate) const ADVANCED: LocalizedText = LocalizedText::new("Advanced", "高级设置");
pub(crate) const ADVANCED_SETTINGS: LocalizedText =
    LocalizedText::new("Advanced settings", "高级设置");
pub(crate) const ALL_CONDITIONS_MUST_MATCH_GROUP_ORDER_DETERMINES_RULE_PRIORITY: LocalizedText =
    LocalizedText::new(
        "All conditions must match. Group order determines rule priority.",
        "同一条规则中的条件必须全部命中；分组顺序决定规则优先级。",
    );
pub(crate) const ALREADY_ADDED: LocalizedText = LocalizedText::new("Already added", "已添加");
pub(crate) const ALREADY_CONFIGURED: LocalizedText =
    LocalizedText::new("Already configured", "已经配置");
pub(crate) const ALWAYS: LocalizedText = LocalizedText::new("Always", "始终识别");
pub(crate) const AND: LocalizedText = LocalizedText::new("AND", "并且");
pub(crate) const AN_EXISTING_SUBSCRIPTION_MUST_KEEP_AN_HTTP_HTTPS_URL: LocalizedText =
    LocalizedText::new(
        "An existing subscription must keep an HTTP/HTTPS URL",
        "现有订阅必须使用 HTTP/HTTPS URL",
    );
pub(crate) const APPLYING_RULE_SOURCE_STATE: LocalizedText =
    LocalizedText::new("Applying rule source state", "正在应用规则来源状态");
pub(crate) const APPLYING_SUBSCRIPTION_STATE: LocalizedText =
    LocalizedText::new("Applying subscription state", "正在应用订阅状态");
pub(crate) const AT_LEAST_ONE_ENABLED_SAVED_VLESS_NODE_IS_REQUIRED: LocalizedText =
    LocalizedText::new(
        "At least one enabled saved VLESS node is required",
        "至少需要启用一个已保存的 VLESS 节点",
    );
pub(crate) const DISABLE_NON_VLESS_SAVED_NODES_BEFORE_SWITCHING_TO_SING_BOX: LocalizedText =
    LocalizedText::new(
        "Disable non-VLESS saved nodes before switching to sing-box",
        "切换到 sing-box 前，请停用已保存的非 VLESS 节点",
    );
pub(crate) const AUTOMATIC: LocalizedText = LocalizedText::new("Automatic", "自动管理");
pub(crate) const AUTONOMOUS_SYSTEM: LocalizedText =
    LocalizedText::new("Autonomous system", "自治系统");
pub(crate) const AVAILABLE_WITH_MIHOMO: LocalizedText =
    LocalizedText::new("Available with Mihomo", "仅 Mihomo 可用");
pub(crate) const A_RULE_CAN_CONTAIN_AT_MOST_FOUR_CONDITIONS: LocalizedText = LocalizedText::new(
    "A rule can contain at most four conditions",
    "一条规则最多包含四个条件",
);
pub(crate) const A_SINGLE_NODE_SOURCE_DOES_NOT_NEED_AN_UPDATE_INTERVAL: LocalizedText =
    LocalizedText::new(
        "A single-node source does not need an update interval.",
        "单节点来源不需要更新间隔。",
    );
pub(crate) const BROWSER_USER_AGENT: LocalizedText =
    LocalizedText::new("Browser user agent", "浏览器标识");
pub(crate) const CHANGED_FROM_THE_MAIN_TOOLBAR: LocalizedText =
    LocalizedText::new("Changed from the main toolbar", "可在主工具栏中切换");
pub(crate) const CHANGES_ARE_STORED_LOCALLY: LocalizedText =
    LocalizedText::new("Changes are stored locally", "更改仅保存在本机");
pub(crate) const CHANGE_TARGET_POLICY_FOR_THIS_RULE_SOURCE: LocalizedText = LocalizedText::new(
    "Change target policy for this rule source",
    "修改这个规则源的目标策略",
);
pub(crate) const CHECK_FOR_UPDATE: LocalizedText =
    LocalizedText::new("Check for update", "检查更新");
pub(crate) const CHOOSE_AN_EXISTING_POLICY_GROUP: LocalizedText =
    LocalizedText::new("Choose an existing policy group", "请选择已有策略组");
pub(crate) const CHOOSE_A_SUBSCRIPTION_OR_A_SINGLE_NODE_SHARE_LINK: LocalizedText =
    LocalizedText::new(
        "Choose a subscription or a single-node share link.",
        "请选择订阅来源或单节点分享链接。",
    );
pub(crate) const CHOOSE_CONDITION_TYPE: LocalizedText =
    LocalizedText::new("Choose condition type", "选择条件类型");
pub(crate) const CHOOSE_RULE_UPDATE_INTERVAL: LocalizedText =
    LocalizedText::new("Choose rule update interval", "选择规则更新间隔");
pub(crate) const CHOOSE_SUBSCRIPTION_UPDATE_INTERVAL: LocalizedText =
    LocalizedText::new("Choose subscription update interval", "选择订阅更新间隔");
pub(crate) const CHOOSE_TARGET_POLICY: LocalizedText =
    LocalizedText::new("Choose target policy", "选择目标策略");
pub(crate) const CLASH_SUBSCRIPTIONS_ARE_PRESENT_MANIS_NEEDS_ITS_NATIVE_PARSER_FIRST:
    LocalizedText = LocalizedText::new(
    "Clash subscriptions are present; Manis needs its native parser first",
    "当前包含 Clash 订阅，需等待 Manis 原生订阅解析器",
);
pub(crate) const CONDITION_1: LocalizedText = LocalizedText::new("Condition 1", "条件 1");
pub(crate) const CORE_AND_UPDATES: LocalizedText =
    LocalizedText::new("Core and updates", "内核与更新");
pub(crate) const COULD_NOT_DETERMINE_WHERE_TO_SAVE_RULES: LocalizedText = LocalizedText::new(
    "Could not determine where to save rules",
    "无法确定规则保存位置",
);
pub(crate) const COULD_NOT_DETERMINE_WHERE_TO_SAVE_THE_NODE: LocalizedText = LocalizedText::new(
    "Could not determine where to save the node",
    "无法确定节点保存位置",
);
pub(crate) const COULD_NOT_DETERMINE_WHERE_TO_SAVE_THE_RULE_SOURCE: LocalizedText =
    LocalizedText::new(
        "Could not determine where to save the rule source",
        "无法确定规则源的保存位置",
    );
pub(crate) const COULD_NOT_READ_MANUAL_RULES: LocalizedText =
    LocalizedText::new("Could not read manual rules: ", "无法读取手动分流规则：");
pub(crate) const COULD_NOT_UPDATE_SOURCE: LocalizedText =
    LocalizedText::new("Could not update source", "无法更新来源");
pub(crate) const COUNTRY_OR_REGION: LocalizedText =
    LocalizedText::new("Country or region", "国家或地区");
pub(crate) const CURRENT: LocalizedText = LocalizedText::new("Current", "当前");
pub(crate) const CURRENT_KERNEL: LocalizedText = LocalizedText::new("Current", "当前使用");
pub(crate) const CURRENT_MANAGED_NETWORK_BEHAVIOR: LocalizedText =
    LocalizedText::new("Current managed network behavior", "当前托管网络行为");
pub(crate) const DAILY: LocalizedText = LocalizedText::new("Daily", "每天");
pub(crate) const DESTINATION_PORT: LocalizedText =
    LocalizedText::new("Destination port", "目标端口");
pub(crate) const DIRECT_GLOBAL_OR_ORDERED_RULES: LocalizedText =
    LocalizedText::new("Direct, global, or ordered rules", "直连、全局或有序规则");
pub(crate) const DISABLED: LocalizedText = LocalizedText::new("Disabled", "已禁用");
pub(crate) const SOURCE_DISABLED_LABEL: LocalizedText = LocalizedText::new("Disabled", "未启用");
pub(crate) const DNS_AND_TUN: LocalizedText = LocalizedText::new("DNS and TUN", "DNS 与 TUN");
pub(crate) const DOMAIN_CONTAINS_KEYWORD: LocalizedText =
    LocalizedText::new("Domain contains keyword", "域名中包含关键词");
pub(crate) const DOMAIN_SUFFIX: LocalizedText = LocalizedText::new("Domain suffix", "域名后缀");
pub(crate) const DOWNLOADING_AND_PARSING_QX_RULES: LocalizedText =
    LocalizedText::new("Downloading and parsing QX rules", "正在下载并解析 QX 规则");
pub(crate) const DOWNLOAD_OR_UPDATE_THE_MANIS_MANAGED_MIHOMO_CORE: LocalizedText =
    LocalizedText::new(
        "Download or update the Manis-managed Mihomo core",
        "下载或更新 Manis 托管的 Mihomo 内核",
    );
pub(crate) const DOWNLOAD_STABLE: LocalizedText =
    LocalizedText::new("Download stable", "下载稳定版");
pub(crate) const EDIT_PROXY_SOURCE: LocalizedText =
    LocalizedText::new("Edit proxy source", "编辑代理来源");
pub(crate) const EDIT_ROUTING_RULE: LocalizedText =
    LocalizedText::new("Edit routing rule", "编辑分流规则");
pub(crate) const EDIT_RULE_SOURCE: LocalizedText =
    LocalizedText::new("Edit rule source", "编辑规则来源");
pub(crate) const EDIT_THIS_RULE_SOURCE: LocalizedText =
    LocalizedText::new("Edit this rule source", "编辑这个规则来源");
pub(crate) const EDIT_THIS_SINGLE_NODE_SOURCE: LocalizedText =
    LocalizedText::new("Edit this single-node source", "编辑这个单节点来源");
pub(crate) const EDIT_THIS_SUBSCRIPTION: LocalizedText =
    LocalizedText::new("Edit this subscription", "编辑这个订阅");
pub(crate) const ENTER_AN_ASN_NUMBER_SUCH_AS_6185: LocalizedText = LocalizedText::new(
    "Enter an ASN number such as 6185",
    "请输入 ASN 数字，例如 6185",
);
pub(crate) const ENTER_AN_IPV4_CIDR_SUCH_AS_192_168_0_1: LocalizedText = LocalizedText::new(
    "Enter an IPv4 CIDR such as 192.168.0.1/24",
    "请输入 IPv4 CIDR，例如 192.168.0.1/24",
);
pub(crate) const ENTER_AN_IPV6_CIDR_SUCH_AS_2001_4860_4860_8888: LocalizedText = LocalizedText::new(
    "Enter an IPv6 CIDR such as 2001:4860:4860::8888/32",
    "请输入 IPv6 CIDR，例如 2001:4860:4860::8888/32",
);
pub(crate) const ENTER_A_DESTINATION_PORT_BETWEEN_1_AND_65535: LocalizedText = LocalizedText::new(
    "Enter a destination port between 1 and 65535",
    "请输入 1 到 65535 之间的目标端口",
);
pub(crate) const ENTER_A_DOMAIN_PATTERN_SUCH_AS_EXAMPLE_COM: LocalizedText = LocalizedText::new(
    "Enter a domain pattern such as *.example.com",
    "请输入域名模式，例如 *.example.com",
);
pub(crate) const ENTER_A_MATCH_PARAMETER: LocalizedText =
    LocalizedText::new("Enter a match parameter", "请输入匹配参数");
pub(crate) const ENTER_A_PLAIN_DOMAIN_SUCH_AS_EXAMPLE_COM: LocalizedText = LocalizedText::new(
    "Enter a plain domain such as example.com",
    "请输入纯域名，例如 example.com",
);
pub(crate) const ENTER_A_SOURCE_NAME: LocalizedText =
    LocalizedText::new("Enter a source name", "请输入来源名称");
pub(crate) const ENTER_A_TWO_LETTER_COUNTRY_CODE_SUCH_AS_US: LocalizedText = LocalizedText::new(
    "Enter a two-letter country code such as US",
    "请输入两位国家代码，例如 US",
);
pub(crate) const ENTER_A_VALID_HTTPS_RULE_URL: LocalizedText = LocalizedText::new(
    "Enter a valid HTTPS rule URL",
    "请输入有效的 HTTPS 规则地址",
);
pub(crate) const EVERY_12_HOURS: LocalizedText = LocalizedText::new("Every 12 hours", "每 12 小时");
pub(crate) const EVERY_1_HOUR: LocalizedText = LocalizedText::new("Every 1 hour", "每 1 小时");
pub(crate) const EVERY_6_HOURS: LocalizedText = LocalizedText::new("Every 6 hours", "每 6 小时");
pub(crate) const EXACT_DOMAIN: LocalizedText = LocalizedText::new("Exact domain", "完整域名");
pub(crate) const FAILED_TO_CHANGE_RULE_SOURCE_STATE: LocalizedText =
    LocalizedText::new("Failed to change rule source state", "规则来源状态修改失败");
pub(crate) const FAILED_TO_CHANGE_SUBSCRIPTION_STATE: LocalizedText =
    LocalizedText::new("Failed to change subscription state", "订阅状态修改失败");
pub(crate) const FAILED_TO_REMOVE_SOURCE: LocalizedText =
    LocalizedText::new("Failed to remove source", "移除来源失败");
pub(crate) const FAILED_TO_SAVE_RULE_SOURCE_POLICY: LocalizedText =
    LocalizedText::new("Failed to save rule source policy", "规则源策略保存失败");
pub(crate) const FALLBACK_ALWAYS_LAST: LocalizedText =
    LocalizedText::new("Fallback · always last", "兜底规则 · 始终最后");
pub(crate) const FALLBACK_FOR_TRAFFIC_NOT_MATCHED_ABOVE: LocalizedText = LocalizedText::new(
    "Fallback for traffic not matched above",
    "兜底处理此前未命中的流量",
);
pub(crate) const FILE_DOWNLOADED_BUT_NO_RECOGNIZABLE_QX_DOMAIN_RULES_WERE_FOUND: LocalizedText =
    LocalizedText::new(
        "File downloaded, but no recognizable QX domain rules were found",
        "文件已下载，但没有可识别的 QX 域名规则",
    );
pub(crate) const FINAL_CANNOT_BE_COMBINED_WITH_ANOTHER_CONDITION: LocalizedText =
    LocalizedText::new(
        "FINAL cannot be combined with another condition",
        "FINAL 不能和其他匹配条件组合",
    );
pub(crate) const FINAL_DOES_NOT_NEED_A_MATCH_PARAMETER: LocalizedText = LocalizedText::new(
    "FINAL does not need a match parameter",
    "FINAL 不需要匹配参数",
);
pub(crate) const FINAL_IS_ALWAYS_EVALUATED_LAST_AND_HANDLES_UNMATCHED_TRAFFIC: LocalizedText =
    LocalizedText::new(
        "FINAL is always evaluated last and handles unmatched traffic.",
        "FINAL 始终最后匹配，用于处理此前未命中的流量。",
    );
pub(crate) const FOLLOW_SYSTEM: LocalizedText = LocalizedText::new("Follow system", "跟随系统");
pub(crate) const GENERAL: LocalizedText = LocalizedText::new("General", "通用");
pub(crate) const GLOBAL_EXIT: LocalizedText = LocalizedText::new("Global exit", "全局出口");
pub(crate) const GROUPS_MATCH_FROM_TOP_TO_BOTTOM_USE_THE_ARROWS_TO: LocalizedText =
    LocalizedText::new(
        "Groups match from top to bottom; use the arrows to change priority.",
        "分组从上到下匹配；使用箭头调整优先级。",
    );
pub(crate) const HTTPS_ONLY_UP_TO_1_MIB_INVALID_LINES_ARE_COUNTED: LocalizedText =
    LocalizedText::new(
        "HTTPS only · Up to 1 MiB · Invalid lines are counted separately",
        "只接受 HTTPS · 最多 1 MiB · 无效行会单独计数",
    );
pub(crate) const INSTALLED: LocalizedText = LocalizedText::new("Installed", "已安装");
pub(crate) const INTERFACE_LANGUAGE: LocalizedText =
    LocalizedText::new("Interface language", "界面语言");
pub(crate) const IPV4_ADDRESS_RANGE: LocalizedText =
    LocalizedText::new("IPv4 address range", "IPv4 地址段");
pub(crate) const IPV6_ADDRESS_RANGE: LocalizedText =
    LocalizedText::new("IPv6 address range", "IPv6 地址段");
pub(crate) const LANGUAGE_CHANGED_BUT_COULD_NOT_BE_SAVED: LocalizedText = LocalizedText::new(
    "Language changed but could not be saved",
    "界面语言已切换，但保存失败",
);
pub(crate) const LANGUAGE_CHANGED_FOR_THIS_SESSION_DATA_DIRECTORY_UNAVAILABLE: LocalizedText =
    LocalizedText::new(
        "Language changed for this session; data directory unavailable.",
        "界面语言已在本次会话生效；无法确定保存位置。",
    );
pub(crate) const LANGUAGE_SAVED: LocalizedText =
    LocalizedText::new("Language saved", "界面语言已保存");
pub(crate) const LAST_UPDATE_FAILED: LocalizedText =
    LocalizedText::new("Last update failed", "上次更新失败");
pub(crate) const MANAGED: LocalizedText = LocalizedText::new("Managed", "Manis 托管");
pub(crate) const MANAGED_SECTION_SUMMARY: LocalizedText = LocalizedText::new("Managed", "托管");
pub(crate) const MANAGE_MANIS_PREFERENCES_AND_DATA_SOURCES: LocalizedText = LocalizedText::new(
    "Manage Manis preferences and data sources",
    "管理 Manis 偏好与数据来源",
);
pub(crate) const MANUAL: LocalizedText = LocalizedText::new("Manual", "手动");
pub(crate) const MANUAL_RULES_UPDATED: LocalizedText =
    LocalizedText::new("Manual rules updated", "手动分流规则已更新");
pub(crate) const MANUAL_RULE_REMOVED: LocalizedText =
    LocalizedText::new("Manual rule removed", "手动规则已删除");
pub(crate) const MATCHES_ONLY_AFTER_EVERY_RULE_ABOVE_MISSES: LocalizedText = LocalizedText::new(
    "Matches only after every rule above misses",
    "仅在上方所有规则均未命中时生效",
);
pub(crate) const NETWORK_BEHAVIOR: LocalizedText =
    LocalizedText::new("Network behavior", "网络行为");
pub(crate) const NEVER_UPDATED: LocalizedText = LocalizedText::new("Never updated", "从未更新");
pub(crate) const NODE_NAME: LocalizedText = LocalizedText::new("Node name", "节点名称");
pub(crate) const NOT_INSTALLED: LocalizedText = LocalizedText::new("Not installed", "尚未安装");
pub(crate) const NO_EXACT_KERNEL_EQUIVALENT: LocalizedText =
    LocalizedText::new("No exact kernel equivalent", "内核无精确等价规则");
pub(crate) const NO_PROXY_SOURCES: LocalizedText =
    LocalizedText::new("No proxy sources", "暂无代理来源");
pub(crate) const NO_RECOGNIZABLE_DOMAIN_RULES: LocalizedText =
    LocalizedText::new("No recognizable domain rules", "没有可识别的域名规则");
pub(crate) const NO_ROUTING_RULES_YET: LocalizedText =
    LocalizedText::new("No routing rules yet", "还没有分流规则");
pub(crate) const NO_RULE_SOURCES: LocalizedText =
    LocalizedText::new("No rule sources", "暂无规则源");
pub(crate) const ONLY_ONE_FINAL_RULE_CAN_BE_CONFIGURED: LocalizedText = LocalizedText::new(
    "Only one FINAL rule can be configured",
    "只能配置一条 FINAL 规则",
);
pub(crate) const OTHER_SAFELY_READABLE_SOURCES_ARE_KEPT_CHECK_THE_USER_DATA: LocalizedText =
    LocalizedText::new(
        "Other safely readable sources are kept; check the user data directory permissions.",
        "其余可安全读取的来源仍然保留；可检查用户数据目录权限。",
    );
pub(crate) const POLICY_GROUP_AFTER_MATCH: LocalizedText =
    LocalizedText::new("Policy group after match", "命中后的策略组");
pub(crate) const PROCESSING: LocalizedText = LocalizedText::new("Processing…", "正在处理…");
pub(crate) const PROCESS_IDENTIFICATION: LocalizedText =
    LocalizedText::new("Process identification", "进程识别");
pub(crate) const PROXY_MODE: LocalizedText = LocalizedText::new("Proxy mode", "代理模式");
pub(crate) const PROXY_SOURCES: LocalizedText = LocalizedText::new("Proxy sources", "代理来源");
pub(crate) const QX_RULES_NOT_IMPORTED_NO_RECOGNIZABLE_DOMAIN_RULES: LocalizedText =
    LocalizedText::new(
        "QX rules not imported: no recognizable domain rules",
        "QX 规则未导入：没有可识别的域名规则",
    );
pub(crate) const QX_RULE_DOWNLOAD_FAILED: LocalizedText =
    LocalizedText::new("QX rule download failed", "QX 规则下载失败");
pub(crate) const QX_RULE_SAVE_FAILED: LocalizedText =
    LocalizedText::new("QX rule save failed", "QX 规则保存失败");
pub(crate) const QX_RULE_UPDATE_FAILED: LocalizedText =
    LocalizedText::new("QX rule update failed", "QX 规则更新失败");
pub(crate) const REMOTE_QX_RULE_REMOVAL_FAILED: LocalizedText =
    LocalizedText::new("Remote QX rule removal failed", "远程 QX 规则移除失败");
pub(crate) const REMOTE_QX_RULE_UPDATE_FAILED: LocalizedText =
    LocalizedText::new("Remote QX rule update failed", "远程 QX 规则更新失败");
pub(crate) const REMOTE_RULE_SETS: LocalizedText =
    LocalizedText::new("Remote rule sets", "远程规则集");
pub(crate) const REMOVE: LocalizedText = LocalizedText::new("Remove", "移除");
pub(crate) const REMOVE_CONDITION: LocalizedText =
    LocalizedText::new("Remove condition", "移除条件");
pub(crate) const REMOVE_SINGLE_NODE_SOURCE: LocalizedText =
    LocalizedText::new("Remove single-node source", "移除单节点来源");
pub(crate) const REMOVE_THIS_CONDITION: LocalizedText =
    LocalizedText::new("Remove this condition", "移除这个条件");
pub(crate) const REMOVE_THIS_SUBSCRIPTION: LocalizedText =
    LocalizedText::new("Remove this subscription", "移除这个订阅");
pub(crate) const REMOVING: LocalizedText = LocalizedText::new("Removing…", "正在移除…");
pub(crate) const REMOVING_REMOTE_QX_RULES: LocalizedText =
    LocalizedText::new("Removing remote QX rules", "正在移除远程 QX 规则");
pub(crate) const ROUTING_MODE: LocalizedText = LocalizedText::new("Routing mode", "路由模式");
pub(crate) const RULE_GROUP_MOVED_DOWN: LocalizedText =
    LocalizedText::new("Rule group moved down", "规则分组已下移");
pub(crate) const RULE_GROUP_MOVED_UP: LocalizedText =
    LocalizedText::new("Rule group moved up", "规则分组已上移");
pub(crate) const RULE_SOURCES: LocalizedText = LocalizedText::new("Rule sources", "规则来源");
pub(crate) const RULE_SOURCE_ALREADY_EXISTS_NO_DUPLICATE_WAS_ADDED: LocalizedText =
    LocalizedText::new(
        "Rule source already exists; no duplicate was added",
        "规则源已存在，未重复添加",
    );
pub(crate) const RULE_SOURCE_DISABLED: LocalizedText =
    LocalizedText::new("Rule source disabled", "规则来源已停用");
pub(crate) const RULE_SOURCE_ENABLED: LocalizedText =
    LocalizedText::new("Rule source enabled", "规则来源已启用");
pub(crate) const RULE_SOURCE_POLICY_SET_TO: LocalizedText =
    LocalizedText::new("Rule source policy set to", "规则源策略已设为");
pub(crate) const RULE_URL: LocalizedText = LocalizedText::new("Rule URL", "规则 URL");
pub(crate) const RUNTIME: LocalizedText = LocalizedText::new("Runtime", "运行内核");
pub(crate) const RUNTIME_KERNEL: LocalizedText = LocalizedText::new("Runtime kernel", "运行内核");
pub(crate) const SAVING: LocalizedText = LocalizedText::new("Saving…", "保存中…");
pub(crate) const SAVING_RULE_SOURCE_POLICY: LocalizedText =
    LocalizedText::new("Saving rule source policy", "正在保存规则源策略");
pub(crate) const SECURELY_DOWNLOADING_PARSING_AND_WRITING_LOCALLY: LocalizedText =
    LocalizedText::new(
        "Securely downloading, parsing, and writing locally…",
        "正在安全下载、解析并写入本机…",
    );
pub(crate) const SELECTED: LocalizedText = LocalizedText::new("Selected", "已选择");
pub(crate) const SELECT_LANGUAGE: LocalizedText =
    LocalizedText::new("Select language", "选择界面语言");
pub(crate) const SETTINGS: LocalizedText = LocalizedText::new("SETTINGS", "设置");
pub(crate) const SINGLE_NODE: LocalizedText = LocalizedText::new("Single node", "单节点");
pub(crate) const SINGLE_NODE_SOURCE: LocalizedText =
    LocalizedText::new("Single node", "单节点来源");
pub(crate) const SINGLE_NODE_SOURCE_REMOVED: LocalizedText =
    LocalizedText::new("Single-node source removed", "单节点来源已移除");
pub(crate) const SINGLE_NODE_SOURCE_SAVE_FAILED: LocalizedText =
    LocalizedText::new("Single-node source save failed", "单节点来源保存失败");
pub(crate) const SINGLE_NODE_SOURCE_UPDATED: LocalizedText =
    LocalizedText::new("Single-node source updated", "单节点来源已更新");
pub(crate) const SING_BOX_WAS_NOT_FOUND_ON_THIS_DEVICE: LocalizedText = LocalizedText::new(
    "sing-box was not found on this device",
    "本机未检测到 sing-box",
);
pub(crate) const SOME_LOCAL_SOURCES_COULD_NOT_BE_RESTORED: LocalizedText = LocalizedText::new(
    "Some local sources could not be restored",
    "部分本地来源未能恢复",
);
pub(crate) const SOURCE_NAME: LocalizedText = LocalizedText::new("Source name", "来源名称");
pub(crate) const RULE_SOURCE_NAME_PLACEHOLDER: LocalizedText =
    LocalizedText::new("Leave blank to use the default name", "留空使用默认名称");
pub(crate) const SOURCE_RECOGNITION_FAILED: LocalizedText =
    LocalizedText::new("Source recognition failed", "来源识别失败");
pub(crate) const SOURCE_TYPE: LocalizedText = LocalizedText::new("Source type", "来源类型");
pub(crate) const SOURCE_URL: LocalizedText = LocalizedText::new("Source URL", "来源 URL");
pub(crate) const SUBSCRIPTION: LocalizedText = LocalizedText::new("Subscription", "订阅来源");
pub(crate) const SUBSCRIPTIONS_AND_NODES: LocalizedText =
    LocalizedText::new("Subscriptions and nodes", "订阅与单节点");
pub(crate) const SUBSCRIPTIONS_POLICY_GROUPS_AND_LATENCY_TESTS: LocalizedText = LocalizedText::new(
    "Subscriptions, policy groups, and latency tests",
    "支持订阅、策略组与测速",
);
pub(crate) const SUBSCRIPTION_DISABLED: LocalizedText =
    LocalizedText::new("Subscription disabled", "订阅已停用");
pub(crate) const SUBSCRIPTION_ENABLED: LocalizedText =
    LocalizedText::new("Subscription enabled", "订阅已启用");
pub(crate) const SUPPORTS_MANUAL_VLESS_SELECTORS_URL_TESTS_AND_ROUTING_RULES: LocalizedText =
    LocalizedText::new(
        "Supports manual VLESS, selectors, URL tests, and routing rules",
        "支持手动 VLESS、选择器、自动测速与分流规则",
    );
pub(crate) const SWITCH_AND_VALIDATE: LocalizedText =
    LocalizedText::new("Switch and validate", "切换并校验");
pub(crate) const SWITCH_TO: LocalizedText = LocalizedText::new("Switch to", "切换到");
pub(crate) const TARGET: LocalizedText = LocalizedText::new("Target", "目标");
pub(crate) const TARGET_POLICY: LocalizedText = LocalizedText::new("Target policy", "目标策略");
pub(crate) const THE_PARAMETER_CANNOT_CONTAIN_COMMAS_TABS_OR_LINE_BREAKS: LocalizedText =
    LocalizedText::new(
        "The parameter cannot contain commas, tabs, or line breaks",
        "参数不能包含逗号、制表符或换行",
    );
pub(crate) const THE_SAME_CONDITION_APPEARS_MORE_THAN_ONCE: LocalizedText = LocalizedText::new(
    "The same condition appears more than once",
    "同一个匹配条件不能重复添加",
);
pub(crate) const THE_TARGET_POLICY_IS_USED_BY_EVERY_RULE_IN_THIS: LocalizedText =
    LocalizedText::new(
        "The target policy is used by every rule in this source.",
        "此来源中的全部规则都会使用所选目标策略。",
    );
pub(crate) const THIS_MANUAL_RULE_ALREADY_EXISTS: LocalizedText =
    LocalizedText::new("This manual rule already exists", "这条手动规则已经存在");
pub(crate) const THIS_RULE_TYPE_CANNOT_BE_MATCHED_EXACTLY_BY_THE_CURRENT: LocalizedText =
    LocalizedText::new(
        "This rule type cannot be matched exactly by the current kernel",
        "当前内核无法精确匹配这种规则类型",
    );
pub(crate) const THIS_SOURCE_MUST_REMAIN_A_SINGLE_NODE_SHARE_LINK: LocalizedText =
    LocalizedText::new(
        "This source must remain a single-node share link",
        "此来源必须保持为单节点分享链接",
    );
pub(crate) const UPDATED_JUST_NOW: LocalizedText =
    LocalizedText::new("Updated just now", "刚刚更新");
pub(crate) const UPDATE_FAILED: LocalizedText = LocalizedText::new("Update failed", "更新失败");
pub(crate) const UPDATE_INTERVAL: LocalizedText = LocalizedText::new("Update interval", "更新间隔");
pub(crate) const UPDATE_NOW: LocalizedText = LocalizedText::new("Update now", "立即更新");
pub(crate) const UPDATE_THIS_SUBSCRIPTION_NOW: LocalizedText =
    LocalizedText::new("Update this subscription now", "立即更新这个订阅");
pub(crate) const UPDATING: LocalizedText = LocalizedText::new("Updating…", "更新中…");
pub(crate) const UPDATE_STATUS: LocalizedText = LocalizedText::new("Updating…", "正在更新…");
pub(crate) const UPDATING_REMOTE_QX_RULES: LocalizedText =
    LocalizedText::new("Updating remote QX rules", "正在更新远程 QX 规则");
pub(crate) const US: LocalizedText = LocalizedText::new("US", "US（国家代码）");
pub(crate) const USED_TO_IMPROVE_NETWORK_ACTIVITY: LocalizedText = LocalizedText::new(
    "Used to improve Network Activity",
    "用于改善网络活动中的进程信息",
);
pub(crate) const USE_THIS_SOURCE: LocalizedText =
    LocalizedText::new("Use this source", "使用此来源");
pub(crate) const VALIDATING: LocalizedText = LocalizedText::new("Validating", "正在校验");
pub(crate) const VALIDATING_AND_SAVING_SINGLE_NODE_SOURCE: LocalizedText = LocalizedText::new(
    "Validating and saving single-node source",
    "正在验证并保存单节点来源",
);
pub(crate) const WILDCARD_DOMAIN: LocalizedText =
    LocalizedText::new("Wildcard domain", "通配符域名");
