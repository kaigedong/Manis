package dev.manis.app.core

import android.content.Context
import dev.manis.app.data.Workspace
import java.io.File
import java.security.SecureRandom
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
            val envelope = Native.renderConfig(Workspace.config.value.toJson(), TUN_DESCRIPTOR)
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

            if (restart && running) {
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

    private fun randomSecret(): String {
        val bytes = ByteArray(24)
        SecureRandom().nextBytes(bytes)
        return bytes.joinToString("") { "%02x".format(it) }
    }
}
