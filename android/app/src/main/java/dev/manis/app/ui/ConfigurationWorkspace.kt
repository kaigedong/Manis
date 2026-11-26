package dev.manis.app.ui

import android.content.ClipData
import androidx.activity.compose.BackHandler
import androidx.annotation.DrawableRes
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.manis.app.R
import dev.manis.app.data.MobileConfig
import dev.manis.app.data.PROXY_DNS_DOH
import dev.manis.app.data.PROXY_DNS_SYSTEM
import dev.manis.app.data.UpdateChecker
import dev.manis.app.data.UpdateManager
import dev.manis.app.data.UpdateState
import dev.manis.app.data.Workspace

/** Drill-down destinations reachable from the Settings root list. */
private enum class SettingsPage(val title: String) {
    Root("设置"),
    ProxySources("订阅与节点"),
    Runtime("路由与内核"),
    Updates("应用更新"),
}

/** Lightweight sheets for single-pick or compact utility actions. */
private enum class SettingsModal {
    Appearance,
    Dns,
    Backup,
    About,
}

/**
 * The settings workspace: a calm inset-grouped root list with a top status hero card, drill-down
 * subpages for richer sections (subscriptions/nodes, runtime/routing, updates), and bottom sheets
 * for quick choices (appearance, DNS, backup, about).
 */
@Composable
fun ConfigurationWorkspace(
    connected: Boolean,
    mode: String,
    onToggleConnection: () -> Unit,
    onModeSelect: (String) -> Unit,
    onMessage: (String) -> Unit,
    onOpenLogs: () -> Unit,
) {
    val colors = LocalManisColors.current
    var page by rememberSaveable { mutableStateOf(SettingsPage.Root) }
    var modal by remember { mutableStateOf<SettingsModal?>(null) }

    val requested = Workspace.requestedSection.value
    LaunchedEffect(requested) {
        if (requested != null) {
            when (requested) {
                "proxy" -> page = SettingsPage.ProxySources
                "runtime" -> page = SettingsPage.Runtime
                "updates" -> page = SettingsPage.Updates
                "about" -> modal = SettingsModal.About
                else -> page = SettingsPage.Root
            }
            Workspace.requestedSection.value = null
        }
    }

    BackHandler(enabled = page != SettingsPage.Root) {
        page = SettingsPage.Root
    }

    AnimatedContent(
        targetState = page,
        modifier = Modifier
            .fillMaxSize()
            .background(colors.surfaceBase),
        transitionSpec = {
            if (targetState == SettingsPage.Root) {
                (slideInHorizontally(tween(220)) { -it / 5 } + fadeIn(tween(180))) togetherWith
                    (slideOutHorizontally(tween(220)) { it / 4 } + fadeOut(tween(150)))
            } else {
                (slideInHorizontally(tween(220)) { it / 4 } + fadeIn(tween(180))) togetherWith
                    (slideOutHorizontally(tween(220)) { -it / 5 } + fadeOut(tween(150)))
            }
        },
        label = "settings-navigation",
    ) { target ->
        when (target) {
            SettingsPage.Root -> SettingsRootScreen(
                connected = connected,
                mode = mode,
                onToggleConnection = onToggleConnection,
                onOpenPage = { page = it },
                onOpenModal = { modal = it },
                onOpenLogs = onOpenLogs,
                onMessage = onMessage,
            )
            SettingsPage.ProxySources -> ProxySourcesSubpage(
                onBack = { page = SettingsPage.Root },
                onMessage = onMessage,
            )
            SettingsPage.Runtime -> RuntimeSubpage(
                connected = connected,
                mode = mode,
                onModeSelect = onModeSelect,
                onBack = { page = SettingsPage.Root },
                onMessage = onMessage,
            )
            SettingsPage.Updates -> UpdatesSubpage(
                onBack = { page = SettingsPage.Root },
                onMessage = onMessage,
            )
        }
    }

    when (modal) {
        SettingsModal.Appearance -> AppearanceSheet(onDismiss = { modal = null })
        SettingsModal.Dns -> DnsSheet(
            onDismiss = { modal = null },
            onMessage = onMessage,
        )
        SettingsModal.Backup -> BackupSheet(
            onDismiss = { modal = null },
            onMessage = onMessage,
        )
        SettingsModal.About -> AboutSheet(
            onDismiss = { modal = null },
            onMessage = onMessage,
        )
        null -> Unit
    }
}

