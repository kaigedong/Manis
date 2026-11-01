package dev.manis.app.nsh

import dev.manis.app.data.MobileConfig
import org.json.JSONArray
import org.json.JSONObject

/** Serializes the editable workspace pieces the NSH subscriptions page needs. */
internal fun buildSubscriptionsData(config: MobileConfig): JSONObject {
    val subscriptions = JSONArray()
    config.subscriptions.forEachIndexed { index, entry ->
        subscriptions.put(
            JSONObject()
                .put("index", index)
                .put("name", entry.name.ifBlank { "订阅 ${index + 1}" })
                .put("displayUrl", maskUrl(entry.url))
                .put("enabled", entry.enabled),
        )
    }

    val nodes = JSONArray()
    config.nodes.forEachIndexed { index, entry ->
        nodes.put(
            JSONObject()
                .put("index", index)
                .put("name", entry.displayName)
                .put("enabled", entry.enabled),
        )
    }

    val groups = JSONArray()
    config.groups.forEachIndexed { index, group ->
        groups.put(
            JSONObject()
                .put("index", index)
                .put("name", group.name)
                .put("strategy", group.strategy)
                .put("subscriptionIndexes", JSONArray(group.subscriptionIndexes))
                .put("nodeIndexes", JSONArray(group.nodeIndexes)),
        )
    }

    return JSONObject()
        .put("subscriptions", subscriptions)
        .put("nodes", nodes)
        .put("groups", groups)
}

private fun maskUrl(url: String): String {
    val scheme = url.substringBefore("://", "")
    val rest = url.substringAfter("://", url)
    val host = rest.substringBefore('/').substringBefore('?')
    return if (scheme.isBlank()) host else "$scheme://$host/…"
}
