import { Storage, signal } from "nsh";

/** The interface languages. */
export type Lang = "zh" | "en";

export const lang = signal<Lang>("zh");

type Entry = { zh: string; en: string };

const table: Record<string, Entry> = {
  "brand.subtitle": { zh: "策略路由工作台", en: "Policy routing workbench" },
  "nav.connection": { zh: "连接", en: "Connection" },
  "nav.nodes": { zh: "节点", en: "Nodes" },
  "nav.subscriptions": { zh: "订阅", en: "Subscriptions" },
  "nav.rules": { zh: "规则", en: "Rules" },
  "nav.settings": { zh: "设置", en: "Settings" },
  "nav.logs": { zh: "日志", en: "Logs" },
  "hint.connection": { zh: "运行状态与出口", en: "Runtime state and exit" },
  "hint.nodes": { zh: "策略组与节点选择", en: "Policy groups and nodes" },
  "hint.subscriptions": { zh: "订阅、手动节点与策略组", en: "Sources, nodes and groups" },
  "hint.rules": { zh: "有序分流规则", en: "Ordered routing rules" },
  "hint.settings": { zh: "外观、更新与内核", en: "Appearance, updates and kernel" },
  "hint.logs": { zh: "内核日志", en: "Kernel log" },
  "chrome.connect": { zh: "连接", en: "Connect" },
  "chrome.disconnect": { zh: "断开", en: "Disconnect" },
  "status.connected": { zh: "已连接", en: "Connected" },
  "status.disconnected": { zh: "未连接", en: "Disconnected" },
  "status.noNode": { zh: "未选择节点", en: "No node selected" },
  "status.waiting": { zh: "等待内核状态", en: "Waiting for the kernel" },
  "status.nodes": { zh: "节点 {count} · NSH", en: "Nodes {count} · NSH" },
  "logs.title": { zh: "日志", en: "Logs" },
  "logs.refresh": { zh: "刷新", en: "Refresh" },
  "logs.refreshing": { zh: "读取中…", en: "Reading…" },
  "logs.copy": { zh: "复制", en: "Copy" },
  "logs.copied": { zh: "日志已复制到剪贴板", en: "Log copied to the clipboard" },
  "logs.empty": { zh: "还没有日志。", en: "No log lines yet." },
  "logs.failed": { zh: "读取日志失败", en: "Reading the log failed" },
  "logs.lines": { zh: "{count} 行", en: "{count} lines" },
  "logs.hint": { zh: "内核日志来自本机 controller。", en: "Kernel lines come from the local controller." },
  "logs.note": { zh: "日志由 Manis 自己的内核产生；复制后可直接贴到 issue 里。", en: "The log comes from Manis's own kernel; copy it straight into an issue." },
  "logs.noHost": { zh: "独立运行模式：没有宿主可读取日志", en: "Standalone: no host to read the log from" },
  "menu.view": { zh: "视图", en: "View" },
  "menu.connection": { zh: "连接", en: "Connection" },
  "menu.edit": { zh: "编辑", en: "Edit" },
  "menu.window": { zh: "窗口", en: "Window" },
  "menu.about": { zh: "关于 Manis", en: "About Manis" },
  "menu.hide": { zh: "隐藏", en: "Hide" },
  "menu.hideOthers": { zh: "隐藏其他", en: "Hide Others" },
  "menu.showAll": { zh: "显示全部", en: "Show All" },
  "menu.quit": { zh: "退出 Manis", en: "Quit Manis" },
  "menu.connectToggle": { zh: "连接 / 断开", en: "Connect / Disconnect" },
  "menu.refreshNodes": { zh: "刷新节点", en: "Refresh nodes" },
  "menu.applyWorkspace": { zh: "应用工作区", en: "Apply workspace" },
  "menu.undo": { zh: "撤销", en: "Undo" },
  "menu.redo": { zh: "重做", en: "Redo" },
  "menu.cut": { zh: "剪切", en: "Cut" },
  "menu.copy": { zh: "复制", en: "Copy" },
  "menu.paste": { zh: "粘贴", en: "Paste" },
  "menu.selectAll": { zh: "全选", en: "Select All" },
  "menu.minimize": { zh: "最小化", en: "Minimize" },
  "menu.zoom": { zh: "缩放", en: "Zoom" },
  "menu.close": { zh: "关闭", en: "Close" },

  "common.current": { zh: "当前", en: "Current" },
  "common.notMeasured": { zh: "未测速", en: "Not measured" },
  "common.ms": { zh: "{value} 毫秒", en: "{value} ms" },
  "common.msShort": { zh: "{value} ms", en: "{value} ms" },
  "conn.title": { zh: "连接", en: "Connection" },
  "conn.hint": { zh: "手机与桌面共用同一份规则和渲染器。", en: "The phone and the desktop share one set of rules and one renderer." },
  "conn.connected": { zh: "已连接", en: "Connected" },
  "conn.disconnected": { zh: "未连接", en: "Disconnected" },
  "conn.loadingNode": { zh: "正在加载节点", en: "Loading the node" },
  "conn.openVpn": { zh: "打开开关建立 VPN", en: "Turn it on to start the VPN" },
  "conn.exit": { zh: "出口", en: "Exit" },
  "conn.switch": { zh: "切换", en: "Switch" },
  "conn.routeMode": { zh: "路由模式", en: "Routing mode" },
  "conn.mode.rule": { zh: "规则", en: "Rule" },
  "conn.mode.global": { zh: "全局", en: "Global" },
  "conn.mode.direct": { zh: "直连", en: "Direct" },
  "conn.reload": { zh: "重新载入配置", en: "Reload configuration" },
  "conn.warnings": { zh: "配置提示", en: "Configuration notes" },
  "conn.moreWarnings": { zh: "以及另外 {count} 条", en: "and {count} more" },
  "conn.unreachable": { zh: "所有节点都连不上", en: "No node is reachable" },
  "conn.unreachableHint": { zh: "到订阅节点的链路超时，流量现在无法出去。先换一个网络，或到节点页点「刷新」重试；仍然失败时，去设置里切换「节点域名解析」。", en: "Every node timed out, so traffic cannot leave. Try another network, or refresh the node list; if it still fails, change the node DNS mode in Settings." },
  "conn.noSources": { zh: "还没有启用的订阅或节点；先在「订阅」里添加一个。", en: "No enabled source yet; add one under Subscriptions." },
  "conn.overview": { zh: "概览", en: "Overview" },
  "conn.status": { zh: "状态", en: "Status" },
  "conn.summary": { zh: "摘要", en: "Summary" },
  "conn.tips": { zh: "提示", en: "Tips" },
  "conn.tipsBody": { zh: "规则、策略组与节点来自同一份已保存工作区；切换节点后立即生效。", en: "Rules, groups and nodes come from one saved workspace; a node switch applies at once." },
  "conn.chain": { zh: "路由链", en: "Route chain" },
  "conn.chainFlow": { zh: "规则 → 策略组 → 节点", en: "Rule → group → node" },
  "conn.currentNode": { zh: "当前节点", en: "Current node" },
  "conn.modeLabel": { zh: "模式", en: "Mode" },
  "nodes.title": { zh: "节点", en: "Nodes" },
  "nodes.hint": { zh: "按来源分组；点选立即切换，搜索支持中文和英文。", en: "Grouped by source; a tap switches at once, and search matches Chinese and English." },
  "nodes.search": { zh: "搜索节点", en: "Search nodes" },
  "nodes.refresh": { zh: "刷新", en: "Refresh" },
  "nodes.refreshing": { zh: "刷新中…", en: "Refreshing…" },
  "nodes.refreshed": { zh: "已刷新", en: "Refreshed" },
  "nodes.refreshFailed": { zh: "刷新失败", en: "Refresh failed" },
  "nodes.noHost": { zh: "独立运行模式：没有宿主可刷新", en: "Standalone: no host to refresh from" },
  "nodes.groups": { zh: "策略组", en: "Policy groups" },
  "nodes.sources": { zh: "订阅与手动节点", en: "Sources and nodes" },
  "nodes.noGroups": { zh: "还没有策略组", en: "No policy groups yet" },
  "nodes.noSources": { zh: "还没有订阅或手动节点", en: "No sources yet" },
  "nodes.noMatch": { zh: "没有匹配的节点。", en: "No matching node." },
  "nodes.globalHint": { zh: "全局策略使用所有已启用的订阅与手动节点；从左侧选择一个策略组或订阅查看节点。", en: "The global policy uses every enabled source; pick a group or source to list its nodes." },
  "nodes.subscription": { zh: "订阅", en: "Subscription" },
  "nodes.manual": { zh: "手动节点", en: "Manual node" },
  "nodes.enabled": { zh: "已启用", en: "Enabled" },
  "nodes.disabled": { zh: "已停用", en: "Disabled" },
  "nodes.count": { zh: "{count} 个", en: "{count}" },
  "nodes.runtime": { zh: "运行状态", en: "Runtime" },
  "nodes.rules": { zh: "规则", en: "Rules" },
  "nodes.noWarnings": { zh: "没有规则警告", en: "No rule warnings" },
  "nodes.warnings": { zh: "{count} 条规则警告", en: "{count} rule warnings" },
  "nodes.noSummary": { zh: "暂无规则摘要", en: "No rule summary yet" },
  "nodes.waiting": { zh: "等待内核状态", en: "Waiting for the kernel" },
  "nodes.mode": { zh: "模式", en: "Mode" },
  "settings.language": { zh: "语言", en: "Language" },
  "settings.languageHint": { zh: "界面语言，立即生效。", en: "Interface language; applies at once." },
};

/** A translated string; `{name}` placeholders come from `values`. */
export function t(key: string, values?: Record<string, string | number>): string {
  const entry = table[key];
  let text = entry ? entry[lang()] : key;
  if (values) {
    for (const [name, value] of Object.entries(values)) {
      text = text.replaceAll(`{${name}}`, String(value));
    }
  }
  return text;
}

/** Loads the saved language; runs once when the module loads. */
export async function loadLanguage(): Promise<void> {
  try {
    const saved = await Storage.getItem("language");
    if (saved === "zh" || saved === "en") lang.set(saved);
  } catch {
    // The default language stays.
  }
}

/** Switches and saves the language. */
export async function setLanguage(next: Lang): Promise<void> {
  lang.set(next);
  try {
    await Storage.setItem("language", next);
  } catch {
    // The switch still applies to this run.
  }
}

if (typeof Storage !== "undefined") {
  void loadLanguage();
}
