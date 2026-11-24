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
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
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
 * The connection screen read top to bottom: what carries the traffic, then the groups that pick
 * among it, then one band with the state and the controls that start and shape the session.
 * Subscriptions and manual nodes expand into their nodes, and every policy group expands into its
 * members with their latency, the active node and a test. The band sits below the list rather than
 * scrolling with it: starting or stopping the tunnel never depends on where the list was left.
 */
@Composable
fun NodesWorkspace(
    connected: Boolean,
    mode: String,
    onToggleConnection: () -> Unit,
    onModeSelect: (String) -> Unit,
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
    var delays by remember { mutableStateOf<Map<String, Int>>(emptyMap()) }
    var addingGroup by remember { mutableStateOf(false) }
    var deleting by remember { mutableStateOf<GroupEntry?>(null) }
    // The sources open on arrival, so the page is a node list and not a wall of collapsed rows.
    // Groups stay closed: their rows already name the current pick, and an open group would print
    // the same nodes a second time.
    var expandedSources by remember {
        mutableStateOf(
            buildSet {
                if (config.subscriptions.isNotEmpty()) add("subscription-0")
                if (config.nodes.any { it.enabled }) add("manual")
            },
        )
    }
    var expandedGroups by remember { mutableStateOf(emptySet<String>()) }

    // The core keeps an internal selector for global-mode exits and the exit-IP probe; neither is
    // a user policy group, so the phone never lists them.
    val groups = remember(snapshot) {
        snapshot?.groups
            ?.filter {
                !it.hidden &&
                    it.name != "__MANIS_GLOBAL__" &&
                    it.name != "__MANIS_EXIT_IP_PROBE__" &&
                    it.name != "GLOBAL"
            }
            ?: emptyList()
    }

    val subscriptions = config.subscriptions.filter { it.enabled }
    var cachedNodes by remember { mutableStateOf<Map<String, List<SubscriptionCache.CachedNode>>>(emptyMap()) }
    LaunchedEffect(config.subscriptions) {
        cachedNodes = withContext(Dispatchers.IO) {
            config.subscriptions.associate { it.url to SubscriptionCache.cachedNodes(context, it.url) }
        }
    }
    // The source list marks the one node the all-node selector points at; a url-test group picks
    // its own exit, and showing both read as two active nodes.
    val activeNode = remember(snapshot) {
        snapshot?.groups?.firstOrNull { it.name == "__MANIS_GLOBAL__" }?.now
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

    fun select(group: String, node: String) {
        scope.launch {
            runCatching {
                ControllerApi.select(group, node)
                // The hidden selector keeps the global-mode exit; mirror the pick so the source
                // list never shows two nodes as active at once.
                val global = snapshot?.groups
                    ?.firstOrNull { it.name == "__MANIS_GLOBAL__" && node in it.all }
                if (global != null && global.name != group && global.now != node) {
                    runCatching { ControllerApi.select(global.name, node) }
                }
            }
                .onSuccess { onMessage("已切换到 $node") }
                .onFailure { onMessage("切换节点失败：${it.message ?: "内核没有响应"}") }
        }
    }

    fun switchTo(name: String) {
        scope.launch {
            // Use the cached snapshot when it has the group; otherwise ask the core once so a
            // slow first snapshot does not push the user into the offline path.
            val snap = snapshot ?: runCatching { ControllerApi.snapshot() }.getOrNull()
            val global = snap?.groups?.firstOrNull {
                it.name == "__MANIS_GLOBAL__" && name in it.all
            }
            val group = snap?.groups
                ?.firstOrNull { name in it.all && it.type.contains("select", ignoreCase = true) }
                ?: global
                ?: snap?.groups?.firstOrNull { name in it.all }
            if (group == null) {
                Workspace.setPendingNode(name)
                onMessage("已选中 $name，连接后自动切换")
                return@launch
            }
            Workspace.setPendingNode(null)
            runCatching {
                ControllerApi.select(group.name, name)
                if (global != null && global.name != group.name && global.now != name) {
                    runCatching { ControllerApi.select(global.name, name) }
                }
            }
                .onSuccess { onMessage("已切换到 $name") }
                .onFailure { onMessage("切换节点失败：${it.message ?: "内核没有响应"}") }
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
                .fillMaxWidth()
                .weight(1f)
                .verticalScroll(rememberScrollState())
                .padding(ManisDimens.spaceLg),
            verticalArrangement = Arrangement.spacedBy(ManisDimens.spaceMd),
        ) {
            // The list is the choosing surface: what carries the traffic first, then the groups
            // that pick among it. Its section bars carry no actions, because refreshing a
            // subscription, adding a node and adding a group all happen where the source is
            // edited, and testing one group stays inside that group's card.
            ManisSectionBar("订阅与节点") {}
            if (subscriptions.isEmpty() && config.nodes.isEmpty()) {
                ManisCard {
                    Text("还没有订阅或手动节点", style = ManisType.label, color = colors.textPrimary)
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
                        ManisMeta("还没有缓存节点，在「配置」里刷新订阅。")
                    } else {
                        for ((position, node) in nodes.withIndex()) {
                            NodeRow(
                                name = node.name,
                                type = node.type,
                                delay = delays[node.name] ?: snapshot?.nodes?.get(node.name)?.delay,
                                selected = activeNode == node.name ||
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
                            selected = activeNode == node.displayName ||
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
            ManisSectionBar("策略组") {}
            if (groups.isEmpty() && config.groups.isEmpty()) {
                ManisCard {
                    Text("还没有策略组", style = ManisType.label, color = colors.textPrimary)
                    ManisMeta("没有策略组时，直接用「订阅与节点」里选中的节点；建组后可以在组内切换。")
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
        ConnectionBar(
            connected = connected,
            mode = mode,
            exit = activeNode,
            onToggle = onToggleConnection,
            onModeSelect = onModeSelect,
        )
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

/**
 * The page's one control band, in two lines: the state with its exit, then the route mode beside
 * the button that starts and ends a session. It sits under the list instead of scrolling with it,
 * so those controls never move, and the list above stays a surface for choosing a node.
 */
@Composable
private fun ConnectionBar(
    connected: Boolean,
    mode: String,
    exit: String?,
    onToggle: () -> Unit,
    onModeSelect: (String) -> Unit,
) {
    val colors = LocalManisColors.current
    val unreachable = RuntimeState.unreachable
    // The exit is the useful half of a connected state, so it shares the state's line. Nothing is
    // reachable in the failure case, so there is no exit worth naming and the line turns warning.
    val stateText = when {
        !connected -> "未连接"
        unreachable -> "已连接 · 所有节点都不可达"
        exit == null -> "已连接"
        else -> "已连接 · 出口 ${displayProxyName(exit) ?: exit}"
    }
    Column(
        Modifier
            .fillMaxWidth()
            .height(ManisDimens.connectionBar)
            .background(colors.surfaceOverlay),
    ) {
        // A hairline is what tells a bar that stays put apart from a list that scrolls under it.
        ManisDivider()
        Column(
            Modifier
                .weight(1f)
                .padding(horizontal = ManisDimens.spaceLg, vertical = ManisDimens.spaceMd),
            verticalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
        ) {
            Row(
                Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
            ) {
                // The dot is the state at a glance; the word beside it is what a colourblind user
                // reads, and what tells the two states apart in a screenshot.
                Box(
                    Modifier
                        .size(8.dp)
                        .clip(CircleShape)
                        .background(if (connected) colors.statusSuccess else colors.textTertiary),
                )
                Text(
                    stateText,
                    modifier = Modifier.weight(1f),
                    style = ManisType.label,
                    color = if (connected && unreachable) {
                        colors.statusWarning
                    } else {
                        colors.textPrimary
                    },
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                // The session totals this page used to print twice, once here and once in the
                // status strip below the navigation. While the tunnel is down there is no session.
                if (connected) {
                    Text(
                        "↓ ${formatTraffic(RuntimeState.downloadTotal)} " +
                            "↑ ${formatTraffic(RuntimeState.uploadTotal)}",
                        style = ManisType.metadata,
                        color = colors.textSecondary,
                        maxLines = 1,
                    )
                }
            }
            // The modes take the full width rather than sharing a row with the button: at 320dp a
            // shared row squeezes each chip below its label, and 全局 clips to 全.
            Row(
                Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                for (value in routeModes) {
                    ManisChip(
                        label = routeModeLabel(value),
                        selected = mode == value,
                        modifier = Modifier.weight(1f),
                        onClick = { onModeSelect(value) },
                    )
                }
            }
            // The one action of this page, full width and under the thumb.
            ManisButton(
                if (connected) "断开代理" else "开启代理",
                modifier = Modifier.fillMaxWidth().height(ManisDimens.controlStandard),
                primary = !connected,
                onClick = onToggle,
            )
        }
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
            color = latencyColor(delay, colors),
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

/** The routing modes in the order the chips read them: cut out, filter, send everything. */
private val routeModes = listOf("direct", "rule", "global")

/** The latency thresholds every node reading shares, so the header and the rows agree. */
private fun latencyColor(delay: Int?, colors: ManisColors): Color = when {
    delay == null -> colors.textTertiary
    delay < 200 -> colors.statusSuccess
    delay < 500 -> colors.statusWarning
    else -> colors.statusError
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
