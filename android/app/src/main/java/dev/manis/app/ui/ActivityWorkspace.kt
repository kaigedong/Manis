package dev.manis.app.ui

import androidx.compose.foundation.background
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
import androidx.compose.material3.Surface
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
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.manis.app.core.ConnectionInfo
import dev.manis.app.core.ConnectionsSnapshot
import dev.manis.app.core.ControllerApi
import dev.manis.app.data.RuleEntry
import dev.manis.app.data.Workspace
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/** The three route segments one live connection proves, plus the tone its exit earns. */
private data class ObservedRoute(
    val rule: String,
    val group: String,
    val exit: String,
    val latency: Int?,
    val direct: Boolean,
    val rejected: Boolean,
)

/**
 * The activity workspace: the live connections of the running kernel, each one drawn as its
 * observed route — the rule that matched, the policy group that took it, and the exit it reached.
 * Tapping a card opens the rule sheet with the host already filled in.
 */
@Composable
fun ActivityWorkspace(onMessage: (String) -> Unit) {
    val colors = LocalManisColors.current
    var snapshot by remember { mutableStateOf<ConnectionsSnapshot?>(null) }
    var refreshing by remember { mutableStateOf(false) }
    var query by remember { mutableStateOf("") }
    var draft by remember { mutableStateOf<RuleEditor?>(null) }
    val config = Workspace.config.value
    val scope = rememberCoroutineScope()

    LaunchedEffect(Unit) {
        while (true) {
            if (RuntimeState.active) {
                runCatching { ControllerApi.connections() }
                    .onSuccess { snapshot = it }
            } else {
                snapshot = null
            }
            delay(2_000)
        }
    }

    val allConnections = snapshot?.connections ?: emptyList()
    val connections = remember(allConnections, query) {
        val newestFirst = allConnections.sortedByDescending { it.start }
        if (query.isBlank()) {
            newestFirst
        } else {
            newestFirst.filter { connection ->
                connection.host.contains(query, ignoreCase = true) ||
                    connection.rule.contains(query, ignoreCase = true) ||
                    connection.chains.any { it.contains(query, ignoreCase = true) }
            }
        }
    }

    val state = draft
    if (state != null) {
        RuleEditorPane(
            initial = state.entry,
            editingIndex = null,
            existing = config.ruleEntries,
            policies = policyOptions(config),
            groups = config.ruleGroups,
            onCancel = { draft = null },
            onDelete = { draft = null },
            onConfirm = { entry ->
                Workspace.update { it.copy(ruleEntries = it.ruleEntries + entry) }
                draft = null
                onMessage("已添加规则，点「应用」后生效")
            },
        )
    }

    Column(Modifier.fillMaxSize().background(colors.surfaceBase)) {
        ManisPageHeader(title = "网络活动")
        Column(
            Modifier
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(
                    start = ManisDimens.spaceLg,
                    end = ManisDimens.spaceLg,
                    bottom = ManisDimens.spaceXl,
                ),
            verticalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
        ) {
            Row(
                Modifier.fillMaxWidth().height(ManisDimens.controlStandard),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
            ) {
                Box(Modifier.weight(1f)) {
                    ManisSearchField(
                        value = query,
                        onValueChange = { query = it },
                        placeholder = "搜索域名、规则或出口",
                    )
                }
                ManisSectionAction(if (refreshing) "刷新中…" else "刷新") {
                    if (!RuntimeState.active) {
                        onMessage("先连接 VPN")
                    } else {
                        refreshing = true
                        scope.launch {
                            runCatching { ControllerApi.connections() }
                                .onSuccess {
                                    snapshot = it
                                    onMessage("已刷新连接")
                                }
                                .onFailure { onMessage("读取连接失败") }
                            refreshing = false
                        }
                    }
                }
            }
            // One compact mono line: how many connections, and what they have moved.
            Text(
                summaryLine(
                    shown = connections.size,
                    total = allConnections.size,
                    download = if (query.isBlank()) {
                        allConnections.sumOf { it.download }
                    } else {
                        connections.sumOf { it.download }
                    },
                    upload = if (query.isBlank()) {
                        allConnections.sumOf { it.upload }
                    } else {
                        connections.sumOf { it.upload }
                    },
                    pending = snapshot == null && RuntimeState.active,
                    idle = !RuntimeState.active && query.isBlank(),
                ),
                style = ManisType.data.copy(fontSize = 11.5.sp),
                color = colors.textTertiary,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            if (!RuntimeState.active) {
                ManisCard {
                    Text("暂无活动连接", style = ManisType.label, color = colors.textPrimary)
                    ManisMeta("连接 VPN 后，这里会显示每条连接的域名、规则与流量。")
                }
            } else if (connections.isEmpty()) {
                ManisCard {
                    ManisMeta("还没有连接；有流量经过内核时会出现在这里。")
                }
            } else {
                for (connection in connections.take(100)) {
                    ConnectionCard(connection, onClick = { draft = ruleDraft(connection) })
                }
            }
        }
    }
}

/**
 * The rule a tap on a connection would create, mirroring the desktop: a domain suffix for a
 * hostname and a single-address CIDR for an IP-only connection.
 */
private fun ruleDraft(connection: ConnectionInfo): RuleEditor? {
    val host = connection.host.trim()
    val address = connection.destinationIp.trim()
    val ipv6 = address.count { it == ':' } >= 2
    val ipv4 = Regex("^\\d{1,3}(\\.\\d{1,3}){3}$").matches(address)
    val entry = when {
        host.isNotEmpty() && host != address -> RuleEntry(kind = "DOMAIN-SUFFIX", value = host)
        ipv6 -> RuleEntry(kind = "IP6-CIDR", value = "$address/128")
        ipv4 -> RuleEntry(kind = "IP-CIDR", value = "$address/32")
        else -> null
    }
    return entry?.let { RuleEditor(null, it) }
}

/** Rule, group, exit and the tone of the exit, read from what the core reported for this link. */
private fun observedRoute(connection: ConnectionInfo): ObservedRoute {
    // The core lists the chain innermost first: the proxy that carried the traffic, then the
    // policy groups above it, ending at the built-in GLOBAL group.
    val chains = connection.chains.filter { it.isNotBlank() }
    val exit = chains.firstOrNull() ?: connection.destinationIp.ifBlank { "未知" }
    val group = chains.drop(1).firstOrNull { it != "GLOBAL" }
        ?: chains.drop(1).firstOrNull()
        ?: "全局"
    val direct = exit == "DIRECT"
    val rejected = exit == "REJECT"
    // The exit's last measured delay is the only latency the core reports per node.
    val latency = if (direct || rejected) null else RuntimeState.snapshot?.nodes?.get(exit)?.delay
    return ObservedRoute(
        rule = connection.rule.ifBlank { "未命中规则" },
        group = group,
        exit = exit,
        latency = latency,
        direct = direct,
        rejected = rejected,
    )
}

/** One live connection: what asked, where it went, and the chain that proves it. */
@Composable
private fun ConnectionCard(connection: ConnectionInfo, onClick: () -> Unit) {
    val colors = LocalManisColors.current
    val route = observedRoute(connection)
    Surface(
        onClick = onClick,
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(ManisDimens.radiusCard),
        color = colors.surfaceElevated,
        border = androidx.compose.foundation.BorderStroke(1.dp, colors.outlineSubtle),
    ) {
        Column(
            Modifier
                .fillMaxWidth()
                .padding(horizontal = ManisDimens.spaceLg, vertical = ManisDimens.spaceMd),
            verticalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
        ) {
            Row(
                Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
            ) {
                Text(
                    connection.host.ifBlank { "未命名连接" },
                    modifier = Modifier.weight(1f),
                    style = ManisType.data,
                    color = colors.textPrimary,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                ManisPill(connection.network.ifBlank { "tcp" }.uppercase(), ManisTone.Neutral)
                Text(
                    "↓ ${formatBytes(connection.download)} · ↑ ${formatBytes(connection.upload)}",
                    style = ManisType.data.copy(fontSize = 11.5.sp),
                    color = colors.textSecondary,
                    maxLines = 1,
                )
            }
            ManisRouteChain(
                rule = route.rule,
                group = route.group,
                exit = route.exit,
                latency = route.latency,
                direct = route.direct,
                rejected = route.rejected,
            )
        }
    }
}

/** The one-line live summary: count, download, upload, and whether it is auto-refreshing. */
private fun summaryLine(
    shown: Int,
    total: Int,
    download: Long,
    upload: Long,
    pending: Boolean,
    idle: Boolean,
): String = when {
    idle -> "0 个连接 · ↓ 0 B · ↑ 0 B"
    pending -> "正在读取连接…"
    shown == total -> "$total 个连接 · ↓ ${formatBytes(download)} · ↑ ${formatBytes(upload)}"
    else -> "$shown/$total 个连接 · ↓ ${formatBytes(download)} · ↑ ${formatBytes(upload)}"
}

private fun formatBytes(bytes: Long): String = when {
    bytes >= 1024L * 1024L * 1024L -> "%.1f GB".format(bytes / (1024.0 * 1024.0 * 1024.0))
    bytes >= 1024L * 1024L -> "%.1f MB".format(bytes / (1024.0 * 1024.0))
    bytes >= 1024L -> "%.1f KB".format(bytes / 1024.0)
    else -> "$bytes B"
}
