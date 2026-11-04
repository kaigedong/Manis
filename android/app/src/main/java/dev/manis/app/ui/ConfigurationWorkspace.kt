package dev.manis.app.ui

import android.content.ClipData
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.material3.ScrollableTabRow
import androidx.compose.material3.Tab
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInParent
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import kotlin.math.roundToInt
import kotlinx.coroutines.launch
import dev.manis.app.data.MobileConfig
import dev.manis.app.data.PROXY_DNS_DOH
import dev.manis.app.data.PROXY_DNS_SYSTEM
import dev.manis.app.data.Workspace

/** The configuration sections, in navigation order, with the keys other workspaces request. */
private enum class ConfigSection(val key: String, val label: String, val detail: String) {
    General("general", "通用", "界面语言与启动"),
    Runtime("runtime", "运行时", "内核与运行时状态"),
    ProxySources("proxy", "订阅与节点", "订阅、手动节点与策略组"),
    RuleSources("rule_sources", "规则集", "远程规则集（暂不支持）"),
    Advanced("advanced", "高级", "网络行为与备份"),
    Updates("updates", "更新", "自动检查应用更新"),
}

/**
 * The configuration workspace. Phones page through one section at a time with a tab strip and a
 * horizontal swipe; wider layouts list the sections beside the selected one.
 */
@Composable
fun ConfigurationWorkspace(onMessage: (String) -> Unit, onOpenLogs: () -> Unit) {
    val colors = LocalManisColors.current
    val compact = LocalSizeClass.current == SizeClass.Compact
    val scope = rememberCoroutineScope()
    val pager = rememberPagerState(pageCount = { ConfigSection.entries.size })

    val requested = Workspace.requestedSection.value
    LaunchedEffect(requested) {
        if (requested != null) {
            val index = ConfigSection.entries.indexOfFirst { it.key == requested }
            if (index >= 0) pager.scrollToPage(index)
            Workspace.requestedSection.value = null
        }
    }

    val active = ConfigSection.entries[pager.currentPage]

    Column(Modifier.fillMaxSize().background(colors.surfaceBase)) {
        Row(
            Modifier
                .fillMaxWidth()
                .padding(
                    horizontal = if (compact) ManisDimens.spaceLg else ManisDimens.spaceXl,
                    vertical = ManisDimens.spaceLg,
                ),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
        ) {
            Column(Modifier.weight(1f)) {
                Text("配置", style = ManisType.pageTitle, color = colors.textPrimary)
                ManisMeta(active.detail)
            }
            Box(
                Modifier
                    .clip(RoundedCornerShape(ManisDimens.radiusControl))
                    .background(colors.surfaceLow)
                    .padding(horizontal = ManisDimens.spaceMd, vertical = ManisDimens.spaceXs),
            ) {
                Text(active.label, style = ManisType.metadata, color = colors.textSecondary)
            }
        }
        ManisDivider()
        if (compact) {
            ScrollableTabRow(
                selectedTabIndex = pager.currentPage,
                edgePadding = ManisDimens.spaceLg,
                containerColor = colors.surfaceBase,
                contentColor = colors.actionPrimary,
                divider = {},
            ) {
                ConfigSection.entries.forEachIndexed { index, entry ->
                    Tab(
                        selected = index == pager.currentPage,
                        onClick = { scope.launch { pager.animateScrollToPage(index) } },
                        text = { Text(entry.label, style = ManisType.label, maxLines = 1) },
                    )
                }
            }
            HorizontalPager(state = pager, modifier = Modifier.weight(1f)) { page ->
                SectionPane(
                    entry = ConfigSection.entries[page],
                    onMessage = onMessage,
                    onOpenLogs = onOpenLogs,
                )
            }
        } else {
            Row(Modifier.fillMaxWidth().weight(1f)) {
                SectionSidebar(active = active) { entry ->
                    scope.launch { pager.scrollToPage(ConfigSection.entries.indexOf(entry)) }
                }
                Box(
                    Modifier
                        .fillMaxHeight()
                        .width(1.dp)
                        .background(colors.outlineSubtle.copy(alpha = 0.5f)),
                )
                SectionPane(
                    entry = active,
                    onMessage = onMessage,
                    onOpenLogs = onOpenLogs,
                    modifier = Modifier.fillMaxHeight().weight(1f),
                )
            }
        }
    }
}

