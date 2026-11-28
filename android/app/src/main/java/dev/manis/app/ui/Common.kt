package dev.manis.app.ui

import dev.manis.app.data.GroupEntry

import android.content.Context
import android.content.Intent
import dev.manis.app.ManisVpnService
import dev.manis.app.data.Workspace

fun startVpn(context: Context) {
    context.startForegroundService(
        Intent(context, ManisVpnService::class.java).setAction(ManisVpnService.ACTION_START),
    )
}

fun stopVpn(context: Context) {
    context.startService(
        Intent(context, ManisVpnService::class.java).setAction(ManisVpnService.ACTION_STOP),
    )
}

/** Tells the running service to rebuild the core from the current workspace. */
fun applyWorkspace(context: Context) {
    Workspace.markApplied()
    context.startService(
        Intent(context, ManisVpnService::class.java).setAction(ManisVpnService.ACTION_RELOAD),
    )
}

internal const val MANIS_GLOBAL_GROUP_NAME = "__MANIS_GLOBAL__"
internal const val MANIS_EXIT_IP_PROBE_GROUP_NAME = "__MANIS_EXIT_IP_PROBE__"

/** Internal controller groups created by Manis or Mihomo that must never appear as user route stages. */
internal fun isInternalProxyGroup(name: String): Boolean {
    val trimmed = name.trim()
    return trimmed == MANIS_GLOBAL_GROUP_NAME ||
        trimmed == MANIS_EXIT_IP_PROBE_GROUP_NAME ||
        trimmed == "GLOBAL"
}

/** Internal controller names that must never reach the UI. */
internal fun displayProxyName(name: String?): String? = when (name) {
    null -> null
    MANIS_GLOBAL_GROUP_NAME, "GLOBAL" -> "全局"
    MANIS_EXIT_IP_PROBE_GROUP_NAME -> "出口探测"
    else -> name
}

/** What a saved policy group pulls in, in the same words the editor uses. */
internal fun GroupEntry.scopeSummary(): String = when {
    scope == "contains" -> "名称包含 ${keyword ?: filter.orEmpty()}"
    selectedNames.isNotEmpty() -> "${selectedNames.size} 个节点"
    filter != null -> "自定义过滤"
    else -> "全部节点"
}
