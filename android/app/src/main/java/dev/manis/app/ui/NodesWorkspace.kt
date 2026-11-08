package dev.manis.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import dev.manis.app.core.ControllerApi
import dev.manis.app.core.GroupInfo
import dev.manis.app.data.GroupEntry
import dev.manis.app.data.SubscriptionCache
import dev.manis.app.data.Workspace
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * The policy workspace: subscriptions and manual nodes expand into their nodes, and every policy
 * group expands into its members with their latency, the active node and a speed test.
 */
@Composable
fun NodesWorkspace(
    proxyLabel: String,
    proxyActive: Boolean,
    onProxyToggle: () -> Unit,
    routingLabel: String,
    onRoutingCycle: () -> Unit,
    onMessage: (String) -> Unit,
    onOpenConfiguration: () -> Unit,
) {
    val openSources = {
        Workspace.requestSection("proxy")
        onOpenConfiguration()
    }
    val colors = LocalManisColors.current
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val snapshot = RuntimeState.snapshot
    val config = Workspace.config.value
    var refreshing by remember { mutableStateOf(false) }
    var delays by remember { mutableStateOf<Map<String, Int>>(emptyMap()) }
    var addingGroup by remember { mutableStateOf(false) }
    var deleting by remember { mutableStateOf<GroupEntry?>(null) }
    var expandedSources by remember { mutableStateOf(emptySet<String>()) }
    var expandedGroups by remember { mutableStateOf(emptySet<String>()) }

    // The core keeps an internal selector for global-mode exits and the exit-IP probe; neither is
    // a user policy group, so the phone never lists them.
    val groups = remember(snapshot) {
        snapshot?.groups
            ?.filter { !it.hidden && it.name != "__MANIS_GLOBAL__" && it.name != "GLOBAL" }
            ?: emptyList()
    }

    val subscriptions = config.subscriptions.filter { it.enabled }
    var cachedNodes by remember { mutableStateOf<Map<String, List<SubscriptionCache.CachedNode>>>(emptyMap()) }
    LaunchedEffect(config.subscriptions) {
        cachedNodes = withContext(Dispatchers.IO) {
            config.subscriptions.associate { it.url to SubscriptionCache.cachedNodes(context, it.url) }
        }
    }
    // Runtime groups come from the core; saved groups fill in while it is off.
    val groupNames = remember(groups, config.groups) {
        (groups.map { it.name } + config.groups.map { it.name }).distinct()
    }

    fun savedMembers(group: GroupEntry?): List<String> {
        if (group == null) return emptyList()
        val picked = group.selectedNames.toSet()
        val fromSubscriptions = group.subscriptionIndexes.flatMap { index ->
            val subscription = config.subscriptions.getOrNull(index) ?: return@flatMap emptyList()
            cachedNodes[subscription.url].orEmpty().map { it.name }
        }
        val manual = group.nodeIndexes.mapNotNull { config.nodes.getOrNull(it)?.displayName }
        return (fromSubscriptions + manual)
            .filter { picked.isEmpty() || it in picked }
            .distinct()
    }

    fun reloadNodes() {
        scope.launch {
            cachedNodes = withContext(Dispatchers.IO) {
                config.subscriptions.associate { it.url to SubscriptionCache.cachedNodes(context, it.url) }
            }
        }
    }

    fun select(group: String, node: String) {
        scope.launch {
            runCatching { ControllerApi.select(group, node) }
                .onSuccess { onMessage("已切换到 $node") }
                .onFailure { onMessage("切换节点失败：${it.message ?: "内核没有响应"}") }
        }
    }

    fun refresh() {
        if (refreshing) return
        refreshing = true
        scope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching { SubscriptionCache.refresh(context, config) }.getOrNull()
            }
            refreshing = false
            if (result == null) {
                onMessage("下载订阅失败")
                return@launch
            }
            reloadNodes()
            val parts = mutableListOf<String>()
            if (result.downloaded > 0) parts += "已下载 ${result.downloaded} 个订阅"
            if (result.nodes > 0) parts += "${result.nodes} 个节点"
            if (result.failures.isNotEmpty()) {
                parts += "失败：${result.failures.first()}"
                if (result.failures.size > 1) parts += "另有 ${result.failures.size - 1} 个失败"
            }
            onMessage(parts.joinToString(" · ").ifBlank { "没有启用的订阅" })
        }
    }

    fun switchTo(name: String) {
        scope.launch {
            // Use the cached snapshot when it has the group; otherwise ask the core once so a
            // slow first snapshot does not push the user into the offline path.
            val snap = snapshot ?: runCatching { ControllerApi.snapshot() }.getOrNull()
            val group = snap?.groups
                ?.firstOrNull { name in it.all && it.type.contains("select", ignoreCase = true) }
                ?: snap?.groups?.firstOrNull { name in it.all }
            if (group == null) {
                Workspace.setPendingNode(name)
                onMessage("已选中 $name，连接后自动切换")
                return@launch
            }
            Workspace.setPendingNode(null)
            runCatching { ControllerApi.select(group.name, name) }
                .onSuccess { onMessage("已切换到 $name") }
                .onFailure { onMessage("切换节点失败：${it.message ?: "内核没有响应"}") }
        }
    }

    fun testAll() {
        if (snapshot == null) {
            onMessage("连接 VPN 后才能测速")
            return
        }
        scope.launch {
            var tested = 0
            var failure: String? = null
            for (group in groups) {
                runCatching { ControllerApi.testGroup(group.name) }
                    .onSuccess {
                        delays = delays + it
                        tested += it.size
                    }
                    .onFailure { failure = failure ?: it.message }
            }
            onMessage(
                when {
                    tested > 0 -> "已测速 $tested 个节点"
                    failure != null -> "测速失败：$failure"
                    else -> "没有可测速的节点"
                },
            )
        }
    }

    fun testNode(node: String) {
        scope.launch {
            runCatching { ControllerApi.testNode(node) }
                .onSuccess { delay ->
                    if (delay != null) delays = delays + (node to delay)
                    onMessage(if (delay == null) "$node 测速失败" else "$node：$delay ms")
                }
                .onFailure { onMessage("测速失败：${it.message ?: "内核没有响应"}") }
        }
    }

    fun testGroup(group: String) {
        scope.launch {
            runCatching { ControllerApi.testGroup(group) }
                .onSuccess {
                    delays = delays + it
                    onMessage("测速完成")
                }
                .onFailure { onMessage("测速失败：${it.message ?: "内核没有响应"}") }
        }
    }

    fun toggle(set: Set<String>, key: String): Set<String> =
        if (key in set) set - key else set + key

    Column(Modifier.fillMaxSize()) {
        Column(
            Modifier
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(ManisDimens.spaceLg),
            verticalArrangement = Arrangement.spacedBy(ManisDimens.spaceMd),
        ) {
            ManisCard {
                Row(
                    Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceMd),
                ) {
                    Column(Modifier.weight(1f)) {
                        Text("VPN 代理", style = ManisType.label, color = colors.textPrimary)
                        ManisMeta(if (proxyActive) "已连接，点右侧断开" else "未连接，点右侧开启")
                    }
                    if (proxyActive) {
                        Box(
                            Modifier
                                .size(8.dp)
                                .clip(CircleShape)
                                .background(colors.statusSuccess),
                        )
                    }
                    ManisButton(proxyLabel, primary = !proxyActive, onClick = onProxyToggle)
                }
                ManisDivider()
                Row(
                    Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceMd),
                ) {
                    Column(Modifier.weight(1f)) {
                        Text("路由模式", style = ManisType.label, color = colors.textPrimary)
                        ManisMeta("规则、全局、直连之间切换")
                    }
                    ManisTextAction(routingLabel, onClick = onRoutingCycle)
                }
            }
            Row(verticalAlignment = Alignment.CenterVertically) {
                ManisSectionTitle("来源")
                Spacer(Modifier.weight(1f))
                ManisTextAction(if (refreshing) "刷新中…" else "刷新") { refresh() }
                ManisTextAction("配置") { openSources() }
            }
            if (subscriptions.isEmpty() && config.nodes.isEmpty()) {
                ManisCard {
                    Text("还没有来源", style = ManisType.label, color = colors.textPrimary)
                    ManisMeta("订阅和手动节点都在配置里管理。")
                    ManisButton("打开配置", primary = true, onClick = openSources)
                }
            }
            subscriptions.forEachIndexed { index, subscription ->
                val key = "subscription-${index}"
                val nodes = cachedNodes[subscription.url].orEmpty()
                val expanded = key in expandedSources
                ExpandableCard(
                    title = subscription.name.ifBlank { "订阅 ${index + 1}" },
                    meta = "${if (subscription.url.startsWith("https")) "HTTPS" else "HTTP"} 订阅 · " +
                        "${nodes.size} 个节点 · ${sourceHost(subscription.url)}",
                    expanded = expanded,
                    onToggle = { expandedSources = toggle(expandedSources, key) },
                ) {
                    if (nodes.isEmpty()) {
                        ManisMeta("还没有缓存节点，点「刷新」下载订阅。")
                    } else {
                        for ((position, node) in nodes.withIndex()) {
                            NodeRow(
                                name = node.name,
                                type = node.type,
                                delay = delays[node.name] ?: snapshot?.nodes?.get(node.name)?.delay,
                                selected = snapshot?.groups?.any { it.now == node.name } == true ||
                                    config.pendingNode == node.name,
                                onSelect = { switchTo(node.name) },
                                onTest = if (snapshot != null) {
                                    { testNode(node.name) }
                                } else {
                                    null
                                },
                            )
                            if (position != nodes.lastIndex) ManisDivider()
                        }
                    }
                }
            }
            if (config.nodes.isNotEmpty()) {
                val key = "manual"
                val expanded = key in expandedSources
                ExpandableCard(
                    title = "手动节点",
                    meta = "${config.nodes.count { it.enabled }} 个已启用的手动节点",
                    expanded = expanded,
                    onToggle = { expandedSources = toggle(expandedSources, key) },
                ) {
                    for ((position, node) in config.nodes.withIndex()) {
                        NodeRow(
                            name = node.displayName,
                            type = "VLESS",
                            delay = delays[node.displayName] ?: snapshot?.nodes?.get(node.displayName)?.delay,
                            selected = snapshot?.groups?.any { it.now == node.displayName } == true ||
                                config.pendingNode == node.displayName,
                            onSelect = { switchTo(node.displayName) },
                            onTest = if (snapshot != null) {
                                { testNode(node.displayName) }
                            } else {
                                null
                            },
                        )
                        if (position != config.nodes.lastIndex) ManisDivider()
                    }
                }
            }

            Row(verticalAlignment = Alignment.CenterVertically) {
                ManisSectionTitle("策略组")
                Spacer(Modifier.weight(1f))
                ManisTextAction("测速") { testAll() }
                ManisTextAction("添加") { addingGroup = true }
            }
            if (groups.isEmpty() && config.groups.isEmpty()) {
                ManisCard {
                    Text("还没有策略组", style = ManisType.label, color = colors.textPrimary)
                    ManisMeta("没有策略组时，直接用「来源」里选中的节点；建组后可以在组内切换。")
                    ManisButton("添加策略组", primary = true, onClick = { addingGroup = true })
                }
            }
            for (name in groupNames) {
                val runtime = groups.firstOrNull { it.name == name }
                val saved = config.groups.firstOrNull { it.name == name }
                GroupCard(
                    title = displayProxyName(name) ?: name,
                    type = runtime?.type ?: saved?.strategy ?: "select",
                    current = runtime?.now,
                    members = runtime?.all ?: savedMembers(saved),
                    delays = delays,
                    expanded = name in expandedGroups,
                    canTest = runtime != null,
                    onToggle = { expandedGroups = toggle(expandedGroups, name) },
                    onTest = { testGroup(name) },
                    onSelect = { node -> select(name, node) },
                    onDelete = saved?.let { entry -> { deleting = entry } },
                )
            }
            ManisMeta(
                when {
                    snapshot != null -> "点节点即可切换；「测速」会检测组内全部成员。"
                    RuntimeState.active -> "正在读取内核状态，稍后即可切换节点和测速。"
                    else -> "未连接时也可以先选节点，连接后会自动切换。"
                },
            )
        }
    }

    val pendingDelete = deleting
    if (pendingDelete != null) {
        ManisDialog(
            title = "删除策略组",
            subtitle = pendingDelete.name,
            onDismiss = { deleting = null },
            actions = {
                Spacer(Modifier.weight(1f))
                ManisButton("取消", onClick = { deleting = null })
                ManisButton("删除", primary = true, onClick = {
                    Workspace.update { current ->
                        current.copy(groups = current.groups.filter { it.name != pendingDelete.name })
                    }
                    deleting = null
                    onMessage("策略组已删除，点「应用」后生效")
                })
            },
        ) {
            ManisMeta("删除后这一组从配置里移除；正在连接时需要点「应用」才会让内核重新加载。")
        }
    }

    if (addingGroup) {
        GroupEditorDialog(
            config = config,
            onDismiss = { addingGroup = false },
            onSave = { group ->
                Workspace.update { current -> current.copy(groups = current.groups + group) }
                addingGroup = false
                onMessage("已添加策略组 ${group.name}")
            },
        )
    }
}

