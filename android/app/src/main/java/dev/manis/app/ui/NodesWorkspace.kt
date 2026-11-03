package dev.manis.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.dp
import dev.manis.app.core.ControllerApi
import dev.manis.app.core.GroupInfo
import dev.manis.app.data.GroupEntry
import dev.manis.app.data.SubscriptionEntry
import dev.manis.app.data.Workspace
import kotlinx.coroutines.launch

/** The GPUI policy workspace, compact: the group list drills into the group detail. */
@Composable
fun NodesWorkspace(
    onMessage: (String) -> Unit,
    onOpenConfiguration: () -> Unit,
) {
    val colors = LocalManisColors.current
    val scope = rememberCoroutineScope()
    val snapshot = RuntimeState.snapshot
    val config = Workspace.config.value
    var detail by rememberSaveable { mutableStateOf<String?>(null) }
    var refreshing by remember { mutableStateOf(false) }
    var delays by remember { mutableStateOf<Map<String, Int>>(emptyMap()) }

    val groups = remember(snapshot) { snapshot?.groups?.filter { !it.hidden } ?: emptyList() }
    val current = detail?.let { name -> groups.firstOrNull { it.name == name } }
    val savedGroups = config.groups

    fun refresh() {
        if (refreshing) return
        refreshing = true
        scope.launch {
            runCatching { ControllerApi.refreshSubscriptions() }
                .onSuccess { onMessage("已刷新订阅") }
                .onFailure { onMessage("刷新订阅失败") }
            refreshing = false
        }
    }

    fun select(group: String, node: String) {
        scope.launch {
            runCatching { ControllerApi.select(group, node) }
                .onSuccess { onMessage("已切换到 $node") }
                .onFailure { onMessage("切换节点失败") }
        }
    }

    fun test(group: String) {
        scope.launch {
            runCatching { ControllerApi.testGroup(group) }
                .onSuccess {
                    delays = it
                    onMessage("测速完成")
                }
                .onFailure { onMessage("测速失败") }
        }
    }

    Column(Modifier.fillMaxSize().background(colors.surfaceBase)) {
        when {
            current != null -> GroupDetail(
                group = current,
                delays = delays,
                onBack = { detail = null },
                onTest = { test(current.name) },
                onSelect = { node -> select(current.name, node) },
            )
            detail != null -> SavedGroupDetail(
                name = detail ?: "",
                group = savedGroups.firstOrNull { it.name == detail },
                config = config,
                onBack = { detail = null },
            )
            else -> {
                WorkspaceHeader(
                    title = "节点",
                    subtitle = when {
                        snapshot != null -> "${groups.size} 个策略组 · ${snapshot.nodes.size} 个节点"
                        savedGroups.isNotEmpty() -> "${savedGroups.size} 个策略组 · 未连接"
                        else -> "暂无节点"
                    },
                    action = if (refreshing) "刷新中…" else "刷新",
                    onAction = ::refresh,
                )
                Column(
                    Modifier
                        .fillMaxSize()
                        .verticalScroll(rememberScrollState())
                        .padding(ManisDimens.spaceMd),
                    verticalArrangement = Arrangement.spacedBy(ManisDimens.spaceMd),
                ) {
                    SourceCards(
                        subscriptions = subscriptionsOf(config),
                        manualCount = config.nodes.count { it.enabled },
                        onOpenConfiguration = onOpenConfiguration,
                    )
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        ManisSectionTitle("策略组")
                        Spacer(Modifier.weight(1f))
                        ManisMeta(if (snapshot != null) "点击分组切换到详情" else "连接后显示运行时状态")
                    }
                    if (groups.isNotEmpty()) {
                        ManisCard {
                            groups.forEachIndexed { index, group ->
                                ManisListRow(
                                    title = displayProxyName(group.name) ?: group.name,
                                    meta = "${strategyLabel(group.type)} · 当前 ${displayProxyName(group.now) ?: "未选择"}",
                                    trailing = "${group.all.size}",
                                    onClick = { detail = group.name },
                                )
                                if (index != groups.lastIndex) ManisDivider()
                            }
                        }
                    } else if (savedGroups.isNotEmpty()) {
                        ManisCard {
                            savedGroups.forEachIndexed { index, group ->
                                ManisListRow(
                                    title = group.name,
                                    meta = "${strategyLabel(group.strategy)} · ${group.subscriptionIndexes.size + group.nodeIndexes.size} 个成员",
                                    onClick = { detail = group.name },
                                )
                                if (index != savedGroups.lastIndex) ManisDivider()
                            }
                        }
                    } else {
                        ManisCard {
                            Text("还没有策略组", style = ManisType.label, color = colors.textPrimary)
                            ManisMeta("在配置里添加订阅或手动节点后，策略组会出现在这里。")
                            ManisButton("打开配置", primary = true, onClick = onOpenConfiguration)
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun WorkspaceHeader(
    title: String,
    subtitle: String,
    action: String? = null,
    onAction: (() -> Unit)? = null,
) {
    val colors = LocalManisColors.current
    Row(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = ManisDimens.spaceMd, vertical = ManisDimens.spaceLg),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceMd),
    ) {
        Column(Modifier.weight(1f)) {
            Text(title, style = ManisType.pageTitle, color = colors.textPrimary)
            ManisMeta(subtitle)
        }
        if (action != null && onAction != null) {
            ManisButton(action, onClick = onAction)
        }
    }
    ManisDivider()
}

@Composable
private fun SourceCards(
    subscriptions: List<SubscriptionEntry>,
    manualCount: Int,
    onOpenConfiguration: () -> Unit,
) {
    val colors = LocalManisColors.current
    if (subscriptions.isEmpty() && manualCount == 0) {
        ManisCard {
            Text("还没有来源", style = ManisType.label, color = colors.textPrimary)
            ManisMeta("订阅和手动节点都在配置里管理。")
            ManisButton("打开配置", primary = true, onClick = onOpenConfiguration)
        }
        return
    }
    for ((index, subscription) in subscriptions.withIndex()) {
        ManisCard {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text(
                        subscription.name.ifBlank { "订阅 ${index + 1}" },
                        style = ManisType.label,
                        color = colors.textPrimary,
                    )
                    ManisMeta(
                        "${if (subscription.url.startsWith("https")) "HTTPS 订阅" else "HTTP 订阅"} · " +
                            sourceHost(subscription.url),
                    )
                }
                ManisTextAction("配置") { onOpenConfiguration() }
            }
        }
    }
    if (manualCount > 0) {
        ManisCard {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text("手动节点", style = ManisType.label, color = colors.textPrimary)
                    ManisMeta("$manualCount 个已启用的手动节点")
                }
                ManisTextAction("配置") { onOpenConfiguration() }
            }
        }
    }
}

