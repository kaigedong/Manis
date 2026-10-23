package dev.manis.app.core

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL

internal data class GroupState(
    val now: String?,
    val all: List<String>,
    val delays: Map<String, Int>,
)

/**
 * Minimal client for the core's loopback controller.
 *
 * The controller requires the per-launch secret, so [configure] must be called with
 * [CoreController.secret] before any request.
 */
internal object ControllerApi {
    const val GROUP = "__MANIS_GLOBAL__"
    private const val BASE = "http://127.0.0.1:9090"

    private val builtIn = setOf("COMPATIBLE", "DIRECT", "REJECT", "PASS", "GLOBAL")

    private var secret: String = ""

    fun configure(secret: String) {
        this.secret = secret
    }

    suspend fun group(): GroupState = withContext(Dispatchers.IO) {
        val group = request("/proxies/$GROUP")
        val all = group.optJSONArray("all")?.let { array ->
            (0 until array.length())
                .map(array::getString)
                .filterNot { it in builtIn }
        } ?: emptyList()
        val now = group.optString("now")
            .takeIf { it.isNotBlank() && it != "null" && it in all }

        val proxies = request("/proxies")
        val delays = buildMap {
            for (name in all) {
                val proxy = proxies.optJSONObject(name) ?: continue
                val history = proxy.optJSONArray("history") ?: continue
                if (history.length() == 0) continue
                val delay = history.getJSONObject(history.length() - 1).optInt("delay", 0)
                if (delay > 0) put(name, delay)
            }
        }
        GroupState(now, all, delays)
    }

    suspend fun select(name: String) = withContext(Dispatchers.IO) {
        request(
            "/proxies/$GROUP",
            method = "PUT",
            body = JSONObject(mapOf("name" to name)).toString(),
        )
        Unit
    }

    private fun request(path: String, method: String = "GET", body: String? = null): JSONObject {
        val connection = URL("$BASE$path").openConnection() as HttpURLConnection
        connection.requestMethod = method
        connection.connectTimeout = 3_000
        connection.readTimeout = 5_000
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
            if (code !in 200..299) error("controller returned $code")
            return if (text.isBlank()) JSONObject() else JSONObject(text)
        } finally {
            connection.disconnect()
        }
    }
}