/** A card that expands to show its content. */
@Composable
private fun ExpandableCard(
    title: String,
    meta: String,
    expanded: Boolean,
    onToggle: () -> Unit,
    content: @Composable ColumnScope.() -> Unit,
) {
    val colors = LocalManisColors.current
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    ManisCard {
        Row(
            Modifier
                .fillMaxWidth()
                .background(
                    if (pressed) colors.surfaceLow else Color.Transparent,
                    RoundedCornerShape(ManisDimens.radiusRow),
                )
                .clickable(
                    interactionSource = interaction,
                    indication = null,
                    onClick = onToggle,
                ),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
        ) {
            Column(Modifier.weight(1f)) {
                Text(title, style = ManisType.label, color = colors.textPrimary, maxLines = 1, overflow = TextOverflow.Ellipsis)
                ManisMeta(meta)
            }
            Icon(
                imageVector = Icons.Filled.KeyboardArrowDown,
                contentDescription = if (expanded) "收起" else "展开",
                modifier = Modifier.size(22.dp).rotate(if (expanded) 180f else 0f),
                tint = colors.textSecondary,
            )
        }
        if (expanded) {
            ManisDivider()
            content()
        }
    }
}

@Composable
private fun NodeRow(
    name: String,
    type: String,
    delay: Int?,
    selected: Boolean,
    onSelect: () -> Unit,
    onTest: (() -> Unit)?,
) {
    val colors = LocalManisColors.current
    Row(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(ManisDimens.radiusRow))
            .background(if (selected) colors.actionSoft else Color.Transparent)
            .clickable(onClick = onSelect)
            .padding(horizontal = ManisDimens.spaceSm, vertical = ManisDimens.spaceXs),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
    ) {
        Column(Modifier.weight(1f)) {
            Text(
                (if (selected) "✓ " else "") + name,
                style = ManisType.body,
                color = if (selected) colors.actionPrimary else colors.textPrimary,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            ManisMeta(type.uppercase())
        }
        Text(
            if (delay == null) "—" else "$delay ms",
            style = ManisType.data,
            color = when {
                delay == null -> colors.textTertiary
                delay < 200 -> colors.statusSuccess
                delay < 500 -> colors.statusWarning
                else -> colors.statusError
            },
        )
        if (onTest != null) {
            ManisTextAction("测速", onClick = onTest)
        }
    }
}

