package dev.manis.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material3.Icon
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupProperties
import dev.manis.app.R
import dev.manis.app.data.MobileConfig
import dev.manis.app.data.RULE_KINDS
import dev.manis.app.data.RuleEntry
import dev.manis.app.data.RuleGroupEntry
import dev.manis.app.data.Workspace
import dev.manis.app.data.nextRuleGroupId
import kotlin.math.roundToInt

/** An open rule editor: a new rule for [entry]'s group, or the rule at [index] in the flat list. */
internal data class RuleEditor(val index: Int?, val entry: RuleEntry)

/** An open group editor. [saved] separates a group already in the workspace from a new draft. */
private data class GroupEditor(val group: RuleGroupEntry, val saved: Boolean)

/** The manual section is one more collapsible section, under a key no generated group id can take. */
private const val ManualSectionKey = "manual"
private const val ManualSectionTitle = "手动规则"

private const val RulesHelp = "从上到下匹配：先规则组，后手动规则。"

/** The distance between one rule row's top and the next, once the list spacing is added. */
private val RuleRowStep = ManisDimens.rowHeight + ManisDimens.spaceSm

/**
 * Where a title, a rule's text and a section's placeholder all start, measured from the list's
 * content edge: the header's own padding, the leading slot every section row shares, and the gap
 * after it. One constant, so the whole page keeps one left edge for text.
 */
private val RulesTextColumnStart =
    ManisDimens.spaceLg + ManisDimens.iconTarget + ManisDimens.spaceSm

/**
 * The routing-rules workspace: rule groups that each route to one policy, then the manual rules,
 * matched in that order. Rows reorder by dragging the grip, which starts immediately instead of
 * after a long press, and only inside their own section. A group header's grip reorders the groups
 * themselves, which always stay above the manual section.
 */
