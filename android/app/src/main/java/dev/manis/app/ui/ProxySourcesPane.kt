package dev.manis.app.ui

import android.content.Context
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

private enum class SourceEditor { Subscription, Node, Group }

/** The GPUI proxy-sources section: subscriptions, manual nodes and policy groups. */
@Composable
fun ProxySourcesPane(onMessage: (String) -> Unit) {
    val colors = LocalManisColors.current
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val config = Workspace.config.value
    var editor by remember { mutableStateOf<SourceEditor?>(null) }
    var refreshing by remember { mutableStateOf(false) }

    if (editor != null) {
        SourceEditorPane(
            editor = editor!!,
            config = config,
            onCancel = { editor = null },
            onSaveSubscription = { name, url ->
                Workspace.update { current ->
                    current.copy(subscriptions = current.subscriptions + SubscriptionEntry(name.trim(), url.trim()))
                }
                editor = null
                onMessage("订阅已添加，正在下载…")
                scope.launch {
                    refreshing = true
                    onMessage(refreshSubscriptions(context, Workspace.config.value))
                    refreshing = false
                }
            },
            onSaveNode = { link ->
                Workspace.update { current -> current.copy(nodes = current.nodes + NodeEntry(link.trim())) }
                editor = null
                onMessage("节点已添加，点「应用」生效")
            },
            onSaveGroup = { group ->
                Workspace.update { current -> current.copy(groups = current.groups + group) }
                editor = null
                onMessage("策略组已添加，点「应用」生效")
            },
        )
        return
    }

    Column(
        Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(ManisDimens.spaceMd),
        verticalArrangement = Arrangement.spacedBy(ManisDimens.spaceMd),
    ) {
        if (Workspace.pending.value) {
            ManisCard {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text("有未应用的更改", style = ManisType.label, color = colors.textPrimary, modifier = Modifier.weight(1f))
                    ManisButton("应用", primary = true, onClick = {
                        applyWorkspace(context)
                        onMessage("已应用工作区")
                    })
                }
            }
        }
        ManisCard {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text("订阅", style = ManisType.label, color = colors.textPrimary, modifier = Modifier.weight(1f))
                ManisButton(if (refreshing) "刷新中…" else "刷新", onClick = {
                    if (refreshing || config.subscriptions.none { it.enabled }) return@ManisButton
                    refreshing = true
                    scope.launch {
                        onMessage(refreshSubscriptions(context, Workspace.config.value))
                        refreshing = false
                    }
                })
                ManisButton("添加", onClick = { editor = SourceEditor.Subscription })
            }
            if (config.subscriptions.isEmpty()) {
                ManisMeta("粘贴 Clash 格式订阅地址；添加后会立即下载，点「应用」生效。")
            }
            config.subscriptions.forEachIndexed { index, entry ->
                Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth()) {
                    Column(Modifier.weight(1f)) {
                        Text(entry.name.ifBlank { "订阅 ${index + 1}" }, style = ManisType.body, color = colors.textPrimary)
                        ManisMeta(maskUrl(entry.url))
                    }
                    ManisToggle(checked = entry.enabled) { enabled ->
                        Workspace.update { current ->
                            current.copy(
                                subscriptions = current.subscriptions.mapIndexed { at, item ->
                                    if (at == index) item.copy(enabled = enabled) else item
                                },
                            )
                        }
                    }
                    ManisTextAction("删除") { Workspace.removeSubscription(index) }
                }
            }
        }
        ManisCard {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text("手动节点", style = ManisType.label, color = colors.textPrimary, modifier = Modifier.weight(1f))
                ManisButton("添加", onClick = { editor = SourceEditor.Node })
            }
            if (config.nodes.isEmpty()) {
                ManisMeta("粘贴 vless:// 分享链接，一行一个。")
            }
            config.nodes.forEachIndexed { index, entry ->
                Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth()) {
                    Column(Modifier.weight(1f)) {
                        Text(entry.displayName, style = ManisType.body, color = colors.textPrimary)
                        ManisMeta("VLESS")
                    }
                    ManisToggle(checked = entry.enabled) { enabled ->
                        Workspace.update { current ->
                            current.copy(
                                nodes = current.nodes.mapIndexed { at, item ->
                                    if (at == index) item.copy(enabled = enabled) else item
                                },
                            )
                        }
                    }
                    ManisTextAction("删除") { Workspace.removeNode(index) }
                }
            }
        }
        ManisCard {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text("策略组", style = ManisType.label, color = colors.textPrimary, modifier = Modifier.weight(1f))
                ManisButton("添加", onClick = { editor = SourceEditor.Group })
            }
            if (config.groups.isEmpty()) {
                ManisMeta("全局组始终存在；再加一个「自动测速」组即可按延迟选最快节点。")
            }
            config.groups.forEachIndexed { index, group ->
                Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth()) {
                    Column(Modifier.weight(1f)) {
                        Text(group.name, style = ManisType.body, color = colors.textPrimary)
                        ManisMeta(
                            "${if (group.strategy == "urltest") "自动测速" else "手动选择"} · " +
                                "${group.subscriptionIndexes.size} 个订阅 / ${group.nodeIndexes.size} 个节点",
                        )
                    }
                    ManisTextAction("删除") {
                        Workspace.update { current ->
                            current.copy(groups = current.groups.filterIndexed { at, _ -> at != index })
                        }
                    }
                }
            }
        }
        ManisMeta("修改会先保存；点「应用」后内核重启并使用已下载的订阅缓存，VPN 接口不会断开。")
    }
}

