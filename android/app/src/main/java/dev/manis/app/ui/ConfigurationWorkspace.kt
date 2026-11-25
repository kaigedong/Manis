package dev.manis.app.ui

import android.content.ClipData
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInParent
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import kotlin.math.roundToInt
import kotlinx.coroutines.launch
import dev.manis.app.data.MobileConfig
import dev.manis.app.data.PROXY_DNS_DOH
import dev.manis.app.data.PROXY_DNS_SYSTEM
import dev.manis.app.data.Workspace

/** The configuration sections, in the order the quick-jump row lists them. */
private enum class ConfigSection(val key: String, val label: String) {
    General("general", "通用"),
    Runtime("runtime", "运行时"),
    ProxySources("proxy", "订阅与节点"),
    RuleSources("rule_sources", "规则集"),
    Advanced("advanced", "高级"),
    About("about", "关于"),
}

/**
 * The settings workspace: one scroll of grouped cards, under a horizontal row of quick-jump pills
 * that highlights whichever section the scroll is inside.
 */
@Composable
fun ConfigurationWorkspace(onMessage: (String) -> Unit, onOpenLogs: () -> Unit) {
    val colors = LocalManisColors.current
    val scope = rememberCoroutineScope()
    val scroll = rememberScrollState()
    val sectionTops = remember { mutableStateMapOf<ConfigSection, Int>() }
    var pendingTarget by remember { mutableStateOf<ConfigSection?>(null) }

    val requested = Workspace.requestedSection.value
    LaunchedEffect(requested) {
        if (requested != null) {
            ConfigSection.entries
                .firstOrNull { it.key == requested || (requested == "updates" && it == ConfigSection.About) }
                ?.let { pendingTarget = it }
            Workspace.requestedSection.value = null
        }
    }
    LaunchedEffect(sectionTops.size, pendingTarget) {
        val target = pendingTarget ?: return@LaunchedEffect
        val top = sectionTops[target] ?: return@LaunchedEffect
        scroll.scrollTo(top)
        pendingTarget = null
    }

    val active by remember {
        derivedStateOf {
            when {
                scroll.value >= scroll.maxValue - 4 -> ConfigSection.entries.last()
                else -> sectionTops.entries
                    .filter { it.value <= scroll.value + 24 }
                    .maxByOrNull { it.value }
                    ?.key
                    ?: ConfigSection.General
            }
        }
    }

    Column(Modifier.fillMaxSize().background(colors.surfaceBase)) {
        ManisPageHeader(title = "设置")
        // The jump row sits outside the scroll, so the current section stays visible while the
        // cards move under it.
        Row(
            Modifier
                .fillMaxWidth()
                .horizontalScroll(rememberScrollState())
                .padding(horizontal = ManisDimens.spaceLg),
            horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceXs),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            for (entry in ConfigSection.entries) {
                ManisChip(
                    label = entry.label,
                    selected = entry == active,
                    onClick = {
                        scope.launch {
                            scroll.animateScrollTo(sectionTops[entry] ?: 0)
                        }
                    },
                )
            }
        }
        Column(
            Modifier
                .fillMaxSize()
                .verticalScroll(scroll)
                .padding(
                    start = ManisDimens.spaceLg,
                    end = ManisDimens.spaceLg,
                    top = ManisDimens.spaceSm,
                    bottom = ManisDimens.spaceXl,
                ),
            verticalArrangement = Arrangement.spacedBy(ManisDimens.spaceLg),
        ) {
            for (entry in ConfigSection.entries) {
                Column(
                    Modifier
                        .fillMaxWidth()
                        .onGloballyPositioned { coordinates ->
                            sectionTops[entry] = coordinates.positionInParent().y.roundToInt()
                        },
                    verticalArrangement = Arrangement.spacedBy(ManisDimens.spaceMd),
                ) {
                    when (entry) {
                        ConfigSection.General -> GeneralSection()
                        ConfigSection.Runtime -> RuntimeSection(onMessage)
                        ConfigSection.ProxySources -> ProxySourcesContent(onMessage)
                        ConfigSection.RuleSources -> RuleSourcesSection()
                        ConfigSection.Advanced -> AdvancedSection(onMessage, onOpenLogs)
                        ConfigSection.About -> AboutContent(onMessage)
                    }
                }
            }
        }
    }
}

