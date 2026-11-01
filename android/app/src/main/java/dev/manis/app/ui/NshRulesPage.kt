package dev.manis.app.ui

import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.viewinterop.AndroidView
import com.longbridge.nsh.NshView
import dev.manis.app.data.RuleEntry
import dev.manis.app.data.Workspace
import dev.manis.app.nsh.buildRulesData
import kotlinx.coroutines.launch
import org.json.JSONObject

/** NSH rule editor; the host keeps persistence and the apply step. */
@Composable
internal fun NshRulesPage(onMessage: (String) -> Unit, modifier: Modifier = Modifier) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val config = Workspace.config.value
    val initial = remember(config) {
        buildRulesData(config, RuntimeState.warnings, RuntimeState.summary)
    }

    AndroidView(
        modifier = modifier.fillMaxSize(),
        factory = { viewContext ->
            NshView(viewContext).apply {
                setErrorListener { onMessage("NSH: $it") }
                setParams(mapOf("page" to "rules", "data" to initial, "embedded" to true))
                handle("rules.toggle") { payload, reply ->
                    val values = payload as? Map<*, *>
                    val index = (values?.get("index") as? Number)?.toInt()
                    val enabled = values?.get("enabled") as? Boolean
                    if (index != null && enabled != null) {
                        Workspace.update { current ->
                            current.copy(
                                ruleEntries = current.ruleEntries.mapIndexed { at, rule ->
                                    if (at == index) rule.copy(enabled = enabled) else rule
                                },
                            )
                        }
                    }
                    reply.resolve(replyPayload())
                }
                handle("rules.delete") { payload, reply ->
                    val index = ((payload as? Map<*, *>)?.get("index") as? Number)?.toInt()
                    if (index != null) {
                        Workspace.update { current ->
                            current.copy(
                                ruleEntries = current.ruleEntries.filterIndexed { at, _ -> at != index },
                            )
                        }
                    }
                    reply.resolve(replyPayload())
                }
                handle("rules.save") { payload, reply ->
                    val values = payload as? Map<*, *>
                    val index = (values?.get("index") as? Number)?.toInt()
                    val kind = (values?.get("kind") as? String)?.trim().orEmpty()
                    val value = (values?.get("value") as? String)?.trim().orEmpty()
                    val policy = (values?.get("policy") as? String)?.trim().orEmpty()
                    val enabled = values?.get("enabled") as? Boolean ?: true
                    val error = when {
                        kind.isBlank() -> "请选择规则类型"
                        kind != "FINAL" && value.isBlank() -> "请填写匹配内容"
                        policy.isBlank() -> "请选择策略"
                        kind == "FINAL" && Workspace.config.value.ruleEntries.withIndex().any { (at, rule) ->
                            at != index && rule.enabled && rule.kind == "FINAL"
                        } -> "只能有一条启用的最终规则"
                        else -> null
                    }
                    if (error != null) {
                        reply.resolve(replyPayload(error))
                    } else {
                        val entry = RuleEntry(
                            kind = kind,
                            value = if (kind == "FINAL") "" else value,
                            policy = policy,
                            enabled = enabled,
                        )
                        Workspace.update { current ->
                            val entries = current.ruleEntries.toMutableList()
                            if (index == null) entries += entry else if (index in entries.indices) entries[index] = entry
                            current.copy(ruleEntries = entries)
                        }
                        reply.resolve(replyPayload("规则已保存，点「应用」生效"))
                    }
                }
                handle("rules.apply") { _, reply ->
                    scope.launch {
                        applyWorkspace(context)
                        reply.resolve(replyPayload("已应用"))
                    }
                }
                if (!loadAssetBundle("nsh")) onMessage("NSH 规则页加载失败")
            }
        },
        update = { view ->
            view.send(
                "rules.updated",
                mapOf(
                    "data" to buildRulesData(
                        Workspace.config.value,
                        RuntimeState.warnings,
                        RuntimeState.summary,
                    ),
                ),
            )
        },
        onRelease = { view -> view.destroy() },
    )
}

private fun NshView.replyPayload(message: String? = null): Map<String, Any?> = buildMap {
    put(
        "data",
        buildRulesData(
            Workspace.config.value,
            RuntimeState.warnings,
            RuntimeState.summary,
        ),
    )
    if (message != null) put("message", message)
}
