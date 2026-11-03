package dev.manis.app.ui

import android.content.ClipData
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import dev.manis.app.data.MobileConfig
import dev.manis.app.data.PROXY_DNS_DOH
import dev.manis.app.data.PROXY_DNS_SYSTEM
import dev.manis.app.data.Workspace

/** The GPUI configuration sections, in navigation order. */
private enum class ConfigSection(val label: String) {
    General("通用"),
    Runtime("运行时"),
    ProxySources("订阅与节点"),
    RuleSources("规则集"),
    Advanced("高级"),
    Updates("更新"),
}

/** The GPUI configuration workspace, compact: a three-column directory and one section at a time. */
@Composable
fun ConfigurationWorkspace(onMessage: (String) -> Unit, onOpenLogs: () -> Unit) {
    val colors = LocalManisColors.current
    var section by rememberSaveable { mutableStateOf(ConfigSection.General) }

    Column(Modifier.fillMaxSize().background(colors.surfaceBase)) {
        Column(
            Modifier
                .fillMaxWidth()
                .background(colors.surfaceChrome)
                .padding(horizontal = ManisDimens.spaceMd, vertical = ManisDimens.spaceSm),
            verticalArrangement = Arrangement.spacedBy(ManisDimens.spaceXs),
        ) {
            Text("设置", style = ManisType.metadata, color = colors.textTertiary)
            for (chunk in ConfigSection.entries.chunked(3)) {
                Row(horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceXs)) {
                    for (entry in chunk) {
                        val selected = entry == section
                        Box(
                            Modifier
                                .weight(1f)
                                .height(ManisDimens.controlCompact)
                                .clip(RoundedCornerShape(ManisDimens.radiusControl))
                                .background(if (selected) colors.actionSoft else colors.buttonBg)
                                .clickable { section = entry },
                            contentAlignment = Alignment.Center,
                        ) {
                            Text(
                                entry.label,
                                style = ManisType.metadata,
                                color = if (selected) colors.actionPrimary else colors.textSecondary,
                            )
                        }
                    }
                }
            }
        }
        ManisDivider()
        Box(Modifier.weight(1f)) {
            when (section) {
                ConfigSection.General -> GeneralSection(onMessage)
                ConfigSection.Runtime -> RuntimeSection(onMessage)
                ConfigSection.ProxySources -> ProxySourcesPane(onMessage = onMessage)
                ConfigSection.RuleSources -> RuleSourcesSection()
                ConfigSection.Advanced -> AdvancedSection(onMessage, onOpenLogs)
                ConfigSection.Updates -> Box(Modifier.padding(ManisDimens.spaceMd)) {
                    UpdateSection(onMessage)
                }
            }
        }
    }
}

@Composable
private fun SectionScroll(content: @Composable () -> Unit) {
    Column(
        Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(ManisDimens.spaceMd),
        verticalArrangement = Arrangement.spacedBy(ManisDimens.spaceMd),
    ) {
        content()
    }
}

@Composable
private fun GeneralSection(onMessage: (String) -> Unit) {
    val colors = LocalManisColors.current
    val context = LocalContext.current
    val version = remember(context) {
        runCatching {
            context.packageManager.getPackageInfo(context.packageName, 0).versionName
        }.getOrNull() ?: "—"
    }
    SectionScroll {
        ManisCard {
            Text("外观", style = ManisType.label, color = colors.textPrimary)
            ManisInfoRow("主题", when (UiPrefs.dark.value) {
                true -> "深色"
                false -> "浅色"
                null -> "跟随系统"
            })
            Row(horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceXs)) {
                ManisChip("跟随系统", UiPrefs.dark.value == null) { UiPrefs.dark.value = null }
                ManisChip("浅色", UiPrefs.dark.value == false) { UiPrefs.dark.value = false }
                ManisChip("深色", UiPrefs.dark.value == true) { UiPrefs.dark.value = true }
            }
            ManisInfoRow("语言", "中文")
        }
        ManisCard {
            Text("关于", style = ManisType.label, color = colors.textPrimary)
            ManisInfoRow("版本", version)
            ManisMeta("Manis Android 实验版")
            ManisMeta(
                "Mihomo 内核 v1.19.33 + Android 兼容补丁，随 APK 以 GPL-3.0 分发；" +
                    "补丁与构建脚本见仓库 android/ 目录。",
            )
            ManisMeta("Manis 应用代码为 Apache-2.0。")
        }
    }
}

