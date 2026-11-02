package dev.manis.app.nsh

import dev.manis.app.core.Native
import dev.manis.app.data.MobileConfig
import org.json.JSONArray
import org.json.JSONObject

/** Serializes the editable workspace pieces the NSH subscriptions page needs. */
internal fun buildSubscriptionsData(config: MobileConfig): JSONObject {
    val subscriptions = JSONArray()
    config.subscriptions.forEach { entry ->
        subscriptions.put(
            JSONObject()
                .put("name", entry.name)
                .put("url", entry.url)
                .put("enabled", entry.enabled),
        )
    }

    val nodes = JSONArray()
    config.nodes.forEach { entry ->
        nodes.put(
            JSONObject()
                .put("name", entry.displayName)
                .put("enabled", entry.enabled),
        )
    }

    val groups = JSONArray()
    config.groups.forEach { group ->
        groups.put(
            JSONObject()
                .put("name", group.name)
                .put("strategy", group.strategy)
                .put("subscriptionIndexes", JSONArray(group.subscriptionIndexes))
                .put("nodeIndexes", JSONArray(group.nodeIndexes)),
        )
    }

    val request = JSONObject()
        .put("subscriptions", subscriptions)
        .put("nodes", nodes)
        .put("groups", groups)
    return runCatching { JSONObject(Native.subscriptionsPage(request.toString())) }
        .getOrElse {
            JSONObject()
                .put("subscriptions", JSONArray())
                .put("nodes", JSONArray())
                .put("groups", JSONArray())
        }
}
