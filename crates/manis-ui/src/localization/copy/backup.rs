use crate::localization::LocalizedText;

pub(crate) const TITLE: LocalizedText = LocalizedText::new("Backup and migration", "备份与迁移");
pub(crate) const DETAIL: LocalizedText = LocalizedText::new(
    "Move your complete Manis configuration to another device using a JSON file.",
    "导出完整的 JSON 配置文件，在另一台设备上的 Manis 中导入。",
);
pub(crate) const EXPORT: LocalizedText = LocalizedText::new("Export configuration", "导出完整配置");
pub(crate) const IMPORT: LocalizedText = LocalizedText::new("Import file", "从文件导入");
pub(crate) const EDIT: LocalizedText = LocalizedText::new("Edit configuration", "修改配置");
pub(crate) const EDIT_DETAIL: LocalizedText = LocalizedText::new(
    "Edit the current complete Manis configuration, or paste an exported JSON file to replace it. Validate and preview before applying.",
    "下方是当前完整的 Manis 配置，可直接修改，也可粘贴导出的 JSON 文件内容替换。应用前需校验并预览。",
);
pub(crate) const LOADING_CURRENT: LocalizedText =
    LocalizedText::new("Loading current configuration…", "正在读取当前配置…");
pub(crate) const EDIT_FAILED: LocalizedText = LocalizedText::new(
    "Could not read the current configuration. Check file access permissions and size limits.",
    "无法读取当前配置，请检查文件访问权限及大小限制。",
);
pub(crate) const VALIDATE: LocalizedText = LocalizedText::new("Validate and preview", "校验并预览");
pub(crate) const BACK_TO_EDIT: LocalizedText = LocalizedText::new("Back to editing", "返回修改");
pub(crate) const SENSITIVE: LocalizedText = LocalizedText::new(
    "Includes subscription URLs and node passwords in plain text. Keep the file private and only import configurations you trust.",
    "配置包含明文订阅链接和节点密码。请妥善保管，不要公开分享；仅导入可信的配置。",
);
pub(crate) const EXCLUDED: LocalizedText = LocalizedText::new(
    "Core binaries, TUN permissions, logs and latency results are not included. Import does not enable a proxy.",
    "内核程序、TUN 权限、日志和测速结果不参与迁移。导入后不会自动开启代理。",
);
pub(crate) const PREVIEW: LocalizedText = LocalizedText::new("Configuration preview", "配置预览");
pub(crate) const REPLACE_NOTICE: LocalizedText = LocalizedText::new(
    "This replaces the current configuration, including any empty sections. Manis will turn off the proxy, stop the core, back up the old configuration, import and restart.",
    "将完整替换当前配置（包括空白的部分），不会合并。Manis 会关闭代理、停止内核，自动备份旧配置，再导入并重启。",
);
pub(crate) const REPLACE: LocalizedText = LocalizedText::new("Replace and restart", "替换并重启");
pub(crate) const SUBSCRIPTIONS: LocalizedText = LocalizedText::new("Subscriptions", "订阅来源");
pub(crate) const NODES: LocalizedText = LocalizedText::new("Saved nodes", "单独保存的节点");
pub(crate) const GROUPS: LocalizedText = LocalizedText::new("Policy groups", "策略组");
pub(crate) const RULE_SOURCES: LocalizedText = LocalizedText::new("Rule sources", "规则来源");
pub(crate) const MANUAL_RULES: LocalizedText = LocalizedText::new("Manual rules", "手动规则");
pub(crate) const EXPORTING: LocalizedText = LocalizedText::new(
    "Choose where to save and grant Manis access to the selected file…",
    "请选择保存位置，并授权 Manis 写入所选文件…",
);
pub(crate) const EXPORT_CANCELLED: LocalizedText =
    LocalizedText::new("Configuration export cancelled.", "已取消导出配置。");
pub(crate) const IMPORT_CANCELLED: LocalizedText =
    LocalizedText::new("Configuration import cancelled.", "已取消导入配置。");
pub(crate) const IMPORT_PERMISSION_DENIED: LocalizedText = LocalizedText::new(
    "Manis could not read the selected file. Select it again and grant access in the open dialog.",
    "Manis 无法读取所选文件。请重新选择，并在打开对话框中授权访问。",
);
pub(crate) const READING: LocalizedText =
    LocalizedText::new("Validating configuration…", "正在校验配置…");
pub(crate) const IMPORTING: LocalizedText = LocalizedText::new(
    "Backing up and importing. Please wait…",
    "正在备份并导入，请稍候…",
);
pub(crate) const EXPORTED: LocalizedText = LocalizedText::new(
    "Configuration exported. Transfer this file privately to your other device.",
    "配置已导出。将此文件私下传到另一台设备后导入即可。",
);
pub(crate) const IMPORTED: LocalizedText = LocalizedText::new(
    "Configuration imported. Restarting Manis…",
    "配置已导入，正在重启 Manis…",
);
pub(crate) const SHOW_FILE: LocalizedText = LocalizedText::new("Show file", "显示文件");
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
pub(crate) const FILE_ERROR: LocalizedText = LocalizedText::new(
    "Could not open the file dialog or access the selected file.",
    "无法打开文件选择器或访问所选文件。",
);
pub(crate) const INVALID: LocalizedText = LocalizedText::new(
    "Invalid, unsupported or oversized Manis backup. No configuration was changed.",
    "配置格式无效、版本不支持或文件过大。当前配置未改动。",
);
pub(crate) const EXPORT_FAILED: LocalizedText = LocalizedText::new(
    "Could not export the configuration. Check the configuration and destination permissions.",
    "无法导出配置，请检查配置是否完整及目标位置的权限。",
);
pub(crate) const EXPORT_PERMISSION_DENIED: LocalizedText = LocalizedText::new(
    "Manis could not write to the selected file. Export again and grant access in the save dialog.",
    "Manis 无法写入所选文件。请重新导出，并在保存对话框中授权访问。",
);
pub(crate) const STOP_FAILED: LocalizedText = LocalizedText::new(
    "Could not safely stop the proxy or core. Import was cancelled; your configuration is unchanged.",
    "无法安全关闭代理或停止内核，已取消导入；当前配置未改动。",
);
pub(crate) const RESTORE_FAILED: LocalizedText = LocalizedText::new(
    "Import failed. Check the automatic backup before restarting the core.",
    "导入失败。请检查自动备份后再启动内核。",
);
pub(crate) const ROLLBACK_FAILED: LocalizedText = LocalizedText::new(
    "Import and automatic rollback failed. Keep the core stopped. Use Show file to recover your previous configuration from the backup.",
    "导入失败，且自动回滚未完成。请保持内核停止，点击「显示文件」从备份恢复原配置。",
);
