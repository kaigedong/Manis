package dev.manis.app.ui

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.manis.app.R
import dev.manis.app.core.ControllerApi
import dev.manis.app.data.GroupEntry
import dev.manis.app.data.SubscriptionCache
import dev.manis.app.data.Workspace
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * The home page read top to bottom: the hero connection cockpit, the groups that pick an exit, then
 * the sources that carry the nodes. The state, the exit, the route mode and the one action that
 * starts or ends the session all live in the hero card, so nothing is pinned over the list; once the
 * hero scrolls away a 52dp mini bar keeps the same state and the same button within thumb reach.
 */
@Composable
fun NodesWorkspace(
    connected: Boolean,
    mode: String,
    darkTheme: Boolean,
    onToggleTheme: () -> Unit,
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
    // Groups stay closed: their rows already name the current pick.
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
    val exitDelay = activeNode?.let { delays[it] ?: snapshot?.nodes?.get(it)?.delay }

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

    val scroll = rememberScrollState()
    var heroHeight by remember { mutableStateOf(0) }
    // The mini bar is the hero's own state and button once the hero has slid under the header, so
    // it appears exactly when the card stops being visible and never twice at the same time.
    val showMiniBar by remember { derivedStateOf { heroHeight > 0 && scroll.value > heroHeight } }

    Column(Modifier.fillMaxSize()) {
        ManisPageHeader(
            title = "Manis",
            brand = true,
            trailing = { ManisThemeToggle(darkTheme, onToggleTheme) },
        )
        Box(Modifier.fillMaxWidth().weight(1f)) {
            Column(
                Modifier
                    .fillMaxSize()
                    .verticalScroll(scroll)
                    .padding(horizontal = ManisDimens.spaceLg)
                    .padding(top = ManisDimens.spaceXs, bottom = ManisDimens.spaceXl),
                verticalArrangement = Arrangement.spacedBy(ManisDimens.spaceMd),
            ) {
                HeroCockpit(
                    connected = connected,
                    mode = mode,
                    exit = activeNode,
                    delay = exitDelay,
                    unreachable = RuntimeState.unreachable,
                    onToggle = onToggleConnection,
                    onModeSelect = onModeSelect,
                    modifier = Modifier.onSizeChanged { heroHeight = it.height },
                )
                // Groups first: they decide which node carries the traffic, and the sources below
                // are the pool they choose from.
                ManisSectionBar("策略组") {}
                if (groups.isEmpty() && config.groups.isEmpty()) {
                    ManisCard {
                        Text("还没有策略组", style = ManisType.label, color = colors.textPrimary)
                        ManisMeta("没有策略组时，直接选下面的节点；建组后可以在组内切换。")
                        ManisButton("添加策略组", primary = true, onClick = { addingGroup = true })
                    }
                }
                for (name in groupNames) {
                    val runtime = groups.firstOrNull { it.name == name }
                    val saved = config.groups.firstOrNull { it.name == name }
                    PolicyGroupCard(
                        title = displayProxyName(name) ?: name,
                        strategy = runtime?.type ?: saved?.strategy ?: "select",
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
                ManisSectionBar("订阅节点") {}
                if (subscriptions.isEmpty() && config.nodes.isEmpty()) {
                    ManisCard {
                        Text("还没有订阅或手动节点", style = ManisType.label, color = colors.textPrimary)
                        ManisMeta("订阅和手动节点都在设置里管理。")
                        ManisButton("打开设置", primary = true, onClick = openSources)
                    }
                }
                subscriptions.forEachIndexed { index, subscription ->
                    val key = "subscription-${index}"
                    val nodes = cachedNodes[subscription.url].orEmpty()
                    val expanded = key in expandedSources
                    SourceCard(
                        title = subscription.name.ifBlank { "订阅 ${index + 1}" },
                        meta = "${nodes.size} 节点 · " +
                            "${if (subscription.url.startsWith("https")) "HTTPS" else "HTTP"} · " +
                            sourceHost(subscription.url),
                        expanded = expanded,
                        onToggle = { expandedSources = toggle(expandedSources, key) },
                    ) {
                        if (nodes.isEmpty()) {
                            ManisMeta("还没有缓存节点，在「设置」里刷新订阅。")
                        } else {
                            for ((position, node) in nodes.withIndex()) {
                                NodeRow(
                                    name = node.name,
                                    protocol = node.type,
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
                    SourceCard(
                        title = "手动节点",
                        meta = "${config.nodes.count { it.enabled }} 个已启用的手动节点",
                        expanded = expanded,
                        onToggle = { expandedSources = toggle(expandedSources, key) },
                    ) {
                        for ((position, node) in config.nodes.withIndex()) {
                            NodeRow(
                                name = node.displayName,
                                protocol = "VLESS",
                                delay = delays[node.displayName]
                                    ?: snapshot?.nodes?.get(node.displayName)?.delay,
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
                ManisMeta(
                    when {
                        snapshot != null -> "点节点即可切换；「测速」会检测组内全部成员。"
                        RuntimeState.active -> "正在读取内核状态，稍后即可切换节点和测速。"
                        else -> "未连接时也可以先选节点，连接后会自动切换。"
                    },
                )
            }
            MiniBarOverlay(
                visible = showMiniBar,
                connected = connected,
                exit = activeNode,
                unreachable = RuntimeState.unreachable,
                onToggle = onToggleConnection,
                modifier = Modifier.align(Alignment.TopCenter),
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

/**
 * The hero connection cockpit: the state and its exit on the left, the power pill on the right, the
 * session telemetry under them, and the route mode across the full width of the card.
 */
@Composable
private fun HeroCockpit(
    connected: Boolean,
    mode: String,
    exit: String?,
    delay: Int?,
    unreachable: Boolean,
    onToggle: () -> Unit,
    onModeSelect: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val colors = LocalManisColors.current
    val degraded = connected && unreachable
    val title = when {
        !connected -> "未连接"
        degraded -> "已保护 · 节点不可达"
        else -> "已保护 · ${modeWord(mode)}"
    }
    val dot = when {
        !connected -> colors.textTertiary
        degraded -> colors.statusWarning
        else -> colors.statusSuccess
    }
    ManisCard(
        modifier = modifier,
        borderColor = if (connected) colors.statusSuccess.copy(alpha = 0.35f) else null,
    ) {
        Row(
            Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceMd),
        ) {
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(7.dp),
                ) {
                    Box(Modifier.size(8.dp).clip(CircleShape).background(dot))
                    Text(
                        title,
                        style = ManisType.sectionTitle,
                        color = if (degraded) colors.statusWarning else colors.textPrimary,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceXs),
                ) {
                    Text(
                        if (connected) {
                            displayProxyName(exit) ?: "未选择出口"
                        } else {
                            "待命出口：${displayProxyName(exit) ?: "未选择"}"
                        },
                        modifier = Modifier.weight(1f, fill = false),
                        style = ManisType.body,
                        color = colors.textSecondary,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                    if (connected) ManisLatency(delay)
                }
            }
            PowerPill(connected = connected, onToggle = onToggle)
        }
        if (connected) TelemetryStrip()
        RouteModeControl(mode = mode, onSelect = onModeSelect)
    }
}

/**
 * The page's one action: a cobalt pill while the tunnel is down, a success-soft pill with the power
 * glyph while it is up. The pill is 44dp as drawn; the touch target around it is the 48dp minimum.
 */
@Composable
private fun PowerPill(connected: Boolean, onToggle: () -> Unit, compact: Boolean = false) {
    val colors = LocalManisColors.current
    val container = if (connected) colors.statusSuccessSoft else colors.actionPrimary
    val content = if (connected) colors.statusSuccess else colors.actionOnPrimary
    Box(
        Modifier
            .height(ManisDimens.minTouch)
            .clip(CircleShape)
            .clickable(onClick = onToggle),
        contentAlignment = Alignment.Center,
    ) {
        Row(
            Modifier
                .height(if (compact) 34.dp else 44.dp)
                .clip(CircleShape)
                .background(container)
                .then(
                    if (connected) {
                        Modifier.border(
                            1.dp,
                            colors.statusSuccess.copy(alpha = 0.35f),
                            CircleShape,
                        )
                    } else {
                        Modifier
                    },
                )
                .padding(
                    horizontal = if (compact) ManisDimens.spaceMd else ManisDimens.spaceLg,
                ),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(7.dp),
        ) {
            Icon(
                painter = painterResource(R.drawable.ic_power),
                contentDescription = null,
                modifier = Modifier.size(if (compact) 14.dp else 16.dp),
                tint = content,
            )
            Text(
                if (connected) "断开" else "连接",
                style = if (compact) {
                    ManisType.metadata.copy(fontSize = 13.sp, fontWeight = FontWeight.SemiBold)
                } else {
                    ManisType.label.copy(fontWeight = FontWeight.SemiBold)
                },
                color = content,
                maxLines = 1,
            )
        }
    }
}

/** The session line under the hero's state: what has moved, and over how many connections. */
@Composable
private fun TelemetryStrip() {
    val colors = LocalManisColors.current
    Row(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(ManisDimens.radiusControl))
            .background(colors.surfaceLow)
            .padding(horizontal = ManisDimens.spaceMd, vertical = ManisDimens.spaceSm),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.SpaceBetween,
    ) {
        Text(
            "↓ ${formatTraffic(RuntimeState.downloadTotal)} · " +
                "↑ ${formatTraffic(RuntimeState.uploadTotal)}",
            style = ManisType.data,
            color = colors.textSecondary,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        Text(
            "${RuntimeState.connectionCount} 活动连接",
            style = ManisType.data,
            color = colors.textSecondary,
            maxLines = 1,
        )
    }
}

/**
 * The route mode as three equal segments across the card, 44dp tall: a recessed track, an elevated
 * thumb on the active segment, and one tap per mode. [ManisSegmented] owns a fixed 40dp height, so
 * the hero carries this 44dp variant instead of squeezing the shared control.
 */
@Composable
private fun RouteModeControl(mode: String, onSelect: (String) -> Unit) {
    val colors = LocalManisColors.current
    val dark = colors.surfaceBase.luminance() < 0.5f
    Row(
        Modifier
            .fillMaxWidth()
            .height(44.dp)
            .clip(RoundedCornerShape(ManisDimens.radiusControl))
            .background(colors.surfaceLow)
            .border(1.dp, colors.outlineSubtle, RoundedCornerShape(ManisDimens.radiusControl))
            .padding(3.dp),
        horizontalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        for (value in routeModes) {
            val selected = mode == value
            Box(
                Modifier
                    .weight(1f)
                    .fillMaxHeight()
                    .clip(RoundedCornerShape(7.dp))
                    .background(
                        if (selected) {
                            if (dark) colors.surfaceHigh else colors.surfaceElevated
                        } else {
                            Color.Transparent
                        },
                    )
                    .clickable { onSelect(value) },
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    routeModeLabel(value),
                    style = ManisType.label,
                    color = if (selected) colors.textPrimary else colors.textSecondary,
                    fontWeight = if (selected) FontWeight.SemiBold else FontWeight.Medium,
                    maxLines = 1,
                )
            }
        }
    }
}

/**
 * The bar that takes over once the hero is out of sight: the same state dot, the same exit and the
 * same button, at 52dp. It sits over the top of the scrolling list, so the bottom bar never moves.
 */
@Composable
private fun MiniBarOverlay(
    visible: Boolean,
    connected: Boolean,
    exit: String?,
    unreachable: Boolean,
    onToggle: () -> Unit,
    modifier: Modifier = Modifier,
) {
    AnimatedVisibility(
        visible = visible,
        modifier = modifier,
        enter = fadeIn(tween(220)) + slideInVertically(tween(220)) { -it / 2 },
        exit = fadeOut(tween(140)) + slideOutVertically(tween(140)) { -it / 2 },
    ) {
        MiniConnectionBar(
            connected = connected,
            exit = exit,
            unreachable = unreachable,
            onToggle = onToggle,
        )
    }
}

@Composable
private fun MiniConnectionBar(
    connected: Boolean,
    exit: String?,
    unreachable: Boolean,
    onToggle: () -> Unit,
) {
    val colors = LocalManisColors.current
    val degraded = connected && unreachable
    Column(Modifier.fillMaxWidth().background(colors.surfaceChrome)) {
        Row(
            Modifier
                .fillMaxWidth()
                .height(52.dp)
                .padding(horizontal = ManisDimens.spaceLg),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
        ) {
            Box(
                Modifier
                    .size(8.dp)
                    .clip(CircleShape)
                    .background(
                        when {
                            !connected -> colors.textTertiary
                            degraded -> colors.statusWarning
                            else -> colors.statusSuccess
                        },
                    ),
            )
            Text(
                when {
                    !connected -> "未连接 · ${displayProxyName(exit) ?: "未选择出口"}"
                    degraded -> "已保护 · 节点不可达"
                    else -> "已保护 · ${displayProxyName(exit) ?: "未选择出口"}"
                },
                modifier = Modifier.weight(1f),
                style = ManisType.label,
                color = if (degraded) colors.statusWarning else colors.textPrimary,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            PowerPill(connected = connected, onToggle = onToggle, compact = true)
        }
        ManisDivider()
    }
}

/**
 * One policy group: its name, strategy and current exit, a per-group test, and the members it picks
 * from once expanded. Every member row clears the 48dp touch minimum.
 */
@Composable
private fun PolicyGroupCard(
    title: String,
    strategy: String,
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
    val currentDelay = current?.let { delays[it] ?: RuntimeState.snapshot?.nodes?.get(it)?.delay }
    ManisCard {
        Column(
            Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(ManisDimens.radiusRow))
                .background(if (pressed) colors.surfaceLow else Color.Transparent)
                .clickable(
                    interactionSource = interaction,
                    indication = null,
                    onClick = onToggle,
                ),
            verticalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
        ) {
            Row(
                Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
            ) {
                Row(
                    Modifier.weight(1f),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
                ) {
                    Text(
                        title,
                        modifier = Modifier.weight(1f, fill = false),
                        style = ManisType.label.copy(fontWeight = FontWeight.SemiBold),
                        color = colors.textPrimary,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                    ManisPill(strategyLabel(strategy), ManisTone.Neutral)
                }
                Icon(
                    imageVector = Icons.Filled.KeyboardArrowDown,
                    contentDescription = if (expanded) "收起" else "展开",
                    modifier = Modifier.size(22.dp).rotate(if (expanded) 180f else 0f),
                    tint = colors.textSecondary,
                )
            }
            // The current exit sits on its own recessed strip, the way the prototype separates a
            // group's choice from its name.
            Row(
                Modifier
                    .fillMaxWidth()
                    .heightIn(min = ManisDimens.minTouch)
                    .clip(RoundedCornerShape(ManisDimens.radiusControl))
                    .background(colors.surfaceLow)
                    .padding(start = ManisDimens.spaceMd, end = ManisDimens.spaceXs),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                Box(
                    Modifier
                        .size(6.dp)
                        .clip(CircleShape)
                        .background(
                            if (current == null) colors.textTertiary else colors.statusSuccess,
                        ),
                )
                Text(
                    displayProxyName(current) ?: "未选择",
                    modifier = Modifier.weight(1f, fill = false),
                    style = ManisType.body,
                    color = colors.textPrimary,
                    fontWeight = FontWeight.SemiBold,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                ManisLatency(currentDelay)
                Spacer(Modifier.weight(1f))
                if (canTest) ManisTextAction("测速", onClick = onTest)
            }
        }
        if (expanded) {
            ManisDivider()
            if (members.isEmpty()) {
                ManisMeta("这个策略组还没有成员。")
            }
            for (node in members) {
                MemberRow(
                    name = displayProxyName(node) ?: node,
                    delay = delays[node] ?: RuntimeState.snapshot?.nodes?.get(node)?.delay,
                    selected = node == current,
                    onSelect = { onSelect(node) },
                )
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

/** One member of a policy group: a 48dp chip with the node's name and its measured latency. */
@Composable
private fun MemberRow(name: String, delay: Int?, selected: Boolean, onSelect: () -> Unit) {
    val colors = LocalManisColors.current
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = ManisDimens.minTouch)
            .clip(RoundedCornerShape(ManisDimens.radiusControl))
            .background(if (selected) colors.actionSoft else colors.surfaceBase)
            .border(
                1.dp,
                if (selected) colors.actionPrimary else colors.outlineSubtle,
                RoundedCornerShape(ManisDimens.radiusControl),
            )
            .clickable(onClick = onSelect)
            .padding(horizontal = ManisDimens.spaceMd),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
    ) {
        Text(
            name,
            modifier = Modifier.weight(1f),
            style = ManisType.body,
            color = if (selected) colors.actionPrimary else colors.textPrimary,
            fontWeight = if (selected) FontWeight.SemiBold else FontWeight.Normal,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        ManisLatency(delay)
    }
}

/** One node inside a source: region code, name, protocol, latency and its own test. */
@Composable
private fun NodeRow(
    name: String,
    protocol: String,
    delay: Int?,
    selected: Boolean,
    onSelect: () -> Unit,
    onTest: (() -> Unit)?,
) {
    val colors = LocalManisColors.current
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = ManisDimens.rowHeight)
            .clip(RoundedCornerShape(ManisDimens.radiusRow))
            .background(if (selected) colors.actionSoft else Color.Transparent)
            .clickable(onClick = onSelect)
            .padding(horizontal = ManisDimens.spaceSm, vertical = ManisDimens.spaceXs),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
    ) {
        ManisRegionBadge(regionCode(name))
        Column(Modifier.weight(1f)) {
            Text(
                name,
                style = ManisType.body,
                color = if (selected) colors.actionPrimary else colors.textPrimary,
                fontWeight = if (selected) FontWeight.SemiBold else FontWeight.Normal,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                protocol.uppercase(),
                style = ManisType.metadata,
                color = colors.textTertiary,
                maxLines = 1,
            )
        }
        ManisLatency(delay)
        if (onTest != null) ManisTextAction("测速", onClick = onTest)
    }
}

/** A source card: its node count, its vehicle and the nodes it carries. */
@Composable
private fun SourceCard(
    title: String,
    meta: String,
    expanded: Boolean,
    onToggle: () -> Unit,
    content: @Composable ColumnScope.() -> Unit,
) {
    val colors = LocalManisColors.current
    // No usage meter here: the Android subscription model carries no quota or traffic allowance, and
    // a bar drawn from a number the app does not have would be a guess.
    ManisCard {
        Row(
            Modifier
                .fillMaxWidth()
                .heightIn(min = ManisDimens.minTouch)
                .clip(RoundedCornerShape(ManisDimens.radiusRow))
                .clickable(onClick = onToggle),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
        ) {
            Column(Modifier.weight(1f)) {
                Text(
                    title,
                    style = ManisType.label.copy(fontWeight = FontWeight.SemiBold),
                    color = colors.textPrimary,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    meta,
                    style = ManisType.metadata,
                    color = colors.textTertiary,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
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

/** The routing modes in the order the segments read them: cut out, filter, send everything. */
private val routeModes = listOf("direct", "rule", "global")

/** What the hero calls the active mode once the tunnel is up. */
private fun modeWord(mode: String): String = when (mode) {
    "direct" -> "直连模式"
    "global" -> "全局模式"
    else -> "规则分流"
}

/**
 * The 2-letter region code the design uses in place of flag emoji: the first hint a node name
 * carries, otherwise its first two ASCII letters.
 */
internal fun regionCode(name: String): String {
    val upper = name.uppercase()
    for ((code, hints) in regionHints) {
        if (hints.any { upper.contains(it) }) return code
    }
    return upper.filter { it in 'A'..'Z' }.take(2).ifEmpty { "--" }
}

private val regionHints = listOf(
    "HK" to listOf("香港", "HK", "HONG KONG", "HONGKONG"),
    "TW" to listOf("台湾", "臺灣", "TW", "TAIWAN"),
    "SG" to listOf("新加坡", "狮城", "SG", "SINGAPORE"),
    "JP" to listOf("日本", "东京", "大阪", "JP", "JAPAN", "TOKYO"),
    "KR" to listOf("韩国", "首尔", "KR", "KOREA", "SEOUL"),
    "US" to listOf("美国", "洛杉矶", "圣何塞", "西雅图", "US", "USA", "UNITED STATES"),
    "CA" to listOf("加拿大", "CA", "CANADA"),
    "GB" to listOf("英国", "伦敦", "GB", "UK", "UNITED KINGDOM", "LONDON"),
    "DE" to listOf("德国", "法兰克福", "DE", "GERMANY", "FRANKFURT"),
    "NL" to listOf("荷兰", "阿姆斯特丹", "NL", "NETHERLANDS"),
    "FR" to listOf("法国", "巴黎", "FR", "FRANCE", "PARIS"),
    "RU" to listOf("俄罗斯", "RU", "RUSSIA"),
    "TR" to listOf("土耳其", "TR", "TURKEY"),
    "IN" to listOf("印度", "IN", "INDIA"),
    "AU" to listOf("澳大利亚", "悉尼", "AU", "AUSTRALIA", "SYDNEY"),
    "BR" to listOf("巴西", "BR", "BRAZIL"),
    "AR" to listOf("阿根廷", "AR", "ARGENTINA"),
    "MY" to listOf("马来西亚", "MY", "MALAYSIA"),
    "TH" to listOf("泰国", "TH", "THAILAND"),
    "VN" to listOf("越南", "VN", "VIETNAM"),
    "PH" to listOf("菲律宾", "PH", "PHILIPPINES"),
    "ID" to listOf("印尼", "印度尼西亚", "ID", "INDONESIA"),
)

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
