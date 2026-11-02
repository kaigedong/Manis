package dev.manis.app.ui

import android.app.Activity
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.net.VpnService
import android.provider.Settings
import android.widget.Toast
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.statusBars
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.viewinterop.AndroidView
import com.longbridge.nsh.NshView
import dev.manis.app.core.ControllerApi
import dev.manis.app.core.CoreController
import dev.manis.app.data.GroupEntry
import dev.manis.app.data.NodeEntry
import dev.manis.app.data.PROXY_DNS_DOH
import dev.manis.app.data.RuleEntry
import dev.manis.app.data.SubscriptionEntry
import dev.manis.app.data.UpdateManager
import dev.manis.app.data.UpdateState
import dev.manis.app.data.Workspace
import dev.manis.app.nsh.buildRootData
import java.io.File
import kotlinx.coroutines.launch
import org.json.JSONArray

/**
 * The single NSH root: bottom navigation, apply bar and all five pages.
 *
 * Compose keeps only the invisible platform shell (polling, VPN permission, installer
 * permission); the visible shell is NSH.
 */
@Composable
internal fun NshRootPage(
    onMessage: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val updateState by UpdateManager.state.collectAsState()
    var localError by remember { mutableStateOf<String?>(null) }
    var pendingInstall by remember { mutableStateOf<File?>(null) }
    val config = Workspace.config.value
    val density = LocalDensity.current
    val topInset = with(density) { WindowInsets.statusBars.getTop(this).toDp().value }
    val bottomInset = with(density) { WindowInsets.navigationBars.getBottom(this).toDp().value }
    // Reading these state objects during composition keeps the payload fresh.
    val data = buildRootData(context, config, localError)

    val vpnPermission = rememberLauncherForActivityResult(
        ActivityResultContracts.StartActivityForResult(),
    ) { result ->
        if (result.resultCode == Activity.RESULT_OK) startVpn(context) else localError = "已取消 VPN 授权"
    }
    val installPermission = rememberLauncherForActivityResult(
        ActivityResultContracts.StartActivityForResult(),
    ) {
        val file = pendingInstall
        pendingInstall = null
        if (file != null) {
            if (context.packageManager.canRequestPackageInstalls()) {
                launchInstaller(context, file, onMessage)
            } else {
                onMessage("仍未允许安装未知应用")
            }
        }
    }

    fun connect() {
        localError = null
        if (config.subscriptions.none { it.enabled } && config.nodes.none { it.enabled }) {
            localError = "先在「订阅」里添加一个订阅或节点"
            return
        }
        val prepare = VpnService.prepare(context)
        if (prepare != null) vpnPermission.launch(prepare) else startVpn(context)
    }

    fun install(file: File) {
        if (!context.packageManager.canRequestPackageInstalls()) {
            pendingInstall = file
            val intent = Intent(
                Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,
                Uri.parse("package:${context.packageName}"),
            )
            runCatching { installPermission.launch(intent) }.onFailure {
                pendingInstall = null
                onMessage("系统没有开放「安装未知应用」设置页，请手动在系统设置中允许")
            }
        } else {
            launchInstaller(context, file, onMessage)
        }
    }

    AndroidView(
        modifier = modifier.fillMaxSize(),
        factory = { viewContext ->
            NshView(viewContext).apply {
                setErrorListener { onMessage("NSH: $it") }
                setParams(
                    mapOf(
                        "data" to data,
                        "embedded" to true,
                        "topInset" to topInset,
                        "bottomInset" to bottomInset,
                    ),
                )
                handle("workspace.apply") { _, reply ->
                    scope.launch {
                        applyWorkspace(context)
                        reply.resolve(rootReply(context, localError, "已应用"))
                    }
                }
                handle("workspace.discard") { _, reply ->
                    Workspace.pending.value = false
                    reply.resolve(rootReply(context, localError, "已放弃更改"))
                }
                handle("nodes.refresh") { _, reply ->
                    scope.launch {
                        runCatching { ControllerApi.snapshot() }.onSuccess { RuntimeState.snapshot = it }
                        reply.resolve(rootReply(context, localError))
                    }
                }
                handle("nodes.select") { payload, reply ->
                    val name = (payload as? Map<*, *>)?.get("name") as? String
                    if (name != null && RuntimeState.active) {
                        scope.launch {
                            ControllerApi.configure(CoreController.secret)
                            runCatching { ControllerApi.select(GLOBAL_GROUP, name) }
                                .onSuccess {
                                    onMessage("已切换到 $name")
                                    runCatching { ControllerApi.snapshot() }.onSuccess { RuntimeState.snapshot = it }
                                }
                                .onFailure { onMessage(it.message ?: "切换失败") }
                            reply.resolve(rootReply(context, localError))
                        }
                    } else {
                        reply.resolve(rootReply(context, localError))
                    }
                }
                handle("nodes.selectGroup") { payload, reply ->
                    val values = payload as? Map<*, *>
                    val group = values?.get("group") as? String
                    val name = values?.get("name") as? String
                    if (group != null && name != null && RuntimeState.active) {
                        scope.launch {
                            ControllerApi.configure(CoreController.secret)
                            runCatching { ControllerApi.select(group, name) }
                                .onSuccess {
                                    onMessage("已选择 $name")
                                    runCatching { ControllerApi.snapshot() }.onSuccess { RuntimeState.snapshot = it }
                                }
                                .onFailure { onMessage(it.message ?: "切换失败") }
                            reply.resolve(rootReply(context, localError))
                        }
                    } else {
                        reply.resolve(rootReply(context, localError))
                    }
                }
                handle("subscriptions.toggle") { payload, reply ->
                    val values = payload as? Map<*, *>
                    val kind = values?.get("kind") as? String
                    val index = (values?.get("index") as? Number)?.toInt()
                    val enabled = values?.get("enabled") as? Boolean
                    if (kind != null && index != null && enabled != null) {
                        Workspace.update { current ->
                            when (kind) {
                                "subscription" -> current.copy(
                                    subscriptions = current.subscriptions.mapIndexed { at, item ->
                                        if (at == index) item.copy(enabled = enabled) else item
                                    },
                                )
                                "node" -> current.copy(
                                    nodes = current.nodes.mapIndexed { at, item ->
                                        if (at == index) item.copy(enabled = enabled) else item
                                    },
                                )
                                else -> current
                            }
                        }
                    }
                    reply.resolve(rootReply(context, localError))
                }
                handle("subscriptions.delete") { payload, reply ->
                    val values = payload as? Map<*, *>
                    val kind = values?.get("kind") as? String
                    val index = (values?.get("index") as? Number)?.toInt()
                    if (index != null) {
                        when (kind) {
                            "subscription" -> Workspace.removeSubscription(index)
                            "node" -> Workspace.removeNode(index)
                            "group" -> Workspace.update { current ->
                                current.copy(groups = current.groups.filterIndexed { at, _ -> at != index })
                            }
                        }
                    }
                    reply.resolve(rootReply(context, localError))
                }
                handle("subscriptions.add") { payload, reply ->
                    val values = payload as? Map<*, *>
                    val name = values?.get("name") as? String
                    val url = values?.get("url") as? String
                    if (url.isNullOrBlank()) {
                        reply.resolve(rootReply(context, localError, "订阅地址不能为空"))
                    } else {
                        Workspace.update { current ->
                            current.copy(subscriptions = current.subscriptions + SubscriptionEntry(name.orEmpty().trim(), url.trim()))
                        }
                        scope.launch {
                            val message = runCatching { refreshSubscriptions(context, Workspace.config.value) }
                                .getOrElse { it.message ?: "订阅下载失败" }
                            reply.resolve(rootReply(context, localError, message))
                        }
                    }
                }
                handle("subscriptions.addNode") { payload, reply ->
                    val link = ((payload as? Map<*, *>)?.get("link") as? String)?.trim()
                    if (link == null || !link.startsWith("vless://")) {
                        reply.resolve(rootReply(context, localError, "请输入 vless:// 分享链接"))
                    } else {
                        Workspace.update { current -> current.copy(nodes = current.nodes + NodeEntry(link)) }
                        reply.resolve(rootReply(context, localError, "节点已添加，点「应用」生效"))
                    }
                }
                handle("subscriptions.addGroup") { payload, reply ->
                    val values = payload as? Map<*, *>
                    val name = (values?.get("name") as? String)?.trim().orEmpty()
                    val strategy = values?.get("strategy") as? String ?: "urltest"
                    val subscriptions = values?.get("subscriptionIndexes").toIndexList()
                    val nodes = values?.get("nodeIndexes").toIndexList()
                    if (name.isBlank() || (subscriptions.isEmpty() && nodes.isEmpty())) {
                        reply.resolve(rootReply(context, localError, "策略组需要名称和至少一个成员"))
                    } else {
                        Workspace.update { current ->
                            current.copy(
                                groups = current.groups + GroupEntry(
                                    name = name,
                                    strategy = strategy,
                                    subscriptionIndexes = subscriptions,
                                    nodeIndexes = nodes,
                                ),
                            )
                        }
                        reply.resolve(rootReply(context, localError, "策略组已添加，点「应用」生效"))
                    }
                }
                handle("subscriptions.refresh") { _, reply ->
                    scope.launch {
                        val message = runCatching { refreshSubscriptions(context, Workspace.config.value) }
                            .getOrElse { it.message ?: "刷新失败" }
                        reply.resolve(rootReply(context, localError, message))
                    }
                }
                handle("subscriptions.apply") { _, reply ->
                    scope.launch {
                        applyWorkspace(context)
                        reply.resolve(rootReply(context, localError, "已应用"))
                    }
                }
                handle("rules.toggle") { payload, reply ->
                    val values = payload as? Map<*, *>
                    val index = (values?.get("index") as? Number)?.toInt()
                    val enabled = values?.get("enabled") as? Boolean
                    if (index != null && enabled != null) {
                        Workspace.update { current ->
                            current.copy(
                                ruleEntries = current.ruleEntries.mapIndexed { at, rule ->
                                    if (at == index) rule.copy(enabled = enabled) else rule
                                },
                            )
                        }
                    }
                    reply.resolve(rootReply(context, localError))
                }
                handle("rules.delete") { payload, reply ->
                    val index = ((payload as? Map<*, *>)?.get("index") as? Number)?.toInt()
                    if (index != null) {
                        Workspace.update { current ->
                            current.copy(ruleEntries = current.ruleEntries.filterIndexed { at, _ -> at != index })
                        }
                    }
                    reply.resolve(rootReply(context, localError))
                }
                handle("rules.save") { payload, reply ->
                    val values = payload as? Map<*, *>
                    val index = (values?.get("index") as? Number)?.toInt()
                    val kind = (values?.get("kind") as? String)?.trim().orEmpty()
                    val value = (values?.get("value") as? String)?.trim().orEmpty()
                    val policy = (values?.get("policy") as? String)?.trim().orEmpty()
                    val enabled = values?.get("enabled") as? Boolean ?: true
                    val error = when {
                        kind.isBlank() -> "请选择规则类型"
                        kind != "FINAL" && value.isBlank() -> "请填写匹配内容"
                        policy.isBlank() -> "请选择策略"
                        kind == "FINAL" && Workspace.config.value.ruleEntries.withIndex().any { (at, rule) ->
                            at != index && rule.enabled && rule.kind == "FINAL"
                        } -> "只能有一条启用的最终规则"
                        else -> null
                    }
                    if (error != null) {
                        reply.resolve(rootReply(context, localError, error))
                    } else {
                        val entry = RuleEntry(
                            kind = kind,
                            value = if (kind == "FINAL") "" else value,
                            policy = policy,
                            enabled = enabled,
                        )
                        Workspace.update { current ->
                            val entries = current.ruleEntries.toMutableList()
                            if (index == null) entries += entry else if (index in entries.indices) entries[index] = entry
                            current.copy(ruleEntries = entries)
                        }
                        reply.resolve(rootReply(context, localError, "规则已保存，点「应用」生效"))
                    }
                }
                handle("rules.apply") { _, reply ->
                    scope.launch {
                        applyWorkspace(context)
                        reply.resolve(rootReply(context, localError, "已应用"))
                    }
                }
                handle("settings.reload") { _, reply ->
                    scope.launch {
                        applyWorkspace(context)
                        reply.resolve(rootReply(context, localError, "已重新载入配置"))
                    }
                }
                handle("settings.logs") { _, reply ->
                    reply.resolve(rootReply(context, localError, logs = readLog(context)))
                }
                handle("settings.copyLogs") { payload, reply ->
                    val lines = ((payload as? Map<*, *>)?.get("lines") as? List<*>)
                        ?.mapNotNull { it as? String }.orEmpty()
                    context.getSystemService(ClipboardManager::class.java)
                        ?.setPrimaryClip(ClipData.newPlainText("Manis 内核日志", lines.joinToString("\n")))
                    reply.resolve(rootReply(context, localError, "日志已复制"))
                }
                handle("settings.copyBackup") { _, reply ->
                    context.getSystemService(ClipboardManager::class.java)
                        ?.setPrimaryClip(ClipData.newPlainText("Manis 配置", Workspace.config.value.toJson()))
                    reply.resolve(rootReply(context, localError, "配置已复制到剪贴板"))
                }
                handle("settings.importBackup") { _, reply ->
                    val clipboard = context.getSystemService(ClipboardManager::class.java)
                    val text = clipboard?.primaryClip
                        ?.takeIf { it.itemCount > 0 }
                        ?.getItemAt(0)
                        ?.coerceToText(context)
                        ?.toString()
                        .orEmpty()
                    val result = importWorkspace(context, text)
                    reply.resolve(rootReply(context, localError, result ?: "已从剪贴板导入并生效"))
                }
                handle("settings.dns.set") { payload, reply ->
                    val values = payload as? Map<*, *>
                    val mode = values?.get("mode") as? String
                    val servers = (values?.get("servers") as? List<*>)
                        ?.mapNotNull { (it as? String)?.trim()?.takeIf(String::isNotEmpty) }
                        .orEmpty()
                    Workspace.update { it.copy(proxyDns = mode ?: PROXY_DNS_DOH, proxyDnsServers = servers) }
                    scope.launch {
                        applyWorkspace(context)
                        reply.resolve(rootReply(context, localError, "节点域名解析已更新并重新载入"))
                    }
                }
                handle("settings.update.check") { _, reply ->
                    UpdateManager.check(context)
                    reply.resolve(rootReply(context, localError, "正在检查更新"))
                }
                handle("settings.update.download") { _, reply ->
                    val update = when (val state = UpdateManager.state.value) {
                        is UpdateState.Available -> state.update
                        is UpdateState.Failed -> state.update
                        else -> null
                    }
                    if (update == null) {
                        reply.resolve(rootReply(context, localError, "当前没有可下载的更新"))
                    } else {
                        UpdateManager.download(context, update)
                        reply.resolve(rootReply(context, localError, "正在下载更新"))
                    }
                }
                handle("settings.update.install") { _, reply ->
                    val state = UpdateManager.state.value
                    if (state is UpdateState.Ready && state.file.isFile) {
                        install(state.file)
                        reply.resolve(rootReply(context, localError, "正在启动安装器"))
                    } else {
                        reply.resolve(rootReply(context, localError, "更新包还没有下载完成"))
                    }
                }
                handle("settings.update.release") { _, reply ->
                    val state = UpdateManager.state.value
                    val url = when (state) {
                        is UpdateState.Available -> state.update.releaseUrl
                        is UpdateState.Downloading -> state.update.releaseUrl
                        is UpdateState.Ready -> state.update.releaseUrl
                        is UpdateState.Failed -> state.update?.releaseUrl
                        else -> null
                    } ?: RELEASE_PAGE
                    openRelease(context, url, onMessage)
                    reply.resolve(rootReply(context, localError, "已打开 Release 页面"))
                }
                handle("connection.toggle") { payload, reply ->
                    val enabled = ((payload as? Map<*, *>)?.get("enabled") as? Boolean) ?: false
                    if (enabled) connect() else {
                        localError = null
                        stopVpn(context)
                    }
                    reply.resolve(rootReply(context, localError))
                }
                handle("connection.mode") { payload, reply ->
                    val mode = (payload as? Map<*, *>)?.get("mode") as? String
                    if (mode != null) {
                        scope.launch {
                            if (!RuntimeState.active) {
                                Workspace.setMode(mode)
                                onMessage("模式已保存，连接后生效")
                            } else {
                                ControllerApi.configure(CoreController.secret)
                                runCatching { ControllerApi.setMode(mode) }
                                    .onSuccess {
                                        Workspace.setMode(mode)
                                        onMessage("模式已切换")
                                    }
                                    .onFailure { onMessage(it.message ?: "切换失败") }
                            }
                            reply.resolve(rootReply(context, localError))
                        }
                    } else {
                        reply.resolve(rootReply(context, localError))
                    }
                }
                handle("connection.reload") { _, reply ->
                    applyWorkspace(context)
                    reply.resolve(rootReply(context, localError, "已重新加载配置"))
                }
                if (!loadAssetBundle("nsh")) onMessage("NSH 页面加载失败")
            }
        },
        update = { view ->
            view.send(
                "app.updated",
                mapOf(
                    "data" to buildRootData(context, Workspace.config.value, localError),
                ),
            )
        },
        onRelease = { view -> view.destroy() },
    )
}


private fun NshView.rootReply(
    context: Context,
    localError: String?,
    message: String? = null,
    logs: List<String>? = null,
): Map<String, Any?> = buildMap {
    put("data", buildRootData(context, Workspace.config.value, localError))
    if (message != null) put("message", message)
    if (logs != null) put("logs", JSONArray(logs))
}

private fun Any?.toIndexList(): List<Int> = when (this) {
    is List<*> -> mapNotNull { (it as? Number)?.toInt() }.distinct()
    else -> emptyList()
}
