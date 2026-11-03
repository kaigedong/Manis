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
