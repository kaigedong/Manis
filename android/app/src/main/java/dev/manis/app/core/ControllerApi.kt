package dev.manis.app.core

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL
import java.net.URLEncoder

data class NodeInfo(
    val name: String,
    val type: String,
    val delay: Int?,
)

data class GroupInfo(
    val name: String,
    val type: String,
    val now: String?,
    val all: List<String>,
)

data class Snapshot(
    val groups: List<GroupInfo>,
    val nodes: Map<String, NodeInfo>,
    val mode: String,
    val version: String,
)

/**
 * Minimal client for the core's loopback controller.
 *
 * The controller requires the per-launch secret, so [configure] must be called with
 * [CoreController.secret] before any request.
 */
internal object ControllerApi {
    private const val BASE = "http://127.0.0.1:9090"
    private const val TEST_URL = "https://www.gstatic.com/generate_204"
    private val groupTypes = setOf("Selector", "URLTest", "Fallback", "LoadBalance")
    private val builtInNodes = setOf("COMPATIBLE", "PASS", "REJECT")

    private var secret: String = ""

    fun configure(secret: String) {
        this.secret = secret
    }

    suspend fun snapshot(): Snapshot = withContext(Dispatchers.IO) {
        val proxies = request("/proxies").optJSONObject("proxies") ?: JSONObject()
        val nodes = mutableMapOf<String, NodeInfo>()
        val groups = mutableListOf<GroupInfo>()
        for (name in proxies.keys()) {
            val proxy = proxies.optJSONObject(name) ?: continue
            val type = proxy.optString("type")
            val delay = proxy.optJSONArray("history")?.let { history ->
                if (history.length() == 0) {
                    null
                } else {
                    history.getJSONObject(history.length() - 1).optInt("delay", 0)
                        .takeIf { it > 0 }
                }
            }
            nodes[name] = NodeInfo(name, type, delay)
            val all = proxy.optJSONArray("all")?.let { array ->
                (0 until array.length())
                    .map(array::getString)
                    .filterNot { it in builtInNodes }
            }
            if (name == "GLOBAL") {
                continue
            }
            if (type in groupTypes && !all.isNullOrEmpty()) {
                groups += GroupInfo(
                    name = name,
                    type = type,
                    now = proxy.optString("now").takeIf { it.isNotBlank() && it != "null" },
                    all = all,
                )
            }
        }
        val mode = request("/configs").optString("mode", "rule")
        val version = request("/version").optString("version", "")
        Snapshot(
            groups.sortedWith(compareBy({ it.name != "__MANIS_GLOBAL__" }, { it.name })),
            nodes,
            mode,
            version,
        )
    }

    suspend fun select(group: String, name: String) = withContext(Dispatchers.IO) {
        request(
            "/proxies/${encode(group)}",
            method = "PUT",
            body = JSONObject(mapOf("name" to name)).toString(),
        )
        Unit
    }

    suspend fun setMode(mode: String) = withContext(Dispatchers.IO) {
        request(
            "/configs",
            method = "PATCH",
            body = JSONObject(mapOf("mode" to mode)).toString(),
        )
        Unit
    }

    /** Health-checks one proxy; returns null when it cannot reach the test URL. */
    suspend fun testNode(name: String): Int? = withContext(Dispatchers.IO) {
        val result = request(
            "/proxies/${encode(name)}/delay?url=${encode(TEST_URL)}&timeout=5000",
        )
        result.optInt("delay", 0).takeIf { it > 0 }
    }

    /** Runs the built-in health check for one group and returns per-node delays. */
    suspend fun testGroup(group: String): Map<String, Int> = withContext(Dispatchers.IO) {
        val result = request(
            "/group/${encode(group)}/delay?url=${encode(TEST_URL)}&timeout=5000",
        )
        buildMap {
            for (name in result.keys()) {
                val delay = result.optInt(name, 0)
                if (delay > 0) put(name, delay)
            }
        }
    }

    private fun encode(value: String): String =
        URLEncoder.encode(value, "UTF-8").replace("+", "%20")

    private fun request(path: String, method: String = "GET", body: String? = null): JSONObject {
        val connection = URL("$BASE$path").openConnection() as HttpURLConnection
        connection.requestMethod = method
        connection.connectTimeout = 3_000
        connection.readTimeout = if (path.contains("/delay")) 15_000 else 5_000
        connection.setRequestProperty("Authorization", "Bearer $secret")
        if (body != null) {
            connection.doOutput = true
            connection.setRequestProperty("Content-Type", "application/json")
            connection.outputStream.use { it.write(body.toByteArray()) }
        }
        try {
            val code = connection.responseCode
            val stream = if (code in 200..299) connection.inputStream else connection.errorStream
            val text = stream?.bufferedReader()?.use { it.readText() }.orEmpty()
            if (code !in 200..299) error("控制器返回 $code")
            return if (text.isBlank()) JSONObject() else JSONObject(text)
        } finally {
            connection.disconnect()
        }
    }
}
