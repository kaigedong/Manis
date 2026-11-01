package dev.manis.app.nsh

import android.app.Activity
import android.graphics.Color
import android.net.Uri
import android.os.Bundle
import android.util.Log
import androidx.core.view.WindowCompat
import com.longbridge.nsh.NshView
import dev.manis.app.data.Workspace
import dev.manis.app.data.subscriptionCachePath
import dev.manis.app.ui.parseProxyNames
import java.io.File

/**
 * Spike-only host for the NSH node page.
 *
 * The existing Manis app keeps its Kotlin `VpnService`, notifications and installer; this Activity
 * demonstrates that an NSH/TSX view can sit beside them and receive node data through the host
 * channel.
 */
class NshSpikeActivity : Activity() {
    private lateinit var nsh: NshView

    override fun onCreate(state: Bundle?) {
        super.onCreate(state)
        WindowCompat.setDecorFitsSystemWindows(window, false)
        window.statusBarColor = Color.TRANSPARENT
        window.navigationBarColor = Color.TRANSPARENT

        nsh = NshView(this)
        nsh.setErrorListener { message -> Log.e(TAG, message) }
        nsh.setParams(
            mapOf(
                "sources" to sources(),
                "stress" to intent.getBooleanExtra("stress", false),
            ),
        )
        nsh.handle("nodes.refresh") { _, reply ->
            reply.resolve(mapOf("sources" to sources()))
        }
        nsh.handle("nodes.select") { data, reply ->
            Log.d(TAG, "select $data")
            reply.resolve(true)
        }
        setContentView(nsh)
        if (intent.getBooleanExtra("rust", false)) {
            // Spike: the host APK's libnsh.so was replaced by a Rust app's library.
            nsh.runRustApp("nsh")
        } else if (!nsh.loadAssetBundle("nsh")) {
            Log.e(TAG, "could not load assets/nsh/bundle.nshc")
        }
    }

    @Suppress("DEPRECATION")
    override fun onBackPressed() {
        if (!nsh.handleBack()) super.onBackPressed()
    }

    override fun onDestroy() {
        nsh.destroy()
        super.onDestroy()
    }

    private fun sources(): List<Map<String, Any?>> {
        val config = Workspace.config.value
        val result = mutableListOf<Map<String, Any?>>()
        config.subscriptions.forEachIndexed { index, subscription ->
            if (!subscription.enabled) return@forEachIndexed
            val names = cachedNames(subscription.url)
            if (names.isEmpty()) return@forEachIndexed
            result += mapOf(
                "name" to subscription.name.ifBlank { "订阅 ${index + 1}" },
                "detail" to "订阅 · ${Uri.parse(subscription.url).host ?: "地址已隐藏"}",
                "nodes" to names.mapIndexed { nodeIndex, name ->
                    node(name, null, current = result.isEmpty() && nodeIndex == 0)
                },
            )
        }
        if (config.nodes.isNotEmpty()) {
            result += mapOf(
                "name" to "手动节点",
                "detail" to "分享链接导入",
                "nodes" to config.nodes.filter { it.enabled }.mapIndexed { index, node ->
                    node(node.displayName, null, current = result.isEmpty() && index == 0)
                },
            )
        }
        return result
    }

    private fun cachedNames(url: String): List<String> {
        val relative = subscriptionCachePath(url).removePrefix("./")
        val file = File(filesDir, "mihomo/$relative")
        if (!file.isFile) return emptyList()
        return runCatching { parseProxyNames(file.readText()) }.getOrDefault(emptyList())
    }

    private fun node(name: String, delay: Int?, current: Boolean = false): Map<String, Any?> =
        mapOf("name" to name, "delay" to delay, "current" to current)

    private companion object {
        const val TAG = "NshSpike"
    }
}
