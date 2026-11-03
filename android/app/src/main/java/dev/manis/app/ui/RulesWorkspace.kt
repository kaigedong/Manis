package dev.manis.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupProperties
import androidx.compose.ui.unit.dp
import dev.manis.app.data.MobileConfig
import dev.manis.app.data.RULE_KINDS
import dev.manis.app.data.RuleEntry
import dev.manis.app.data.Workspace

private data class RuleEditor(val index: Int?, val entry: RuleEntry)

/** The GPUI routing-rules workspace, compact: the list drills into the rule editor. */
@Composable
fun RulesWorkspace(onMessage: (String) -> Unit) {
    val colors = LocalManisColors.current
    val context = LocalContext.current
    val config = Workspace.config.value
    val pending = Workspace.pending.value
    var editor by remember { mutableStateOf<RuleEditor?>(null) }

    fun save(entries: List<RuleEntry>) {
        Workspace.update { it.copy(ruleEntries = entries) }
    }

    fun moveRule(from: Int, to: Int) {
        val entries = config.ruleEntries.toMutableList()
        if (from !in entries.indices || to !in entries.indices) return
        val moved = entries.removeAt(from)
        entries.add(to, moved)
        save(entries)
    }

    Column(Modifier.fillMaxSize().background(colors.surfaceBase)) {
        val state = editor
        if (state != null) {
            RuleEditorPane(
                initial = state.entry,
                editingIndex = state.index,
                existing = config.ruleEntries,
                policies = policyOptions(config),
                onCancel = { editor = null },
                onDelete = {
                    if (state.index != null) {
                        save(config.ruleEntries.filterIndexed { at, _ -> at != state.index })
                        onMessage("规则已删除")
                    }
                    editor = null
                },
                onConfirm = { entry ->
                    val entries = config.ruleEntries.toMutableList()
                    if (state.index == null) entries += entry else entries[state.index] = entry
                    save(entries)
                    editor = null
                    onMessage("规则已保存，点「应用」生效")
                },
            )
        } else {
            Row(
                Modifier
                    .fillMaxWidth()
                    .padding(horizontal = ManisDimens.spaceMd, vertical = ManisDimens.spaceLg),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
            ) {
                Column(Modifier.weight(1f)) {
                    Text("分流规则", style = ManisType.pageTitle, color = colors.textPrimary)
                    val enabledRules = config.ruleEntries.count { it.enabled }
                    val disabledRules = config.ruleEntries.size - enabledRules
                    ManisMeta(
                        if (disabledRules == 0) {
                            "${config.ruleEntries.size} 条规则"
                        } else {
                            "$enabledRules 条生效 · $disabledRules 条已禁用"
                        },
                    )
                }
                if (pending) {
                    ManisButton("应用", primary = true, onClick = {
                        applyWorkspace(context)
                        onMessage("已应用工作区")
                    })
                }
                ManisButton("添加规则", onClick = { editor = RuleEditor(null, RuleEntry()) })
            }
            ManisDivider()
            Column(
                Modifier
                    .fillMaxSize()
                    .verticalScroll(rememberScrollState())
                    .padding(ManisDimens.spaceMd),
                verticalArrangement = Arrangement.spacedBy(ManisDimens.spaceMd),
            ) {
                if (config.ruleEntries.isEmpty()) {
                    ManisCard {
                        Text("还没有规则", style = ManisType.label, color = colors.textPrimary)
                        ManisMeta("添加一条 DOMAIN-SUFFIX 或 IP-CIDR 规则即可开始。")
                    }
                } else {
                    ManisCard {
                        config.ruleEntries.forEachIndexed { index, rule ->
                            RuleListRow(
                                order = index + 1,
                                rule = rule,
                                onToggle = { enabled ->
                                    save(
                                        config.ruleEntries.mapIndexed { at, item ->
                                            if (at == index) item.copy(enabled = enabled) else item
                                        },
                                    )
                                },
                                onEdit = { editor = RuleEditor(index, rule) },
                                onMoveUp = if (index > 0) {
                                    { moveRule(index, index - 1) }
                                } else {
                                    null
                                },
                                onMoveDown = if (index < config.ruleEntries.lastIndex) {
                                    { moveRule(index, index + 1) }
                                } else {
                                    null
                                },
                            )
                            if (index != config.ruleEntries.lastIndex) ManisDivider()
                        }
                    }
                }
                ManisMeta("规则从上到下匹配；策略可以是 DIRECT、REJECT 或任意策略组。最终规则之后的规则不会再生效。")
                if (RuntimeState.warnings.isNotEmpty()) {
                    ManisCard {
                        Text("解析提示", style = ManisType.label, color = colors.textPrimary)
                        RuntimeState.warnings.take(8).forEach { warning ->
                            ManisMeta(warning, color = colors.statusWarning)
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun RuleListRow(
    order: Int,
    rule: RuleEntry,
    onToggle: (Boolean) -> Unit,
    onEdit: () -> Unit,
    onMoveUp: (() -> Unit)?,
    onMoveDown: (() -> Unit)?,
) {
    val colors = LocalManisColors.current
    Row(
        Modifier
            .fillMaxWidth()
            .clickable(onClick = onEdit)
            .padding(horizontal = ManisDimens.spaceMd, vertical = ManisDimens.spaceSm),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
    ) {
        Text("$order", style = ManisType.metadata, color = colors.textTertiary, modifier = Modifier.width(20.dp))
        Column(Modifier.weight(1f)) {
            Text(
                if (rule.kind == "FINAL") "所有剩余流量" else rule.value,
                style = ManisType.body,
                color = if (rule.enabled) colors.textPrimary else colors.textTertiary,
            )
            ManisMeta("${rule.kind} → ${policyLabel(rule.policy)}")
        }
        ManisToggle(checked = rule.enabled, onCheckedChange = onToggle)
        onMoveUp?.let { move -> ManisTextAction("↑", move) }
        onMoveDown?.let { move -> ManisTextAction("↓", move) }
    }
}

@Composable
private fun RuleEditorPane(
    initial: RuleEntry,
    editingIndex: Int?,
    existing: List<RuleEntry>,
    policies: List<String>,
    onCancel: () -> Unit,
    onDelete: () -> Unit,
    onConfirm: (RuleEntry) -> Unit,
) {
    val colors = LocalManisColors.current
    var kind by remember { mutableStateOf(initial.kind) }
    var value by remember { mutableStateOf(initial.value) }
    var policy by remember { mutableStateOf(initial.policy) }
    var error by remember { mutableStateOf<String?>(null) }

    fun submit() {
        if (kind != "FINAL" && value.isBlank()) {
            error = "请填写匹配内容"
            return
        }
        val anotherFinal = existing.withIndex().any { (index, rule) ->
            index != editingIndex && rule.enabled && rule.kind == "FINAL"
        }
        if (kind == "FINAL" && anotherFinal) {
            error = "只能有一条启用的最终规则"
            return
        }
        onConfirm(
            RuleEntry(
                kind = kind,
                value = if (kind == "FINAL") "" else value.trim(),
                policy = policy,
                enabled = initial.enabled,
            ),
        )
    }

    Popup(onDismissRequest = onCancel, properties = PopupProperties(focusable = true)) {
    ManisDialog(
        title = if (editingIndex == null) "添加规则" else "编辑规则",
        subtitle = "规则按顺序匹配",
        onDismiss = onCancel,
        actions = {
            if (editingIndex != null) {
                ManisButton("删除", onClick = onDelete)
            }
            Spacer(Modifier.weight(1f))
            ManisButton("取消", onClick = onCancel)
            ManisButton("保存", primary = true, onClick = ::submit)
        },
    ) {
        Text("类型", style = ManisType.label, color = colors.textPrimary)
        ChipGrid(options = RULE_KINDS.toList(), selected = kind) {
            kind = it
            if (it == "FINAL") value = ""
        }
        if (kind != "FINAL") {
            Text("匹配", style = ManisType.label, color = colors.textPrimary)
            ManisTextField(value = value, onValueChange = { value = it }, placeholder = kindPlaceholder(kind))
        }
        Text("策略", style = ManisType.label, color = colors.textPrimary)
        ChipGrid(options = policies, selected = policy, display = ::policyLabel) { policy = it }
        ManisMeta(kindHelp(kind))
        error?.let { Text(it, style = ManisType.metadata, color = colors.statusError) }
    }
    }
}

@Composable
private fun ChipGrid(
    options: List<String>,
    selected: String,
    display: (String) -> String = { it },
    onSelect: (String) -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(ManisDimens.spaceXs)) {
        for (chunk in options.chunked(3)) {
            Row(horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceXs)) {
                for (option in chunk) {
                    ManisChip(display(option), option == selected) { onSelect(option) }
                }
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

private fun kindPlaceholder(kind: String): String = when (kind) {
    "DOMAIN", "DOMAIN-SUFFIX", "DOMAIN-KEYWORD", "DOMAIN-WILDCARD" -> "example.com"
    "IP-CIDR", "IP6-CIDR" -> "10.0.0.0/8"
    "GEOIP" -> "CN"
    "IP-ASN" -> "4134"
    else -> "匹配内容"
}

private fun kindHelp(kind: String): String = when (kind) {
    "DOMAIN" -> "精确匹配一个域名。"
    "DOMAIN-SUFFIX" -> "匹配域名及其所有子域名。"
    "DOMAIN-KEYWORD" -> "匹配域名中包含该关键词的流量。"
    "DOMAIN-WILDCARD" -> "用 * 和 ? 通配域名。"
    "IP-CIDR", "IP6-CIDR" -> "匹配目标地址所在网段。"
    "GEOIP" -> "按国家/地区代码匹配，例如 CN。"
    "IP-ASN" -> "按自治系统号匹配。"
    "FINAL" -> "兜底规则，所有未命中的流量都走这里的策略。"
    else -> ""
}
