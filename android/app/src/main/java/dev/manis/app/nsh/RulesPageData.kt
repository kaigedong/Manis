package dev.manis.app.nsh

import dev.manis.app.core.Native
import dev.manis.app.data.MobileConfig
import org.json.JSONArray
import org.json.JSONObject

/** Serializes the rule editor and the connection summary for the NSH rules page. */
internal fun buildRulesData(
    config: MobileConfig,
    warnings: List<String>,
    summary: String,
): JSONObject {
    val rules = JSONArray()
    config.ruleEntries.forEach { rule ->
        rules.put(
            JSONObject()
                .put("kind", rule.kind)
                .put("value", rule.value)
                .put("policy", rule.policy)
                .put("enabled", rule.enabled),
        )
    }
    val policies = JSONArray()
    config.groups.forEach { group -> policies.put(group.name) }
    val warningArray = JSONArray()
    warnings.forEach(warningArray::put)
    val request = JSONObject()
        .put("rules", rules)
        .put("policies", policies)
        .put("warnings", warningArray)
        .put("summary", summary)
    return runCatching { JSONObject(Native.rulesPage(request.toString())) }
        .getOrElse {
            JSONObject()
                .put("rules", JSONArray())
                .put("kinds", JSONArray())
                .put("policies", JSONArray())
                .put("warnings", JSONArray())
                .put("summary", summary)
        }
}