private fun subscriptionsOf(config: dev.manis.app.data.MobileConfig): List<SubscriptionEntry> =
    config.subscriptions.filter { it.enabled }

/** The host of a subscription URL, as GPUI shows the transport target. */
private fun sourceHost(url: String): String {
    val rest = url.substringAfter("://", url)
    return rest.substringBefore('/').substringBefore('?').ifBlank { url }
}

@Composable
private fun GroupDetail(
    group: GroupInfo,
    delays: Map<String, Int>,
    onBack: () -> Unit,
    onTest: () -> Unit,
    onSelect: (String) -> Unit,
) {
    val colors = LocalManisColors.current
    Column(Modifier.fillMaxSize()) {
        Row(
            Modifier
                .fillMaxWidth()
                .padding(ManisDimens.spaceMd),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
        ) {
            BackButton(onBack)
            Column(Modifier.weight(1f)) {
                Text(
                    displayProxyName(group.name) ?: group.name,
                    style = ManisType.sectionTitle,
                    color = colors.textPrimary,
                )
                ManisMeta(strategyLabel(group.type))
            }
            ManisButton("测速", onClick = onTest)
        }
        ManisDivider()
        Column(
            Modifier
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(ManisDimens.spaceMd),
            verticalArrangement = Arrangement.spacedBy(ManisDimens.spaceMd),
        ) {
            ManisCard {
                Text("当前节点", style = ManisType.label, color = colors.textPrimary)
                Text(
                    displayProxyName(group.now) ?: "未选择",
                    style = ManisType.body,
                    color = colors.textSecondary,
                )
            }
            ManisCard {
                group.all.forEachIndexed { index, node ->
                    val delay = delays[node] ?: RuntimeState.snapshot?.nodes?.get(node)?.delay
                    ManisListRow(
                        title = node,
                        trailing = if (delay == null) "—" else "$delay ms",
                        selected = node == group.now,
                        onClick = { onSelect(node) },
                    )
                    if (index != group.all.lastIndex) ManisDivider()
                }
                if (group.all.isEmpty()) {
                    ManisMeta("这个策略组还没有成员。")
                }
            }
        }
    }
}

@Composable
private fun SavedGroupDetail(
    name: String,
    group: GroupEntry?,
    config: dev.manis.app.data.MobileConfig,
    onBack: () -> Unit,
) {
    val colors = LocalManisColors.current
    val subscriptionMembers = group?.subscriptionIndexes.orEmpty().mapNotNull { index ->
        config.subscriptions.getOrNull(index)?.let { "订阅 · ${it.name.ifBlank { "订阅 ${index + 1}" }}" }
    }
    val nodeMembers = group?.nodeIndexes.orEmpty().mapNotNull { index ->
        config.nodes.getOrNull(index)?.displayName
    }
    Column(Modifier.fillMaxSize()) {
        Row(
            Modifier
                .fillMaxWidth()
                .padding(ManisDimens.spaceMd),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
        ) {
            BackButton(onBack)
            Column(Modifier.weight(1f)) {
                Text(name, style = ManisType.sectionTitle, color = colors.textPrimary)
                ManisMeta(group?.let { strategyLabel(it.strategy) } ?: "策略组")
            }
        }
        ManisDivider()
        Column(
            Modifier
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(ManisDimens.spaceMd),
            verticalArrangement = Arrangement.spacedBy(ManisDimens.spaceMd),
        ) {
            ManisCard {
                ManisMeta("连接后可以测速和切换节点。")
            }
            if (subscriptionMembers.isNotEmpty() || nodeMembers.isNotEmpty()) {
                ManisCard {
                    for ((index, member) in (subscriptionMembers + nodeMembers).withIndex()) {
                        Text(member, style = ManisType.body, color = colors.textPrimary)
                        if (index != (subscriptionMembers + nodeMembers).lastIndex) ManisDivider()
                    }
                }
            }
        }
    }
}

@Composable
private fun BackButton(onBack: () -> Unit) {
    val colors = LocalManisColors.current
    Box(
        Modifier
            .size(ManisDimens.controlStandard)
            .clip(RoundedCornerShape(ManisDimens.radiusControl))
            .background(colors.buttonBg)
            .clickable(onClick = onBack),
        contentAlignment = Alignment.Center,
    ) {
        Text("‹", style = ManisType.sectionTitle, color = colors.textPrimary)
    }
}

/** The GPUI strategy labels for the controller's group types. */
private fun strategyLabel(type: String): String = when (type.lowercase()) {
    "selector" -> "手动选择"
    "urltest" -> "自动测速"
    "fallback" -> "故障转移"
    "loadbalance" -> "负载均衡"
    "relay" -> "链式代理"
    else -> type
}