@Composable
private fun GroupCard(
    title: String,
    type: String,
    current: String?,
    members: List<String>,
    delays: Map<String, Int>,
    expanded: Boolean,
    canTest: Boolean,
    onToggle: () -> Unit,
    onTest: () -> Unit,
    onSelect: (String) -> Unit,
    onDelete: (() -> Unit)?,
) {
    val colors = LocalManisColors.current
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    ManisCard {
        Row(
            Modifier
                .fillMaxWidth()
                .background(
                    if (pressed) colors.surfaceLow else Color.Transparent,
                    RoundedCornerShape(ManisDimens.radiusRow),
                )
                .clickable(
                    interactionSource = interaction,
                    indication = null,
                    onClick = onToggle,
                ),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
        ) {
            Column(Modifier.weight(1f)) {
                Text(
                    title,
                    style = ManisType.label,
                    color = colors.textPrimary,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                ManisMeta(
                    "${strategyLabel(type)} · 当前 ${displayProxyName(current) ?: "未选择"} · " +
                        "${members.size} 个节点",
                )
            }
            if (canTest) ManisTextAction("测速", onClick = onTest)
            Icon(
                imageVector = Icons.Filled.KeyboardArrowDown,
                contentDescription = if (expanded) "收起" else "展开",
                modifier = Modifier.size(22.dp).rotate(if (expanded) 180f else 0f),
                tint = colors.textSecondary,
            )
        }
        if (expanded) {
            ManisDivider()
            if (members.isEmpty()) {
                ManisMeta("这个策略组还没有成员。")
            }
            for ((index, node) in members.withIndex()) {
                val delay = delays[node] ?: RuntimeState.snapshot?.nodes?.get(node)?.delay
                ManisListRow(
                    title = displayProxyName(node) ?: node,
                    trailing = if (delay == null) "—" else "$delay ms",
                    selected = node == current,
                    onClick = { onSelect(node) },
                )
                if (index != members.lastIndex) ManisDivider()
            }
            if (onDelete != null) {
                ManisDivider()
                Row(
                    Modifier.fillMaxWidth().padding(vertical = ManisDimens.spaceXs),
                    horizontalArrangement = Arrangement.End,
                ) {
                    ManisTextAction("删除策略组", tone = colors.statusError, onClick = onDelete)
                }
            }
        }
    }
}

/** The host of a subscription URL, as the desktop shows the transport target. */
private fun sourceHost(url: String): String {
    val rest = url.substringAfter("://", url)
    return rest.substringBefore('/').substringBefore('?').ifBlank { url }
}

/** The strategy labels for the controller's group types. */
private fun strategyLabel(type: String): String = when (type.lowercase()) {
    "select", "selector" -> "手动选择"
    "urltest" -> "自动测速"
    "fallback" -> "故障转移"
    "loadbalance" -> "负载均衡"
    "relay" -> "链式代理"
    else -> type
}