/** One section: its own scroll container, so paging never mixes two sections. */
@Composable
private fun SectionPane(
    entry: ConfigSection,
    onMessage: (String) -> Unit,
    onOpenLogs: () -> Unit,
    modifier: Modifier = Modifier.fillMaxSize(),
) {
    Column(
        modifier
            .verticalScroll(rememberScrollState())
            .padding(ManisDimens.spaceLg),
        verticalArrangement = Arrangement.spacedBy(ManisDimens.spaceMd),
    ) {
        when (entry) {
            ConfigSection.General -> GeneralSection(onMessage)
            ConfigSection.Runtime -> RuntimeSection(onMessage)
            ConfigSection.ProxySources -> ProxySourcesContent(onMessage)
            ConfigSection.RuleSources -> RuleSourcesSection()
            ConfigSection.Advanced -> AdvancedSection(onMessage, onOpenLogs)
            ConfigSection.Updates -> UpdatesContent(onMessage)
        }
    }
}

/** The desktop section directory beside the content. */
@Composable
private fun SectionSidebar(active: ConfigSection, onSelect: (ConfigSection) -> Unit) {
    val colors = LocalManisColors.current
    Column(
        Modifier
            .width(228.dp)
            .fillMaxHeight()
            .background(colors.surfaceBase)
            .padding(ManisDimens.spaceMd),
        verticalArrangement = Arrangement.spacedBy(ManisDimens.spaceXs),
    ) {
        Text(
            "设置",
            style = ManisType.metadata,
            color = colors.textTertiary,
            modifier = Modifier.padding(horizontal = ManisDimens.spaceMd, vertical = ManisDimens.spaceSm),
        )
        for (entry in ConfigSection.entries) {
            val selected = entry == active
            Row(
                Modifier
                    .fillMaxWidth()
                    .height(ManisDimens.controlStandard)
                    .clip(RoundedCornerShape(ManisDimens.radiusRow))
                    .background(if (selected) colors.actionSoft else Color.Transparent)
                    .clickable { onSelect(entry) }
                    .padding(horizontal = ManisDimens.spaceMd),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    entry.label,
                    style = ManisType.label,
                    color = if (selected) colors.actionPrimary else colors.textPrimary,
                )
            }
        }
    }
}

@Composable
private fun PaneColumn(content: @Composable ColumnScope.() -> Unit) {
    Column(
        Modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(ManisDimens.spaceMd),
        content = content,
    )
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
    PaneColumn {
        ManisCard {
            Text("外观", style = ManisType.label, color = colors.textPrimary)
            val themeChoice = when (UiPrefs.dark.value) {
                true -> "dark"
                false -> "light"
                null -> "system"
            }
            ManisSelect(
                label = "主题",
                options = listOf("system", "light", "dark"),
                selected = themeChoice,
                display = { option ->
                    when (option) {
                        "light" -> "浅色"
                        "dark" -> "深色"
                        else -> "跟随系统"
                    }
                },
                onSelect = { option ->
                    UiPrefs.setDark(
                        when (option) {
                            "light" -> false
                            "dark" -> true
                            else -> null
                        },
                    )
                },
            )
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
    PaneColumn {
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
    PaneColumn {
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
    PaneColumn {
        ManisCard {
            Text("节点域名解析", style = ManisType.label, color = colors.textPrimary)
            ManisMeta(proxyDnsLabel(config))
            ManisSelect(
                label = "解析方式",
                options = listOf(PROXY_DNS_DOH, PROXY_DNS_SYSTEM),
                selected = config.proxyDns,
                display = { if (it == PROXY_DNS_DOH) "加密 DNS（DoH）" else "本机 DNS" },
                onSelect = { option ->
                    Workspace.update { it.copy(proxyDns = option) }
                    applyWorkspace(context)
                    onMessage(if (option == PROXY_DNS_DOH) "已切换为加密 DNS" else "已切换为本机 DNS")
                },
            )
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
