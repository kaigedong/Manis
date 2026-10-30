package dev.manis.app.data

import android.content.Context
import java.net.HttpURLConnection
import java.net.URL

/**
 * Discovers the airport's own resolvers from the subscription document.
 *
 * Airports that rotate node entry addresses publish a top-level `dns:` block whose
 * `proxy-server-nameserver` list points at their private DoH endpoints. Only those endpoints return
 * an entry address that actually accepts connections, so public DNS makes every node look dead.
 * The desktop build reads the same block; this keeps the phone identical without asking the user to
 * paste addresses.
 */
internal object SubscriptionDns {
    private const val USER_AGENT = "clash.meta"
    private const val CONNECT_TIMEOUT_MS = 4_000
    private const val READ_TIMEOUT_MS = 6_000
    private const val MAX_BYTES = 2 * 1024 * 1024
    private const val MAX_SERVERS = 8

    /**
     * Refreshes the discovered resolvers and returns true when they changed.
     *
     * A manually entered list always wins, so discovery is skipped entirely in that case.
     */
    fun refresh(context: Context): Boolean {
        val config = Workspace.config.value
        if (config.proxyDnsServers.isNotEmpty()) return false
        val enabled = config.subscriptions.filter { it.enabled }
        if (enabled.isEmpty()) return false

        val discovered = LinkedHashSet<String>()
        for (subscription in enabled) {
            val document = runCatching { fetch(subscription.url) }.getOrNull() ?: continue
            discovered += extract(document)
            if (discovered.size >= MAX_SERVERS) break
        }
        val servers = discovered.take(MAX_SERVERS)
        if (servers == config.discoveredDns) return false
        // Endpoints can disappear; keep the last working list instead of clearing it on a bad fetch.
        if (servers.isEmpty()) return false
        Workspace.setDiscoveredDns(servers)
        return true
    }

    private fun fetch(url: String): String? {
        val connection = URL(url).openConnection() as HttpURLConnection
        connection.connectTimeout = CONNECT_TIMEOUT_MS
        connection.readTimeout = READ_TIMEOUT_MS
        connection.instanceFollowRedirects = true
        connection.setRequestProperty("User-Agent", USER_AGENT)
        try {
            if (connection.responseCode !in 200..299) return null
            val body = connection.inputStream.use { stream ->
                val buffer = ByteArray(16 * 1024)
                val output = java.io.ByteArrayOutputStream()
                while (output.size() <= MAX_BYTES) {
                    val read = stream.read(buffer)
                    if (read < 0) break
                    output.write(buffer, 0, read)
                }
                output.toByteArray()
            }
            if (body.size > MAX_BYTES) return null
            return String(body, Charsets.UTF_8)
        } finally {
            connection.disconnect()
        }
    }

    /** Mirrors the desktop reader: the top-level `dns:` block, then its resolver list. */
    internal fun extract(document: String): List<String> {
        val servers = LinkedHashSet<String>()
        var dnsIndent: Int? = null
        var listIndent: Int? = null
        for (line in document.lineSequence()) {
            val trimmed = line.trim()
            if (trimmed.isEmpty() || trimmed.startsWith("#")) continue
            val indent = line.length - line.trimStart().length

            val activeDns = dnsIndent
            if (activeDns == null) {
                if (indent == 0 && trimmed == "dns:") dnsIndent = indent
                continue
            }
            if (indent <= activeDns) break

            val activeList = listIndent
            if (activeList != null) {
                if (indent <= activeList) break
                add(trimmed.removePrefix("-"), servers)
                if (servers.size >= MAX_SERVERS) break
                continue
            }

            if (!trimmed.startsWith("proxy-server-nameserver:")) continue
            listIndent = indent
            val value = trimmed.removePrefix("proxy-server-nameserver:").trim()
            if (value.startsWith("[") && value.endsWith("]")) {
                for (scalar in value.substring(1, value.length - 1).split(',')) {
                    add(scalar, servers)
                }
                break
            }
        }
        return servers.take(MAX_SERVERS)
    }

    private fun add(raw: String, into: MutableSet<String>) {
        var value = raw.trim().trimEnd(',')
        if (value.length >= 2 &&
            ((value.startsWith("'") && value.endsWith("'")) ||
                (value.startsWith("\"") && value.endsWith("\"")))
        ) {
            value = value.substring(1, value.length - 1)
        }
        if (!value.startsWith("https://") || value.any { it.isWhitespace() }) return
        into += value
    }
}
