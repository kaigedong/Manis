package dev.manis.app.ui

import android.app.Activity
import android.net.ConnectivityManager
import android.net.Network
import android.net.VpnService
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import dev.manis.app.ManisVpnService
import dev.manis.app.core.ControllerApi
import dev.manis.app.core.CoreController
import dev.manis.app.core.Snapshot
import dev.manis.app.data.Workspace
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

private fun formatTraffic(bytes: Long): String = when {
    bytes >= 1024L * 1024L * 1024L -> "%.1f GB".format(bytes / (1024.0 * 1024.0 * 1024.0))
    bytes >= 1024L * 1024L -> "%.1f MB".format(bytes / (1024.0 * 1024.0))
    bytes >= 1024L -> "%.1f KB".format(bytes / 1024.0)
    else -> "$bytes B"
}

/** Runtime state polled from the service and the controller. */
object RuntimeState {
    var active by mutableStateOf(false)
    var snapshot by mutableStateOf<Snapshot?>(null)
    var error by mutableStateOf<String?>(null)
    var summary by mutableStateOf("")
    var controllerError by mutableStateOf<String?>(null)
    var warnings by mutableStateOf(listOf<String>())
    var downloadTotal by mutableStateOf(0L)
    var uploadTotal by mutableStateOf(0L)
    var unreachable by mutableStateOf(false)
    var networkEpoch by mutableStateOf(0)
}

