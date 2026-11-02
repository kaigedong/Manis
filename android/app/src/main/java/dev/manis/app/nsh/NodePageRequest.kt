package dev.manis.app.nsh

import android.content.Context
import android.net.Uri
import dev.manis.app.core.Native
import dev.manis.app.core.Snapshot
import dev.manis.app.data.MobileConfig
import dev.manis.app.data.subscriptionCachePath
import dev.manis.app.ui.GLOBAL_GROUP
import dev.manis.app.ui.parseProxyNames
import java.io.File
import org.json.JSONArray
import org.json.JSONObject

/**
 * Builds the shared node-page model through `manis-presentation`.
 *
 * Both the Compose fallback and the NSH screens consume this JSON, so source grouping, display
 * names and internal-group filtering stay in one place.
 */
internal fun buildNodePage(
    context: Context,
    config: MobileConfig,
    snapshot: Snapshot?,
    connected: Boolean,
): JSONObject {
    val request = JSONObject().put("connected", connected)

    val sources = JSONArray()
    config.subscriptions.forEachIndexed { index, subscription ->
        val names = if (subscription.enabled) cachedNames(context, subscription.url) else emptyList()
        sources.put(
            JSONObject()
                .put("id", "subscription-$index")
                .put("name", subscription.name.ifBlank { "订阅 ${index + 1}" })
                .put("detail", "订阅 · ${Uri.parse(subscription.url).host ?: "地址已隐藏"}")
                .put("kind", "subscription")
                .put("enabled", subscription.enabled)
                .put("nodeNames", JSONArray(names)),
        )
    }
    if (config.nodes.isNotEmpty()) {
        sources.put(
            JSONObject()
                .put("id", "manual")
                .put("name", "手动节点")
                .put("detail", "分享链接导入")
                .put("kind", "manual")
                .put("enabled", true)
                .put("nodeNames", JSONArray(config.nodes.filter { it.enabled }.map { it.displayName })),
        )
    }
    request.put("sources", sources)

    val global = snapshot?.groups?.firstOrNull { it.name == GLOBAL_GROUP }
    request.put("globalMembers", JSONArray(global?.all.orEmpty()))
    request.put("globalCurrent", global?.now ?: JSONObject.NULL)

    val runtimeNodes = JSONObject()
    snapshot?.nodes?.forEach { (name, node) ->
        runtimeNodes.put(name, JSONObject().put("delayMs", node.delay ?: JSONObject.NULL))
    }
    request.put("runtimeNodes", runtimeNodes)

    val policyGroups = JSONArray()
    snapshot?.groups?.forEach { group ->
        policyGroups.put(
            JSONObject()
                .put("id", group.name)
                .put("label", group.name)
                .put("kind", group.type)
                .put("current", group.now ?: JSONObject.NULL)
                .put("members", JSONArray(group.all)),
        )
    }
    request.put("policyGroups", policyGroups)

    return JSONObject(Native.nodePage(request.toString()))
}

private fun cachedNames(context: Context, url: String): List<String> {
    val relative = subscriptionCachePath(url).removePrefix("./")
    val file = File(context.filesDir, "mihomo/$relative")
    if (!file.isFile) return emptyList()
    return runCatching { parseProxyNames(file.readText()) }.getOrDefault(emptyList())
}
