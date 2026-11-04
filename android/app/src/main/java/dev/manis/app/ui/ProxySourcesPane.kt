package dev.manis.app.ui

import android.content.Context
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.height
import dev.manis.app.R
import androidx.compose.foundation.clickable
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.res.painterResource
import androidx.compose.material3.Icon
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
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
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupProperties
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

internal enum class SourceEditor { Subscription, Node, Group }

/** The GPUI proxy-sources section: subscriptions, manual nodes and policy groups. */
@Composable
fun ProxySourcesContent(onMessage: (String) -> Unit) {
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
        Modifier.fillMaxWidth(),
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
            SourceHeader("订阅") {
                ManisTextAction(if (refreshing) "刷新中…" else "刷新", onClick = {
                    if (refreshing || config.subscriptions.none { it.enabled }) return@ManisTextAction
                    refreshing = true
                    scope.launch {
                        onMessage(refreshSubscriptions(context, Workspace.config.value))
                        refreshing = false
                    }
                })
                ManisTextAction("添加", onClick = { editor = SourceEditor.Subscription })
            }
            if (config.subscriptions.isEmpty()) {
                ManisMeta("粘贴 Clash 订阅地址，添加后立即下载。")
            }
            config.subscriptions.forEachIndexed { index, entry ->
                ManisDivider()
                SourceRow(
                    title = entry.name.ifBlank { "订阅 ${index + 1}" },
                    subtitle = maskUrl(entry.url),
                    checked = entry.enabled,
                    onToggle = { enabled ->
                        Workspace.update { current ->
                            current.copy(
                                subscriptions = current.subscriptions.mapIndexed { at, item ->
                                    if (at == index) item.copy(enabled = enabled) else item
                                },
                            )
                        }
                    },
                    onDelete = { Workspace.removeSubscription(index) },
                )
            }
        }
        ManisCard {
            SourceHeader("手动节点") {
                ManisTextAction("添加", onClick = { editor = SourceEditor.Node })
            }
            if (config.nodes.isEmpty()) {
                ManisMeta("粘贴 vless:// 分享链接，一行一个。")
            }
            config.nodes.forEachIndexed { index, entry ->
                ManisDivider()
                SourceRow(
                    title = entry.displayName,
                    subtitle = "VLESS",
                    checked = entry.enabled,
                    onToggle = { enabled ->
                        Workspace.update { current ->
                            current.copy(
                                nodes = current.nodes.mapIndexed { at, item ->
                                    if (at == index) item.copy(enabled = enabled) else item
                                },
                            )
                        }
                    },
                    onDelete = { Workspace.removeNode(index) },
                )
            }
        }
        ManisCard {
            SourceHeader("策略组") {
                ManisTextAction("添加", onClick = { editor = SourceEditor.Group })
            }
            if (config.groups.isEmpty()) {
                ManisMeta("没有策略组时，直接使用来源里选中的节点。")
            }
            config.groups.forEachIndexed { index, group ->
                ManisDivider()
                SourceRow(
                    title = group.name,
                    subtitle = "${if (group.strategy == "urltest") "自动测速" else "手动选择"} · " +
                        "${group.subscriptionIndexes.size} 个订阅 / ${group.nodeIndexes.size} 个节点",
                    checked = null,
                    onToggle = null,
                    onDelete = {
                        Workspace.update { current ->
                            current.copy(groups = current.groups.filterIndexed { at, _ -> at != index })
                        }
                    },
                )
            }
        }
        ManisMeta("修改会先保存；点「应用」后内核重载配置，VPN 不会断开。")
    }
}

/** A section header inside a card: a label and its text actions on one line. */
@Composable
private fun SourceHeader(title: String, actions: @Composable RowScope.() -> Unit) {
    val colors = LocalManisColors.current
    Row(
        Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            title,
            style = ManisType.label,
            color = colors.textPrimary,
            modifier = Modifier.weight(1f),
        )
        actions()
    }
}