@Composable
fun ManisApp() {
    val scope = rememberCoroutineScope()
    var toast by remember { mutableStateOf<String?>(null) }

    fun say(message: String) {
        toast = message
    }

    LaunchedEffect(toast) {
        if (toast != null) {
            delay(2_400)
            toast = null
        }
    }

    // Re-run the recovery probe when the phone changes networks: a node that timed out on Wi-Fi
    // may be reachable over cellular.
    val context = LocalContext.current
    DisposableEffect(Unit) {
        val manager = context.getSystemService(ConnectivityManager::class.java)
        val callback = object : ConnectivityManager.NetworkCallback() {
            override fun onAvailable(network: Network) {
                RuntimeState.networkEpoch += 1
            }

            override fun onLost(network: Network) {
                RuntimeState.networkEpoch += 1
            }
        }
        manager?.registerDefaultNetworkCallback(callback)
        onDispose { manager?.unregisterNetworkCallback(callback) }
    }

    LaunchedEffect(Unit) {
        var autoFixDone = false
        var pendingAttempt: String? = null
        var epoch = RuntimeState.networkEpoch
        var ticks = 0
        while (true) {
            ticks += 1
            RuntimeState.active = ManisVpnService.active
            RuntimeState.error = ManisVpnService.lastError ?: CoreController.lastError
            RuntimeState.summary = CoreController.summary
            RuntimeState.warnings = CoreController.warnings
            if (RuntimeState.active) {
                ControllerApi.configure(CoreController.secret)
                runCatching { ControllerApi.snapshot() }
                    .onSuccess {
                        RuntimeState.snapshot = it
                        RuntimeState.controllerError = null
                    }
                    .onFailure {
                        RuntimeState.controllerError = it.message
                        android.util.Log.w("Manis", "controller snapshot failed", it)
                    }
            } else {
                RuntimeState.snapshot = null
                RuntimeState.unreachable = false
                RuntimeState.downloadTotal = 0
                RuntimeState.uploadTotal = 0
                autoFixDone = false
            }
            if (RuntimeState.active && ticks % 2 == 0) {
                runCatching { ControllerApi.connections() }.onSuccess { traffic ->
                    RuntimeState.downloadTotal = traffic.downloadTotal
                    RuntimeState.uploadTotal = traffic.uploadTotal
                }
            }
            if (epoch != RuntimeState.networkEpoch) {
                epoch = RuntimeState.networkEpoch
                autoFixDone = false
            }
            val pendingNode = Workspace.config.value.pendingNode
            if (RuntimeState.active && pendingNode != null && pendingNode != pendingAttempt) {
                pendingAttempt = pendingNode
                scope.launch {
                    ControllerApi.configure(CoreController.secret)
                    val snap = runCatching { ControllerApi.snapshot() }.getOrNull()
                    val group = snap?.groups?.firstOrNull { pendingNode in it.all }
                    if (group != null &&
                        runCatching {
                            ControllerApi.select(group.name, pendingNode)
                            val global = snap.groups.firstOrNull {
                                it.name == "__MANIS_GLOBAL__" && pendingNode in it.all
                            }
                            if (global != null && global.name != group.name) {
                                runCatching { ControllerApi.select(global.name, pendingNode) }
                            }
                        }.isSuccess
                    ) {
                        Workspace.setPendingNode(null)
                        pendingAttempt = null
                    }
                }
            }
            if (!RuntimeState.active) pendingAttempt = null
            if (RuntimeState.active && !autoFixDone) {
                autoFixDone = true
                scope.launch {
                    ControllerApi.configure(CoreController.secret)
                    for (attempt in 0 until 6) {
                        delay(3_000)
                        val snapshot = runCatching { ControllerApi.snapshot() }.getOrNull()
                            ?: continue
                        val group = snapshot.groups
                            .firstOrNull { it.name == "__MANIS_GLOBAL__" }
                            ?: continue
                        val node = group.now ?: continue
                        if (group.all.size < 2) continue
                        if (runCatching { ControllerApi.testNode(node) }.getOrNull() != null) {
                            return@launch
                        }
                        val delays = runCatching { ControllerApi.testGroup(group.name) }
                            .onFailure { error ->
                                android.util.Log.w("Manis", "group health check failed", error)
                            }
                            .getOrElse { emptyMap() }
                        val best = delays.minByOrNull { it.value }
                        if (best == null) {
                            RuntimeState.unreachable = true
                            say("所有节点都连不上：手机网络到订阅节点的链路超时")
                            return@launch
                        }
                        runCatching { ControllerApi.select(group.name, best.key) }.onSuccess {
                            say("当前节点不可用，已切换到 ${best.key}")
                            runCatching { ControllerApi.snapshot() }
                                .onSuccess { RuntimeState.snapshot = it }
                        }
                        return@launch
                    }
                }
            }
            delay(1_500)
        }
    }

    var workspace by rememberSaveable { mutableStateOf(PrimaryWorkspace.Nodes) }
    val systemDark = isSystemInDarkTheme()
    val dark = UiPrefs.dark.value ?: systemDark
    val config = Workspace.config.value
    val mode = RuntimeState.snapshot?.mode ?: config.mode
    val routingLabel = when (mode) {
        "global" -> "全局"
        "direct" -> "直连"
        else -> "规则"
    }
    val proxyLabel = if (RuntimeState.active) "VPN 代理" else "关闭代理"

    val permission = rememberLauncherForActivityResult(
        ActivityResultContracts.StartActivityForResult(),
    ) { result ->
        if (result.resultCode == Activity.RESULT_OK) startVpn(context) else say("已取消 VPN 授权")
    }

    fun toggleProxy() {
        if (RuntimeState.active) {
            stopVpn(context)
            return
        }
        if (config.subscriptions.none { it.enabled } && config.nodes.none { it.enabled }) {
            say("先在配置里添加订阅或节点")
            workspace = PrimaryWorkspace.Configuration
            return
        }
        val prepare = VpnService.prepare(context)
        if (prepare != null) permission.launch(prepare) else startVpn(context)
    }

    fun cycleRouting() {
        if (!RuntimeState.active) {
            say("连接后才能切换路由模式")
            return
        }
        val next = when (mode) {
            "rule" -> "global"
            "global" -> "direct"
            else -> "rule"
        }
        val label = when (next) {
            "global" -> "全局"
            "direct" -> "直连"
            else -> "规则"
        }
        scope.launch {
            runCatching { ControllerApi.setMode(next) }
                .onSuccess { say("路由模式：$label") }
                .onFailure { say("切换路由模式失败") }
        }
    }

    ManisShell(
        workspace = workspace,
        onWorkspace = { workspace = it },
        darkTheme = dark,
        onToggleTheme = { UiPrefs.setDark(!dark) },
        statusText = RuntimeState.summary.ifBlank { if (RuntimeState.active) "已连接" else "未连接" },
        statusTone = when {
            !RuntimeState.active -> StatusTone.Route
            RuntimeState.unreachable -> StatusTone.Error
            else -> StatusTone.Ok
        },
        downloadText = formatTraffic(RuntimeState.downloadTotal),
        uploadText = formatTraffic(RuntimeState.uploadTotal),
        showGlobalHeader = workspace == PrimaryWorkspace.Nodes,
    ) {
        Box(Modifier.fillMaxSize()) {
            when (workspace) {
                PrimaryWorkspace.Nodes -> NodesWorkspace(
                    proxyLabel = if (RuntimeState.active) "断开代理" else "开启代理",
                    proxyActive = RuntimeState.active,
                    onProxyToggle = ::toggleProxy,
                    routingLabel = routingLabel,
                    onRoutingCycle = ::cycleRouting,
                    onMessage = ::say,
                    onOpenConfiguration = { workspace = PrimaryWorkspace.Configuration },
                )
                PrimaryWorkspace.RoutingRules -> RulesWorkspace(onMessage = ::say)
                PrimaryWorkspace.Activity -> ActivityWorkspace(onMessage = ::say)
                PrimaryWorkspace.Logs -> LogsWorkspace(onMessage = ::say)
                PrimaryWorkspace.Configuration -> ConfigurationWorkspace(
                    onMessage = ::say,
                    onOpenLogs = { workspace = PrimaryWorkspace.Logs },
                )
            }
            toast?.let { message ->
                ManisToast(
                    message,
                    Modifier
                        .align(Alignment.BottomCenter)
                        .padding(ManisDimens.spaceLg),
                )
            }
        }
    }
}
