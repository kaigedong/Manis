package dev.manis.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.union
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.zIndex
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

/** The manual section is one more pipeline step, under a key no generated group id can take. */
private const val ManualSectionKey = "manual"
private const val ManualSectionTitle = "手动规则"

private const val RulesHelp = "从上到下匹配：先规则组，后手动规则，最后落到兜底。"

/** One pipeline header, and the 8dp list gap under every row. */
private val PipelineHeaderHeight = ManisDimens.rowHeight
private val PipelineRuleHeight = 52.dp
private val PipelineGap = ManisDimens.spaceSm

/** The distance between one rule row's top and the next, once the list spacing is added. */
private val RuleRowStep = PipelineRuleHeight + PipelineGap

/**
 * The routing-rules workspace: a priority pipeline of rule groups, then the manual rules, then the
 * FINAL fallback every unmatched connection lands on.
 *
 * Each header carries its 2-digit order, an immediate drag grip, the royal-violet target pill, an
 * enable switch and the expand arrow. Dragging the grip reorders the groups themselves; tapping it
 * opens the same reorder as two plain buttons, which is the single-pointer alternative WCAG 2.2
 * asks for. Rule rows reorder by dragging anywhere on the row, only inside their own section.
 * Both editors are 20dp bottom sheets, so the list stays visible behind the form.
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
    // A dragged header floats alone; the list keeps its order until the drop.
    var draggingGroupId by remember { mutableStateOf<String?>(null) }
    var groupDragOffset by remember { mutableFloatStateOf(0f) }
    var groupDropIndex by remember { mutableStateOf<Int?>(null) }
    var groupDropLineY by remember { mutableStateOf<Float?>(null) }
    val rowHeightPx = with(LocalDensity.current) { RuleRowStep.toPx() }
    val pillHeightPx = with(LocalDensity.current) { PipelineHeaderHeight.toPx() }
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

    /** One group swapped with its neighbour: the menu path to the same order a drag produces. */
    fun moveGroup(index: Int, delta: Int) {
        val to = index + delta
        Workspace.update { current ->
            if (index !in current.ruleGroups.indices || to !in current.ruleGroups.indices) {
                current
            } else {
                val groups = current.ruleGroups.toMutableList()
                groups.add(to, groups.removeAt(index))
                current.copy(ruleGroups = groups)
            }
        }
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

    Column(Modifier.fillMaxSize().background(colors.surfaceBase)) {
        ManisPageHeader(
            title = "分流规则",
            trailing = {
                CompositionLocalProvider(LocalManisTightActions provides true) {
                    ManisButton(
                        "新建组",
                        modifier = Modifier.widthIn(min = 64.dp),
                        onClick = {
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
                    Spacer(Modifier.width(ManisDimens.spaceXs))
                    RulesApplyAction(
                        pending = pending,
                        onApply = {
                            applyWorkspace(context)
                            onMessage("已应用工作区")
                        },
                    )
                }
            },
        )

        Box(Modifier.fillMaxSize()) {
            LazyColumn(
                state = listState,
                modifier = Modifier.fillMaxSize(),
                contentPadding = PaddingValues(
                    start = ManisDimens.spaceLg,
                    end = ManisDimens.spaceLg,
                    top = 0.dp,
                    bottom = ManisDimens.spaceXl,
                ),
                verticalArrangement = Arrangement.spacedBy(PipelineGap),
            ) {
                item(key = "rules-meta") {
                    PipelineMeta(
                        meta = "${config.ruleGroups.size} 组 · ${config.ruleEntries.size} 条",
                        hint = "自上而下依次匹配",
                    )
                }
                for ((index, group) in config.ruleGroups.withIndex()) {
                    val rules = config.ruleEntries.filter { it.groupId == group.id }
                    val expanded = group.id !in collapsed
                    item(key = "group-${group.id}") {
                        PipelineCard(
                            order = orderLabel(index + 1),
                            title = group.name,
                            target = policyLabel(group.policy),
                            count = rules.size,
                            titleColor = if (group.enabled) colors.textPrimary else colors.textTertiary,
                            expanded = expanded,
                            dragging = draggingGroupId == group.id,
                            // After a drop the section settles into its new slot instead of jumping.
                            modifier = if (draggingGroupId == null) Modifier.animateItem() else Modifier,
                            onToggleExpanded = {
                                collapsed = if (expanded) collapsed + group.id else collapsed - group.id
                            },
                            grip = {
                                PipelineGrip(
                                    key = group.id,
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
                                    menu = { dismiss ->
                                        ReorderMenu(
                                            canMoveUp = index > 0,
                                            canMoveDown = index < config.ruleGroups.lastIndex,
                                            onMoveUp = {
                                                dismiss()
                                                moveGroup(index, -1)
                                            },
                                            onMoveDown = {
                                                dismiss()
                                                moveGroup(index, 1)
                                            },
                                            onSettings = {
                                                dismiss()
                                                groupEditor = GroupEditor(group, saved = true)
                                            },
                                        )
                                    },
                                )
                            },
                            switch = {
                                ManisToggle(checked = group.enabled, onCheckedChange = { enabled ->
                                    Workspace.update { current ->
                                        current.copy(
                                            ruleGroups = current.ruleGroups.map {
                                                if (it.id == group.id) it.copy(enabled = enabled) else it
                                            },
                                        )
                                    }
                                })
                            },
                        )
                    }
                    if (expanded) {
                        if (rules.isEmpty()) {
                            item(key = "group-empty-${group.id}") {
                                PipelineNote("这个组还没有规则。")
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
                                onDragStart = { at ->
                                    draggingId = rules[at].id
                                    draggingIndex = at
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
                            PipelineAddRow(
                                onClick = {
                                    ruleEditor = RuleEditor(null, RuleEntry(groupId = group.id))
                                },
                                extraLabel = "组设置",
                                onExtra = { groupEditor = GroupEditor(group, saved = true) },
                            )
                        }
                    }
                }

                val manualRules = config.ruleEntries.filter { it.groupId == null }
                val manualExpanded = ManualSectionKey !in collapsed
                val pageEmpty = config.ruleGroups.isEmpty() && config.ruleEntries.isEmpty()
                item(key = "rules-manual-header") {
                    PipelineCard(
                        order = orderLabel(config.ruleGroups.size + 1),
                        title = ManualSectionTitle,
                        target = null,
                        meta = "${manualRules.size} 条 · 最后匹配",
                        expanded = manualExpanded,
                        dragging = false,
                        onToggleExpanded = {
                            collapsed = if (manualExpanded) {
                                collapsed + ManualSectionKey
                            } else {
                                collapsed - ManualSectionKey
                            }
                        },
                    )
                }
                if (manualExpanded) {
                    if (manualRules.isEmpty()) {
                        if (pageEmpty) {
                            item(key = "rules-empty") {
                                EmptyRules(onAdd = { ruleEditor = RuleEditor(null, RuleEntry()) })
                            }
                        } else {
                            item(key = "rules-manual-empty") { PipelineNote("还没有手动规则。") }
                        }
                    } else {
                        ruleRows(
                            rules = manualRules,
                            target = null,
                            draggingId = draggingId,
                            dragOffset = dragOffset,
                            onToggle = ::toggleRule,
                            onEdit = ::editRule,
                            onDragStart = { at ->
                                draggingId = manualRules[at].id
                                draggingIndex = at
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
                            PipelineAddRow(onClick = { ruleEditor = RuleEditor(null, RuleEntry()) })
                        }
                    }
                }

                item(key = "rules-fallback") { FallbackCard() }
                item(key = "rules-help") { ManisMeta(RulesHelp, maxLines = 2) }
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

            // Where the dragged group header will land. The padding shift is what turns an item
            // offset into a position in this Box, since the list measures items from its content.
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
            // The header itself stays in place, so the list keeps showing what is being passed; a
            // pill under the finger says which group is moving, even when the finger reaches the end.
            val draggedGroup = config.ruleGroups.firstOrNull { it.id == draggingGroupId }
            val draggedHeader = draggingGroupId?.let(::groupHeader)
            if (draggedGroup != null && draggedHeader != null) {
                val padding = listState.layoutInfo.beforeContentPadding
                // The pill follows the finger, but stays inside the list: an unclamped one would
                // leave the page bounds whenever the finger reaches the fallback card.
                val maxY = (listState.layoutInfo.viewportEndOffset + padding - pillHeightPx)
                    .coerceAtLeast(0f)
                val pillY = (draggedHeader.offset + padding + groupDragOffset).coerceIn(0f, maxY)
                Surface(
                    modifier = Modifier
                        .padding(horizontal = ManisDimens.spaceLg)
                        .fillMaxWidth()
                        .offset { IntOffset(0, pillY.roundToInt()) },
                    shape = RoundedCornerShape(ManisDimens.radiusCard),
                    color = colors.surfaceOverlay,
                    border = androidx.compose.foundation.BorderStroke(1.dp, colors.outlineSubtle),
                    shadowElevation = 8.dp,
                ) {
                    Row(
                        Modifier
                            .height(PipelineHeaderHeight)
                            .padding(horizontal = ManisDimens.spaceLg),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
                    ) {
                        Column(Modifier.weight(1f)) {
                            Text(
                                draggedGroup.name,
                                style = ManisType.label,
                                color = colors.textPrimary,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                            ManisPill("→ ${policyLabel(draggedGroup.policy)}", ManisTone.Route)
                        }
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

/** `01`, `02`, …: the priority position that reads as a pipeline step. */
private fun orderLabel(position: Int): String = position.toString().padStart(2, '0')

/** The rows of one section, with drag targets clamped to it. */
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
        PipelineRuleRow(
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

/** The page's own meta line: how many steps there are, and which way they run. */
@Composable
private fun PipelineMeta(meta: String, hint: String) {
    val colors = LocalManisColors.current
    Row(
        Modifier
            .fillMaxWidth()
            .padding(top = ManisDimens.spaceXs, bottom = ManisDimens.spaceXs),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
    ) {
        Text(meta, style = ManisType.data, color = colors.textSecondary, maxLines = 1)
        Spacer(Modifier.weight(1f))
        Text(
            hint,
            style = ManisType.metadata,
            color = colors.textTertiary,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

/** The 2-digit mono order badge that opens every pipeline step. */
@Composable
private fun OrderBadge(text: String) {
    val colors = LocalManisColors.current
    Box(
        Modifier
            .widthIn(min = 26.dp)
            .height(24.dp)
            .clip(RoundedCornerShape(ManisDimens.radiusBadge))
            .background(colors.surfaceLow)
            .padding(horizontal = ManisDimens.spaceXs),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            text,
            style = ManisType.data.copy(fontSize = 11.sp, fontWeight = FontWeight.SemiBold),
            color = colors.textSecondary,
            maxLines = 1,
        )
    }
}

/**
 * One pipeline step: order badge, grip, name over its route pill, enable switch and expand arrow.
 * The grip slot is held open even where there is nothing to drag, so every step keeps one x grid.
 */
@Composable
private fun PipelineCard(
    order: String,
    title: String,
    modifier: Modifier = Modifier,
    /** The royal-violet route target; null in the manual section, which routes per rule. */
    target: String? = null,
    count: Int = 0,
    meta: String? = null,
    titleColor: Color? = null,
    expanded: Boolean = true,
    dragging: Boolean = false,
    grip: (@Composable () -> Unit)? = null,
    switch: (@Composable () -> Unit)? = null,
    onToggleExpanded: (() -> Unit)? = null,
) {
    val colors = LocalManisColors.current
    Surface(
        modifier = modifier.fillMaxWidth(),
        shape = RoundedCornerShape(ManisDimens.radiusCard),
        color = if (dragging) colors.actionSoft else colors.surfaceElevated,
        border = androidx.compose.foundation.BorderStroke(1.dp, colors.outlineSubtle),
        shadowElevation = if (dragging) 6.dp else 0.dp,
    ) {
        Row(
            Modifier
                .fillMaxWidth()
                .height(PipelineHeaderHeight)
                .padding(start = ManisDimens.spaceSm, end = ManisDimens.spaceXs),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceXs),
        ) {
            OrderBadge(order)
            if (grip != null) {
                Box(Modifier.size(ManisDimens.minTouch), contentAlignment = Alignment.Center) {
                    grip()
                }
            } else {
                // The manual step cannot be dragged, so its slot stays empty rather than collapsing.
                Spacer(Modifier.width(ManisDimens.minTouch))
            }
            Column(
                Modifier
                    .weight(1f)
                    .then(
                        if (onToggleExpanded != null) {
                            Modifier.clickable(onClick = onToggleExpanded)
                        } else {
                            Modifier
                        },
                    )
                    .padding(vertical = ManisDimens.spaceXs),
                verticalArrangement = Arrangement.spacedBy(3.dp),
            ) {
                Text(
                    title,
                    style = ManisType.label,
                    color = titleColor ?: colors.textPrimary,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                when {
                    target != null -> ManisPill(
                        if (count > 0) "→ $target · $count 条" else "→ $target",
                        ManisTone.Route,
                    )
                    meta != null -> Text(
                        meta,
                        style = ManisType.metadata,
                        color = colors.textTertiary,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
            Box(
                Modifier
                    .width(ManisDimens.switchWidth)
                    .height(ManisDimens.minTouch),
                contentAlignment = Alignment.Center,
            ) {
                switch?.invoke()
            }
            if (onToggleExpanded != null) {
                ManisIconButton(
                    icon = Icons.Filled.KeyboardArrowDown,
                    contentDescription = if (expanded) "收起" else "展开",
                    onClick = onToggleExpanded,
                    rotationDegrees = if (expanded) 180f else 0f,
                )
            } else {
                Spacer(Modifier.width(ManisDimens.iconTarget))
            }
        }
    }
}

/**
 * The grip: drag it to reorder, or tap it for the 上移 / 下移 menu. Both paths end in the same
 * order state, and the menu is what makes the reorder reachable with a single pointer.
 */
@Composable
private fun PipelineGrip(
    key: String,
    onDragStart: () -> Unit,
    onDrag: (Float) -> Unit,
    onDragEnd: () -> Unit,
    onDragCancel: () -> Unit,
    menu: @Composable (dismiss: () -> Unit) -> Unit,
) {
    val colors = LocalManisColors.current
    var menuOpen by remember { mutableStateOf(false) }
    val dragStart by rememberUpdatedState(onDragStart)
    val drag by rememberUpdatedState(onDrag)
    val dragEnd by rememberUpdatedState(onDragEnd)
    val dragCancel by rememberUpdatedState(onDragCancel)

    Box {
        Box(
            Modifier
                .size(ManisDimens.minTouch)
                .clip(RoundedCornerShape(ManisDimens.radiusControl))
                .pointerInput(key) {
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
                .pointerInput(key) { detectTapGestures { menuOpen = true } },
            contentAlignment = Alignment.Center,
        ) {
            Icon(
                painter = painterResource(R.drawable.ic_drag_handle),
                contentDescription = "拖动排序，或点按打开排序菜单",
                modifier = Modifier.size(24.dp),
                tint = colors.textTertiary,
            )
        }
        DropdownMenu(
            expanded = menuOpen,
            onDismissRequest = { menuOpen = false },
            containerColor = colors.surfaceOverlay,
            shape = RoundedCornerShape(ManisDimens.radiusCard),
        ) {
            menu { menuOpen = false }
        }
    }
}

/** The single-pointer reorder: the same two moves the drag produces, plus the group's own settings. */
@Composable
private fun ReorderMenu(
    canMoveUp: Boolean,
    canMoveDown: Boolean,
    onMoveUp: () -> Unit,
    onMoveDown: () -> Unit,
    onSettings: () -> Unit,
) {
    val colors = LocalManisColors.current
    DropdownMenuItem(
        text = {
            Text(
                "上移优先级",
                style = ManisType.body,
                color = if (canMoveUp) colors.textPrimary else colors.textTertiary,
            )
        },
        enabled = canMoveUp,
        onClick = onMoveUp,
    )
    DropdownMenuItem(
        text = {
            Text(
                "下移优先级",
                style = ManisType.body,
                color = if (canMoveDown) colors.textPrimary else colors.textTertiary,
            )
        },
        enabled = canMoveDown,
        onClick = onMoveDown,
    )
    DropdownMenuItem(
        text = { Text("组设置", style = ManisType.body, color = colors.textPrimary) },
        onClick = onSettings,
    )
}

/** The last step: everything the rules above did not match. Dashed, so it reads as the floor. */
@Composable
private fun FallbackCard() {
    val colors = LocalManisColors.current
    Row(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(ManisDimens.radiusCard))
            .background(colors.surfaceLow)
            .drawBehind {
                drawRoundRect(
                    color = colors.outlineStrong,
                    cornerRadius = CornerRadius(ManisDimens.radiusCard.toPx()),
                    style = Stroke(
                        width = 1.dp.toPx(),
                        pathEffect = PathEffect.dashPathEffect(
                            floatArrayOf(6.dp.toPx(), 4.dp.toPx()),
                        ),
                    ),
                )
            }
            .padding(horizontal = ManisDimens.spaceLg, vertical = ManisDimens.spaceMd),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceMd),
    ) {
        OrderBadge("END")
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text("FINAL · 默认兜底", style = ManisType.label, color = colors.textPrimary, maxLines = 1)
            Text(
                "未命中以上任何规则的流量",
                style = ManisType.metadata,
                color = colors.textTertiary,
                maxLines = 2,
            )
        }
        ManisPill("→ 全局", ManisTone.Route)
    }
}

/** A rule row: its kind, what it matches, and the switch that keeps it in force. */
@Composable
private fun PipelineRuleRow(
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
    val dragStart by rememberUpdatedState(onDragStart)
    val drag by rememberUpdatedState(onDrag)
    val dragEnd by rememberUpdatedState(onDragEnd)
    Surface(
        onClick = onEdit,
        modifier = modifier
            .fillMaxWidth()
            // One step in from the header so the rows read as the body of the step above them.
            .padding(start = ManisDimens.spaceLg)
            .height(PipelineRuleHeight)
            .zIndex(if (dragging) 1f else 0f)
            .graphicsLayer { translationY = offset },
        shape = RoundedCornerShape(ManisDimens.radiusControl),
        color = if (dragging) colors.actionSoft else colors.surfaceElevated,
        border = androidx.compose.foundation.BorderStroke(1.dp, colors.outlineSubtle),
        shadowElevation = if (dragging) 6.dp else 0.dp,
    ) {
        Row(
            Modifier
                .fillMaxSize()
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
                }
                .padding(start = ManisDimens.spaceMd, end = ManisDimens.spaceXs),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
        ) {
            ManisPill(rule.kind, ManisTone.Neutral)
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(1.dp)) {
                Text(
                    if (rule.kind == "FINAL") "所有剩余流量" else rule.value,
                    style = ManisType.data,
                    color = if (rule.enabled) colors.textPrimary else colors.textTertiary,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                // A group header states the target for all its rows; a manual rule states its own.
                if (groupTarget == null) {
                    Text(
                        "→ ${policyLabel(rule.policy)}",
                        style = ManisType.metadata,
                        color = colors.textTertiary,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
            Box(
                Modifier
                    .width(ManisDimens.switchWidth)
                    .height(ManisDimens.minTouch),
                contentAlignment = Alignment.Center,
            ) {
                ManisToggle(checked = rule.enabled, onCheckedChange = onToggle)
            }
        }
    }
}

/** A section's placeholder, indented to the rows so it reads as part of that step. */
@Composable
private fun PipelineNote(text: String) {
    Text(
        text,
        style = ManisType.metadata,
        color = LocalManisColors.current.textTertiary,
        maxLines = 2,
        overflow = TextOverflow.Ellipsis,
        modifier = Modifier
            .fillMaxWidth()
            .padding(start = ManisDimens.spaceLg + ManisDimens.spaceMd),
    )
}

/** The one way to add a rule to a section, under its last row. */
@Composable
private fun PipelineAddRow(
    onClick: () -> Unit,
    extraLabel: String? = null,
    onExtra: (() -> Unit)? = null,
) {
    val colors = LocalManisColors.current
    Row(
        Modifier
            .fillMaxWidth()
            .height(ManisDimens.minTouch)
            .clip(RoundedCornerShape(ManisDimens.radiusControl))
            .clickable(onClick = onClick)
            .padding(start = ManisDimens.spaceLg + ManisDimens.spaceMd, end = ManisDimens.spaceSm),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text("＋ 添加规则", style = ManisType.metadata, color = colors.textSecondary, maxLines = 1)
        if (extraLabel != null && onExtra != null) {
            Spacer(Modifier.weight(1f))
            // The group's own settings sit with its content, so a header tap means one thing.
            ManisTextAction(extraLabel, onClick = onExtra)
        }
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

/** The cobalt apply action: filled only while the workspace has changes waiting. */
@Composable
private fun RulesApplyAction(pending: Boolean, onApply: () -> Unit) {
    val colors = LocalManisColors.current
    Row(
        Modifier
            .height(ManisDimens.controlCompact)
            .clip(CircleShape)
            .background(if (pending) colors.actionPrimary else colors.buttonBg)
            .clickable(enabled = pending, onClick = onApply)
            .padding(horizontal = ManisDimens.spaceLg),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        // The amber dot is the pending-change mark: colour alone never carries the state.
        if (pending) {
            Box(Modifier.size(6.dp).clip(CircleShape).background(colors.statusWarning))
        }
        Text(
            "应用",
            style = ManisType.label,
            color = if (pending) colors.actionOnPrimary else colors.textTertiary,
            maxLines = 1,
        )
    }
}

/**
 * The 20dp editor sheet: drag handle, scrolling body and a 48dp submit bar that rides the keyboard
 * via `imePadding`. Shared by the rule and rule-group editors.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun EditorSheet(
    title: String,
    onDismiss: () -> Unit,
    footer: @Composable RowScope.() -> Unit,
    subtitle: String? = null,
    content: @Composable ColumnScope.() -> Unit,
) {
    val colors = LocalManisColors.current
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheetState,
        shape = RoundedCornerShape(
            topStart = ManisDimens.radiusSheet,
            topEnd = ManisDimens.radiusSheet,
        ),
        containerColor = colors.surfaceOverlay,
        scrimColor = colors.scrim,
        dragHandle = {
            Box(
                Modifier
                    .padding(top = ManisDimens.spaceMd, bottom = ManisDimens.spaceXs)
                    .size(width = 36.dp, height = 4.dp)
                    .clip(RoundedCornerShape(9999.dp))
                    .background(colors.outlineStrong.copy(alpha = 0.45f)),
            )
        },
    ) {
        Column(
            Modifier
                .fillMaxWidth()
                .windowInsetsPadding(WindowInsets.ime.union(WindowInsets.navigationBars)),
        ) {
            Row(
                Modifier
                    .fillMaxWidth()
                    .padding(start = ManisDimens.spaceXl, end = ManisDimens.spaceSm),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Column(Modifier.weight(1f)) {
                    Text(title, style = ManisType.dialogTitle, color = colors.textPrimary)
                    if (subtitle != null) ManisMeta(subtitle, maxLines = 2)
                }
                Box(
                    Modifier
                        .size(ManisDimens.minTouch)
                        .clip(CircleShape)
                        .clickable(onClick = onDismiss),
                    contentAlignment = Alignment.Center,
                ) {
                    Icon(
                        painter = painterResource(R.drawable.ic_close),
                        contentDescription = "关闭",
                        modifier = Modifier.size(20.dp),
                        tint = colors.textSecondary,
                    )
                }
            }
            Column(
                Modifier
                    .fillMaxWidth()
                    .weight(1f, fill = false)
                    .verticalScroll(rememberScrollState())
                    .padding(horizontal = ManisDimens.spaceXl, vertical = ManisDimens.spaceMd),
                verticalArrangement = Arrangement.spacedBy(ManisDimens.spaceMd),
                content = content,
            )
            ManisDivider()
            Row(
                Modifier
                    .fillMaxWidth()
                    .height(ManisDimens.minTouch)
                    .padding(horizontal = ManisDimens.spaceLg),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
                content = footer,
            )
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

    EditorSheet(
        title = if (editingIndex == null) "添加规则" else "编辑规则",
        subtitle = "规则按顺序匹配",
        onDismiss = onCancel,
        footer = {
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
                selected = groupChoices.firstOrNull { it.first == groupId }?.second
                    ?: ManualSectionTitle,
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
            ManisTextField(
                value = value,
                onValueChange = { value = it },
                placeholder = kindPlaceholder(kind),
            )
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

    EditorSheet(
        title = if (saved) "编辑规则组" else "新建规则组",
        subtitle = if (saved) null else "组内规则统一走同一个目标",
        onDismiss = onCancel,
        footer = {
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
                Modifier.fillMaxWidth().height(ManisDimens.minTouch),
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