@Composable
private fun RuntimeSection(onMessage: (String) -> Unit) {
    val colors = LocalManisColors.current
    val context = LocalContext.current
    val config = Workspace.config.value
    val abi = android.os.Build.SUPPORTED_ABIS.firstOrNull() ?: "unknown"
    SectionScroll {
        ManisCard {
            Text("连接", style = ManisType.label, color = colors.textPrimary)
            ManisInfoRow("状态", if (RuntimeState.active) "已连接" else "未连接")
            ManisInfoRow("路由模式", modeLabel(RuntimeState.snapshot?.mode ?: config.mode))
            ManisInfoRow("节点域名解析", proxyDnsLabel(config))
        }
        ManisCard {
            Text("配置摘要", style = ManisType.label, color = colors.textPrimary)
            ManisInfoRow("内核版本", RuntimeState.snapshot?.version?.ifBlank { "—" } ?: "—")
            ManisInfoRow("配置", RuntimeState.summary.ifBlank { "—" })
            ManisInfoRow(
                "工作区",
                "${config.subscriptions.size} 订阅 · ${config.nodes.size} 节点 · ${config.groups.size} 策略组",
            )
            ManisInfoRow("设备 ABI", abi)
            ManisInfoRow("数据目录", "files/mihomo")
            ManisButton("重新载入配置", onClick = {
                applyWorkspace(context)
                onMessage("已重新载入配置")
            })
        }
    }
}

@Composable
private fun RuleSourcesSection() {
    val colors = LocalManisColors.current
    SectionScroll {
        ManisCard {
            Text("远程规则集", style = ManisType.label, color = colors.textPrimary)
            ManisMeta("Android 版暂不支持远程规则集；规则在「规则」工作区里手动维护。")
        }
    }
}

@Composable
private fun AdvancedSection(onMessage: (String) -> Unit, onOpenLogs: () -> Unit) {
    val colors = LocalManisColors.current
    val context = LocalContext.current
    val config = Workspace.config.value
    SectionScroll {
        ManisCard {
            Text("节点域名解析", style = ManisType.label, color = colors.textPrimary)
            ManisMeta(proxyDnsLabel(config))
            Row(horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceXs)) {
                ManisChip("加密 DNS（DoH）", config.proxyDns == PROXY_DNS_DOH) {
                    Workspace.update { it.copy(proxyDns = PROXY_DNS_DOH) }
                    applyWorkspace(context)
                    onMessage("已切换为加密 DNS")
                }
                ManisChip("本机 DNS", config.proxyDns == PROXY_DNS_SYSTEM) {
                    Workspace.update { it.copy(proxyDns = PROXY_DNS_SYSTEM) }
                    applyWorkspace(context)
                    onMessage("已切换为本机 DNS")
                }
            }
        }
        ManisCard {
            Text("备份与恢复", style = ManisType.label, color = colors.textPrimary)
            ManisMeta("导出的是 Manis 备份格式（含订阅链接和节点密钥），不是交给其他客户端的 YAML。")
            Row(horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceXs)) {
                ManisButton("复制到剪贴板", onClick = {
                    val clipboard = context.getSystemService(android.content.ClipboardManager::class.java)
                    clipboard?.setPrimaryClip(
                        ClipData.newPlainText("Manis 配置", Workspace.config.value.toJson()),
                    )
                    onMessage("配置已复制到剪贴板")
                })
                ManisButton("从剪贴板导入", onClick = {
                    val clipboard = context.getSystemService(android.content.ClipboardManager::class.java)
                    val text = clipboard?.primaryClip
                        ?.takeIf { it.itemCount > 0 }
                        ?.getItemAt(0)
                        ?.coerceToText(context)
                        ?.toString()
                        .orEmpty()
                    when (val result = importWorkspace(context, text)) {
                        null -> onMessage("已从剪贴板导入并生效")
                        else -> onMessage(result)
                    }
                })
            }
        }
        ManisCard {
            Text("诊断", style = ManisType.label, color = colors.textPrimary)
            ManisButton("查看内核日志", onClick = onOpenLogs)
        }
    }
}

private fun modeLabel(mode: String): String = when (mode) {
    "global" -> "全局"
    "direct" -> "直连"
    else -> "规则"
}

private fun proxyDnsLabel(config: MobileConfig): String = when {
    config.proxyDnsServers.isNotEmpty() -> "自定义 DoH（${config.proxyDnsServers.size} 条）"
    config.discoveredDns.isNotEmpty() -> "机场 DNS（自动 ${config.discoveredDns.size} 条）"
    config.proxyDns == PROXY_DNS_SYSTEM -> "本机 DNS"
    else -> "加密 DNS（DoH）"
}

private fun importWorkspace(context: android.content.Context, text: String): String? {
    if (text.isBlank()) return "剪贴板里没有配置"
    val imported = runCatching { MobileConfig.fromJson(text) }
        .getOrElse { return "剪贴板内容不是 Manis 配置" }
    if (imported.subscriptions.none { it.enabled } && imported.nodes.none { it.enabled }) {
        return "导入的配置里没有启用的订阅或节点"
    }
    val compiled = runCatching {
        dev.manis.app.core.Native.renderConfig(imported.toJson(), 3)
    }.getOrElse { error -> return error.message ?: "配置校验失败" }
    if (compiled.isBlank()) return "配置校验失败"
    Workspace.update { imported }
    applyWorkspace(context)
    return null
}
