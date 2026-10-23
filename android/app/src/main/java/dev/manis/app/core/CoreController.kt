package dev.manis.app.core

import android.content.Context
import java.io.File
import java.security.SecureRandom

/**
 * Owns the bundled Mihomo process for the lifetime of the VPN service.
 *
 * The TUN descriptor is produced by [dev.manis.app.ManisVpnService] and handed to Rust, which
 * duplicates it onto fd 3 in the child and releases the parent copy.
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
    var running: Boolean = false
        private set

    @Volatile
    var secret: String = ""
        private set

    fun start(context: Context, subscriptionUrl: String, tunDescriptor: Int): Boolean {
        lastError = null
        return try {
            val dataDirectory = File(context.filesDir, "mihomo").apply { mkdirs() }
            val config = File(dataDirectory, "config.yaml")
            config.writeText(
                Native.renderConfig(subscriptionUrl, MIXED_PORT, TUN_DESCRIPTOR),
            )
            val binary = File(context.applicationInfo.nativeLibraryDir, CORE_BINARY)
            check(binary.isFile) { "打包的 Mihomo 内核缺失（$CORE_BINARY）" }

            secret = randomSecret()
            val pid = Native.startCore(
                binary.absolutePath,
                dataDirectory.absolutePath,
                config.absolutePath,
                secret,
                tunDescriptor,
            )
            running = pid > 0
            if (!running) lastError = "Mihomo 未能启动"
            running
        } catch (error: Throwable) {
            lastError = error.message ?: error.javaClass.simpleName
            running = false
            false
        }
    }

    fun stop() {
        Native.stopCore()
        running = false
    }

    fun controllerEndpoint(): String = "127.0.0.1:$CONTROLLER_PORT"

    private fun randomSecret(): String {
        val bytes = ByteArray(24)
        SecureRandom().nextBytes(bytes)
        return bytes.joinToString("") { "%02x".format(it) }
    }
}
