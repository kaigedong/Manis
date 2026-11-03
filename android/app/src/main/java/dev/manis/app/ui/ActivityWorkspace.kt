package dev.manis.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
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
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/** The GPUI activity workspace: the live connections of the running kernel. */
@Composable
fun ActivityWorkspace(onMessage: (String) -> Unit) {
    val colors = LocalManisColors.current
    var snapshot by remember { mutableStateOf<ConnectionsSnapshot?>(null) }
    var refreshing by remember { mutableStateOf(false) }
    var query by remember { mutableStateOf("") }
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

    Column(Modifier.fillMaxSize().background(colors.surfaceBase)) {
        Row(
            Modifier
                .fillMaxWidth()
                .padding(horizontal = ManisDimens.spaceMd, vertical = ManisDimens.spaceLg),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
        ) {
            Column(Modifier.weight(1f)) {
                Text("网络活动", style = ManisType.pageTitle, color = colors.textPrimary)
                ManisMeta(
                    when {
                        !RuntimeState.active -> "连接后显示实时连接"
                        snapshot == null -> "正在读取连接…"
                        else -> "${connections.size} 个连接 · ↓ ${formatBytes(connections.sumOf { it.download })} ↑ ${formatBytes(connections.sumOf { it.upload })}"
                    },
                )
            }
            ManisButton(if (refreshing) "刷新中…" else "刷新", onClick = {
                if (!RuntimeState.active) {
                    onMessage("先连接 VPN")
                    return@ManisButton
                }
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
            })
        }
        ManisDivider()
        Column(
            Modifier
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(ManisDimens.spaceMd),
            verticalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
        ) {
            ManisTextField(value = query, onValueChange = { query = it }, placeholder = "搜索连接")
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
                    ConnectionCard(connection)
                }
            }
        }
    }
}

@Composable
private fun ConnectionCard(connection: ConnectionInfo) {
    val colors = LocalManisColors.current
    ManisCard {
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
