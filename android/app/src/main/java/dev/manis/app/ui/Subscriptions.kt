package dev.manis.app.ui

import android.content.Context
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Checkbox
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import dev.manis.app.core.ControllerApi
import dev.manis.app.core.CoreController
import dev.manis.app.data.GroupEntry
import dev.manis.app.data.MobileConfig
import dev.manis.app.data.NodeEntry
import dev.manis.app.data.SubscriptionCache
import dev.manis.app.data.SubscriptionEntry
import dev.manis.app.data.Workspace
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

@Composable
fun SubscriptionsScreen(onMessage: (String) -> Unit) {
    val context = LocalContext.current
    val config = Workspace.config.value
    val scope = rememberCoroutineScope()
    var addSubscription by remember { mutableStateOf(false) }
    var addNode by remember { mutableStateOf(false) }
    var addGroup by remember { mutableStateOf(false) }
    var refreshing by remember { mutableStateOf(false) }

    Column(
        Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(20.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        ScreenTitle("订阅", "订阅、手动节点和策略组都会进入同一份内核配置")
        ApplyBar()

        SectionCard(
            "订阅",
            trailing = {
                Row {
                    TextButton(
                        enabled = !refreshing && config.subscriptions.any { it.enabled },
                        onClick = {
                            refreshing = true
                            scope.launch {
                                val message = refreshSubscriptions(context, Workspace.config.value)
                                refreshing = false
                                onMessage(message)
                            }
                        },
                    ) { Text(if (refreshing) "刷新中…" else "刷新") }
                    TextButton(onClick = { addSubscription = true }) { Text("添加") }
                }
            },
        ) {
            if (config.subscriptions.isEmpty()) {
                Text(
                    "粘贴 Clash 格式订阅地址；添加后会立即下载，点「应用」生效。",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            for ((index, entry) in config.subscriptions.withIndex()) {
                Row(
                    Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Column(Modifier.weight(1f)) {
                        Text(
                            entry.name.ifBlank { "订阅 ${index + 1}" },
                            style = MaterialTheme.typography.bodyLarge,
                        )
                        Text(
                            maskUrl(entry.url),
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                    Switch(
                        checked = entry.enabled,
                        onCheckedChange = { enabled ->
                            Workspace.update { current ->
                                current.copy(
                                    subscriptions = current.subscriptions.mapIndexed { at, item ->
                                        if (at == index) item.copy(enabled = enabled) else item
                                    },
                                )
                            }
                        },
                    )
                    TextButton(onClick = { Workspace.removeSubscription(index) }) { Text("删除") }
                }
            }
        }

        SectionCard(
            "手动节点",
            trailing = { TextButton(onClick = { addNode = true }) { Text("添加") } },
        ) {
            if (config.nodes.isEmpty()) {
                Text(
                    "粘贴 vless:// 分享链接，一行一个。",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            for ((index, entry) in config.nodes.withIndex()) {
                Row(
                    Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Column(Modifier.weight(1f)) {
                        Text(entry.displayName, style = MaterialTheme.typography.bodyLarge)
                        Text(
                            "VLESS",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                    Switch(
                        checked = entry.enabled,
                        onCheckedChange = { enabled ->
                            Workspace.update { current ->
                                current.copy(
                                    nodes = current.nodes.mapIndexed { at, item ->
                                        if (at == index) item.copy(enabled = enabled) else item
                                    },
                                )
                            }
                        },
                    )
                    TextButton(onClick = { Workspace.removeNode(index) }) { Text("删除") }
                }
            }
        }

        SectionCard(
            "策略组",
            trailing = { TextButton(onClick = { addGroup = true }) { Text("添加") } },
        ) {
            if (config.groups.isEmpty()) {
                Text(
                    "全局组始终存在；再加一个「自动测速」组即可按延迟选最快节点。",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            for ((index, group) in config.groups.withIndex()) {
                Row(
                    Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Column(Modifier.weight(1f)) {
                        Text(group.name, style = MaterialTheme.typography.bodyLarge)
                        Text(
                            "${if (group.strategy == "urltest") "自动测速" else "手动选择"} · " +
                                "${group.subscriptionIndexes.size} 个订阅 / " +
                                "${group.nodeIndexes.size} 个节点",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                    TextButton(onClick = {
                        Workspace.update { current ->
                            current.copy(groups = current.groups.filterIndexed { at, _ -> at != index })
                        }
                    }) { Text("删除") }
                }
            }
        }

        Text(
            "修改会先保存；点顶部的「应用」后内核重启并使用已下载的订阅缓存，VPN 接口不会断开。" +
                "点「刷新」会直接下载订阅，不需要先连接 VPN；下载好的节点在连接后立即生效。",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }

    if (addSubscription) {
        AddSubscriptionDialog(
            onDismiss = { addSubscription = false },
            onConfirm = { name, url ->
                Workspace.update { current ->
                    current.copy(
                        subscriptions = current.subscriptions + SubscriptionEntry(name.trim(), url.trim()),
                    )
                }
                addSubscription = false
                onMessage("订阅已添加，正在下载…")
                scope.launch {
                    val message = refreshSubscriptions(context, Workspace.config.value)
                    onMessage(message)
                }
            },
        )
    }

    if (addNode) {
        AddNodeDialog(
            onDismiss = { addNode = false },
            onConfirm = { link ->
                Workspace.update { current ->
                    current.copy(nodes = current.nodes + NodeEntry(link.trim()))
                }
                addNode = false
                onMessage("节点已添加，点「应用」生效")
            },
        )
    }

    if (addGroup) {
        AddGroupDialog(
            config = config,
            onDismiss = { addGroup = false },
            onConfirm = { group ->
                Workspace.update { current -> current.copy(groups = current.groups + group) }
                addGroup = false
                onMessage("策略组已添加，点「应用」生效")
            },
        )
    }
}

@Composable
private fun AddSubscriptionDialog(
    onDismiss: () -> Unit,
    onConfirm: (String, String) -> Unit,
) {
    var name by remember { mutableStateOf("") }
    var url by remember { mutableStateOf("") }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("添加订阅") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                OutlinedTextField(
                    value = name,
                    onValueChange = { name = it },
                    label = { Text("名称（可选）") },
                    singleLine = true,
                )
                OutlinedTextField(
                    value = url,
                    onValueChange = { url = it },
                    label = { Text("订阅地址") },
                    placeholder = { Text("https://…") },
                    singleLine = true,
                )
            }
        },
        confirmButton = {
            TextButton(
                onClick = { onConfirm(name, url) },
                enabled = url.isNotBlank(),
            ) { Text("添加") }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("取消") } },
    )
}

@Composable
private fun AddNodeDialog(
    onDismiss: () -> Unit,
    onConfirm: (String) -> Unit,
) {
    var link by remember { mutableStateOf("") }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("添加 VLESS 节点") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                OutlinedTextField(
                    value = link,
                    onValueChange = { link = it },
                    label = { Text("vless://…") },
                    minLines = 4,
                    maxLines = 8,
                )
                Text(
                    "支持标准 VLESS 分享链接（TLS / WebSocket / Reality）。",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        },
        confirmButton = {
            TextButton(
                onClick = { onConfirm(link) },
                enabled = link.startsWith("vless://"),
            ) { Text("添加") }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("取消") } },
    )
}

@Composable
private fun AddGroupDialog(
    config: dev.manis.app.data.MobileConfig,
    onDismiss: () -> Unit,
    onConfirm: (GroupEntry) -> Unit,
) {
    var name by remember { mutableStateOf("") }
    var strategy by remember { mutableStateOf("urltest") }
    var subscriptions by remember { mutableStateOf(config.subscriptions.indices.toSet()) }
    var nodes by remember { mutableStateOf(config.nodes.indices.toSet()) }

    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("添加策略组") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                OutlinedTextField(
                    value = name,
                    onValueChange = { name = it },
                    label = { Text("组名") },
                    singleLine = true,
                )
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    FilterChip(
                        selected = strategy == "select",
                        onClick = { strategy = "select" },
                        label = { Text("手动选择") },
                    )
                    FilterChip(
                        selected = strategy == "urltest",
                        onClick = { strategy = "urltest" },
                        label = { Text("自动测速") },
                    )
                }
                Text("成员", style = MaterialTheme.typography.titleSmall)
                for ((index, entry) in config.subscriptions.withIndex()) {
                    Row(
                        Modifier.fillMaxWidth().clickable {
                            subscriptions = subscriptions.toggle(index)
                        },
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Checkbox(
                            checked = index in subscriptions,
                            onCheckedChange = { subscriptions = subscriptions.toggle(index) },
                        )
                        Text(entry.name.ifBlank { "订阅 ${index + 1}" })
                    }
                }
                for ((index, entry) in config.nodes.withIndex()) {
                    Row(
                        Modifier.fillMaxWidth().clickable { nodes = nodes.toggle(index) },
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Checkbox(
                            checked = index in nodes,
                            onCheckedChange = { nodes = nodes.toggle(index) },
                        )
                        Text(entry.displayName)
                    }
                }
            }
        },
        confirmButton = {
            TextButton(
                onClick = {
                    onConfirm(
                        GroupEntry(
                            name = name.trim(),
                            strategy = strategy,
                            subscriptionIndexes = subscriptions.sorted(),
                            nodeIndexes = nodes.sorted(),
                        ),
                    )
                },
                enabled = name.isNotBlank() && (subscriptions.isNotEmpty() || nodes.isNotEmpty()),
            ) { Text("添加") }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("取消") } },
    )
}

private fun Set<Int>.toggle(value: Int): Set<Int> =
    if (value in this) this - value else this + value

/**
 * Refreshes subscriptions without requiring the VPN: the download happens in the app and lands in
 * the core's provider cache. When the VPN is already running, the core is also asked to refresh so
 * the visible node list updates immediately.
 */
private suspend fun refreshSubscriptions(context: Context, config: MobileConfig): String {
    val cached = withContext(Dispatchers.IO) { SubscriptionCache.refresh(context, config) }
    val parts = mutableListOf<String>()
    if (cached.downloaded > 0) {
        parts += if (RuntimeState.active) {
            "已更新 ${cached.downloaded} 个订阅（${cached.nodes} 个节点）"
        } else {
            "已下载 ${cached.downloaded} 个订阅（${cached.nodes} 个节点），连接后生效"
        }
    } else if (cached.nodes > 0) {
        parts += "沿用本地缓存（${cached.nodes} 个节点）"
    }
    if (cached.failures.isNotEmpty()) {
        parts += "失败：${cached.failures.first()}"
        if (cached.failures.size > 1) parts += "另有 ${cached.failures.size - 1} 个订阅失败"
    }
    if (RuntimeState.active) {
        ControllerApi.configure(CoreController.secret)
        runCatching { ControllerApi.refreshSubscriptions() }
            .onSuccess { parts += it }
            .onFailure {
                // The downloaded cache is already on disk; restarting the core makes it use it.
                applyWorkspace(context)
                parts += "内核刷新失败，已用本地缓存重新载入"
            }
    }
    return parts.joinToString("；").ifBlank { "没有启用的订阅" }
}

private fun maskUrl(url: String): String {
    val scheme = url.substringBefore("://", "")
    val rest = url.substringAfter("://", url)
    val host = rest.substringBefore('/').substringBefore('?')
    return if (scheme.isBlank()) host else "$scheme://$host/…"
}
