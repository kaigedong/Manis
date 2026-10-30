package dev.manis.app.ui

import android.content.ClipData

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
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
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
import dev.manis.app.data.MobileConfig
import dev.manis.app.data.PROXY_DNS_DOH
import dev.manis.app.data.PROXY_DNS_SYSTEM
import dev.manis.app.data.Workspace
import java.io.File

@Composable
fun SettingsScreen(onMessage: (String) -> Unit) {
    val context = LocalContext.current
    var showLogs by remember { mutableStateOf(false) }
    var showAbout by remember { mutableStateOf(false) }
    var showProxyDns by remember { mutableStateOf(false) }
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

        SectionCard("连接") {
            InfoRow("状态", if (RuntimeState.active) "已连接" else "未连接")
            InfoRow("路由模式", RuntimeState.snapshot?.mode ?: config.mode)
            InfoRow("节点域名解析", proxyDnsLabel(config.proxyDns, config.proxyDnsServers, config.discoveredDns))
            TextButton(onClick = { showProxyDns = true }) { Text("节点域名解析") }
        }

        SectionCard("配置摘要") {
            InfoRow("内核版本", RuntimeState.snapshot?.version?.ifBlank { "—" } ?: "—")
            InfoRow("配置", RuntimeState.summary.ifBlank { "—" })
            InfoRow("工作区", "${config.subscriptions.size} 订阅 · ${config.nodes.size} 节点 · ${config.groups.size} 策略组")
            InfoRow("设备 ABI", abi)
            InfoRow("数据目录", "files/mihomo")
            TextButton(onClick = {
                applyWorkspace(context)
                onMessage("已重新载入配置")
            }) { Text("重新载入配置") }
        }

        SectionCard("备份与恢复") {
            Text(
                "导出的是 Manis 备份格式（含订阅链接和节点密钥），不是交给其他客户端的 YAML。",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Row {
                TextButton(onClick = {
                    val clipboard = context.getSystemService(android.content.ClipboardManager::class.java)
                    clipboard?.setPrimaryClip(
                        ClipData.newPlainText("Manis 配置", Workspace.config.value.toJson()),
                    )
                    onMessage("配置已复制到剪贴板")
                }) { Text("复制到剪贴板") }
                TextButton(onClick = {
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
                }) { Text("从剪贴板导入") }
            }
        }

        SectionCard("诊断") {
            TextButton(onClick = { showLogs = true }) { Text("查看内核日志") }
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

    if (showProxyDns) {
        ProxyDnsDialog(
            current = config.proxyDns,
            custom = config.proxyDnsServers,
            discovered = config.discoveredDns,
            onDismiss = { showProxyDns = false },
            onSelect = { choice ->
                Workspace.update { it.copy(proxyDns = choice, proxyDnsServers = emptyList()) }
                applyWorkspace(context)
                showProxyDns = false
                onMessage("节点域名改用 ${proxyDnsLabel(choice, emptyList(), emptyList())} 解析并重新载入")
            },
            onSaveCustom = { servers ->
                Workspace.update { it.copy(proxyDnsServers = servers) }
                applyWorkspace(context)
                showProxyDns = false
                onMessage(
                    if (servers.isEmpty()) "已清除自定义 DoH，重新载入"
                    else "已保存 ${servers.size} 条自定义 DoH 并重新载入",
                )
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
    var copied by remember { mutableStateOf(false) }

    fun copyLog() {
        val clipboard = context.getSystemService(android.content.ClipboardManager::class.java)
        clipboard?.setPrimaryClip(
            ClipData.newPlainText("Manis 内核日志", lines.joinToString("\n")),
        )
        copied = true
    }

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
        confirmButton = {
            Row {
                TextButton(onClick = { copyLog() }) { Text(if (copied) "已复制" else "复制") }
                TextButton(onClick = {
                    lines = readLog(context)
                    copied = false
                }) { Text("刷新") }
            }
        },
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

private fun proxyDnsLabel(mode: String, custom: List<String>, discovered: List<String>): String = when {
    custom.isNotEmpty() -> "自定义 DoH（${custom.size} 条）"
    discovered.isNotEmpty() -> "机场 DNS（自动 ${discovered.size} 条）"
    mode == PROXY_DNS_SYSTEM -> "本机 DNS"
    else -> "加密 DNS（DoH）"
}

/**
 * Chooses how the core turns proxy server hostnames into addresses. This is the one part of the
 * generated config that differs from the desktop build, so it is also the first thing to check
 * when every node times out on the phone but works on a computer.
 */
@Composable
private fun ProxyDnsDialog(
    current: String,
    custom: List<String>,
    discovered: List<String>,
    onDismiss: () -> Unit,
    onSelect: (String) -> Unit,
    onSaveCustom: (List<String>) -> Unit,
) {
    var text by remember { mutableStateOf(custom.joinToString("\n")) }

    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("节点域名解析") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                Text(
                    "只影响「节点服务器域名 → IP」这一步，不影响网页和应用的 DNS。",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                TextButton(onClick = { onSelect(PROXY_DNS_DOH) }) {
                    Text(if (custom.isEmpty() && current == PROXY_DNS_DOH) "✓ 加密 DNS（DoH）" else "加密 DNS（DoH）")
                }
                TextButton(onClick = { onSelect(PROXY_DNS_SYSTEM) }) {
                    Text(if (custom.isEmpty() && current == PROXY_DNS_SYSTEM) "✓ 本机 DNS" else "本机 DNS")
                }
                Text(
                    "自定义 DoH：填机场自己提供的加密 DNS（一行一个地址）。节点入口是轮换 IP 的机场，" +
                        "只有机场自己的 DNS 才会返回能用的入口；电脑能用而手机不行时优先试这个。",
                    style = MaterialTheme.typography.bodySmall,
                )
                if (discovered.isNotEmpty()) {
                    Text(
                        "已从订阅自动读到 ${discovered.size} 条机场 DNS，自动生效；在上面填内容会覆盖它。",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.primary,
                    )
                }
                OutlinedTextField(
                    value = text,
                    onValueChange = { text = it },
                    modifier = Modifier.fillMaxWidth().heightIn(min = 110.dp),
                    textStyle = MaterialTheme.typography.bodySmall,
                    placeholder = { Text("https://dns.example.com/dns-query") },
                )
                TextButton(onClick = {
                    onSaveCustom(text.lines().map { it.trim() }.filter { it.isNotEmpty() })
                }) { Text("保存自定义 DoH") }
            }
        },
        confirmButton = { TextButton(onClick = onDismiss) { Text("关闭") } },
    )
}

/**
 * Replaces the workspace with a pasted backup.
 *
 * The document is compiled by the same Rust renderer the core uses, so a broken or empty workspace
 * is rejected here instead of after the VPN restarts. Returns null on success, otherwise a message.
 */
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
