package dev.manis.app.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import dev.manis.app.core.CoreController
import dev.manis.app.data.Workspace
import java.io.File

@Composable
fun SettingsScreen(onMessage: (String) -> Unit) {
    val context = LocalContext.current
    var showLogs by remember { mutableStateOf(false) }
    var showAbout by remember { mutableStateOf(false) }
    var showHosts by remember { mutableStateOf(false) }
    val config = Workspace.config.value
    val abi = android.os.Build.SUPPORTED_ABIS.firstOrNull() ?: "unknown"

    Column(
        Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(20.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        ScreenTitle("设置")
        ApplyBar()

        SectionCard("运行状态") {
            InfoRow("状态", if (RuntimeState.active) "已连接" else "未连接")
            InfoRow("路由模式", RuntimeState.snapshot?.mode ?: config.mode)
            InfoRow("内核版本", RuntimeState.snapshot?.version?.ifBlank { "—" } ?: "—")
            InfoRow("配置", RuntimeState.summary.ifBlank { "—" })
            InfoRow("工作区", "${config.subscriptions.size} 订阅 · ${config.nodes.size} 节点 · ${config.groups.size} 策略组")
            InfoRow("设备 ABI", abi)
            InfoRow("地址覆盖", "${config.hosts.size} 条")
            InfoRow("数据目录", "files/mihomo")
        }

        SectionCard("维护") {
            TextButton(onClick = {
                applyWorkspace(context)
                onMessage("已重新载入配置")
            }) { Text("重新载入配置") }
            TextButton(onClick = { showLogs = true }) { Text("查看内核日志") }
            TextButton(onClick = { showHosts = true }) { Text("节点地址覆盖") }
        }

        SectionCard("关于") {
            Text("Manis Android 实验版", style = MaterialTheme.typography.bodyLarge)
            Text(
                "Mihomo 内核 v1.19.33 + Android 兼容补丁，随 APK 以 GPL-3.0 分发；" +
                    "补丁与构建脚本见仓库 android/ 目录。",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            TextButton(onClick = { showAbout = true }) { Text("开源与许可") }
        }
    }

    if (showLogs) {
        LogsDialog(onDismiss = { showLogs = false })
    }

    if (showHosts) {
        HostsDialog(
            initial = config.hosts,
            onDismiss = { showHosts = false },
            onSave = { hosts ->
                Workspace.update { it.copy(hosts = hosts) }
                applyWorkspace(context)
                showHosts = false
                onMessage("已保存 ${hosts.size} 条地址覆盖并重新载入")
            },
        )
    }

    if (showAbout) {
        AlertDialog(
            onDismissRequest = { showAbout = false },
            title = { Text("许可") },
            text = {
                Text(
                    "Manis 应用代码为 Apache-2.0。Mihomo 是 GPL-3.0，APK 内打包了自行编译的内核，" +
                        "对应源码与补丁随仓库发布。",
                )
            },
            confirmButton = { TextButton(onClick = { showAbout = false }) { Text("知道了") } },
        )
    }
}

@Composable
private fun InfoRow(label: String, value: String) {
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
        Text(
            label,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Text(value, style = MaterialTheme.typography.bodyMedium)
    }
}

@Composable
private fun LogsDialog(onDismiss: () -> Unit) {
    val context = LocalContext.current
    var lines by remember { mutableStateOf(readLog(context)) }

    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("内核日志") },
        text = {
            LazyColumn(Modifier.fillMaxWidth().heightIn(max = 420.dp)) {
                items(lines) { line ->
                    Text(
                        line,
                        style = MaterialTheme.typography.bodySmall.copy(
                            fontFamily = androidx.compose.ui.text.font.FontFamily.Monospace,
                        ),
                    )
                }
            }
        },
        confirmButton = { TextButton(onClick = { lines = readLog(context) }) { Text("刷新") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("关闭") } },
    )
}

private fun readLog(context: android.content.Context): List<String> {
    val file = File(context.filesDir, "mihomo/core.log")
    if (!file.isFile) return listOf("还没有日志：连接一次 VPN 后生成。")
    return runCatching {
        file.readLines().takeLast(300)
    }.getOrDefault(listOf("日志读取失败"))
}

@Composable
private fun HostsDialog(
    initial: Map<String, String>,
    onDismiss: () -> Unit,
    onSave: (Map<String, String>) -> Unit,
) {
    var text by remember {
        mutableStateOf(initial.entries.joinToString("\n") { (host, address) -> "$host=$address" })
    }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("节点地址覆盖") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(
                    "每行一条「域名=IP」。当机场 DNS 返回的地址在你当前网络不可达时，用它把节点域名" +
                        "固定到能连通的 IP；订阅更新后依然生效。",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                OutlinedTextField(
                    value = text,
                    onValueChange = { text = it },
                    modifier = Modifier.fillMaxWidth().heightIn(min = 140.dp),
                    textStyle = MaterialTheme.typography.bodySmall,
                    placeholder = { Text("naiu-hk.example.com=203.0.113.7") },
                )
            }
        },
        confirmButton = {
            TextButton(onClick = {
                val hosts = buildMap {
                    for (line in text.lines()) {
                        val trimmed = line.trim()
                        if (trimmed.isEmpty() || !trimmed.contains('=')) continue
                        val host = trimmed.substringBefore('=').trim()
                        val address = trimmed.substringAfter('=').trim()
                        if (host.isNotEmpty() && address.isNotEmpty()) put(host, address)
                    }
                }
                onSave(hosts)
            }) { Text("保存") }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("取消") } },
    )
}