@Composable
private fun SettingsRootScreen(
    connected: Boolean,
    mode: String,
    onToggleConnection: () -> Unit,
    onOpenPage: (SettingsPage) -> Unit,
    onOpenModal: (SettingsModal) -> Unit,
    onOpenLogs: () -> Unit,
    onMessage: (String) -> Unit,
) {
    val colors = LocalManisColors.current
    val context = LocalContext.current
    val config = Workspace.config.value
    val pending = Workspace.pending.value
    val updateState by UpdateManager.state.collectAsState()
    val currentVersion = remember { UpdateChecker.installedVersion(context).ifBlank { "未知" } }
    val enabledSubs = config.subscriptions.count { it.enabled }
    val enabledNodes = config.nodes.count { it.enabled }
    val hasSources = enabledSubs > 0 || enabledNodes > 0

    val activeExit = remember(RuntimeState.snapshot) {
        displayProxyName(
            RuntimeState.snapshot?.groups
                ?.firstOrNull { it.name == "__MANIS_GLOBAL__" }
                ?.now,
        )
    }

    val sourcesSummary = when {
        pending -> "有未应用更改"
        config.subscriptions.isEmpty() && config.nodes.isEmpty() -> "未添加"
        else -> "${config.subscriptions.size} 订阅 · ${config.nodes.size} 节点"
    }

    val kernelVersion = RuntimeState.snapshot?.version?.ifBlank { null }
    val runtimeSummary = if (kernelVersion != null) {
        "${modeLabel(mode)} · $kernelVersion"
    } else {
        "${modeLabel(mode)}分流"
    }

    val (updateLabel, updateTone) = updateSummary(updateState, colors)

    Column(Modifier.fillMaxSize()) {
        SettingsTopBar(title = SettingsPage.Root.title)
        Box(Modifier.fillMaxSize(), contentAlignment = Alignment.TopCenter) {
            LazyColumn(
                modifier = Modifier
                    .fillMaxSize()
                    .widthIn(max = 640.dp),
                contentPadding = PaddingValues(
                    start = ManisDimens.spaceLg,
                    end = ManisDimens.spaceLg,
                    top = ManisDimens.spaceSm,
                    bottom = ManisDimens.space2xl,
                ),
                verticalArrangement = Arrangement.spacedBy(20.dp),
            ) {
                item(key = "hero") {
                    Column(verticalArrangement = Arrangement.spacedBy(ManisDimens.spaceMd)) {
                        SettingsStatusHero(
                            connected = connected,
                            hasSources = hasSources,
                            mode = mode,
                            activeExit = activeExit,
                            enabledSubs = enabledSubs,
                            enabledNodes = enabledNodes,
                            onAction = {
                                if (!hasSources && !connected) {
                                    onOpenPage(SettingsPage.ProxySources)
                                } else {
                                    onToggleConnection()
                                }
                            },
                        )
                        if (pending) {
                            PendingApplyBanner(
                                onApply = {
                                    applyWorkspace(context)
                                    onMessage("已应用配置更改")
                                },
                            )
                        }
                    }
                }

                item(key = "network") {
                    SettingsGroup(
                        header = "代理与网络",
                        footer = "代理规则与分流顺序可在「规则」页维护；远程规则集暂未开放。",
                    ) {
                        SettingsEntryRow(
                            iconRes = R.drawable.ic_layers,
                            title = "订阅与节点",
                            value = sourcesSummary,
                            valueColor = if (pending) colors.actionPrimary else colors.textTertiary,
                            onClick = { onOpenPage(SettingsPage.ProxySources) },
                        )
                        SettingsInsetDivider()
                        SettingsEntryRow(
                            iconRes = R.drawable.ic_sliders,
                            title = "路由与内核",
                            value = runtimeSummary,
                            onClick = { onOpenPage(SettingsPage.Runtime) },
                        )
                        SettingsInsetDivider()
                        SettingsEntryRow(
                            iconRes = R.drawable.ic_globe,
                            title = "节点域名解析",
                            value = proxyDnsShortLabel(config),
                            onClick = { onOpenModal(SettingsModal.Dns) },
                        )
                    }
                }

                item(key = "preferences") {
                    SettingsGroup(header = "外观与偏好") {
                        SettingsEntryRow(
                            iconRes = R.drawable.ic_palette,
                            title = "外观",
                            value = appearanceLabel(UiPrefs.dark.value),
                            onClick = { onOpenModal(SettingsModal.Appearance) },
                        )
                        SettingsInsetDivider()
                        SettingsEntryRow(
                            iconRes = R.drawable.ic_language,
                            title = "语言",
                            value = "简体中文",
                            onClick = null,
                        )
                    }
                }

                item(key = "data") {
                    SettingsGroup(header = "数据与诊断") {
                        SettingsEntryRow(
                            iconRes = R.drawable.ic_backup,
                            title = "备份与恢复",
                            value = "剪贴板",
                            onClick = { onOpenModal(SettingsModal.Backup) },
                        )
                        SettingsInsetDivider()
                        SettingsEntryRow(
                            iconRes = R.drawable.ic_terminal,
                            title = "内核日志",
                            onClick = onOpenLogs,
                        )
                    }
                }

                item(key = "about") {
                    SettingsGroup(header = "关于") {
                        SettingsEntryRow(
                            iconRes = R.drawable.ic_download,
                            title = "应用更新",
                            value = updateLabel,
                            valueColor = updateTone,
                            onClick = { onOpenPage(SettingsPage.Updates) },
                        )
                        SettingsInsetDivider()
                        SettingsEntryRow(
                            iconRes = R.drawable.ic_info,
                            title = "关于 Manis",
                            value = currentVersion,
                            onClick = { onOpenModal(SettingsModal.About) },
                        )
                    }
                }
            }
        }
    }
}

