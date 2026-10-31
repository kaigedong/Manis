package dev.manis.app.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExposedDropdownMenuBox
import androidx.compose.material3.ExposedDropdownMenuDefaults
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.MenuAnchorType
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import dev.manis.app.data.MobileConfig
import dev.manis.app.data.RULE_KINDS
import dev.manis.app.data.RuleEntry
import dev.manis.app.data.Workspace

private data class RuleEditor(val index: Int?, val entry: RuleEntry)

@Composable
fun RulesScreen(onMessage: (String) -> Unit) {
    val config = Workspace.config.value
    var editor by remember { mutableStateOf<RuleEditor?>(null) }

    fun save(entries: List<RuleEntry>) {
        Workspace.update { it.copy(ruleEntries = entries) }
    }

    fun updateRule(index: Int, entry: RuleEntry) {
        save(
            config.ruleEntries.mapIndexed { at, item ->
                if (at == index) entry else item
            },
        )
    }

    fun deleteRule(index: Int) {
        save(config.ruleEntries.filterIndexed { at, _ -> at != index })
    }

    Column(
        Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(20.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        ScreenTitle("规则", "按顺序匹配；未命中的流量自动走全局组")
        ApplyBar()

        SectionCard(
            "规则",
            trailing = {
                TextButton(onClick = { editor = RuleEditor(null, RuleEntry()) }) { Text("添加") }
            },
        ) {
            if (config.ruleEntries.isEmpty()) {
                Text(
                    "还没有规则。添加一条 DOMAIN-SUFFIX 或 IP-CIDR 规则即可开始。",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            for ((index, rule) in config.ruleEntries.withIndex()) {
                RuleRow(
                    rule = rule,
                    onToggle = { enabled -> updateRule(index, rule.copy(enabled = enabled)) },
                    onEdit = { editor = RuleEditor(index, rule) },
                    onDelete = { deleteRule(index) },
                )
                if (index != config.ruleEntries.lastIndex) HorizontalDivider()
            }
            Text(
                "规则从上到下匹配；策略可以是 DIRECT、REJECT 或任意策略组。" +
                    "最终规则之后的规则不会再生效。",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }

        if (RuntimeState.warnings.isNotEmpty()) {
            SectionCard("解析提示") {
                for (warning in RuntimeState.warnings.take(8)) {
                    Text(
                        warning,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        }

        Text(
            RuntimeState.summary.ifBlank { "尚未连接，应用后可在「连接」页查看摘要" },
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }

    editor?.let { state ->
        RuleDialog(
            initial = state.entry,
            editingIndex = state.index,
            existing = config.ruleEntries,
            policies = policyOptions(config),
            onDismiss = { editor = null },
            onConfirm = { entry ->
                val entries = config.ruleEntries.toMutableList()
                if (state.index == null) {
                    entries += entry
                } else {
                    entries[state.index] = entry
                }
                save(entries)
                editor = null
                onMessage("规则已保存，点「应用」生效")
            },
        )
    }
}

@Composable
private fun RuleRow(
    rule: RuleEntry,
    onToggle: (Boolean) -> Unit,
    onEdit: () -> Unit,
    onDelete: () -> Unit,
) {
    Row(
        Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(
                rule.kind,
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.primary,
            )
            Text(
                if (rule.kind == "FINAL") "所有剩余流量" else rule.value,
                style = MaterialTheme.typography.bodyLarge,
            )
            Text(
                "→ ${policyLabel(rule.policy)}",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        Column(horizontalAlignment = Alignment.End) {
            Switch(checked = rule.enabled, onCheckedChange = onToggle)
            Row {
                IconButton(onClick = onEdit) {
                    Icon(Icons.Filled.Edit, contentDescription = "编辑规则")
                }
                IconButton(onClick = onDelete) {
                    Icon(Icons.Filled.Delete, contentDescription = "删除规则")
                }
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun RuleDialog(
    initial: RuleEntry,
    editingIndex: Int?,
    existing: List<RuleEntry>,
    policies: List<String>,
    onDismiss: () -> Unit,
    onConfirm: (RuleEntry) -> Unit,
) {
    var kind by remember { mutableStateOf(initial.kind) }
    var value by remember { mutableStateOf(initial.value) }
    var policy by remember { mutableStateOf(initial.policy) }
    var error by remember { mutableStateOf<String?>(null) }

    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(if (editingIndex == null) "添加规则" else "编辑规则") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                ChoiceField(
                    label = "类型",
                    value = kind,
                    options = RULE_KINDS,
                    onSelect = {
                        kind = it
                        if (it == "FINAL") value = ""
                    },
                )
                if (kind != "FINAL") {
                    OutlinedTextField(
                        value = value,
                        onValueChange = { value = it },
                        label = { Text(kindLabel(kind)) },
                        placeholder = { Text(kindPlaceholder(kind)) },
                        singleLine = true,
                    )
                }
                ChoiceField(
                    label = "策略",
                    value = policy,
                    options = policies,
                    display = ::policyLabel,
                    onSelect = { policy = it },
                )
                Text(
                    kindHelp(kind),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
            }
        },
        confirmButton = {
            TextButton(onClick = {
                if (kind != "FINAL" && value.isBlank()) {
                    error = "请填写匹配内容"
                    return@TextButton
                }
                val anotherFinal = existing.withIndex().any { (index, rule) ->
                    index != editingIndex && rule.enabled && rule.kind == "FINAL"
                }
                if (kind == "FINAL" && anotherFinal) {
                    error = "只能有一条启用的最终规则"
                    return@TextButton
                }
                onConfirm(
                    RuleEntry(
                        kind = kind,
                        value = if (kind == "FINAL") "" else value.trim(),
                        policy = policy,
                        enabled = initial.enabled,
                    ),
                )
            }) { Text("保存") }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("取消") } },
    )
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ChoiceField(
    label: String,
    value: String,
    options: List<String>,
    display: (String) -> String = { it },
    onSelect: (String) -> Unit,
) {
    var expanded by remember { mutableStateOf(false) }
    ExposedDropdownMenuBox(expanded = expanded, onExpandedChange = { expanded = it }) {
        OutlinedTextField(
            value = display(value),
            onValueChange = {},
            readOnly = true,
            label = { Text(label) },
            trailingIcon = { ExposedDropdownMenuDefaults.TrailingIcon(expanded = expanded) },
            modifier = Modifier.menuAnchor(MenuAnchorType.PrimaryNotEditable).fillMaxWidth(),
        )
        ExposedDropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
            for (option in options) {
                DropdownMenuItem(
                    text = { Text(display(option)) },
                    onClick = {
                        onSelect(option)
                        expanded = false
                    },
                )
            }
        }
    }
}

private fun policyOptions(config: MobileConfig): List<String> {
    val options = mutableListOf("DIRECT", "REJECT", "__MANIS_GLOBAL__")
    for (group in config.groups) {
        if (group.name !in options) options += group.name
    }
    return options
}

private fun policyLabel(policy: String): String = when (policy) {
    "__MANIS_GLOBAL__" -> "全局"
    else -> policy
}

private fun kindLabel(kind: String): String = when (kind) {
    "DOMAIN" -> "域名"
    "DOMAIN-SUFFIX" -> "域名后缀"
    "DOMAIN-KEYWORD" -> "域名关键字"
    "DOMAIN-WILDCARD" -> "域名通配符"
    "IP-CIDR" -> "IPv4 CIDR"
    "IP6-CIDR" -> "IPv6 CIDR"
    "GEOIP" -> "国家/地区"
    "IP-ASN" -> "ASN"
    "DST-PORT" -> "目标端口"
    else -> "匹配内容"
}

private fun kindPlaceholder(kind: String): String = when (kind) {
    "DOMAIN" -> "example.com"
    "DOMAIN-SUFFIX" -> "example.com"
    "DOMAIN-KEYWORD" -> "google"
    "DOMAIN-WILDCARD" -> "*.example.com"
    "IP-CIDR" -> "10.0.0.0/8"
    "IP6-CIDR" -> "2001:db8::/32"
    "GEOIP" -> "CN"
    "IP-ASN" -> "13335"
    "DST-PORT" -> "443"
    else -> ""
}

private fun kindHelp(kind: String): String = when (kind) {
    "DOMAIN" -> "只匹配完整域名。"
    "DOMAIN-SUFFIX" -> "匹配该域名及其所有子域名。"
    "DOMAIN-KEYWORD" -> "域名中包含该关键字即匹配。"
    "DOMAIN-WILDCARD" -> "支持 * 和 ? 的域名通配符。"
    "IP-CIDR", "IP6-CIDR" -> "按目标 IP 网段匹配，不触发 DNS 解析。"
    "GEOIP" -> "按目标 IP 的国家/地区代码匹配。"
    "IP-ASN" -> "按目标 IP 所属 ASN 匹配。"
    "DST-PORT" -> "按目标端口匹配。"
    "FINAL" -> "兜底规则，匹配所有剩余流量；只能启用一条。"
    else -> ""
}
