package dev.manis.app.ui

import android.content.Context
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.viewinterop.AndroidView
import com.longbridge.nsh.NshView
import dev.manis.app.data.GroupEntry
import dev.manis.app.data.NodeEntry
import dev.manis.app.data.SubscriptionEntry
import dev.manis.app.data.Workspace
import dev.manis.app.nsh.buildSubscriptionsData
import kotlinx.coroutines.launch
import org.json.JSONObject

/**
 * NSH subscriptions editor. The Kotlin host still owns persistence, downloads and the apply step;
 * the page only sends intent-shaped commands.
 */
@Composable
internal fun NshSubscriptionsPage(onMessage: (String) -> Unit, modifier: Modifier = Modifier) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val config = Workspace.config.value
    val initial = remember(config) { buildSubscriptionsData(config) }

    AndroidView(
        modifier = modifier.fillMaxSize(),
        factory = { viewContext ->
            NshView(viewContext).apply {
                setErrorListener { onMessage("NSH: $it") }
                setParams(mapOf("page" to "subscriptions", "data" to initial, "embedded" to true))
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
                    reply.resolve(replyPayload())
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
                    reply.resolve(replyPayload())
                }
                handle("subscriptions.add") { payload, reply ->
                    val values = payload as? Map<*, *>
                    val name = values?.get("name") as? String
                    val url = values?.get("url") as? String
                    if (url.isNullOrBlank()) {
                        reply.resolve(mapOf("data" to buildSubscriptionsData(Workspace.config.value), "message" to "订阅地址不能为空"))
                    } else {
                        Workspace.update { current ->
                            current.copy(subscriptions = current.subscriptions + SubscriptionEntry(name.orEmpty().trim(), url.trim()))
                        }
                        scope.launch {
                            val message = runCatching { refreshSubscriptions(context, Workspace.config.value) }
                                .getOrElse { it.message ?: "订阅下载失败" }
                            reply.resolve(mapOf("data" to buildSubscriptionsData(Workspace.config.value), "message" to message))
                        }
                    }
                }
                handle("subscriptions.addNode") { payload, reply ->
                    val values = payload as? Map<*, *>
                    val link = (values?.get("link") as? String)?.trim()
                    if (link == null || !link.startsWith("vless://")) {
                        reply.resolve(mapOf("data" to buildSubscriptionsData(Workspace.config.value), "message" to "请输入 vless:// 分享链接"))
                    } else {
                        Workspace.update { current -> current.copy(nodes = current.nodes + NodeEntry(link)) }
                        reply.resolve(replyPayload("节点已添加，点「应用」生效"))
                    }
                }
                handle("subscriptions.addGroup") { payload, reply ->
                    val values = payload as? Map<*, *>
                    val name = (values?.get("name") as? String)?.trim().orEmpty()
                    val strategy = values?.get("strategy") as? String ?: "urltest"
                    val subscriptions = values?.get("subscriptionIndexes").toIndexList()
                    val nodes = values?.get("nodeIndexes").toIndexList()
                    if (name.isBlank() || (subscriptions.isEmpty() && nodes.isEmpty())) {
                        reply.resolve(mapOf("data" to buildSubscriptionsData(Workspace.config.value), "message" to "策略组需要名称和至少一个成员"))
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
                        reply.resolve(replyPayload("策略组已添加，点「应用」生效"))
                    }
                }
                handle("subscriptions.refresh") { _, reply ->
                    scope.launch {
                        val message = runCatching { refreshSubscriptions(context, Workspace.config.value) }
                            .getOrElse { it.message ?: "刷新失败" }
                        reply.resolve(mapOf("data" to buildSubscriptionsData(Workspace.config.value), "message" to message))
                    }
                }
                handle("subscriptions.apply") { _, reply ->
                    scope.launch {
                        applyWorkspace(context)
                        reply.resolve(replyPayload("已应用"))
                    }
                }
                if (!loadAssetBundle("nsh")) onMessage("NSH 订阅页加载失败")
            }
        },
        update = { view ->
            view.send(
                "subscriptions.updated",
                mapOf("data" to buildSubscriptionsData(Workspace.config.value)),
            )
        },
        onRelease = { view -> view.destroy() },
    )
}

private fun NshView.replyPayload(message: String? = null): Map<String, Any?> = buildMap {
    put("data", buildSubscriptionsData(Workspace.config.value))
    if (message != null) put("message", message)
}

private fun Any?.toIndexList(): List<Int> = when (this) {
    is List<*> -> mapNotNull { (it as? Number)?.toInt() }.distinct()
    else -> emptyList()
}
