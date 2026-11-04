package dev.manis.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material3.Icon
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupProperties
import dev.manis.app.R
import dev.manis.app.data.MobileConfig
import dev.manis.app.data.RULE_KINDS
import dev.manis.app.data.RuleEntry
import dev.manis.app.data.Workspace
import kotlin.math.roundToInt

private data class RuleEditor(val index: Int?, val entry: RuleEntry)

/** The row height, and the distance between row tops once the list spacing is added. */
private val RuleRowHeight = 56.dp
private val RuleRowStep = 64.dp

/**
 * The routing-rules workspace: one collapsible list of the manual rules. Rows reorder by dragging
 * the grip, which starts immediately instead of after a long press.
 */
@Composable
fun RulesWorkspace(onMessage: (String) -> Unit) {
    val colors = LocalManisColors.current
    val context = LocalContext.current
    val config = Workspace.config.value
    val pending = Workspace.pending.value
    var editor by remember { mutableStateOf<RuleEditor?>(null) }
    var expanded by rememberSaveable { mutableStateOf(true) }
    var draggingId by remember { mutableStateOf<Long?>(null) }
    var draggingIndex by remember { mutableStateOf(-1) }
    var dragOffset by remember { mutableFloatStateOf(0f) }
    val rowHeightPx = with(LocalDensity.current) { RuleRowStep.toPx() }

    fun save(entries: List<RuleEntry>) {
        Workspace.update { it.copy(ruleEntries = entries) }
    }

    fun moveRule(from: Int, to: Int) {
        // Read the live order: a drag can move the row several times before it recomposes.
        val entries = Workspace.config.value.ruleEntries.toMutableList()
        if (from !in entries.indices || to !in entries.indices) return
        val moved = entries.removeAt(from)
        entries.add(to, moved)
        save(entries)
    }

    Column(Modifier.fillMaxSize().background(colors.surfaceBase)) {
        LazyColumn(
            modifier = Modifier.fillMaxSize(),
            contentPadding = PaddingValues(ManisDimens.spaceLg),
            verticalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
        ) {
            item(key = "rules-header") {
                RuleGroupHeader(
                    count = config.ruleEntries.size,
                    expanded = expanded,
                    pending = pending,
                    onToggle = { expanded = !expanded },
                    onAdd = { editor = RuleEditor(null, RuleEntry()) },
                    onApply = {
                        applyWorkspace(context)
                        onMessage("已应用工作区")
                    },
                )
            }
            if (expanded) {
                if (config.ruleEntries.isEmpty()) {
                    item(key = "rules-empty") {
                        ManisCard {
                            Text("还没有规则", style = ManisType.label, color = colors.textPrimary)
                            ManisMeta("添加一条 DOMAIN-SUFFIX 或 IP-CIDR 规则即可开始。")
                        }
                    }
                }
                itemsIndexed(
                    items = config.ruleEntries,
                    key = { _, rule -> rule.id },
                ) { index, rule ->
                    val dragging = draggingId == rule.id
                    RuleRow(
                        order = index + 1,
                        rule = rule,
                        dragging = dragging,
                        offset = if (dragging) dragOffset else 0f,
                        modifier = if (draggingId == rule.id) Modifier else Modifier.animateItem(),
                        onToggle = { enabled ->
                            save(
                                config.ruleEntries.mapIndexed { at, item ->
                                    if (at == index) item.copy(enabled = enabled) else item
                                },
                            )
                        },
                        onEdit = { editor = RuleEditor(index, rule) },
                        onDragStart = {
                            draggingId = rule.id
                            draggingIndex = index
                            dragOffset = 0f
                        },
                        onDrag = { delta ->
                            val lastIndex = Workspace.config.value.ruleEntries.lastIndex
                            if (draggingIndex !in 0..lastIndex) return@RuleRow
                            dragOffset += delta
                            val target = (draggingIndex + (dragOffset / rowHeightPx).roundToInt())
                                .coerceIn(0, lastIndex)
                            if (target != draggingIndex) {
                                moveRule(draggingIndex, target)
                                dragOffset += (draggingIndex - target) * rowHeightPx
                                draggingIndex = target
                            }
                        },
                        onDragEnd = {
                            draggingId = null
                            draggingIndex = -1
                            dragOffset = 0f
                        },
                    )
                }
            }
            item(key = "rules-help") {
                ManisMeta("规则从上到下匹配；策略可以是 DIRECT、REJECT 或任意策略组。最终规则之后的规则不会再生效。")
            }
            if (RuntimeState.warnings.isNotEmpty()) {
                item(key = "rules-warnings") {
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
                if (state.index == null) {
                    entries += entry
                } else {
                    entries[state.index] = entry.copy(id = state.entry.id)
                }
                save(entries)
                editor = null
                onMessage("规则已保存，点「应用」生效")
            },
        )
    }
}

@Composable
private fun RuleGroupHeader(
    count: Int,
    expanded: Boolean,
    pending: Boolean,
    onToggle: () -> Unit,
    onAdd: () -> Unit,
    onApply: () -> Unit,
) {
    val colors = LocalManisColors.current
    Surface(
        modifier = Modifier.fillMaxWidth().clickable(onClick = onToggle),
        shape = RoundedCornerShape(16.dp),
        color = colors.surfaceOverlay,
    ) {
        Row(
            Modifier.padding(horizontal = ManisDimens.spaceLg, vertical = ManisDimens.spaceMd),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
        ) {
            Column(Modifier.weight(1f)) {
                Text("手动规则", style = ManisType.label, color = colors.textPrimary)
                ManisMeta("$count 条 · 从上到下匹配")
            }
            if (pending) ManisTextAction("应用", onApply)
            ManisTextAction("添加", onAdd)
            Icon(
                imageVector = Icons.Filled.KeyboardArrowDown,
                contentDescription = if (expanded) "收起" else "展开",
                modifier = Modifier.size(22.dp).rotate(if (expanded) 180f else 0f),
                tint = colors.textSecondary,
            )
        }
    }
}

@Composable
private fun RuleRow(
    order: Int,
    rule: RuleEntry,
    dragging: Boolean,
    offset: Float,
    modifier: Modifier = Modifier,
    onToggle: (Boolean) -> Unit,
    onEdit: () -> Unit,
    onDragStart: () -> Unit,
    onDrag: (Float) -> Unit,
    onDragEnd: () -> Unit,
) {
    val colors = LocalManisColors.current
    val dragStart by androidx.compose.runtime.rememberUpdatedState(onDragStart)
    val drag by androidx.compose.runtime.rememberUpdatedState(onDrag)
    val dragEnd by androidx.compose.runtime.rememberUpdatedState(onDragEnd)
    Surface(
        modifier = modifier
            .fillMaxWidth()
            .height(RuleRowHeight)
            .zIndex(if (dragging) 1f else 0f)
            .graphicsLayer { translationY = offset }
            .clickable(onClick = onEdit),
        shape = RoundedCornerShape(16.dp),
        color = if (dragging) colors.actionSoft else colors.surfaceOverlay,
        shadowElevation = if (dragging) 6.dp else 0.dp,
    ) {
        Row(
            Modifier
                .fillMaxSize()
                .padding(start = ManisDimens.spaceLg, end = ManisDimens.spaceSm)
                .pointerInput(rule.id) {
                    detectDragGestures(
                        onDragStart = { dragStart() },
                        onDragEnd = { dragEnd() },
                        onDragCancel = { dragEnd() },
                        onDrag = { change, amount ->
                            change.consume()
                            drag(amount.y)
                        },
                    )
                },
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceMd),
        ) {
            Text(
                "$order",
                style = ManisType.metadata,
                color = colors.textTertiary,
                modifier = Modifier.width(20.dp),
            )
            Column(Modifier.weight(1f)) {
                Text(
                    if (rule.kind == "FINAL") "所有剩余流量" else rule.value,
                    style = ManisType.body,
                    color = if (rule.enabled) colors.textPrimary else colors.textTertiary,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                ManisMeta("${rule.kind} → ${policyLabel(rule.policy)}")
            }
            ManisToggle(checked = rule.enabled, onCheckedChange = onToggle)
            Icon(
                painter = painterResource(R.drawable.ic_drag_handle),
                contentDescription = "拖动排序",
                modifier = Modifier.size(24.dp),
                tint = colors.textTertiary,
            )
        }
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
        ManisSelect(
            label = "类型",
            options = RULE_KINDS.toList(),
            selected = kind,
            onSelect = {
                kind = it
                if (it == "FINAL") value = ""
            },
        )
        if (kind != "FINAL") {
            Text("匹配", style = ManisType.label, color = colors.textPrimary)
            ManisTextField(value = value, onValueChange = { value = it }, placeholder = kindPlaceholder(kind))
        }
        ManisSelect(
            label = "策略",
            options = policies,
            selected = policy,
            display = ::policyLabel,
            onSelect = { policy = it },
        )
        ManisMeta(kindHelp(kind))
        error?.let { Text(it, style = ManisType.metadata, color = colors.statusError) }
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
