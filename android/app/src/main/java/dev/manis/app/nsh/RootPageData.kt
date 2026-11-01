package dev.manis.app.nsh

import android.content.Context
import dev.manis.app.data.MobileConfig
import dev.manis.app.data.UpdateManager
import dev.manis.app.data.Workspace
import dev.manis.app.ui.RuntimeState
import org.json.JSONObject

/** One payload for the single NSH root; all tabs read from this snapshot. */
internal fun buildRootData(
    context: Context,
    config: MobileConfig,
    localError: String?,
): JSONObject = JSONObject()
    .put("pending", Workspace.pending.value)
    .put("nodes", buildNodePage(context, config, RuntimeState.snapshot, RuntimeState.active))
    .put("subscriptions", buildSubscriptionsData(config))
    .put(
        "rules",
        buildRulesData(config, RuntimeState.warnings, RuntimeState.summary),
    )
    .put(
        "settings",
        buildSettingsData(context, config, UpdateManager.state.value),
    )
    .put("connection", buildConnectionData(config, localError))
