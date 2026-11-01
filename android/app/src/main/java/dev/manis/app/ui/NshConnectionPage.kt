package dev.manis.app.ui

import android.app.Activity
import android.net.VpnService
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.viewinterop.AndroidView
import com.longbridge.nsh.NshView
import dev.manis.app.core.ControllerApi
import dev.manis.app.core.CoreController
import dev.manis.app.data.Workspace
import dev.manis.app.nsh.buildConnectionData
import kotlinx.coroutines.launch

@Composable
internal fun NshConnectionPage(
    onMessage: (String) -> Unit,
    onNavigate: (Tab) -> Unit,
    modifier: Modifier = Modifier,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var localError by remember { mutableStateOf<String?>(null) }
    val config = Workspace.config.value
    val active = RuntimeState.active
    val mode = RuntimeState.snapshot?.mode ?: config.mode
    val node = RuntimeState.snapshot?.groups
        ?.firstOrNull { it.name == GLOBAL_GROUP }
        ?.now
    val data = remember(
        config,
        active,
        mode,
        node,
        RuntimeState.error,
        RuntimeState.controllerError,
        RuntimeState.summary,
        RuntimeState.unreachable,
        RuntimeState.warnings,
        localError,
    ) {
        buildConnectionData(config, localError)
    }

    val permission = rememberLauncherForActivityResult(
        ActivityResultContracts.StartActivityForResult(),
    ) { result ->
        if (result.resultCode == Activity.RESULT_OK) {
            startVpn(context)
        } else {
            localError = "已取消 VPN 授权"
        }
    }

    fun connect() {
        localError = null
        if (config.subscriptions.none { it.enabled } && config.nodes.none { it.enabled }) {
            localError = "先在「订阅」里添加一个订阅或节点"
            return
        }
        val prepare = VpnService.prepare(context)
        if (prepare != null) permission.launch(prepare) else startVpn(context)
    }

    AndroidView(
        modifier = modifier.fillMaxSize(),
        factory = { viewContext ->
            NshView(viewContext).apply {
                setErrorListener { onMessage("NSH: $it") }
                setParams(mapOf("page" to "connection", "data" to data, "embedded" to true))
                handle("connection.toggle") { payload, reply ->
                    val enabled = ((payload as? Map<*, *>)?.get("enabled") as? Boolean) ?: false
                    if (enabled) connect() else {
                        localError = null
                        stopVpn(context)
                    }
                    reply.resolve(connectionPayload(context, localError))
                }
                handle("connection.mode") { payload, reply ->
                    val modeValue = (payload as? Map<*, *>)?.get("mode") as? String
                    if (modeValue != null) {
                        scope.launch {
                            if (!RuntimeState.active) {
                                Workspace.setMode(modeValue)
                                onMessage("模式已保存，连接后生效")
                            } else {
                                ControllerApi.configure(CoreController.secret)
                                runCatching { ControllerApi.setMode(modeValue) }
                                    .onSuccess {
                                        Workspace.setMode(modeValue)
                                        onMessage("模式已切换")
                                    }
                                    .onFailure { onMessage(it.message ?: "切换失败") }
                            }
                            reply.resolve(connectionPayload(context, localError))
                        }
                    } else {
                        reply.resolve(connectionPayload(context, localError))
                    }
                }
                handle("connection.reload") { _, reply ->
                    applyWorkspace(context)
                    reply.resolve(connectionPayload(context, localError, "已重新加载配置"))
                }
                handle("connection.openNodes") { _, reply ->
                    onNavigate(Tab.Nodes)
                    reply.resolve(connectionPayload(context, localError))
                }
                if (!loadAssetBundle("nsh")) onMessage("NSH 连接页加载失败")
            }
        },
        update = { view ->
            view.send(
                "connection.updated",
                mapOf("data" to buildConnectionData(Workspace.config.value, localError)),
            )
        },
        onRelease = { view -> view.destroy() },
    )
}

private fun NshView.connectionPayload(
    context: android.content.Context,
    localError: String?,
    message: String? = null,
): Map<String, Any?> = buildMap {
    put("data", buildConnectionData(Workspace.config.value, localError))
    if (message != null) put("message", message)
}
