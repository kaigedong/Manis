package dev.manis.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
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
import androidx.compose.ui.unit.dp
import dev.manis.app.core.ConnectionInfo
import dev.manis.app.core.ConnectionsSnapshot
import dev.manis.app.core.ControllerApi
import dev.manis.app.data.RuleEntry
import dev.manis.app.data.Workspace
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/** The GPUI activity workspace: the live connections of the running kernel. */
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
        if (query.isBlank()) {
            allConnections
        } else {
            allConnections.filter { connection ->
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
        Column(
            Modifier
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(ManisDimens.spaceMd),
            verticalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
        ) {
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Box(Modifier.weight(1f)) {
                    ManisTextField(value = query, onValueChange = { query = it }, placeholder = "搜索连接")
                }
                ManisTextAction(if (refreshing) "刷新中…" else "刷新") {
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
            ManisMeta(
                when {
                    !RuntimeState.active && query.isBlank() -> "0 个连接 · ↓ 0 B · ↑ 0 B"
                    snapshot == null -> "正在读取连接…"
                    query.isBlank() -> "${allConnections.size} 个连接 · ↓ ${formatBytes(allConnections.sumOf { it.download })} · ↑ ${formatBytes(allConnections.sumOf { it.upload })}"
                    else -> "${connections.size}/${allConnections.size} 个连接 · ↓ ${formatBytes(connections.sumOf { it.download })} · ↑ ${formatBytes(connections.sumOf { it.upload })}"
                },
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

@Composable
private fun ConnectionCard(connection: ConnectionInfo, onClick: () -> Unit) {
    val colors = LocalManisColors.current
    ManisCard(onClick = onClick) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text(
                    connection.host.ifBlank { "未命名连接" },
                    style = ManisType.body,
                    color = colors.textPrimary,
                )
                ManisMeta(
                    buildString {
                        append(connection.network.ifBlank { "tcp" })
                        if (connection.rule.isNotBlank()) append(" · ${connection.rule}")
                        if (connection.chains.isNotEmpty()) append(" · ${connection.chains.joinToString(" → ")}")
                    },
                )
            }
            Column(horizontalAlignment = Alignment.End) {
                Text(
                    "↑ ${formatBytes(connection.upload)}",
                    style = ManisType.metadata,
                    color = colors.textSecondary,
                )
                Text(
                    "↓ ${formatBytes(connection.download)}",
                    style = ManisType.metadata,
                    color = colors.textSecondary,
                )
            }
        }
    }
}

private fun formatBytes(bytes: Long): String = when {
    bytes >= 1024L * 1024L -> "%.1f MB".format(bytes / (1024.0 * 1024.0))
    bytes >= 1024L -> "%.1f KB".format(bytes / 1024.0)
    else -> "$bytes B"
}
