package dev.manis.app.nsh

import dev.manis.app.core.Native
import dev.manis.app.data.MobileConfig
import dev.manis.app.ui.RuntimeState
import dev.manis.app.ui.displayProxyName
import org.json.JSONArray
import org.json.JSONObject

/** Snapshot of the connection page; local error wins over the service/controller error. */
internal fun buildConnectionData(
    config: MobileConfig,
    localError: String?,
): JSONObject {
    val global = RuntimeState.snapshot?.groups?.firstOrNull { it.name == "__MANIS_GLOBAL__" }
    val warnings = JSONArray()
    RuntimeState.warnings.take(4).forEach(warnings::put)
    val request = JSONObject()
        .put("active", RuntimeState.active)
        .put("node", displayProxyName(global?.now) ?: "")
        .put("mode", RuntimeState.snapshot?.mode ?: config.mode)
        .put("summary", RuntimeState.summary)
        .put("error", localError ?: RuntimeState.error ?: RuntimeState.controllerError ?: "")
        .put("unreachable", RuntimeState.unreachable)
        .put("warnings", warnings)
        .put("warningCount", RuntimeState.warnings.size)
        .put(
            "hasSources",
            config.subscriptions.any { it.enabled } || config.nodes.any { it.enabled },
        )
    return runCatching { JSONObject(Native.connectionPage(request.toString())) }
        .getOrElse { request }
}