/** One source row: what it is, whether it is on, and a quiet delete action. */
@Composable
private fun SourceRow(
    title: String,
    subtitle: String,
    checked: Boolean?,
    onToggle: ((Boolean) -> Unit)?,
    onDelete: () -> Unit,
) {
    val colors = LocalManisColors.current
    Row(
        Modifier.fillMaxWidth().height(56.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
    ) {
        Column(Modifier.weight(1f)) {
            Text(
                title,
                style = ManisType.body,
                color = colors.textPrimary,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                subtitle,
                style = ManisType.metadata,
                color = colors.textTertiary,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        if (checked != null && onToggle != null) {
            ManisToggle(checked = checked, onCheckedChange = onToggle)
        }
        Icon(
            painter = painterResource(R.drawable.ic_delete),
            contentDescription = "删除 $title",
            modifier = Modifier.size(20.dp).clickable(onClick = onDelete),
            tint = colors.textTertiary,
        )
    }
}

/** The group editor, shared with the nodes workspace so a group can be added from there. */
@Composable
fun GroupEditorDialog(
    config: MobileConfig,
    onDismiss: () -> Unit,
    onSave: (GroupEntry) -> Unit,
) {
    SourceEditorPane(
        editor = SourceEditor.Group,
        config = config,
        onCancel = onDismiss,
        onSaveSubscription = { _, _ -> },
        onSaveNode = { },
        onSaveGroup = onSave,
    )
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

    fun submit() {
        when (editor) {
            SourceEditor.Subscription -> {
                if (url.isBlank()) {
                    error = "订阅地址不能为空"
                    return
                }
                onSaveSubscription(name, url)
            }
            SourceEditor.Node -> {
                if (!link.startsWith("vless://")) {
                    error = "请输入 vless:// 分享链接"
                    return
                }
                onSaveNode(link)
            }
            SourceEditor.Group -> {
                if (name.isBlank() || (subscriptionMembers.isEmpty() && nodeMembers.isEmpty())) {
                    error = "策略组需要名称和至少一个成员"
                    return
                }
                onSaveGroup(
                    GroupEntry(
                        name = name.trim(),
                        strategy = strategy,
                        subscriptionIndexes = subscriptionMembers.sorted(),
                        nodeIndexes = nodeMembers.sorted(),
                    ),
                )
            }
        }
    }

    Popup(onDismissRequest = onCancel, properties = PopupProperties(focusable = true)) {
        ManisDialog(
            title = when (editor) {
                SourceEditor.Subscription -> "添加订阅"
                SourceEditor.Node -> "添加 VLESS 节点"
                SourceEditor.Group -> "添加策略组"
            },
            subtitle = when (editor) {
                SourceEditor.Subscription -> "粘贴 Clash 格式订阅地址"
                SourceEditor.Node -> "支持 TLS / WebSocket / Reality 分享链接"
                SourceEditor.Group -> "选择成员；自动测速组按延迟选最快节点"
            },
            onDismiss = onCancel,
            actions = {
                Spacer(Modifier.weight(1f))
                ManisButton("取消", onClick = onCancel)
                ManisButton("添加", primary = true, onClick = ::submit)
            },
        ) {
            when (editor) {
                SourceEditor.Subscription -> {
                    Text("名称（可选）", style = ManisType.label, color = colors.textPrimary)
                    ManisTextField(value = name, onValueChange = { name = it }, placeholder = "名称")
                    Text("订阅地址", style = ManisType.label, color = colors.textPrimary)
                    ManisTextField(value = url, onValueChange = { url = it }, placeholder = "https://…")
                }
                SourceEditor.Node -> {
                    Text("vless:// 分享链接", style = ManisType.label, color = colors.textPrimary)
                    ManisTextField(value = link, onValueChange = { link = it }, placeholder = "vless://…")
                }
                SourceEditor.Group -> {
                    Text("组名", style = ManisType.label, color = colors.textPrimary)
                    ManisTextField(value = name, onValueChange = { name = it }, placeholder = "组名")
                    ManisSelect(
                        label = "策略",
                        options = listOf("urltest", "select"),
                        selected = strategy,
                        display = { if (it == "urltest") "自动测速" else "手动选择" },
                        onSelect = { strategy = it },
                    )
                    ManisMultiSelect(
                        label = "订阅成员",
                        options = config.subscriptions.mapIndexed { index, entry ->
                            entry.name.ifBlank { "订阅 ${index + 1}" }
                        },
                        selected = subscriptionMembers,
                        onToggle = { subscriptionMembers = subscriptionMembers.toggle(it) },
                    )
                    ManisMultiSelect(
                        label = "手动节点成员",
                        options = config.nodes.map { it.displayName },
                        selected = nodeMembers,
                        onToggle = { nodeMembers = nodeMembers.toggle(it) },
                    )
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
        parts += "已下载 ${cached.downloaded} 个订阅（${cached.nodes} 个节点）"
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
