package dev.manis.app.nsh

import dev.manis.app.data.MobileConfig
import dev.manis.app.data.RULE_KINDS
import org.json.JSONArray
import org.json.JSONObject

/** Serializes the rule editor and the connection summary for the NSH rules page. */
internal fun buildRulesData(
    config: MobileConfig,
    warnings: List<String>,
    summary: String,
): JSONObject {
    val rules = JSONArray()
    config.ruleEntries.forEachIndexed { index, rule ->
        rules.put(
            JSONObject()
                .put("index", index)
                .put("kind", rule.kind)
                .put("value", rule.value)
                .put("policy", rule.policy)
                .put("policyLabel", policyLabel(rule.policy))
                .put("enabled", rule.enabled),
        )
    }
    val policies = JSONArray()
    policies.put("DIRECT")
    policies.put("REJECT")
    policies.put("__MANIS_GLOBAL__")
    config.groups.forEach { group ->
        if (policies.toList().none { it == group.name }) policies.put(group.name)
    }
    val warningArray = JSONArray()
    warnings.take(8).forEach(warningArray::put)
    return JSONObject()
        .put("rules", rules)
        .put("kinds", JSONArray(RULE_KINDS))
        .put("policies", policies)
        .put("warnings", warningArray)
        .put("summary", summary)
}

private fun policyLabel(policy: String): String = when (policy) {
    "__MANIS_GLOBAL__" -> "全局"
    else -> policy
}

private fun JSONArray.toList(): List<String> = (0 until length()).map(::getString)
