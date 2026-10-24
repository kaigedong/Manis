package dev.manis.app.data

import android.content.Context
import androidx.compose.runtime.mutableStateOf
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.net.URLDecoder

data class SubscriptionEntry(
    val name: String,
    val url: String,
    val enabled: Boolean = true,
)

data class NodeEntry(
    val link: String,
    val enabled: Boolean = true,
) {
    val displayName: String
        get() {
            val fragment = link.substringAfterLast('#', "")
            if (fragment.isNotBlank()) {
                return runCatching { URLDecoder.decode(fragment, "UTF-8") }.getOrDefault(fragment)
            }
            val host = link.substringAfter("//", "").substringAfter('@', "").substringBefore('/')
            return host.substringBefore('?').ifBlank { "未命名节点" }
        }
}

data class GroupEntry(
    val name: String,
    val strategy: String = "select",
    val subscriptionIndexes: List<Int> = emptyList(),
    val nodeIndexes: List<Int> = emptyList(),
    val filter: String? = null,
)

data class MobileConfig(
    val mode: String = "rule",
    val mixedPort: Int = 7890,
    val subscriptions: List<SubscriptionEntry> = emptyList(),
    val nodes: List<NodeEntry> = emptyList(),
    val groups: List<GroupEntry> = emptyList(),
    val rules: String = "",
    /** Pinned proxy-server addresses, used when a resolver hands out unreachable IPs. */
    val hosts: Map<String, String> = emptyMap(),
) {
    fun toJson(): String {
        val subscriptionsJson = JSONArray()
        for (entry in subscriptions) {
            subscriptionsJson.put(
                JSONObject()
                    .put("name", entry.name)
                    .put("url", entry.url)
                    .put("enabled", entry.enabled),
            )
        }
        val nodesJson = JSONArray()
        for (entry in nodes) {
            nodesJson.put(JSONObject().put("link", entry.link).put("enabled", entry.enabled))
        }
        val groupsJson = JSONArray()
        for (group in groups) {
            groupsJson.put(
                JSONObject()
                    .put("name", group.name)
                    .put("strategy", group.strategy)
                    .put("subscription_indexes", JSONArray(group.subscriptionIndexes))
                    .put("node_indexes", JSONArray(group.nodeIndexes))
                    .put("filter", group.filter ?: JSONObject.NULL),
            )
        }
        val hostsJson = JSONObject()
        for ((host, address) in hosts) {
            hostsJson.put(host, address)
        }
        return JSONObject()
            .put("hosts", hostsJson)
            .put("mode", mode)
            .put("mixed_port", mixedPort)
            .put("subscriptions", subscriptionsJson)
            .put("nodes", nodesJson)
            .put("groups", groupsJson)
            .put("rules", rules)
            .toString()
    }

    companion object {
        private const val FILE_NAME = "workspace.json"

        fun fromJson(raw: String): MobileConfig {
            if (raw.isBlank()) return MobileConfig()
            val json = JSONObject(raw)
            val subscriptions = mutableListOf<SubscriptionEntry>()
            json.optJSONArray("subscriptions")?.let { array ->
                for (index in 0 until array.length()) {
                    val item = array.getJSONObject(index)
                    subscriptions += SubscriptionEntry(
                        name = item.optString("name"),
                        url = item.optString("url"),
                        enabled = item.optBoolean("enabled", true),
                    )
                }
            }
            val nodes = mutableListOf<NodeEntry>()
            json.optJSONArray("nodes")?.let { array ->
                for (index in 0 until array.length()) {
                    val item = array.getJSONObject(index)
                    nodes += NodeEntry(
                        link = item.optString("link"),
                        enabled = item.optBoolean("enabled", true),
                    )
                }
            }
            val groups = mutableListOf<GroupEntry>()
            json.optJSONArray("groups")?.let { array ->
                for (index in 0 until array.length()) {
                    val item = array.getJSONObject(index)
                    groups += GroupEntry(
                        name = item.optString("name"),
                        strategy = item.optString("strategy", "select"),
                        subscriptionIndexes = item.optJSONArray("subscription_indexes").ints(),
                        nodeIndexes = item.optJSONArray("node_indexes").ints(),
                        filter = item.optString("filter").takeIf { it.isNotBlank() && it != "null" },
                    )
                }
            }
            val hosts = mutableMapOf<String, String>()
            json.optJSONObject("hosts")?.let { objectValue ->
                for (host in objectValue.keys()) {
                    val address = objectValue.optString(host)
                    if (address.isNotBlank()) hosts[host] = address
                }
            }
            return MobileConfig(
                mode = json.optString("mode", "rule"),
                mixedPort = json.optInt("mixed_port", 7890),
                subscriptions = subscriptions,
                nodes = nodes,
                groups = groups,
                rules = json.optString("rules"),
                hosts = hosts,
            )
        }

        fun load(context: Context): MobileConfig {
            val file = File(context.filesDir, FILE_NAME)
            if (!file.isFile) return MobileConfig()
            return runCatching { fromJson(file.readText()) }.getOrDefault(MobileConfig())
        }

        fun save(context: Context, config: MobileConfig) {
            val target = File(context.filesDir, FILE_NAME)
            val temporary = File(context.filesDir, "$FILE_NAME.tmp")
            temporary.writeText(config.toJson())
            if (!temporary.renameTo(target)) {
                target.writeText(temporary.readText())
                temporary.delete()
            }
        }
    }
}

private fun GroupEntry.withoutSubscription(index: Int): GroupEntry = copy(
    subscriptionIndexes = subscriptionIndexes
        .filter { it != index }
        .map { if (it > index) it - 1 else it },
)

private fun GroupEntry.withoutNode(index: Int): GroupEntry = copy(
    nodeIndexes = nodeIndexes
        .filter { it != index }
        .map { if (it > index) it - 1 else it },
)

private fun JSONArray?.ints(): List<Int> {
    if (this == null) return emptyList()
    return (0 until length()).map { optInt(it) }
}

/**
 * Single source of truth for the editable workspace. Compose observes [config]; any mutation is
 * persisted immediately, and [apply] asks the service to restart the core with the new document.
 */
object Workspace {
    val config = mutableStateOf(MobileConfig())
    val pending = mutableStateOf(false)

    fun load(context: Context) {
        config.value = MobileConfig.load(context)
    }

    fun update(transform: (MobileConfig) -> MobileConfig) {
        val next = transform(config.value)
        config.value = next
        MobileConfig.save(AppContext.get(), next)
        pending.value = true
    }

    /** Live changes that do not need a core restart (routing mode). */
    fun setMode(mode: String) {
        val next = config.value.copy(mode = mode)
        config.value = next
        MobileConfig.save(AppContext.get(), next)
    }

    fun removeSubscription(index: Int) {
        update { current ->
            current.copy(
                subscriptions = current.subscriptions.filterIndexed { at, _ -> at != index },
                groups = current.groups.map { group -> group.withoutSubscription(index) },
            )
        }
    }

    fun removeNode(index: Int) {
        update { current ->
            current.copy(
                nodes = current.nodes.filterIndexed { at, _ -> at != index },
                groups = current.groups.map { group -> group.withoutNode(index) },
            )
        }
    }
}

/** Holds the application context without leaking an Activity. */
object AppContext {
    private lateinit var context: android.content.Context

    fun install(context: android.content.Context) {
        this.context = context.applicationContext
    }

    fun get(): android.content.Context {
        check(::context.isInitialized) { "AppContext is not installed" }
        return context
    }
}