@Composable
private fun SourceEditorPane(
    editor: SourceEditor,
    config: MobileConfig,
    onCancel: () -> Unit,
    onSaveSubscription: (String, String) -> Unit,
    onSaveNode: (String) -> Unit,
    onSaveGroup: (GroupEntry) -> Unit,
) {
    val colors = LocalManisColors.current
    var name by remember { mutableStateOf("") }
    var url by remember { mutableStateOf("") }
    var link by remember { mutableStateOf("") }
    var strategy by remember { mutableStateOf("urltest") }
    var subscriptionMembers by remember { mutableStateOf(setOf<Int>()) }
    var nodeMembers by remember { mutableStateOf(setOf<Int>()) }
    var error by remember { mutableStateOf<String?>(null) }

    Column(Modifier.fillMaxSize()) {
        Row(
            Modifier.fillMaxWidth().padding(ManisDimens.spaceMd),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
        ) {
            ManisBackButton(onCancel)
            Column(Modifier.weight(1f)) {
                Text(
                    when (editor) {
                        SourceEditor.Subscription -> "添加订阅"
                        SourceEditor.Node -> "添加 VLESS 节点"
                        SourceEditor.Group -> "添加策略组"
                    },
                    style = ManisType.sectionTitle,
                    color = colors.textPrimary,
                )
                ManisMeta(
                    when (editor) {
                        SourceEditor.Subscription -> "粘贴 Clash 格式订阅地址"
                        SourceEditor.Node -> "支持 TLS / WebSocket / Reality 分享链接"
                        SourceEditor.Group -> "选择成员；自动测速组按延迟选最快节点"
                    },
                )
            }
        }
        ManisDivider()
        Column(
            Modifier
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(ManisDimens.spaceMd),
            verticalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
        ) {
            when (editor) {
                SourceEditor.Subscription -> {
                    Text("名称（可选）", style = ManisType.label, color = colors.textPrimary)
                    ManisTextField(value = name, onValueChange = { name = it }, placeholder = "名称")
                    Text("订阅地址", style = ManisType.label, color = colors.textPrimary)
                    ManisTextField(value = url, onValueChange = { url = it }, placeholder = "https://…")
                    ManisButton("添加", primary = true, onClick = {
                        if (url.isBlank()) {
                            error = "订阅地址不能为空"
                            return@ManisButton
                        }
                        onSaveSubscription(name, url)
                    })
                }
                SourceEditor.Node -> {
                    Text("vless:// 分享链接", style = ManisType.label, color = colors.textPrimary)
                    ManisTextField(value = link, onValueChange = { link = it }, placeholder = "vless://…")
                    ManisButton("添加", primary = true, onClick = {
                        if (!link.startsWith("vless://")) {
                            error = "请输入 vless:// 分享链接"
                            return@ManisButton
                        }
                        onSaveNode(link)
                    })
                }
                SourceEditor.Group -> {
                    Text("组名", style = ManisType.label, color = colors.textPrimary)
                    ManisTextField(value = name, onValueChange = { name = it }, placeholder = "组名")
                    Text("策略", style = ManisType.label, color = colors.textPrimary)
                    Row(horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceXs)) {
                        ManisChip("自动测速", strategy == "urltest") { strategy = "urltest" }
                        ManisChip("手动选择", strategy == "select") { strategy = "select" }
                    }
                    Text("订阅成员", style = ManisType.label, color = colors.textPrimary)
                    if (config.subscriptions.isEmpty()) {
                        ManisMeta("还没有订阅")
                    }
                    for ((index, entry) in config.subscriptions.withIndex()) {
                        ManisChip(entry.name.ifBlank { "订阅 ${index + 1}" }, index in subscriptionMembers) {
                            subscriptionMembers = subscriptionMembers.toggle(index)
                        }
                    }
                    Text("手动节点成员", style = ManisType.label, color = colors.textPrimary)
                    if (config.nodes.isEmpty()) {
                        ManisMeta("还没有手动节点")
                    }
                    for ((index, entry) in config.nodes.withIndex()) {
                        ManisChip(entry.displayName, index in nodeMembers) {
                            nodeMembers = nodeMembers.toggle(index)
                        }
                    }
                    ManisButton("添加", primary = true, onClick = {
                        if (name.isBlank() || (subscriptionMembers.isEmpty() && nodeMembers.isEmpty())) {
                            error = "策略组需要名称和至少一个成员"
                            return@ManisButton
                        }
                        onSaveGroup(
                            GroupEntry(
                                name = name.trim(),
                                strategy = strategy,
                                subscriptionIndexes = subscriptionMembers.sorted(),
                                nodeIndexes = nodeMembers.sorted(),
                            ),
                        )
                    })
                }
            }
            error?.let { Text(it, style = ManisType.metadata, color = colors.statusError) }
        }
    }
}

private fun Set<Int>.toggle(index: Int): Set<Int> =
    if (contains(index)) this - index else this + index

private fun maskUrl(url: String): String {
    val scheme = url.substringBefore("://", "")
    val rest = url.substringAfter("://", url)
    val host = rest.substringBefore('/').substringBefore('?')
    return if (scheme.isBlank()) host else "$scheme://$host/…"
}

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
                applyWorkspace(context)
                parts += "内核刷新失败，已用本地缓存重新载入"
            }
    }
    return parts.joinToString("；").ifBlank { "没有启用的订阅" }
}
