package dev.manis.app.core

import android.content.Context
import android.net.ConnectivityManager
import dev.manis.app.data.PROXY_DNS_SYSTEM
import dev.manis.app.data.SubscriptionCache
import dev.manis.app.data.Workspace
import java.io.File
import java.security.SecureRandom
import org.json.JSONArray
import org.json.JSONObject

/**
 * Owns the bundled Mihomo process for the lifetime of the VPN service.
 *
 * The VpnService descriptor stays open in the service, so a workspace change only restarts the
 * core: the tunnel never goes down and Android keeps routing through the same interface.
 */
internal object CoreController {
    const val MIXED_PORT = 7890
    private const val TUN_DESCRIPTOR = 3
    private const val CORE_BINARY = "libmihomo.so"
    private const val CONTROLLER_PORT = 9090
    /** DNS stub the [dev.manis.app.ManisVpnService] tunnel hands to Android. */
    private const val TUNNEL_DNS = "172.19.0.2"
    private const val TUNNEL_DNS_INET6 = "fd00:1::1"

    @Volatile
    var lastError: String? = null
        private set

    @Volatile
    var warnings: List<String> = emptyList()
        private set

    @Volatile
    var running: Boolean = false
        private set

    @Volatile
    var secret: String = ""
        private set

    @Volatile
    var summary: String = ""
        private set

    fun start(context: Context, descriptor: Int): Boolean =
        renderAndRun(context, descriptor, restart = false)

    fun reload(context: Context, descriptor: Int): Boolean =
        renderAndRun(context, descriptor, restart = true)

    fun stop() {
        Native.stopCore()
        running = false
    }

    fun controllerEndpoint(): String = "127.0.0.1:$CONTROLLER_PORT"

    private fun renderAndRun(context: Context, descriptor: Int, restart: Boolean): Boolean {
        lastError = null
        android.util.Log.d("Manis", "renderAndRun restart=$restart")
        return try {
            val dataDirectory = File(context.filesDir, "mihomo").apply { mkdirs() }
            val config = File(dataDirectory, "config.yaml")
            // Drop provider caches that no longer belong to the enabled subscription at this index;
            // Mihomo will fetch the correct URL instead of loading another airport's nodes.
            SubscriptionCache.prune(context, Workspace.config.value)
            val document = JSONObject(Workspace.config.value.toJson())
            val customResolvers = Workspace.config.value.proxyDnsServers
                .ifEmpty { Workspace.config.value.discoveredDns }
            if (customResolvers.isNotEmpty()) {
                document.put("system_dns", JSONArray(customResolvers))
            } else if (Workspace.config.value.proxyDns == PROXY_DNS_SYSTEM) {
                val resolvers = systemDns(context)
                if (resolvers.isNotEmpty()) {
                    document.put("system_dns", JSONArray(resolvers))
                }
            }
            val envelope = Native.renderConfig(document.toString(), TUN_DESCRIPTOR)
            val rendered = JSONObject(envelope)
            config.writeText(rendered.getString("yaml"))
            warnings = rendered.optJSONArray("warnings")?.let { array ->
                (0 until array.length()).map(array::getString)
            } ?: emptyList()
            summary = buildString {
                append(rendered.optInt("providers"))
                append(" 个订阅 · ")
                append(rendered.optInt("nodes"))
                append(" 个节点 · ")
                append(rendered.optInt("rules"))
                append(" 条规则")
            }

            val binary = File(context.applicationInfo.nativeLibraryDir, CORE_BINARY)
            check(binary.isFile) { "打包的 Mihomo 内核缺失（$CORE_BINARY）" }

            // Android can restart the foreground service (for example after an install); never
            // leave the previous core running when a new one is started.
            if (running) {
                Native.stopCore()
            }
            android.util.Log.d("Manis", "render ok rules=${rendered.optInt("rules")}")
            secret = randomSecret()
            val pid = Native.startCore(
                binary.absolutePath,
                dataDirectory.absolutePath,
                config.absolutePath,
                secret,
                descriptor,
            )
            android.util.Log.d("Manis", "core pid=$pid")
            running = pid > 0
            if (!running) lastError = "Mihomo 未能启动"
            running
        } catch (error: Throwable) {
            android.util.Log.w("Manis", "renderAndRun failed", error)
            lastError = error.message ?: error.javaClass.simpleName
            running = false
            false
        }
    }

    /**
     * Resolvers of the active network, used only when the user asks the core to resolve proxy
     * hostnames the way the phone does. A carrier or hotel resolver often answers an airport's
     * node domain with a blocked address, which is why encrypted DNS stays the default.
     *
     * The tunnel's own stub is dropped: the core bypasses the tunnel, so it has no route to an
     * address that only exists on the tunnel interface.
     */
    private fun systemDns(context: Context): List<String> {
        val manager = context.getSystemService(ConnectivityManager::class.java) ?: return emptyList()
        val network = manager.activeNetwork ?: return emptyList()
        val properties = manager.getLinkProperties(network) ?: return emptyList()
        return properties.dnsServers
            .mapNotNull { address -> address.hostAddress }
            .filterNot { address ->
                address.startsWith("fe80") || address == "127.0.0.1" || address == "::1" ||
                    address == TUNNEL_DNS || address == TUNNEL_DNS_INET6
            }
            .distinct()
            .take(4)
    }

    private fun randomSecret(): String {
        val bytes = ByteArray(24)
        SecureRandom().nextBytes(bytes)
        return bytes.joinToString("") { "%02x".format(it) }
    }
}