/** Top status hero card modeled after calm inset settings hubs. */
@Composable
private fun SettingsStatusHero(
    connected: Boolean,
    hasSources: Boolean,
    mode: String,
    activeExit: String?,
    enabledSubs: Int,
    enabledNodes: Int,
    onAction: () -> Unit,
) {
    val colors = LocalManisColors.current
    val dark = colors.surfaceBase.luminance() < 0.5f
    val badgeBg = when {
        connected && dark -> colors.statusSuccessSoft
        connected -> colors.statusSuccess
        dark -> colors.actionSoft
        else -> colors.actionPrimary
    }
    val badgeTint = when {
        connected && dark -> colors.statusSuccess
        connected -> colors.actionOnPrimary
        dark -> colors.actionPrimary
        else -> colors.actionOnPrimary
    }
    val title = if (connected) "Manis 已连接" else "Manis 未连接"
    val subtitle = when {
        connected -> "${modeLabel(mode)}分流 · ${activeExit ?: "本机 VPN"}"
        hasSources -> "$enabledSubs 个订阅 · $enabledNodes 个节点"
        else -> "添加订阅或节点后开启"
    }
    val actionLabel = when {
        connected -> "断开"
        hasSources -> "开启"
        else -> "去添加"
    }

    SettingsInsetCard {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = ManisDimens.spaceLg, vertical = 16.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            Box(
                modifier = Modifier
                    .size(46.dp)
                    .clip(RoundedCornerShape(13.dp))
                    .background(badgeBg),
                contentAlignment = Alignment.Center,
            ) {
                Icon(
                    painter = painterResource(R.drawable.ic_power),
                    contentDescription = null,
                    modifier = Modifier.size(22.dp),
                    tint = badgeTint,
                )
            }
            Column(
                modifier = Modifier.weight(1f),
                verticalArrangement = Arrangement.spacedBy(3.dp),
            ) {
                Text(
                    title,
                    style = ManisType.sectionTitle.copy(fontSize = 16.5.sp),
                    color = colors.textPrimary,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    subtitle,
                    style = ManisType.metadata.copy(fontSize = 13.sp),
                    color = colors.textTertiary,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            Box(
                modifier = Modifier
                    .height(40.dp)
                    .widthIn(min = 72.dp)
                    .clip(CircleShape)
                    .background(if (dark) colors.surfaceHigh else colors.surfaceLow)
                    .clickable(onClick = onAction)
                    .padding(horizontal = 16.dp),
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    actionLabel,
                    style = ManisType.label.copy(fontWeight = FontWeight.SemiBold),
                    color = colors.textPrimary,
                    maxLines = 1,
                )
            }
        }
    }
}

