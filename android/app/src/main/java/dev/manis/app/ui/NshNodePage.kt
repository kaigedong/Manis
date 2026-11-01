package dev.manis.app.ui

import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.viewinterop.AndroidView
import com.longbridge.nsh.NshView
import dev.manis.app.core.ControllerApi
import dev.manis.app.core.CoreController
import dev.manis.app.data.Workspace
import dev.manis.app.nsh.buildNodePage
import kotlinx.coroutines.launch
import org.json.JSONObject

/**
 * The migrated Nodes page rendered by NSH.
 *
 * The Kotlin host keeps the bottom navigation, VPN lifecycle and update plumbing; the node page
 * itself is the shared NSH UI. Data is rebuilt by `manis-presentation` through `Native.nodePage`.
 */
@Composable
internal fun NshNodePage(onMessage: (String) -> Unit, modifier: Modifier = Modifier) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val snapshot = RuntimeState.snapshot
    val active = RuntimeState.active
    val config = Workspace.config.value
    val initialPage = remember(snapshot, active, config) {
        runCatching { buildNodePage(context, config, snapshot, active) }
            .onFailure { onMessage("NSH 页面数据失败：${it.message ?: "未知错误"}") }
            .getOrElse { JSONObject() }
    }

    AndroidView(
        modifier = modifier.fillMaxSize(),
        factory = { context ->
            NshView(context).apply {
                setErrorListener { message -> onMessage("NSH: $message") }
                setParams(mapOf("page" to initialPage, "embedded" to true))
                handle("nodes.refresh") { _, reply ->
                    scope.launch {
                        runCatching { ControllerApi.snapshot() }
                            .onSuccess { RuntimeState.snapshot = it }
                        val page = runCatching {
                            buildNodePage(
                                context,
                                Workspace.config.value,
                                RuntimeState.snapshot,
                                RuntimeState.active,
                            )
                        }.getOrElse { JSONObject() }
                        reply.resolve(mapOf("page" to page))
                    }
                }
                handle("nodes.select") { data, reply ->
                    val name = (data as? Map<*, *>)?.get("name") as? String
                    if (name != null && RuntimeState.active) {
                        scope.launch {
                            ControllerApi.configure(CoreController.secret)
                            runCatching { ControllerApi.select(GLOBAL_GROUP, name) }
                                .onSuccess {
                                    onMessage("已切换到 $name")
                                    runCatching { ControllerApi.snapshot() }
                                        .onSuccess { RuntimeState.snapshot = it }
                                }
                                .onFailure { onMessage(it.message ?: "切换失败") }
                            reply.resolve(true)
                        }
                    } else {
                        reply.resolve(true)
                    }
                }
                handle("nodes.selectGroup") { data, reply ->
                    val values = data as? Map<*, *>
                    val group = values?.get("group") as? String
                    val name = values?.get("name") as? String
                    if (group != null && name != null && RuntimeState.active) {
                        scope.launch {
                            ControllerApi.configure(CoreController.secret)
                            runCatching { ControllerApi.select(group, name) }
                                .onSuccess {
                                    onMessage("已选择 $name")
                                    runCatching { ControllerApi.snapshot() }
                                        .onSuccess { RuntimeState.snapshot = it }
                                }
                                .onFailure { onMessage(it.message ?: "切换失败") }
                            reply.resolve(true)
                        }
                    } else {
                        reply.resolve(true)
                    }
                }
                if (!loadAssetBundle("nsh")) {
                    onMessage("NSH 节点页加载失败")
                }
            }
        },
        update = { view ->
            view.send("nodes.updated", mapOf("page" to initialPage))
        },
        onRelease = { view -> view.destroy() },
    )
}
