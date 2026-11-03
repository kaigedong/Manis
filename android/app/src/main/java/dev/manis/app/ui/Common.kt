package dev.manis.app.ui

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
    Workspace.pending.value = false
    context.startService(
        Intent(context, ManisVpnService::class.java).setAction(ManisVpnService.ACTION_RELOAD),
    )
}

/** Internal controller names that must never reach the UI. */
internal fun displayProxyName(name: String?): String? = when (name) {
    null -> null
    "__MANIS_GLOBAL__", "GLOBAL" -> "全局"
    else -> name
}