@Composable
private fun PendingApplyBanner(onApply: () -> Unit) {
    val colors = LocalManisColors.current
    Surface(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(16.dp),
        color = colors.actionSoft,
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(start = ManisDimens.spaceLg, end = ManisDimens.spaceMd, top = 10.dp, bottom = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
        ) {
            Text(
                "配置有未应用的更改",
                style = ManisType.label,
                color = colors.actionPrimary,
                modifier = Modifier.weight(1f),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            ManisButton("应用", primary = true, onClick = onApply)
        }
    }
}

@Composable
private fun ProxySourcesSubpage(
    onBack: () -> Unit,
    onMessage: (String) -> Unit,
) {
    val context = LocalContext.current
    val pending = Workspace.pending.value

    Column(Modifier.fillMaxSize()) {
        SettingsTopBar(
            title = SettingsPage.ProxySources.title,
            onBack = onBack,
            trailing = {
                if (pending) {
                    ManisButton("应用", primary = true, onClick = {
                        applyWorkspace(context)
                        onMessage("已应用工作区")
                    })
                }
            },
        )
        Box(Modifier.fillMaxSize(), contentAlignment = Alignment.TopCenter) {
            LazyColumn(
                modifier = Modifier
                    .fillMaxSize()
                    .widthIn(max = 640.dp),
                contentPadding = PaddingValues(
                    start = ManisDimens.spaceLg,
                    end = ManisDimens.spaceLg,
                    top = ManisDimens.spaceSm,
                    bottom = ManisDimens.space2xl,
                ),
            ) {
                item(key = "proxy-sources") {
                    ProxySourcesContent(onMessage = onMessage, showPendingBanner = false)
                }
            }
        }
    }
}

@Composable
private fun RuntimeSubpage(
    connected: Boolean,
    mode: String,
    onModeSelect: (String) -> Unit,
    onBack: () -> Unit,
    onMessage: (String) -> Unit,
) {
    val colors = LocalManisColors.current
    val context = LocalContext.current
    val config = Workspace.config.value
    val abi = remember { android.os.Build.SUPPORTED_ABIS.firstOrNull() ?: "unknown" }

    Column(Modifier.fillMaxSize()) {
        SettingsTopBar(title = SettingsPage.Runtime.title, onBack = onBack)
        Box(Modifier.fillMaxSize(), contentAlignment = Alignment.TopCenter) {
            LazyColumn(
                modifier = Modifier
                    .fillMaxSize()
                    .widthIn(max = 640.dp),
                contentPadding = PaddingValues(
                    start = ManisDimens.spaceLg,
                    end = ManisDimens.spaceLg,
                    top = ManisDimens.spaceSm,
                    bottom = ManisDimens.space2xl,
                ),
                verticalArrangement = Arrangement.spacedBy(20.dp),
            ) {
                item(key = "routing-mode") {
                    SettingsGroup(header = "路由模式") {
                        SettingsChoiceRow(
                            title = "规则分流",
                            subtitle = "按规则匹配直连或代理（推荐）",
                            selected = mode != "global" && mode != "direct",
                            onClick = { onModeSelect("rule") },
                        )
                        SettingsInsetDivider(startIndent = ManisDimens.spaceLg)
                        SettingsChoiceRow(
                            title = "全局代理",
                            subtitle = "所有流量经所选出口节点转发",
                            selected = mode == "global",
                            onClick = { onModeSelect("global") },
                        )
                        SettingsInsetDivider(startIndent = ManisDimens.spaceLg)
                        SettingsChoiceRow(
                            title = "直连模式",
                            subtitle = "全部流量直接连接，不经过代理节点",
                            selected = mode == "direct",
                            onClick = { onModeSelect("direct") },
                        )
                    }
                }

                item(key = "kernel-info") {
                    SettingsGroup(
                        header = "运行状态",
                        footer = RuntimeState.summary.ifBlank { "开启代理后将在此展示内核运行摘要。" },
                    ) {
                        SettingsValueRow(
                            title = "连接状态",
                            trailing = {
                                ManisPill(
                                    if (connected) "已连接" else "未连接",
                                    if (connected) ManisTone.Success else ManisTone.Neutral,
                                )
                            },
                        )
                        SettingsInsetDivider(startIndent = ManisDimens.spaceLg)
                        SettingsValueRow(
                            title = "内核版本",
                            value = RuntimeState.snapshot?.version?.ifBlank { "—" } ?: "—",
                        )
                        SettingsInsetDivider(startIndent = ManisDimens.spaceLg)
                        SettingsValueRow(
                            title = "工作区",
                            value = "${config.subscriptions.size} 订阅 · ${config.nodes.size} 节点",
                        )
                        SettingsInsetDivider(startIndent = ManisDimens.spaceLg)
                        SettingsValueRow(
                            title = "策略组",
                            value = "${config.groups.size} 组",
                        )
                        SettingsInsetDivider(startIndent = ManisDimens.spaceLg)
                        SettingsValueRow(
                            title = "设备 ABI",
                            value = abi,
                        )
                        SettingsInsetDivider(startIndent = ManisDimens.spaceLg)
                        SettingsValueRow(
                            title = "数据目录",
                            value = "files/mihomo",
                        )
                    }
                }

                item(key = "reload") {
                    SettingsGroup(header = "操作") {
                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .heightIn(min = 54.dp)
                                .clickable {
                                    applyWorkspace(context)
                                    onMessage("已重新载入配置")
                                }
                                .padding(horizontal = ManisDimens.spaceLg),
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.Center,
                        ) {
                            Text(
                                "重新载入配置",
                                style = ManisType.body.copy(fontWeight = FontWeight.Medium),
                                color = colors.actionPrimary,
                            )
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun UpdatesSubpage(
    onBack: () -> Unit,
    onMessage: (String) -> Unit,
) {
    Column(Modifier.fillMaxSize()) {
        SettingsTopBar(title = SettingsPage.Updates.title, onBack = onBack)
        Box(Modifier.fillMaxSize(), contentAlignment = Alignment.TopCenter) {
            LazyColumn(
                modifier = Modifier
                    .fillMaxSize()
                    .widthIn(max = 640.dp),
                contentPadding = PaddingValues(
                    start = ManisDimens.spaceLg,
                    end = ManisDimens.spaceLg,
                    top = ManisDimens.spaceSm,
                    bottom = ManisDimens.space2xl,
                ),
            ) {
                item(key = "updates") {
                    UpdatesContent(onMessage = onMessage)
                }
            }
        }
    }
}

@Composable
private fun AppearanceSheet(onDismiss: () -> Unit) {
    val current = UiPrefs.dark.value
    SettingsBottomSheet(
        title = "外观",
        subtitle = "本地保存，重启后自动保持",
        onDismiss = onDismiss,
    ) {
        SettingsInsetCard {
            SettingsChoiceRow(
                title = "跟随系统",
                subtitle = "自动匹配系统浅色与深色模式",
                selected = current == null,
                onClick = {
                    UiPrefs.setDark(null)
                    onDismiss()
                },
            )
            SettingsInsetDivider(startIndent = ManisDimens.spaceLg)
            SettingsChoiceRow(
                title = "浅色",
                selected = current == false,
                onClick = {
                    UiPrefs.setDark(false)
                    onDismiss()
                },
            )
            SettingsInsetDivider(startIndent = ManisDimens.spaceLg)
            SettingsChoiceRow(
                title = "深色",
                selected = current == true,
                onClick = {
                    UiPrefs.setDark(true)
                    onDismiss()
                },
            )
        }
    }
}

@Composable
private fun DnsSheet(
    onDismiss: () -> Unit,
    onMessage: (String) -> Unit,
) {
    val context = LocalContext.current
    val config = Workspace.config.value
    val activeHint = when {
        config.proxyDnsServers.isNotEmpty() -> "当前已配置 ${config.proxyDnsServers.size} 条自定义 DoH 服务器。"
        config.discoveredDns.isNotEmpty() -> "当前正在使用订阅自动发现的 ${config.discoveredDns.size} 条机场 DNS。"
        else -> null
    }
    SettingsBottomSheet(
        title = "节点域名解析",
        subtitle = activeHint ?: "选择解析代理节点服务器域名的方式",
        onDismiss = onDismiss,
    ) {
        SettingsInsetCard {
            SettingsChoiceRow(
                title = "加密 DNS（DoH）",
                subtitle = "通过加密连接解析节点域名，减少 DNS 污染",
                selected = config.proxyDns == PROXY_DNS_DOH,
                onClick = {
                    if (config.proxyDns != PROXY_DNS_DOH) {
                        Workspace.update { it.copy(proxyDns = PROXY_DNS_DOH) }
                        applyWorkspace(context)
                        onMessage("已切换为加密 DNS")
                    }
                    onDismiss()
                },
            )
            SettingsInsetDivider(startIndent = ManisDimens.spaceLg)
            SettingsChoiceRow(
                title = "本机 DNS",
                subtitle = "使用系统当前网络提供的 DNS 解析",
                selected = config.proxyDns == PROXY_DNS_SYSTEM,
                onClick = {
                    if (config.proxyDns != PROXY_DNS_SYSTEM) {
                        Workspace.update { it.copy(proxyDns = PROXY_DNS_SYSTEM) }
                        applyWorkspace(context)
                        onMessage("已切换为本机 DNS")
                    }
                    onDismiss()
                },
            )
        }
    }
}

@Composable
private fun BackupSheet(
    onDismiss: () -> Unit,
    onMessage: (String) -> Unit,
) {
    val context = LocalContext.current
    val colors = LocalManisColors.current
    SettingsBottomSheet(
        title = "备份与恢复",
        subtitle = "通过系统剪贴板导出或还原 Manis 配置",
        onDismiss = onDismiss,
    ) {
        SettingsInsetCard {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .heightIn(min = 56.dp)
                    .clickable {
                        val clipboard = context.getSystemService(android.content.ClipboardManager::class.java)
                        clipboard?.setPrimaryClip(
                            ClipData.newPlainText("Manis 配置", Workspace.config.value.toJson()),
                        )
                        onMessage("配置已复制到剪贴板")
                        onDismiss()
                    }
                    .padding(horizontal = ManisDimens.spaceLg, vertical = 12.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceMd),
            ) {
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                    Text(
                        "导出到剪贴板",
                        style = ManisType.body.copy(fontWeight = FontWeight.Medium),
                        color = colors.textPrimary,
                    )
                    ManisMeta("复制完整订阅、节点、策略组与规则备份")
                }
                Icon(
                    painter = painterResource(R.drawable.ic_chevron_right),
                    contentDescription = null,
                    modifier = Modifier.size(16.dp),
                    tint = colors.textTertiary.copy(alpha = 0.6f),
                )
            }
            SettingsInsetDivider(startIndent = ManisDimens.spaceLg)
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .heightIn(min = 56.dp)
                    .clickable {
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
                        onDismiss()
                    }
                    .padding(horizontal = ManisDimens.spaceLg, vertical = 12.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceMd),
            ) {
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                    Text(
                        "从剪贴板导入",
                        style = ManisType.body.copy(fontWeight = FontWeight.Medium),
                        color = colors.textPrimary,
                    )
                    ManisMeta("校验通过后立即覆盖并重载配置")
                }
                Icon(
                    painter = painterResource(R.drawable.ic_chevron_right),
                    contentDescription = null,
                    modifier = Modifier.size(16.dp),
                    tint = colors.textTertiary.copy(alpha = 0.6f),
                )
            }
        }
        Text(
            "导出的是 Manis 备份格式（含订阅链接和节点密钥），不是交给其他客户端的 YAML。",
            style = ManisType.metadata.copy(fontSize = 12.5.sp, lineHeight = 18.sp),
            color = colors.textTertiary,
            modifier = Modifier.padding(horizontal = ManisDimens.spaceXs),
        )
    }
}

@Composable
private fun AboutSheet(
    onDismiss: () -> Unit,
    onMessage: (String) -> Unit,
) {
    SettingsBottomSheet(
        title = "关于 Manis",
        onDismiss = onDismiss,
    ) {
        AboutSheetContent(onMessage = onMessage)
    }
}

/**
 * Centered top bar used by the settings root and its drill-down subpages.
 */
@Composable
internal fun SettingsTopBar(
    title: String,
    onBack: (() -> Unit)? = null,
    trailing: @Composable RowScope.() -> Unit = {},
) {
    val colors = LocalManisColors.current
    Box(
        modifier = Modifier
            .fillMaxWidth()
            .height(52.dp)
            .padding(horizontal = ManisDimens.spaceSm),
    ) {
        if (onBack != null) {
            Box(
                modifier = Modifier
                    .align(Alignment.CenterStart)
                    .size(ManisDimens.minTouch)
                    .clip(CircleShape)
                    .clickable(onClick = onBack),
                contentAlignment = Alignment.Center,
            ) {
                Icon(
                    imageVector = Icons.AutoMirrored.Filled.ArrowBack,
                    contentDescription = "返回",
                    modifier = Modifier.size(22.dp),
                    tint = colors.textPrimary,
                )
            }
        }
        Text(
            title,
            modifier = Modifier
                .align(Alignment.Center)
                .padding(horizontal = 56.dp),
            style = ManisType.sectionTitle.copy(fontSize = 18.sp, fontWeight = FontWeight.SemiBold),
            color = colors.textPrimary,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        Row(
            modifier = Modifier
                .align(Alignment.CenterEnd)
                .padding(end = ManisDimens.spaceSm),
            verticalAlignment = Alignment.CenterVertically,
            content = trailing,
        )
    }
}

/**
 * A calm inset card: crisp elevated white floating on alabaster in light mode without heavy
 * borders, and an elevated dark surface with a hairline outline in dark mode.
 */
@Composable
internal fun SettingsInsetCard(
    modifier: Modifier = Modifier,
    content: @Composable ColumnScope.() -> Unit,
) {
    val colors = LocalManisColors.current
    val dark = colors.surfaceBase.luminance() < 0.5f
    Surface(
        modifier = modifier.fillMaxWidth(),
        shape = RoundedCornerShape(18.dp),
        color = colors.surfaceElevated,
        border = if (dark) BorderStroke(0.5.dp, colors.outlineSubtle) else null,
    ) {
        Column(
            modifier = Modifier.fillMaxWidth(),
            content = content,
        )
    }
}

/**
 * Grouped settings block: optional muted header outside the card, clean rounded card body, and
 * optional muted explanatory footer outside the card.
 */
@Composable
internal fun SettingsGroup(
    header: String? = null,
    footer: String? = null,
    headerAction: (@Composable RowScope.() -> Unit)? = null,
    content: @Composable ColumnScope.() -> Unit,
) {
    val colors = LocalManisColors.current
    Column(Modifier.fillMaxWidth()) {
        if (header != null || headerAction != null) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(start = 12.dp, end = 4.dp, bottom = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                if (header != null) {
                    Text(
                        header,
                        modifier = Modifier.weight(1f),
                        style = ManisType.metadata.copy(
                            fontSize = 13.sp,
                            fontWeight = FontWeight.Medium,
                        ),
                        color = colors.textTertiary,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                } else {
                    Spacer(Modifier.weight(1f))
                }
                headerAction?.invoke(this)
            }
        }
        SettingsInsetCard(content = content)
        if (footer != null) {
            Text(
                footer,
                modifier = Modifier.padding(start = 12.dp, end = 12.dp, top = 8.dp),
                style = ManisType.metadata.copy(fontSize = 12.5.sp, lineHeight = 18.sp),
                color = colors.textTertiary,
            )
        }
    }
}

@Composable
internal fun SettingsInsetDivider(startIndent: Dp = 60.dp) {
    val colors = LocalManisColors.current
    HorizontalDivider(
        modifier = Modifier.padding(start = startIndent, end = ManisDimens.spaceLg),
        thickness = 0.5.dp,
        color = colors.outlineSubtle.copy(alpha = 0.75f),
    )
}

/**
 * A single-line navigation row inside a [SettingsGroup]: uniform 32dp rounded-square brand icon
 * badge on the left, title, optional right summary value, and subtle disclosure chevron.
 */
@Composable
private fun SettingsEntryRow(
    @DrawableRes iconRes: Int,
    title: String,
    value: String? = null,
    valueColor: Color? = null,
    onClick: (() -> Unit)?,
) {
    val colors = LocalManisColors.current
    val dark = colors.surfaceBase.luminance() < 0.5f
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = 56.dp)
            .then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
            .padding(horizontal = ManisDimens.spaceLg, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Box(
            modifier = Modifier
                .size(32.dp)
                .clip(RoundedCornerShape(9.dp))
                .background(if (dark) colors.actionSoft else colors.actionPrimary),
            contentAlignment = Alignment.Center,
        ) {
            Icon(
                painter = painterResource(iconRes),
                contentDescription = null,
                modifier = Modifier.size(17.dp),
                tint = if (dark) colors.actionPrimary else colors.actionOnPrimary,
            )
        }
        Text(
            title,
            modifier = Modifier.weight(1f),
            style = ManisType.body.copy(fontSize = 15.5.sp, fontWeight = FontWeight.Normal),
            color = colors.textPrimary,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        if (!value.isNullOrBlank()) {
            Text(
                value,
                modifier = Modifier.widthIn(max = 160.dp),
                style = ManisType.body.copy(fontSize = 14.sp),
                color = valueColor ?: colors.textTertiary,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        if (onClick != null) {
            Icon(
                painter = painterResource(R.drawable.ic_chevron_right),
                contentDescription = null,
                modifier = Modifier.size(16.dp),
                tint = colors.textTertiary.copy(alpha = 0.6f),
            )
        }
    }
}

/** Key-value detail row used inside settings subpages (runtime info, updates, about). */
@Composable
internal fun SettingsValueRow(
    title: String,
    value: String? = null,
    valueColor: Color? = null,
    trailing: (@Composable () -> Unit)? = null,
) {
    val colors = LocalManisColors.current
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = 52.dp)
            .padding(horizontal = ManisDimens.spaceLg, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceMd),
    ) {
        Text(
            title,
            modifier = Modifier.weight(1f),
            style = ManisType.body.copy(fontSize = 15.sp),
            color = colors.textPrimary,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        if (value != null) {
            Text(
                value,
                style = ManisType.data,
                color = valueColor ?: colors.textSecondary,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        trailing?.invoke()
    }
}

/** Single-select radio row with checkmark used in sheets and subpages. */
@Composable
private fun SettingsChoiceRow(
    title: String,
    selected: Boolean,
    subtitle: String? = null,
    onClick: () -> Unit,
) {
    val colors = LocalManisColors.current
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = 54.dp)
            .clickable(onClick = onClick)
            .padding(horizontal = ManisDimens.spaceLg, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceMd),
    ) {
        Column(
            modifier = Modifier.weight(1f),
            verticalArrangement = Arrangement.spacedBy(2.dp),
        ) {
            Text(
                title,
                style = ManisType.body.copy(
                    fontSize = 15.sp,
                    fontWeight = if (selected) FontWeight.SemiBold else FontWeight.Normal,
                ),
                color = if (selected) colors.actionPrimary else colors.textPrimary,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            if (subtitle != null) {
                ManisMeta(subtitle, maxLines = 2)
            }
        }
        if (selected) {
            Icon(
                painter = painterResource(R.drawable.ic_check),
                contentDescription = "已选中",
                modifier = Modifier.size(18.dp),
                tint = colors.actionPrimary,
            )
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun SettingsBottomSheet(
    title: String,
    subtitle: String? = null,
    onDismiss: () -> Unit,
    content: @Composable ColumnScope.() -> Unit,
) {
    val colors = LocalManisColors.current
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheetState,
        shape = RoundedCornerShape(
            topStart = ManisDimens.radiusSheet,
            topEnd = ManisDimens.radiusSheet,
        ),
        containerColor = colors.surfaceBase,
        scrimColor = colors.scrim,
        dragHandle = {
            Box(
                Modifier
                    .padding(top = ManisDimens.spaceMd, bottom = ManisDimens.spaceXs)
                    .size(width = 36.dp, height = 4.dp)
                    .clip(RoundedCornerShape(9999.dp))
                    .background(colors.outlineStrong.copy(alpha = 0.45f)),
            )
        },
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .windowInsetsPadding(WindowInsets.navigationBars)
                .padding(
                    start = ManisDimens.spaceLg,
                    end = ManisDimens.spaceLg,
                    top = ManisDimens.spaceXs,
                    bottom = ManisDimens.spaceXl,
                ),
            verticalArrangement = Arrangement.spacedBy(ManisDimens.spaceMd),
        ) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Column(
                    modifier = Modifier
                        .weight(1f)
                        .padding(start = ManisDimens.spaceXs),
                    verticalArrangement = Arrangement.spacedBy(2.dp),
                ) {
                    Text(
                        title,
                        style = ManisType.dialogTitle.copy(fontSize = 18.sp),
                        color = colors.textPrimary,
                    )
                    if (subtitle != null) {
                        ManisMeta(subtitle, maxLines = 2)
                    }
                }
                Box(
                    modifier = Modifier
                        .size(ManisDimens.minTouch)
                        .clip(CircleShape)
                        .clickable(onClick = onDismiss),
                    contentAlignment = Alignment.Center,
                ) {
                    Icon(
                        painter = painterResource(R.drawable.ic_close),
                        contentDescription = "关闭",
                        modifier = Modifier.size(20.dp),
                        tint = colors.textSecondary,
                    )
                }
            }
            content()
        }
    }
}

private fun appearanceLabel(dark: Boolean?): String = when (dark) {
    true -> "深色"
    false -> "浅色"
    null -> "跟随系统"
}

private fun modeLabel(mode: String): String = when (mode) {
    "global" -> "全局"
    "direct" -> "直连"
    else -> "规则"
}

private fun proxyDnsShortLabel(config: MobileConfig): String = when {
    config.proxyDnsServers.isNotEmpty() -> "自定义 DoH"
    config.discoveredDns.isNotEmpty() -> "机场 DNS"
    config.proxyDns == PROXY_DNS_SYSTEM -> "本机 DNS"
    else -> "加密 DNS"
}

private fun updateSummary(state: UpdateState, colors: ManisColors): Pair<String, Color> = when (state) {
    UpdateState.Idle -> "" to colors.textTertiary
    UpdateState.Checking -> "检查中…" to colors.textTertiary
    UpdateState.UpToDate -> "已是最新" to colors.textTertiary
    is UpdateState.Available -> "新版本 ${state.update.version}" to colors.actionPrimary
    is UpdateState.Downloading -> "下载中 ${state.progress}%" to colors.actionPrimary
    is UpdateState.Ready -> "可安装" to colors.statusSuccess
    is UpdateState.Failed -> "检查失败" to colors.statusError
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
