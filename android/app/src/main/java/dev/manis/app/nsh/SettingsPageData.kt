package dev.manis.app.nsh

import android.content.Context
import dev.manis.app.core.Native
import dev.manis.app.data.AvailableUpdate
import dev.manis.app.data.MobileConfig
import dev.manis.app.data.UpdateChecker
import dev.manis.app.data.UpdateState
import dev.manis.app.ui.RuntimeState
import dev.manis.app.ui.proxyDnsLabel
import org.json.JSONArray
import org.json.JSONObject

/** Snapshot of the settings page, including the current update flow state. */
internal fun buildSettingsData(
    context: Context,
    config: MobileConfig,
    update: UpdateState,
): JSONObject {
    val updateJson = JSONObject()
    when (update) {
        UpdateState.Idle -> updateJson.put("state", "idle")
        UpdateState.Checking -> updateJson.put("state", "checking")
        UpdateState.UpToDate -> updateJson.put("state", "upToDate")
        is UpdateState.Available -> {
            updateJson.put("state", "available")
            updateJson.put("update", update.update.toJson())
        }
        is UpdateState.Downloading -> {
            updateJson.put("state", "downloading")
            updateJson.put("update", update.update.toJson())
            updateJson.put("progress", update.progress)
        }
        is UpdateState.Ready -> {
            updateJson.put("state", "ready")
            updateJson.put("update", update.update.toJson())
        }
        is UpdateState.Failed -> {
            updateJson.put("state", "failed")
            updateJson.put("message", update.message)
            update.update?.let { updateJson.put("update", it.toJson()) }
        }
    }

    val request = JSONObject()
        .put("status", if (RuntimeState.active) "已连接" else "未连接")
        .put("mode", RuntimeState.snapshot?.mode ?: config.mode)
        .put(
            "proxyDns",
            JSONObject()
                .put("mode", config.proxyDns)
                .put("custom", JSONArray(config.proxyDnsServers))
                .put("discovered", JSONArray(config.discoveredDns))
                .put(
                    "label",
                    proxyDnsLabel(config.proxyDns, config.proxyDnsServers, config.discoveredDns),
                ),
        )
        .put("kernelVersion", RuntimeState.snapshot?.version?.ifBlank { "—" } ?: "—")
        .put("summary", RuntimeState.summary.ifBlank { "—" })
        .put("subscriptions", config.subscriptions.size)
        .put("nodes", config.nodes.size)
        .put("groups", config.groups.size)
        .put("abi", android.os.Build.SUPPORTED_ABIS.firstOrNull() ?: "unknown")
        .put("dataDir", "files/mihomo")
        .put("installedVersion", UpdateChecker.installedVersion(context).ifBlank { "未知" })
        .put("update", updateJson)
    return runCatching { JSONObject(Native.settingsPage(request.toString())) }
        .getOrElse { request }
}

private fun AvailableUpdate.toJson(): JSONObject = JSONObject()
    .put("version", version)
    .put("size", size)
    .put("releaseUrl", releaseUrl)