@Composable
fun RulesWorkspace(onMessage: (String) -> Unit) {
    val colors = LocalManisColors.current
    val context = LocalContext.current
    val config = Workspace.config.value
    val pending = Workspace.pending.value
    var ruleEditor by remember { mutableStateOf<RuleEditor?>(null) }
    var groupEditor by remember { mutableStateOf<GroupEditor?>(null) }
    // Collapsed sections by id: survives rotation, and an unknown id stays expanded.
    var collapsed by rememberSaveable { mutableStateOf(emptyList<String>()) }
    var draggingId by remember { mutableStateOf<Long?>(null) }
    var draggingIndex by remember { mutableStateOf(-1) }
    var dragOffset by remember { mutableFloatStateOf(0f) }
    // A dragged group header floats alone; the list keeps its order until the drop.
    var draggingGroupId by remember { mutableStateOf<String?>(null) }
    var groupDragOffset by remember { mutableFloatStateOf(0f) }
    var groupDropIndex by remember { mutableStateOf<Int?>(null) }
    var groupDropLineY by remember { mutableStateOf<Float?>(null) }
    val rowHeightPx = with(LocalDensity.current) { RuleRowStep.toPx() }
    val pillHeightPx = with(LocalDensity.current) { ManisDimens.rowHeight.toPx() }
    val listState = rememberLazyListState()

    fun saveEntries(entries: List<RuleEntry>) {
        Workspace.update { it.copy(ruleEntries = entries) }
    }

    /** The rules of one section, live: a null [groupId] is the manual section. */
    fun sectionRules(groupId: String?): List<RuleEntry> =
        Workspace.config.value.ruleEntries.filter { it.groupId == groupId }

    fun toggleRule(rule: RuleEntry, enabled: Boolean) {
        saveEntries(
            Workspace.config.value.ruleEntries.map {
                if (it.id == rule.id) it.copy(enabled = enabled) else it
            },
        )
    }

    fun moveRule(groupId: String?, from: Int, to: Int) {
        val section = sectionRules(groupId).toMutableList()
        if (from !in section.indices || to !in section.indices) return
        section.add(to, section.removeAt(from))
        // Refill this section's slots in the flat list; the other sections keep their positions.
        val ordered = section.iterator()
        saveEntries(
            Workspace.config.value.ruleEntries.map {
                if (it.groupId == groupId) ordered.next() else it
            },
        )
    }

    fun dragRule(groupId: String?, delta: Float) {
        val lastIndex = sectionRules(groupId).lastIndex
        if (draggingIndex !in 0..lastIndex) return
        dragOffset += delta
        val target = (draggingIndex + (dragOffset / rowHeightPx).roundToInt()).coerceIn(0, lastIndex)
        if (target != draggingIndex) {
            moveRule(groupId, draggingIndex, target)
            dragOffset += (draggingIndex - target) * rowHeightPx
            draggingIndex = target
        }
    }

    /** A group header as currently laid out, or null when that group is scrolled out of view. */
    fun groupHeader(groupId: String) =
        listState.layoutInfo.visibleItemsInfo.firstOrNull { it.key == "group-$groupId" }

    /**
     * The slot and the line the dragged header would land in. The header follows the finger, so its
     * centre is where it was laid out plus the drag offset; it goes before the first other header
     * whose centre is still below that, and last when none is. Headers off screen keep their order,
     * which is what the finger is passing anyway.
     */
    fun trackGroupDrop() {
        val id = draggingGroupId ?: return
        val header = groupHeader(id) ?: return
        val others = Workspace.config.value.ruleGroups.filterNot { it.id == id }
        val centre = header.offset + header.size / 2f + groupDragOffset
        var slot = others.size
        var line: Float? = null
        for ((index, group) in others.withIndex()) {
            val info = groupHeader(group.id) ?: continue
            if (centre < info.offset + info.size / 2f) {
                slot = index
                line = info.offset.toFloat()
                break
            }
            slot = index + 1
            line = (info.offset + info.size).toFloat()
        }
        // Item offsets are measured from the content, which starts below the top content padding.
        val padding = listState.layoutInfo.beforeContentPadding
        groupDropIndex = slot
        groupDropLineY = line?.let { it + padding }
    }

    /** Drops the drag without touching the order, which nothing has changed yet. */
    fun cancelGroupDrag() {
        draggingGroupId = null
        groupDragOffset = 0f
        groupDropIndex = null
        groupDropLineY = null
    }

    /** One move, and only when the header actually left its slot. */
    fun dropGroup() {
        val id = draggingGroupId
        val slot = groupDropIndex
        cancelGroupDrag()
        if (id == null || slot == null) return
        Workspace.update { current ->
            val from = current.ruleGroups.indexOfFirst { it.id == id }
            if (from < 0 || from == slot) return@update current
            val groups = current.ruleGroups.toMutableList()
            groups.add(slot, groups.removeAt(from))
            current.copy(ruleGroups = groups)
        }
    }

    fun editRule(rule: RuleEntry) {
        val index = Workspace.config.value.ruleEntries.indexOfFirst { it.id == rule.id }
        ruleEditor = RuleEditor(index.takeIf { it >= 0 }, rule)
    }

    Box(Modifier.fillMaxSize().background(colors.surfaceBase)) {
        LazyColumn(
            state = listState,
            modifier = Modifier.fillMaxSize(),
            contentPadding = PaddingValues(ManisDimens.spaceLg),
            verticalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
        ) {
            item(key = "rules-header") {
                RulesHeader(
                    groupCount = config.ruleGroups.size,
                    ruleCount = config.ruleEntries.size,
                    pending = pending,
                    onApply = {
                        applyWorkspace(context)
                        onMessage("已应用工作区")
                    },
                    onCreateGroup = {
                        groupEditor = GroupEditor(
                            group = RuleGroupEntry(
                                id = nextRuleGroupId(),
                                name = "",
                                // A new group routes to the first policy group when there is one.
                                policy = config.groups.firstOrNull()?.name ?: "__MANIS_GLOBAL__",
                            ),
                            saved = false,
                        )
                    },
                )
            }
            for (group in config.ruleGroups) {
                val rules = config.ruleEntries.filter { it.groupId == group.id }
                val expanded = group.id !in collapsed
                item(key = "group-${group.id}") {
                    RuleGroupHeader(
                        groupId = group.id,
                        name = group.name,
                        // The count only earns its room when the rows are hidden.
                        meta = if (expanded) {
                            "→ ${policyLabel(group.policy)}"
                        } else {
                            "→ ${policyLabel(group.policy)} · ${rules.size} 条"
                        },
                        enabled = group.enabled,
                        expanded = expanded,
                        dragging = draggingGroupId == group.id,
                        // After a drop the section settles into its new slot instead of jumping.
                        modifier = if (draggingGroupId == null) Modifier.animateItem() else Modifier,
                        onOpenEditor = { groupEditor = GroupEditor(group, saved = true) },
                        onToggleExpanded = {
                            collapsed = if (expanded) collapsed + group.id else collapsed - group.id
                        },
                        onToggleEnabled = { enabled ->
                            Workspace.update { current ->
                                current.copy(
                                    ruleGroups = current.ruleGroups.map {
                                        if (it.id == group.id) it.copy(enabled = enabled) else it
                                    },
                                )
                            }
                        },
                        onDragStart = {
                            draggingGroupId = group.id
                            groupDragOffset = 0f
                            groupDropIndex = null
                            groupDropLineY = null
                        },
                        onDrag = { delta ->
                            groupDragOffset += delta
                            trackGroupDrop()
                        },
                        onDragEnd = ::dropGroup,
                        onDragCancel = ::cancelGroupDrag,
                    )
                }
                if (expanded) {
                    if (rules.isEmpty()) {
                        item(key = "group-empty-${group.id}") {
                            SectionNote("这个组还没有规则。")
                        }
                    } else {
                        ruleRows(
                            rules = rules,
                            // The group header states the target once, so rows only name the kind.
                            target = group.policy,
                            draggingId = draggingId,
                            dragOffset = dragOffset,
                            onToggle = ::toggleRule,
                            onEdit = ::editRule,
                            onDragStart = { index ->
                                draggingId = rules[index].id
                                draggingIndex = index
                                dragOffset = 0f
                            },
                            onDrag = { delta -> dragRule(group.id, delta) },
                            onDragEnd = {
                                draggingId = null
                                draggingIndex = -1
                                dragOffset = 0f
                            },
                        )
                    }
                    item(key = "group-add-${group.id}") {
                        SectionAddRow(
                            onClick = {
                                ruleEditor = RuleEditor(null, RuleEntry(groupId = group.id))
                            },
                        )
                    }
                }
            }
            val manualRules = config.ruleEntries.filter { it.groupId == null }
            val manualExpanded = ManualSectionKey !in collapsed
            val pageEmpty = config.ruleGroups.isEmpty() && config.ruleEntries.isEmpty()
            item(key = "rules-manual-header") {
                ManualRulesHeader(
                    count = manualRules.size,
                    expanded = manualExpanded,
                    onToggleExpanded = {
                        collapsed = if (manualExpanded) collapsed + ManualSectionKey else collapsed - ManualSectionKey
                    },
                )
            }
            if (manualExpanded) {
                if (manualRules.isEmpty()) {
                    if (pageEmpty) {
                        item(key = "rules-empty") {
                            EmptyRules(
                                onAdd = { ruleEditor = RuleEditor(null, RuleEntry()) },
                            )
                        }
                    } else {
                        item(key = "rules-manual-empty") { SectionNote("还没有手动规则。") }
                    }
                } else {
                    ruleRows(
                        rules = manualRules,
                        target = null,
                        draggingId = draggingId,
                        dragOffset = dragOffset,
                        onToggle = ::toggleRule,
                        onEdit = ::editRule,
                        onDragStart = { index ->
                            draggingId = manualRules[index].id
                            draggingIndex = index
                            dragOffset = 0f
                        },
                        onDrag = { delta -> dragRule(null, delta) },
                        onDragEnd = {
                            draggingId = null
                            draggingIndex = -1
                            dragOffset = 0f
                        },
                    )
                }
                // The empty page already offers one filled call to action, so it gets no second.
                if (!pageEmpty) {
                    item(key = "rules-manual-add") {
                        SectionAddRow(onClick = { ruleEditor = RuleEditor(null, RuleEntry()) })
                    }
                }
            }
            item(key = "rules-help") {
                ManisMeta(RulesHelp, maxLines = 2)
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

        // Where the dragged group header will land. The padding shift is what turns an item offset
        // into a position in this Box, since the list measures items from its content, not its edge.
        val dropLineY = groupDropLineY
        if (dropLineY != null) {
            Spacer(
                Modifier
                    .fillMaxWidth()
                    .height(2.dp)
                    .offset { IntOffset(0, dropLineY.roundToInt()) }
                    .background(colors.actionPrimary),
            )
        }
        // The header itself stays in place, so the list keeps showing what is being passed; a pill
        // under the finger says which group is moving, even when the finger reaches the footer.
        val draggedGroup = config.ruleGroups.firstOrNull { it.id == draggingGroupId }
        val draggedHeader = draggingGroupId?.let(::groupHeader)
        if (draggedGroup != null && draggedHeader != null) {
            val padding = listState.layoutInfo.beforeContentPadding
            // The pill follows the finger, but stays inside the list: an unclamped one would leave
            // the page bounds whenever the finger reaches the footer.
            val maxY = (listState.layoutInfo.viewportEndOffset + padding - pillHeightPx).coerceAtLeast(0f)
            val pillY = (draggedHeader.offset + padding + groupDragOffset).coerceIn(0f, maxY)
            Surface(
                modifier = Modifier
                    .padding(horizontal = ManisDimens.spaceLg)
                    .fillMaxWidth()
                    .offset { IntOffset(0, pillY.roundToInt()) },
                shape = RoundedCornerShape(16.dp),
                color = colors.actionSoft,
                shadowElevation = 8.dp,
            ) {
                Row(
                    Modifier
                        .height(ManisDimens.rowHeight)
                        .padding(horizontal = ManisDimens.spaceLg),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
                ) {
                    // A lifted header keeps the grip slot the real one reserves, so the name under
                    // the finger sits where the name in the list sits.
                    Spacer(Modifier.width(ManisDimens.iconTarget))
                    Column(Modifier.weight(1f)) {
                        Text(
                            draggedGroup.name,
                            style = ManisType.label,
                            color = colors.textPrimary,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                        ManisMeta("→ ${policyLabel(draggedGroup.policy)}", maxLines = 1)
                    }
                }
            }
        }
    }

    val state = ruleEditor
    if (state != null) {
        RuleEditorPane(
            initial = state.entry,
            editingIndex = state.index,
            existing = config.ruleEntries,
            policies = policyOptions(config),
            groups = config.ruleGroups,
            onCancel = { ruleEditor = null },
            onDelete = {
                val existingRules = Workspace.config.value.ruleEntries
                saveEntries(existingRules.filterNot { it.id == state.entry.id })
                ruleEditor = null
                onMessage("规则已删除")
            },
            onConfirm = { entry ->
                val existingRules = Workspace.config.value.ruleEntries
                saveEntries(
                    if (state.index == null) {
                        existingRules + entry
                    } else {
                        existingRules.map { if (it.id == state.entry.id) entry.copy(id = it.id) else it }
                    },
                )
                ruleEditor = null
                onMessage("规则已保存，点「应用」生效")
            },
        )
    }

    val openGroup = groupEditor
    if (openGroup != null) {
        RuleGroupEditorPane(
            initial = openGroup.group,
            saved = openGroup.saved,
            siblings = config.ruleGroups,
            ruleCount = config.ruleEntries.count { it.groupId == openGroup.group.id },
            policies = policyOptions(config),
            onCancel = { groupEditor = null },
            onDelete = {
                Workspace.update { current ->
                    current.copy(
                        ruleGroups = current.ruleGroups.filterNot { it.id == openGroup.group.id },
                        ruleEntries = current.ruleEntries.filterNot { it.groupId == openGroup.group.id },
                    )
                }
                groupEditor = null
                onMessage("规则组已删除")
            },
            onConfirm = { group ->
                Workspace.update { current ->
                    current.copy(
                        ruleGroups = if (current.ruleGroups.any { it.id == group.id }) {
                            current.ruleGroups.map { if (it.id == group.id) group else it }
                        } else {
                            current.ruleGroups + group
                        },
                    )
                }
                groupEditor = null
                onMessage("规则组已保存，点「应用」生效")
            },
        )
    }
}

/** The rows of one section, numbered within it, with drag targets clamped to it. */
private fun LazyListScope.ruleRows(
    rules: List<RuleEntry>,
    /** The group's policy, or null in the manual section where rows state their own. */
    target: String?,
    draggingId: Long?,
    dragOffset: Float,
    onToggle: (RuleEntry, Boolean) -> Unit,
    onEdit: (RuleEntry) -> Unit,
    onDragStart: (Int) -> Unit,
    onDrag: (Float) -> Unit,
    onDragEnd: () -> Unit,
) {
    itemsIndexed(items = rules, key = { _, rule -> rule.id }) { index, rule ->
        val dragging = draggingId == rule.id
        RuleRow(
            order = index + 1,
            rule = rule,
            groupTarget = target,
            dragging = dragging,
            offset = if (dragging) dragOffset else 0f,
            modifier = if (dragging) Modifier else Modifier.animateItem(),
            onToggle = { enabled -> onToggle(rule, enabled) },
            onEdit = { onEdit(rule) },
            onDragStart = { onDragStart(index) },
            onDrag = onDrag,
            onDragEnd = onDragEnd,
        )
    }
}

/**
 * The one section header, and the grid every rule row under it repeats: a 44dp leading slot, the
 * text column, the 52x32 switch and one 44dp trailing control. The leading slot is reserved even
 * where it holds nothing and the height is fixed, so a header cannot move a title when a control
 * appears, and the switch lands on the same x in a header as in the rows below it.
 */
@Composable
private fun RulesSectionHeader(
    title: String,
    meta: String,
    modifier: Modifier = Modifier,
    titleColor: Color? = null,
    /** A group's grip reorders the groups; the page and manual headers leave the slot empty. */
    leading: (@Composable BoxScope.() -> Unit)? = null,
    onClick: (() -> Unit)? = null,
    /** A dragged bar lifts off the list, the same way a dragged rule row does. */
    dragging: Boolean = false,
    /** The section's switch, or null where it has none: the slot is kept either way. */
    switch: (@Composable () -> Unit)? = null,
    /** The page header has no chevron, so it spends the switch slot on its two buttons instead. */
    reserveSwitchSlot: Boolean = true,
    trailing: @Composable RowScope.() -> Unit = {},
) {
    val colors = LocalManisColors.current
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    Surface(
        modifier = modifier
            .fillMaxWidth()
            .background(
                if (pressed && onClick != null) colors.surfaceLow else colors.surfaceOverlay,
                RoundedCornerShape(ManisDimens.radiusPane),
            )
            .then(
                if (onClick != null) {
                    Modifier.clickable(interactionSource = interaction, indication = null, onClick = onClick)
                } else {
                    Modifier
                },
            ),
        shape = RoundedCornerShape(ManisDimens.radiusPane),
        color = if (dragging) colors.actionSoft else Color.Transparent,
        shadowElevation = if (dragging) 6.dp else 0.dp,
    ) {
        Row(
            Modifier
                .fillMaxWidth()
                // One height with the children centred. The title and meta are 20dp and 16dp of
                // type, so vertical padding here would only choose which of the two gets clipped.
                .height(ManisDimens.rowHeight)
                .padding(horizontal = ManisDimens.spaceLg),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
        ) {
            Box(Modifier.size(ManisDimens.iconTarget), contentAlignment = Alignment.Center) {
                leading?.invoke(this)
            }
            Column(Modifier.weight(1f)) {
                Text(
                    title,
                    style = ManisType.label,
                    color = titleColor ?: colors.textPrimary,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    meta,
                    style = ManisType.metadata,
                    color = colors.textTertiary,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            if (reserveSwitchSlot) {
                // 52dp wide, 48dp tall: the track is 32dp, the rest is Material's touch target.
                Box(
                    Modifier
                        .width(ManisDimens.switchWidth)
                        .height(ManisDimens.controlStandard),
                    contentAlignment = Alignment.Center,
                ) {
                    switch?.invoke()
                }
            }
            trailing()
        }
    }
}

@Composable
private fun RulesHeader(
    groupCount: Int,
    ruleCount: Int,
    pending: Boolean,
    onApply: () -> Unit,
    onCreateGroup: () -> Unit,
) {
    RulesSectionHeader(
        title = "分流规则",
        // Short enough to survive a 320 dp page header, where the title column is narrow.
        meta = "$groupCount 组 · $ruleCount 条",
        reserveSwitchSlot = false,
        trailing = {
            // Both actions are always here. 应用 only dims when nothing is pending, so the header
            // cannot re-flow under the user in the middle of an edit.
            CompositionLocalProvider(LocalManisTightActions provides true) {
                ManisButton(
                    "新建组",
                    modifier = Modifier.widthIn(min = 64.dp),
                    onClick = onCreateGroup,
                )
                ManisButton("应用", primary = true, enabled = pending, onClick = onApply)
            }
        },
    )
}

@Composable
private fun RuleGroupHeader(
    groupId: String,
    name: String,
    meta: String,
    enabled: Boolean,
    expanded: Boolean,
    dragging: Boolean,
    modifier: Modifier = Modifier,
    onOpenEditor: () -> Unit,
    onToggleExpanded: () -> Unit,
    onToggleEnabled: (Boolean) -> Unit,
    onDragStart: () -> Unit,
    onDrag: (Float) -> Unit,
    onDragEnd: () -> Unit,
    onDragCancel: () -> Unit,
) {
    val colors = LocalManisColors.current
    val dragStart by androidx.compose.runtime.rememberUpdatedState(onDragStart)
    val drag by androidx.compose.runtime.rememberUpdatedState(onDrag)
    val dragEnd by androidx.compose.runtime.rememberUpdatedState(onDragEnd)
    val dragCancel by androidx.compose.runtime.rememberUpdatedState(onDragCancel)
    RulesSectionHeader(
        title = name,
        meta = meta,
        modifier = modifier,
        titleColor = if (enabled) colors.textPrimary else colors.textTertiary,
        onClick = onOpenEditor,
        dragging = dragging,
        leading = {
            // The grip drags the whole group. It swallows the touch down as well, so neither a tap
            // nor a drag on it can reach the bar's click and open the editor.
            Box(
                modifier = Modifier
                    .fillMaxSize()
                    .pointerInput(groupId) {
                        detectDragGestures(
                            onDragStart = { dragStart() },
                            onDragEnd = { dragEnd() },
                            onDragCancel = { dragCancel() },
                            onDrag = { change, amount ->
                                change.consume()
                                drag(amount.y)
                            },
                        )
                    }
                    .pointerInput(groupId) { detectTapGestures { } },
                contentAlignment = Alignment.Center,
            ) {
                Icon(
                    painter = painterResource(R.drawable.ic_drag_handle),
                    contentDescription = "拖动排序",
                    modifier = Modifier.size(24.dp),
                    tint = colors.textTertiary,
                )
            }
        },
        switch = { ManisToggle(checked = enabled, onCheckedChange = onToggleEnabled) },
        trailing = {
            ManisIconButton(
                icon = Icons.Filled.KeyboardArrowDown,
                contentDescription = if (expanded) "收起" else "展开",
                onClick = onToggleExpanded,
                rotationDegrees = if (expanded) 180f else 0f,
            )
        },
    )
}

@Composable
private fun ManualRulesHeader(
    count: Int,
    expanded: Boolean,
    onToggleExpanded: () -> Unit,
) {
    RulesSectionHeader(
        title = ManualSectionTitle,
        meta = "$count 条 · 最后匹配",
        onClick = onToggleExpanded,
        // No section-level switch, so the slot stays empty and the chevron keeps the group x.
        switch = null,
        trailing = {
            ManisIconButton(
                icon = Icons.Filled.KeyboardArrowDown,
                contentDescription = if (expanded) "收起" else "展开",
                onClick = onToggleExpanded,
                rotationDegrees = if (expanded) 180f else 0f,
            )
        },
    )
}

/** A section's placeholder, indented to the text column so it reads as part of that section. */
@Composable
private fun SectionNote(text: String) {
    Text(
        text,
        style = ManisType.metadata,
        color = LocalManisColors.current.textTertiary,
        maxLines = 2,
        overflow = TextOverflow.Ellipsis,
        modifier = Modifier
            .fillMaxWidth()
            .padding(start = RulesTextColumnStart),
    )
}

/**
 * The one way to add a rule to a section. It sits under the section's last row instead of in its
 * header, so every header keeps two trailing slots and one fixed height.
 */
@Composable
private fun SectionAddRow(onClick: () -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .height(ManisDimens.controlCompact)
            .clip(RoundedCornerShape(ManisDimens.radiusControl))
            .clickable(onClick = onClick)
            .padding(start = RulesTextColumnStart),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            "＋ 添加规则",
            style = ManisType.metadata,
            color = LocalManisColors.current.textSecondary,
            maxLines = 1,
        )
    }
}

/** The page with nothing in it: one sentence and one filled way to start. */
@Composable
private fun EmptyRules(onAdd: () -> Unit) {
    ManisCard {
        ManisMeta("还没有规则。添加一条 DOMAIN-SUFFIX 或 IP-CIDR 规则即可开始。")
        ManisButton("添加规则", primary = true, onClick = onAdd)
    }
}

@Composable
private fun RuleRow(
    order: Int,
    rule: RuleEntry,
    /** The group's policy, or null in the manual section where the rule's own policy applies. */
    groupTarget: String?,
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
        onClick = onEdit,
        modifier = modifier
            .fillMaxWidth()
            .height(ManisDimens.rowHeight)
            .zIndex(if (dragging) 1f else 0f)
            .graphicsLayer { translationY = offset },
        shape = RoundedCornerShape(16.dp),
        color = if (dragging) colors.actionSoft else colors.surfaceOverlay,
        shadowElevation = if (dragging) 6.dp else 0.dp,
    ) {
        Row(
            Modifier
                .fillMaxSize()
                // The same insets as a section header, so the switch and the grip below a header
                // share its columns instead of drifting a few dp to the right.
                .padding(horizontal = ManisDimens.spaceLg)
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
            horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
        ) {
            // The header's grip slot, holding the order instead: one left edge for the whole page.
            Box(Modifier.size(ManisDimens.iconTarget), contentAlignment = Alignment.Center) {
                Text("$order", style = ManisType.metadata, color = colors.textTertiary)
            }
            Column(Modifier.weight(1f)) {
                Text(
                    if (rule.kind == "FINAL") "所有剩余流量" else rule.value,
                    style = ManisType.body,
                    color = if (rule.enabled) colors.textPrimary else colors.textTertiary,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                ManisMeta(
                    if (groupTarget == null) {
                        "${rule.kind} → ${policyLabel(rule.policy)}"
                    } else {
                        rule.kind
                    },
                    maxLines = 1,
                )
            }
            Box(
                Modifier
                    .width(ManisDimens.switchWidth)
                    .height(ManisDimens.controlStandard),
                contentAlignment = Alignment.Center,
            ) {
                ManisToggle(checked = rule.enabled, onCheckedChange = onToggle)
            }
            // The drag gesture covers the whole row; this slot is the grip that says so.
            Box(Modifier.size(ManisDimens.iconTarget), contentAlignment = Alignment.Center) {
                Icon(
                    painter = painterResource(R.drawable.ic_drag_handle),
                    contentDescription = "拖动排序",
                    modifier = Modifier.size(24.dp),
                    tint = colors.textTertiary,
                )
            }
        }
    }
}

@Composable
internal fun RuleEditorPane(
    initial: RuleEntry,
    editingIndex: Int?,
    existing: List<RuleEntry>,
    policies: List<String>,
    groups: List<RuleGroupEntry> = emptyList(),
    onCancel: () -> Unit,
    onDelete: () -> Unit,
    onConfirm: (RuleEntry) -> Unit,
) {
    val colors = LocalManisColors.current
    // The picker lists the manual group plus every saved group, and maps names back to ids.
    val groupChoices = listOf<Pair<String?, String>>(null to ManualSectionTitle) +
        groups.map { it.id to it.name }
    var kind by remember { mutableStateOf(initial.kind) }
    var value by remember { mutableStateOf(initial.value) }
    // Untouched while the rule sits in a group, so moving it back to manual restores its policy.
    var policy by remember { mutableStateOf(initial.policy) }
    var groupId by remember { mutableStateOf(initial.groupId) }
    var error by remember { mutableStateOf<String?>(null) }
    val group = groups.firstOrNull { it.id == groupId }

    fun submit() {
        if (kind != "FINAL" && value.isBlank()) {
            error = "请填写匹配内容"
            return
        }
        val anotherFinal = existing.withIndex().any { (index, rule) ->
            index != editingIndex && rule.enabled && rule.kind == "FINAL" && rule.groupId == null
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
                groupId = groupId,
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
                    ManisTextAction("删除", tone = colors.statusError, onClick = onDelete)
                }
                Spacer(Modifier.weight(1f))
                ManisButton("取消", onClick = onCancel)
                ManisButton("保存", primary = true, onClick = ::submit)
            },
        ) {
            if (groups.isNotEmpty()) {
                ManisSelect(
                    label = "规则组",
                    options = groupChoices.map { it.second },
                    selected = groupChoices.firstOrNull { it.first == groupId }?.second ?: ManualSectionTitle,
                    onSelect = { label ->
                        groupId = groupChoices.firstOrNull { it.second == label }?.first
                        // FINAL is manual-only, so moving such a rule into a group needs a real kind.
                        if (groupId != null && kind == "FINAL") kind = "DOMAIN-SUFFIX"
                    },
                )
            }
            ManisSelect(
                label = "类型",
                options = if (groupId == null) RULE_KINDS.toList() else RULE_KINDS - "FINAL",
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
            if (group != null) {
                ManisMeta("由规则组「${group.name}」决定：${policyLabel(group.policy)}")
            } else {
                ManisSelect(
                    label = "策略",
                    options = policies,
                    selected = policy,
                    display = ::policyLabel,
                    onSelect = { policy = it },
                )
            }
            ManisMeta(kindHelp(kind))
            error?.let { Text(it, style = ManisType.metadata, color = colors.statusError) }
        }
    }
}

@Composable
private fun RuleGroupEditorPane(
    initial: RuleGroupEntry,
    saved: Boolean,
    siblings: List<RuleGroupEntry>,
    ruleCount: Int,
    policies: List<String>,
    onCancel: () -> Unit,
    onDelete: () -> Unit,
    onConfirm: (RuleGroupEntry) -> Unit,
) {
    val colors = LocalManisColors.current
    var name by remember { mutableStateOf(initial.name) }
    var policy by remember { mutableStateOf(initial.policy) }
    var enabled by remember { mutableStateOf(initial.enabled) }
    var error by remember { mutableStateOf<String?>(null) }
    var confirmingDelete by remember { mutableStateOf(false) }

    fun submit() {
        val trimmed = name.trim()
        if (trimmed.isEmpty()) {
            error = "请填写规则组名称"
            return
        }
        val taken = trimmed == ManualSectionTitle ||
            siblings.any { it.id != initial.id && it.name == trimmed }
        if (taken) {
            error = "已有同名规则组"
            return
        }
        onConfirm(initial.copy(name = trimmed, policy = policy, enabled = enabled))
    }

    Popup(onDismissRequest = onCancel, properties = PopupProperties(focusable = true)) {
        ManisDialog(
            title = if (saved) "编辑规则组" else "新建规则组",
            subtitle = if (saved) null else "组内规则统一走同一个目标",
            onDismiss = onCancel,
            actions = {
                if (saved && confirmingDelete) {
                    ManisButton("取消删除", onClick = { confirmingDelete = false })
                    Spacer(Modifier.weight(1f))
                    ManisButton("确认删除", primary = true, onClick = onDelete)
                } else {
                    if (saved) {
                        ManisTextAction(
                            "删除",
                            tone = colors.statusError,
                            onClick = { confirmingDelete = true },
                        )
                    }
                    Spacer(Modifier.weight(1f))
                    ManisButton("取消", onClick = onCancel)
                    ManisButton("保存", primary = true, onClick = ::submit)
                }
            },
        ) {
            if (confirmingDelete) {
                Text(
                    "删除规则组「${initial.name}」后，组内 $ruleCount 条规则也会一起删除。",
                    style = ManisType.body,
                    color = colors.statusError,
                )
            } else {
                Text("名称", style = ManisType.label, color = colors.textPrimary)
                ManisTextField(value = name, onValueChange = { name = it }, placeholder = "例如：工作")
                ManisSelect(
                    label = "目标",
                    options = policies,
                    selected = policy,
                    display = ::policyLabel,
                    onSelect = { policy = it },
                )
                Row(
                    Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
                ) {
                    Text(
                        "启用",
                        style = ManisType.label,
                        color = colors.textPrimary,
                        modifier = Modifier.weight(1f),
                    )
                    ManisToggle(checked = enabled, onCheckedChange = { enabled = it })
                }
                ManisMeta("组内每条规则都走「${policyLabel(policy)}」，规则自己的策略不参与匹配。")
                error?.let { Text(it, style = ManisType.metadata, color = colors.statusError) }
            }
        }
    }
}

internal fun policyOptions(config: MobileConfig): List<String> {
    val options = mutableListOf("DIRECT", "REJECT", "__MANIS_GLOBAL__")
    for (group in config.groups) {
        if (group.name !in options) options += group.name
    }
    return options
}

internal fun policyLabel(policy: String): String = when (policy) {
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
