use crate::localization::LocalizedText;

pub(crate) const TITLE: LocalizedText = LocalizedText::new("Configuration editor", "配置编辑器");
pub(crate) const DETAIL: LocalizedText = LocalizedText::new(
    "Open the complete JSON configuration to copy, replace or edit it directly in Manis.",
    "在 Manis 中打开完整 JSON 配置，可直接复制、粘贴替换或修改。",
);
pub(crate) const EDIT: LocalizedText = LocalizedText::new("Open configuration", "打开配置");
pub(crate) const EDITOR_TITLE: LocalizedText = LocalizedText::new("Edit configuration", "编辑配置");
pub(crate) const EDIT_DETAIL: LocalizedText = LocalizedText::new(
    "This is the current complete Manis configuration. Copy it for migration, paste another complete configuration to replace it, or edit it here. Manis validates and previews changes before applying them.",
    "下方是当前完整的 Manis 配置。可复制内容用于迁移，粘贴另一份完整配置进行替换，也可直接修改。Manis 会在应用前校验并预览更改。",
);
pub(crate) const LOADING_CURRENT: LocalizedText =
    LocalizedText::new("Loading current configuration…", "正在读取当前配置…");
pub(crate) const EDIT_FAILED: LocalizedText = LocalizedText::new(
    "Could not read the current configuration. Check file access permissions and size limits.",
    "无法读取当前配置，请检查文件访问权限及大小限制。",
);
pub(crate) const VALIDATE: LocalizedText = LocalizedText::new("Validate and preview", "校验并预览");
pub(crate) const BACK_TO_EDIT: LocalizedText = LocalizedText::new("Back to editing", "返回修改");
pub(crate) const COPY_ALL: LocalizedText = LocalizedText::new("Copy all", "复制全部");
pub(crate) const PASTE_REPLACE: LocalizedText =
    LocalizedText::new("Paste and replace", "粘贴并替换");
pub(crate) const COPIED: LocalizedText =
    LocalizedText::new("Configuration copied.", "配置已复制。");
pub(crate) const PASTED: LocalizedText = LocalizedText::new(
    "Clipboard content replaced the editor text. Validate it before applying.",
    "已用剪贴板内容替换编辑器文本，请先校验再应用。",
);
pub(crate) const CLIPBOARD_EMPTY: LocalizedText =
    LocalizedText::new("The clipboard does not contain text.", "剪贴板中没有文本。");
pub(crate) const SENSITIVE: LocalizedText = LocalizedText::new(
    "Includes subscription URLs and node passwords in plain text. Keep copied content private and only paste configurations you trust.",
    "配置包含明文订阅链接和节点密码。请妥善保管复制的内容，并且只粘贴可信配置。",
);
pub(crate) const EXCLUDED: LocalizedText = LocalizedText::new(
    "Core binaries, TUN permissions, logs and latency results are not included. Applying changes does not enable a proxy.",
    "内核程序、TUN 权限、日志和测速结果不在这份配置中。应用更改后不会自动开启代理。",
);
pub(crate) const PREVIEW: LocalizedText = LocalizedText::new("Configuration preview", "配置预览");
pub(crate) const REPLACE_NOTICE: LocalizedText = LocalizedText::new(
    "This replaces the current configuration, including any empty sections. Manis will turn off the proxy, stop the core, back up the old configuration, apply the changes and restart.",
    "将完整替换当前配置（包括空白的部分），不会合并。Manis 会关闭代理、停止内核，自动备份旧配置，再应用更改并重启。",
);
pub(crate) const REPLACE: LocalizedText = LocalizedText::new("Replace and restart", "替换并重启");
pub(crate) const SUBSCRIPTIONS: LocalizedText = LocalizedText::new("Subscriptions", "订阅来源");
pub(crate) const NODES: LocalizedText = LocalizedText::new("Saved nodes", "单独保存的节点");
pub(crate) const GROUPS: LocalizedText = LocalizedText::new("Policy groups", "策略组");
pub(crate) const RULE_SOURCES: LocalizedText = LocalizedText::new("Rule sources", "规则来源");
pub(crate) const MANUAL_RULES: LocalizedText = LocalizedText::new("Manual rules", "手动规则");
pub(crate) const EDIT_CANCELLED: LocalizedText =
    LocalizedText::new("Configuration editing cancelled.", "已取消修改配置。");
pub(crate) const READING: LocalizedText =
    LocalizedText::new("Validating configuration…", "正在校验配置…");
pub(crate) const APPLYING: LocalizedText = LocalizedText::new(
    "Backing up and applying. Please wait…",
    "正在备份并应用，请稍候…",
);
pub(crate) const APPLIED: LocalizedText = LocalizedText::new(
    "Configuration applied. Restarting Manis…",
    "配置已应用，正在重启 Manis…",
);
pub(crate) const SHOW_BACKUP: LocalizedText =
    LocalizedText::new("Show automatic backup", "查看自动备份");
pub(crate) const SHOW_BACKUPS: LocalizedText =
    LocalizedText::new("Show automatic backups", "查看自动备份");
pub(crate) const DONE: LocalizedText = LocalizedText::new("Done", "完成");
pub(crate) const BUSY: LocalizedText = LocalizedText::new(
    "Wait for the current refresh, speed test or configuration change to finish, then try again.",
    "请等待正在进行的刷新、测速或配置更改完成后重试。",
);
pub(crate) const NO_STORE: LocalizedText = LocalizedText::new(
    "The local configuration directory is unavailable.",
    "本地配置目录不可用。",
);
pub(crate) const INVALID: LocalizedText = LocalizedText::new(
    "The JSON configuration is invalid, unsupported or too large. No configuration was changed; return to the editor to fix it.",
    "JSON 配置无效、版本不支持或内容过大。当前配置未改动，请返回编辑器修正。",
);
pub(crate) const STOP_FAILED: LocalizedText = LocalizedText::new(
    "Could not safely stop the proxy or core. Applying was cancelled; your configuration is unchanged.",
    "无法安全关闭代理或停止内核，已取消应用；当前配置未改动。",
);
pub(crate) const RESTORE_FAILED: LocalizedText = LocalizedText::new(
    "Applying the configuration failed. Check the automatic backup before restarting the core.",
    "应用配置失败。请检查自动备份后再启动内核。",
);
pub(crate) const ROLLBACK_FAILED: LocalizedText = LocalizedText::new(
    "Applying and automatic rollback both failed. Keep the core stopped and open the automatic backup to recover your previous configuration.",
    "应用配置及自动回滚均失败。请保持内核停止，并打开自动备份恢复原配置。",
);
