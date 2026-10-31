package dev.manis.app.data

import android.content.Context
import java.io.ByteArrayOutputStream
import java.io.File
import java.net.HttpURLConnection
import java.net.URL

/**
 * Downloads subscriptions without the VPN and stores them where the core expects its provider
 * cache.
 *
 * Each enabled subscription gets a provider path derived from a hash of its URL. When that file
 * exists, Mihomo uses it immediately and updates it from the URL in the background, so a first
 * connect does not have to wait for the subscription to be reachable from the tunnel. Because the
 * path changes with the URL, replacing a subscription can never load another airport's cached
 * nodes.
 */
internal object SubscriptionCache {
    private const val USER_AGENT = "clash.meta"
    private const val CONNECT_TIMEOUT_MS = 10_000
    private const val READ_TIMEOUT_MS = 15_000
    private const val MAX_BYTES = 8 * 1024 * 1024
    private const val DIRECTORY = "mihomo/proxy_providers"

    data class Result(
        val downloaded: Int,
        val nodes: Int,
        val failures: List<String>,
    )

    /** Re-downloads every enabled subscription into the provider cache. */
    fun refresh(context: Context, config: MobileConfig): Result {
        val directory = File(context.filesDir, DIRECTORY).apply { mkdirs() }
        val failures = mutableListOf<String>()
        var downloaded = 0
        var nodes = 0

        config.subscriptions.filter { it.enabled }.forEachIndexed { index, subscription ->
            val file = File(directory, fileName(subscription.url))
            val label = subscription.name.ifBlank { "订阅 ${index + 1}" }
            val body = runCatching { fetch(subscription.url) }
                .onFailure { failures += "$label：${it.message ?: "下载失败"}" }
                .getOrNull()
            if (body != null) {
                if (!body.contains("proxies:")) {
                    failures += "$label：不是 Clash 订阅（未找到 proxies）"
                } else {
                    file.writeText(body)
                    downloaded += 1
                    nodes += countProxies(body)
                }
            } else if (file.isFile) {
                // The path is the URL hash, so an existing file is always this subscription.
                nodes += countProxies(file.readText())
            }
        }

        val expected = config.subscriptions
            .filter { it.enabled }
            .map { fileName(it.url) }
            .toSet()
        directory.listFiles()?.forEach { file ->
            if (isCacheFile(file.name) && file.name !in expected) file.delete()
        }
        return Result(downloaded, nodes, failures)
    }

    /**
     * Removes provider files that no enabled subscription references before the core starts.
     *
     * The hash-based names already prevent a replaced URL from reusing another file; this only
     * reclaims space for removed or disabled subscriptions.
     */
    fun prune(context: Context, config: MobileConfig) {
        val directory = File(context.filesDir, DIRECTORY)
        if (!directory.isDirectory) return
        val expected = config.subscriptions
            .filter { it.enabled }
            .map { fileName(it.url) }
            .toSet()
        directory.listFiles()?.forEach { file ->
            if (isCacheFile(file.name) && file.name !in expected) file.delete()
        }
    }

    /** Counts the top-level `proxies:` list entries without pretending to parse all of YAML. */
    internal fun countProxies(document: String): Int {
        var proxiesIndent: Int? = null
        var itemIndent: Int? = null
        var count = 0
        for (line in document.lineSequence()) {
            if (line.isBlank()) continue
            val trimmed = line.trimStart()
            if (trimmed.startsWith("#")) continue
            val indent = line.length - trimmed.length
            val active = proxiesIndent
            if (active == null) {
                if (indent == 0 && trimmed.startsWith("proxies:")) proxiesIndent = indent
                continue
            }
            if (indent <= active) break
            if (!trimmed.startsWith("-")) continue
            val item = itemIndent
            if (item == null) itemIndent = indent
            if (indent == itemIndent) count += 1
        }
        return count
    }

    private fun fileName(url: String): String = subscriptionCachePath(url).substringAfterLast('/')

    private fun isCacheFile(name: String): Boolean =
        name.endsWith(".yaml") && (name.startsWith("sub-") || name.startsWith("subscription-"))

    private fun fetch(url: String): String {
        val connection = URL(url).openConnection() as HttpURLConnection
        connection.connectTimeout = CONNECT_TIMEOUT_MS
        connection.readTimeout = READ_TIMEOUT_MS
        connection.instanceFollowRedirects = true
        connection.setRequestProperty("User-Agent", USER_AGENT)
        try {
            val code = connection.responseCode
            if (code !in 200..299) error("HTTP $code")
            val output = ByteArrayOutputStream()
            connection.inputStream.use { stream ->
                val buffer = ByteArray(16 * 1024)
                while (true) {
                    val read = stream.read(buffer)
                    if (read < 0) break
                    output.write(buffer, 0, read)
                    if (output.size() > MAX_BYTES) error("订阅超过 8 MB")
                }
            }
            return String(output.toByteArray(), Charsets.UTF_8)
        } finally {
            connection.disconnect()
        }
    }
}