/**
 * One row of a settings card: a title, optional supporting text on the left and one value or
 * control on the right. Every row clears 52dp, so the grouped cards read as even steps.
 */
@Composable
internal fun SettingRow(
    title: String,
    modifier: Modifier = Modifier,
    meta: String? = null,
    value: String? = null,
    valueTone: Color? = null,
    onClick: (() -> Unit)? = null,
    trailing: (@Composable () -> Unit)? = null,
) {
    val colors = LocalManisColors.current
    Row(
        modifier
            .fillMaxWidth()
            .heightIn(min = 52.dp)
            .clip(RoundedCornerShape(ManisDimens.radiusRow))
            .then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceMd),
    ) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(
                title,
                style = ManisType.label,
                color = colors.textPrimary,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
            if (meta != null) ManisMeta(meta, maxLines = 2)
        }
        if (value != null) {
            Text(
                value,
                style = ManisType.data,
                color = valueTone ?: colors.textSecondary,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        trailing?.invoke()
    }
}

/** The general card: the theme choice, and the language the strings ship in. */
@Composable
private fun GeneralSection() {
    val colors = LocalManisColors.current
    val choice = when (UiPrefs.dark.value) {
        true -> 2
        false -> 1
        null -> 0
    }
    ManisCard {
        ManisSectionTitle("通用与外观")
        SettingRow("主题外观", meta = "本地保存，重启后保持")
        ManisSegmented(
            options = listOf("跟随系统", "浅色", "深色"),
            selectedIndex = choice,
            onSelect = { index ->
                UiPrefs.setDark(
                    when (index) {
                        1 -> false
                        2 -> true
                        else -> null
                    },
                )
            },
        )
        ManisDivider()
        SettingRow(
            "界面语言",
            value = "简体中文",
            valueTone = colors.textSecondary,
        )
    }
}

/** The runtime card: what the kernel is doing, and how to make it reload. */
@Composable
private fun RuntimeSection(onMessage: (String) -> Unit) {
    val colors = LocalManisColors.current
    val context = LocalContext.current
    val config = Workspace.config.value
    val abi = android.os.Build.SUPPORTED_ABIS.firstOrNull() ?: "unknown"
    ManisCard {
        ManisSectionTitle("运行内核")
        SettingRow(
            "连接状态",
            trailing = {
                ManisPill(
                    if (RuntimeState.active) "已连接" else "未连接",
                    if (RuntimeState.active) ManisTone.Success else ManisTone.Neutral,
                )
            },
            meta = if (RuntimeState.active) "流量经本机 VPN 转发" else "开启后在此显示内核状态",
        )
        ManisDivider()
        SettingRow("路由模式", value = modeLabel(RuntimeState.snapshot?.mode ?: config.mode))
        SettingRow("节点域名解析", value = proxyDnsLabel(config))
        ManisDivider()
        SettingRow("内核版本", value = RuntimeState.snapshot?.version?.ifBlank { "—" } ?: "—")
        SettingRow("工作区", value = "${config.subscriptions.size} 订阅 · ${config.nodes.size} 节点")
        SettingRow("策略组", value = "${config.groups.size} 组")
        SettingRow("设备 ABI", value = abi)
        SettingRow("数据目录", value = "files/mihomo")
        ManisMeta(RuntimeState.summary.ifBlank { "尚无内核摘要" })
        ManisButton("重新载入配置", onClick = {
            applyWorkspace(context)
            onMessage("已重新载入配置")
        })
    }
}

/** Rule sources are not supported on Android yet; the card says so instead of hiding the section. */
@Composable
private fun RuleSourcesSection() {
    ManisCard {
        ManisSectionTitle("规则集")
        ManisMeta("Android 版暂不支持远程规则集；规则在「分流规则」页里手动维护。")
    }
}

/** DNS resolution, the backup format and the kernel log entry point. */
@Composable
private fun AdvancedSection(onMessage: (String) -> Unit, onOpenLogs: () -> Unit) {
    val colors = LocalManisColors.current
    val context = LocalContext.current
    val config = Workspace.config.value
    ManisCard {
        ManisSectionTitle("节点域名解析")
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
        ManisSectionTitle("备份与恢复")
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
        ManisSectionTitle("诊断")
        Text(
            "内核日志会记录每次配置载入与连接错误。",
            style = ManisType.metadata,
            color = colors.textTertiary,
        )
        ManisButton("查看内核日志", onClick = onOpenLogs)
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
