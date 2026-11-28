package dev.manis.app.ui

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.clickable
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.FilledTonalIconButton
import androidx.compose.material3.FilterChip
import androidx.compose.material3.FilterChipDefaults
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButtonDefaults
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.setValue
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import dev.manis.app.R
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp

/**
 * Set where several actions must share one row on a 320dp screen: a dialog footer, or the rules
 * page header. Those buttons trade half their side padding for width; a card's button keeps the
 * roomier default.
 */
internal val LocalManisTightActions = staticCompositionLocalOf { false }

/** A Material 3 filled card: tonal surface, 16dp corners, no hairline border. */
@Composable
fun ManisCard(
    modifier: Modifier = Modifier,
    onClick: (() -> Unit)? = null,
    borderColor: Color? = null,
    content: @Composable ColumnScope.() -> Unit,
) {
    val colors = LocalManisColors.current
    Surface(
        modifier = modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(ManisDimens.radiusCard))
            .then(
                if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier,
            ),
        shape = RoundedCornerShape(ManisDimens.radiusCard),
        color = colors.surfaceElevated,
        border = BorderStroke(1.dp, borderColor ?: colors.outlineSubtle),
    ) {
        Column(
            Modifier.padding(ManisDimens.spaceLg),
            verticalArrangement = Arrangement.spacedBy(ManisDimens.spaceMd),
            content = content,
        )
    }
}

@Composable
fun ManisSectionTitle(text: String, modifier: Modifier = Modifier) {
    Text(
        text,
        modifier = modifier,
        style = ManisType.sectionTitle,
        color = LocalManisColors.current.textPrimary,
        // One line: a section bar owns a fixed band, so a long title ellipsizes instead of growing it.
        maxLines = 1,
        overflow = TextOverflow.Ellipsis,
    )
}

/**
 * The band every page puts a section title on: title left, actions right, one 56dp row. It owns that
 * height, so a section lands on the same line whether its page offers one action or three, and the
 * actions keep their 40dp step instead of growing with the word they carry.
 */
@Composable
fun ManisSectionBar(
    title: String,
    modifier: Modifier = Modifier,
    actions: @Composable RowScope.() -> Unit,
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .height(ManisDimens.rowHeight),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
    ) {
        // The title gives way to the actions: a long section name shortens rather than pushing a
        // button past the row's edge.
        ManisSectionTitle(title, Modifier.weight(1f))
        actions()
    }
}

/**
 * One action inside a [ManisSectionBar]. It is text, not a filled button: section actions are
 * secondary next to the page's own primary, and a wall of tonal pills reads heavier than the list
 * they sit above. The 40dp height and the 56dp label floor still stop 刷新中… from resizing the row.
 */
@Composable
fun ManisSectionAction(
    label: String,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    onClick: () -> Unit,
) {
    TextButton(
        onClick = onClick,
        modifier = modifier.height(ManisDimens.controlCompact),
        enabled = enabled,
        shape = RoundedCornerShape(ManisDimens.radiusControl),
        contentPadding = PaddingValues(horizontal = ManisDimens.spaceMd),
    ) {
        Text(
            label,
            modifier = Modifier.widthIn(min = 56.dp),
            style = ManisType.label,
            textAlign = TextAlign.Center,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

/** The tonal treatment a plain button and a section action share. */
@Composable
private fun manisTonalButtonColors() = ButtonDefaults.filledTonalButtonColors(
    containerColor = MaterialTheme.colorScheme.surfaceContainerHigh,
    contentColor = MaterialTheme.colorScheme.onSurface,
)

@Composable
fun ManisMeta(text: String, color: Color? = null, maxLines: Int = Int.MAX_VALUE) {
    Text(
        text,
        style = ManisType.metadata,
        color = color ?: LocalManisColors.current.textTertiary,
        maxLines = maxLines,
        overflow = TextOverflow.Ellipsis,
    )
}

/** A Material 3 button: filled for the primary action, tonal for everything else. */
@Composable
fun ManisButton(
    label: String,
    modifier: Modifier = Modifier,
    primary: Boolean = false,
    enabled: Boolean = true,
    onClick: () -> Unit,
) {
    val shape = CircleShape
    val contentPadding = PaddingValues(
        horizontal = if (LocalManisTightActions.current) {
            ManisDimens.spaceMd
        } else {
            ManisDimens.spaceXl
        },
    )
    val colors = ButtonDefaults.buttonColors(
        containerColor = MaterialTheme.colorScheme.primary,
        contentColor = MaterialTheme.colorScheme.onPrimary,
    )
    val tonalColors = manisTonalButtonColors()
    if (primary) {
        Button(
            onClick = onClick,
            modifier = modifier.height(ManisDimens.controlCompact),
            enabled = enabled,
            shape = shape,
            colors = colors,
            contentPadding = contentPadding,
        ) {
            Text(label, style = ManisType.label, maxLines = 1)
        }
    } else {
        FilledTonalButton(
            onClick = onClick,
            modifier = modifier.height(ManisDimens.controlCompact),
            enabled = enabled,
            shape = shape,
            colors = tonalColors,
            contentPadding = contentPadding,
        ) {
            Text(label, style = ManisType.label, maxLines = 1)
        }
    }
}

/** A list row: title, supporting text and an optional trailing value. */
@Composable
fun ManisListRow(
    title: String,
    meta: String? = null,
    trailing: String? = null,
    selected: Boolean = false,
    enabled: Boolean = true,
    onClick: (() -> Unit)? = null,
    leading: (@Composable RowScope.() -> Unit)? = null,
) {
    val colors = LocalManisColors.current
    Row(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(ManisDimens.radiusRow))
            .background(if (selected) colors.actionSoft else Color.Transparent)
            .clickable(enabled = enabled && onClick != null) { onClick?.invoke() }
            .padding(horizontal = ManisDimens.spaceMd, vertical = ManisDimens.spaceMd),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceMd),
    ) {
        leading?.invoke(this)
        Column(Modifier.weight(1f)) {
            Text(
                title,
                style = ManisType.label,
                color = if (selected) colors.actionPrimary else colors.textPrimary,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
            if (meta != null) ManisMeta(meta)
        }
        if (trailing != null) {
            Text(trailing, style = ManisType.data, color = colors.textSecondary)
        }
    }
}

@Composable
fun ManisDivider() {
    HorizontalDivider(
        thickness = 1.dp,
        color = LocalManisColors.current.outlineSubtle.copy(alpha = 0.6f),
    )
}

/** A Material 3 switch. */
@Composable
fun ManisToggle(checked: Boolean, onCheckedChange: (Boolean) -> Unit) {
    Switch(checked = checked, onCheckedChange = onCheckedChange)
}

/** A Material 3 filter chip. */
@Composable
fun ManisChip(
    label: String,
    selected: Boolean,
    modifier: Modifier = Modifier,
    onClick: () -> Unit,
) {
    val colors = LocalManisColors.current
    FilterChip(
        selected = selected,
        onClick = onClick,
        modifier = modifier,
        label = { Text(label, style = ManisType.data, maxLines = 1) },
        shape = RoundedCornerShape(ManisDimens.radiusControl),
        colors = FilterChipDefaults.filterChipColors(
            containerColor = colors.surfaceLow,
            labelColor = colors.textSecondary,
            selectedContainerColor = colors.actionSoft,
            selectedLabelColor = colors.actionPrimary,
        ),
        border = FilterChipDefaults.filterChipBorder(
            enabled = true,
            selected = selected,
            borderColor = colors.outlineSubtle,
            selectedBorderColor = Color.Transparent,
        ),
    )
}

/** The compact back control used by the drill-in panes. */
@Composable
fun ManisBackButton(onBack: () -> Unit) {
    FilledTonalIconButton(
        onClick = onBack,
        modifier = Modifier.size(ManisDimens.controlStandard),
        shape = CircleShape,
        colors = IconButtonDefaults.filledTonalIconButtonColors(
            containerColor = MaterialTheme.colorScheme.surfaceContainerHigh,
            contentColor = MaterialTheme.colorScheme.onSurface,
        ),
    ) {
        Icon(
            imageVector = Icons.AutoMirrored.Filled.ArrowBack,
            contentDescription = "返回",
            modifier = Modifier.size(20.dp),
        )
    }
}

/** A Material 3 outlined text field. */
/**
 * A compact 40dp search field: leading magnifier, inline placeholder and a clear action, so it
 * sits in the same row as the page's text actions instead of towering over them.
 */
@Composable
fun ManisSearchField(
    value: String,
    onValueChange: (String) -> Unit,
    placeholder: String = "搜索",
) {
    val colors = LocalManisColors.current
    Surface(
        modifier = Modifier.fillMaxWidth().height(ManisDimens.controlStandard),
        shape = RoundedCornerShape(ManisDimens.controlStandard / 2),
        color = colors.surfaceLow,
    ) {
        Row(
            Modifier.fillMaxSize().padding(horizontal = ManisDimens.spaceMd),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
        ) {
            Icon(
                painter = painterResource(R.drawable.ic_search),
                contentDescription = null,
                modifier = Modifier.size(18.dp),
                tint = colors.textTertiary,
            )
            Box(Modifier.weight(1f)) {
                if (value.isEmpty()) {
                    Text(
                        placeholder,
                        style = ManisType.body,
                        color = colors.textTertiary,
                        maxLines = 1,
                    )
                }
                BasicTextField(
                    value = value,
                    onValueChange = onValueChange,
                    singleLine = true,
                    textStyle = ManisType.body.copy(color = colors.textPrimary),
                    cursorBrush = SolidColor(colors.actionPrimary),
                    modifier = Modifier.fillMaxWidth(),
                )
            }
            if (value.isNotEmpty()) {
                Icon(
                    painter = painterResource(R.drawable.ic_close),
                    contentDescription = "清除",
                    modifier = Modifier
                        .size(20.dp)
                        .clickable { onValueChange("") },
                    tint = colors.textTertiary,
                )
            }
        }
    }
}

@Composable
fun ManisTextField(
    value: String,
    onValueChange: (String) -> Unit,
    placeholder: String = "",
) {
    val colors = LocalManisColors.current
    OutlinedTextField(
        value = value,
        onValueChange = onValueChange,
        modifier = Modifier.fillMaxWidth(),
        singleLine = true,
        shape = RoundedCornerShape(ManisDimens.radiusControl),
        textStyle = ManisType.body,
        placeholder = {
            Text(placeholder, style = ManisType.body, color = colors.textTertiary)
        },
        colors = OutlinedTextFieldDefaults.colors(
            focusedContainerColor = colors.surfaceOverlay,
            unfocusedContainerColor = colors.surfaceOverlay,
            focusedBorderColor = colors.actionPrimary,
            unfocusedBorderColor = colors.outlineSubtle,
            cursorColor = colors.actionPrimary,
            focusedTextColor = colors.textPrimary,
            unfocusedTextColor = colors.textPrimary,
        ),
    )
}

/** A label/value row. */
@Composable
fun ManisInfoRow(label: String, value: String) {
    val colors = LocalManisColors.current
    Row(
        Modifier.fillMaxWidth().padding(vertical = ManisDimens.spaceXs),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(label, style = ManisType.body, color = colors.textSecondary)
        Spacer(Modifier.weight(1f))
        Text(value, style = ManisType.data, color = colors.textPrimary)
    }
}

/** A Material 3 text action. */
@Composable
fun ManisTextAction(label: String, tone: Color? = null, onClick: () -> Unit) {
    TextButton(
        onClick = onClick,
        shape = CircleShape,
        contentPadding = PaddingValues(horizontal = ManisDimens.spaceMd),
    ) {
        Text(label, style = ManisType.label, color = tone ?: LocalContentColor.current)
    }
}

/**
 * A square icon control. Section headers and rule rows use it in place of a labelled button, so one
 * action keeps the same target and the same position whatever word it would have needed.
 */
@Composable
fun ManisIconButton(
    icon: ImageVector,
    contentDescription: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    /** The collapse chevron is this one glyph turned over, so the app needs no second vector. */
    rotationDegrees: Float = 0f,
) {
    val colors = LocalManisColors.current
    Box(
        modifier = modifier
            .size(ManisDimens.iconTarget)
            .clip(RoundedCornerShape(ManisDimens.radiusControl))
            .clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            imageVector = icon,
            contentDescription = contentDescription,
            modifier = Modifier.size(ManisDimens.iconGlyph).rotate(rotationDegrees),
            tint = colors.textSecondary,
        )
    }
}

/** A Material 3 progress bar. */
@Composable
fun ManisProgress(fraction: Float) {
    val colors = LocalManisColors.current
    LinearProgressIndicator(
        progress = { fraction.coerceIn(0f, 1f) },
        modifier = Modifier
            .fillMaxWidth()
            .height(8.dp)
            .clip(RoundedCornerShape(4.dp)),
        color = colors.actionPrimary,
        trackColor = colors.surfaceLow,
        drawStopIndicator = {},
    )
}

/** A Material 3 snackbar-style toast. */
@Composable
fun ManisToast(message: String, modifier: Modifier = Modifier) {
    Surface(
        modifier = modifier,
        shape = RoundedCornerShape(ManisDimens.radiusControl),
        color = MaterialTheme.colorScheme.inverseSurface,
        contentColor = MaterialTheme.colorScheme.inverseOnSurface,
        shadowElevation = 6.dp,
    ) {
        Text(
            message,
            style = ManisType.body,
            color = MaterialTheme.colorScheme.inverseOnSurface,
            modifier = Modifier.padding(
                horizontal = ManisDimens.spaceLg,
                vertical = ManisDimens.spaceMd,
            ),
        )
    }
}

/** A Material 3 dialog: scrim, rounded surface, title, body and actions. */
@Composable
fun ManisDialog(
    title: String,
    subtitle: String? = null,
    onDismiss: () -> Unit,
    actions: @Composable RowScope.() -> Unit,
    content: @Composable ColumnScope.() -> Unit,
) {
    val colors = LocalManisColors.current
    Box(
        Modifier
            .fillMaxSize()
            .background(MaterialTheme.colorScheme.scrim.copy(alpha = 0.45f))
            .clickable(onClick = onDismiss),
        contentAlignment = Alignment.Center,
    ) {
        Surface(
            modifier = Modifier
                .fillMaxWidth()
                .padding(ManisDimens.spaceXl)
                .clickable(
                    interactionSource = remember { MutableInteractionSource() },
                    indication = null,
                ) {},
            shape = RoundedCornerShape(28.dp),
            color = MaterialTheme.colorScheme.surfaceContainerHigh,
            tonalElevation = 3.dp,
        ) {
            Column {
                Column(Modifier.padding(ManisDimens.spaceXl)) {
                    Text(title, style = ManisType.dialogTitle, color = colors.textPrimary)
                    if (subtitle != null) {
                        Spacer(Modifier.height(ManisDimens.spaceXs))
                        ManisMeta(subtitle)
                    }
                }
                Column(
                    Modifier
                        .padding(horizontal = ManisDimens.spaceXl)
                        .verticalScroll(rememberScrollState()),
                    verticalArrangement = Arrangement.spacedBy(ManisDimens.spaceMd),
                    content = content,
                )
                // 40dp actions on one glued row: a destructive text action, then cancel and save.
                // The tight padding is what keeps three of them from clipping at 320dp wide.
                CompositionLocalProvider(LocalManisTightActions provides true) {
                    Row(
                        Modifier
                            .fillMaxWidth()
                            .padding(
                                horizontal = ManisDimens.spaceLg,
                                vertical = ManisDimens.spaceSm,
                            ),
                        horizontalArrangement = Arrangement.spacedBy(
                            ManisDimens.spaceSm,
                            Alignment.End,
                        ),
                        verticalAlignment = Alignment.CenterVertically,
                        content = actions,
                    )
                }
            }
        }
    }
}

/** A Material 3 exposed dropdown for single-choice lists. */
@OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class)
@Composable
fun ManisSelect(
    label: String,
    options: List<String>,
    selected: String,
    modifier: Modifier = Modifier,
    display: (String) -> String = { it },
    onSelect: (String) -> Unit,
) {
    var expanded by remember { mutableStateOf(false) }
    val colors = LocalManisColors.current
    androidx.compose.material3.ExposedDropdownMenuBox(
        expanded = expanded,
        onExpandedChange = { expanded = it },
        modifier = modifier,
    ) {
        OutlinedTextField(
            value = display(selected),
            onValueChange = {},
            readOnly = true,
            label = { Text(label, style = ManisType.metadata) },
            trailingIcon = {
                androidx.compose.material3.ExposedDropdownMenuDefaults.TrailingIcon(expanded = expanded)
            },
            modifier = Modifier
                .fillMaxWidth()
                .menuAnchor(androidx.compose.material3.MenuAnchorType.PrimaryNotEditable),
            singleLine = true,
            shape = RoundedCornerShape(ManisDimens.radiusControl),
            textStyle = ManisType.body,
            colors = OutlinedTextFieldDefaults.colors(
                focusedContainerColor = colors.surfaceOverlay,
                unfocusedContainerColor = colors.surfaceOverlay,
                focusedBorderColor = colors.actionPrimary,
                unfocusedBorderColor = colors.outlineSubtle,
                focusedTextColor = colors.textPrimary,
                unfocusedTextColor = colors.textPrimary,
            ),
        )
        ExposedDropdownMenu(
            expanded = expanded,
            onDismissRequest = { expanded = false },
        ) {
            for (option in options) {
                androidx.compose.material3.DropdownMenuItem(
                    text = { Text(display(option), style = ManisType.body) },
                    onClick = {
                        onSelect(option)
                        expanded = false
                    },
                )
            }
        }
    }
}

/** A dropdown with checkable entries, for member lists. */
@OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class)
@Composable
fun ManisMultiSelect(
    label: String,
    options: List<String>,
    selected: Set<Int>,
    modifier: Modifier = Modifier,
    onToggle: (Int) -> Unit,
) {
    var expanded by remember { mutableStateOf(false) }
    val colors = LocalManisColors.current
    val summary = options.withIndex()
        .filter { it.index in selected }
        .joinToString("、") { it.value }
        .ifBlank { "未选择" }
    androidx.compose.material3.ExposedDropdownMenuBox(
        expanded = expanded,
        onExpandedChange = { expanded = it },
        modifier = modifier,
    ) {
        OutlinedTextField(
            value = summary,
            onValueChange = {},
            readOnly = true,
            label = { Text(label, style = ManisType.metadata) },
            trailingIcon = {
                androidx.compose.material3.ExposedDropdownMenuDefaults.TrailingIcon(expanded = expanded)
            },
            modifier = Modifier
                .fillMaxWidth()
                .menuAnchor(androidx.compose.material3.MenuAnchorType.PrimaryNotEditable),
            singleLine = true,
            shape = RoundedCornerShape(ManisDimens.radiusControl),
            textStyle = ManisType.body,
            colors = OutlinedTextFieldDefaults.colors(
                focusedContainerColor = colors.surfaceOverlay,
                unfocusedContainerColor = colors.surfaceOverlay,
                focusedBorderColor = colors.actionPrimary,
                unfocusedBorderColor = colors.outlineSubtle,
                focusedTextColor = colors.textPrimary,
                unfocusedTextColor = colors.textPrimary,
            ),
        )
        ExposedDropdownMenu(
            expanded = expanded,
            onDismissRequest = { expanded = false },
        ) {
            if (options.isEmpty()) {
                androidx.compose.material3.DropdownMenuItem(
                    text = { Text("没有可选项", style = ManisType.body) },
                    onClick = { expanded = false },
                )
            }
            for ((index, option) in options.withIndex()) {
                androidx.compose.material3.DropdownMenuItem(
                    text = { Text(option, style = ManisType.body) },
                    trailingIcon = {
                        androidx.compose.material3.Checkbox(
                            checked = index in selected,
                            onCheckedChange = null,
                        )
                    },
                    onClick = { onToggle(index) },
                )
            }
        }
    }
}


// ===== Manis UI 2.0 signature components =====

/** The semantic tones a pill can carry. Route is reserved for the rule -> group -> exit chain. */
enum class ManisTone { Neutral, Accent, Route, Success, Warning, Error }

private data class ToneColors(val foreground: Color, val background: Color)

@Composable
private fun toneColors(tone: ManisTone): ToneColors {
    val colors = LocalManisColors.current
    return when (tone) {
        ManisTone.Neutral -> ToneColors(colors.textSecondary, colors.surfaceLow)
        ManisTone.Accent -> ToneColors(colors.actionPrimary, colors.actionSoft)
        ManisTone.Route -> ToneColors(colors.routeTrace, colors.routeSoft)
        ManisTone.Success -> ToneColors(colors.statusSuccess, colors.statusSuccessSoft)
        ManisTone.Warning -> ToneColors(colors.statusWarning, colors.statusWarningSoft)
        ManisTone.Error -> ToneColors(colors.statusError, colors.statusErrorSoft)
    }
}

/** A compact property pill: 11sp medium, 6dp corners, soft tone fill. No emoji, ever. */
@Composable
fun ManisPill(label: String, tone: ManisTone = ManisTone.Neutral, modifier: Modifier = Modifier) {
    val toneColors = toneColors(tone)
    Row(
        modifier = modifier
            .height(20.dp)
            .clip(RoundedCornerShape(ManisDimens.radiusBadge))
            .background(toneColors.background)
            .padding(horizontal = ManisDimens.spaceSm),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(5.dp),
    ) {
        Text(
            label,
            style = ManisType.metadata.copy(fontSize = 11.sp, fontWeight = FontWeight.Medium),
            color = toneColors.foreground,
            maxLines = 1,
        )
    }
}

/** The crisp 2-letter ISO region square that replaces flag emoji. */
@Composable
fun ManisRegionBadge(code: String, modifier: Modifier = Modifier) {
    val colors = LocalManisColors.current
    Box(
        modifier = modifier
            .size(width = 26.dp, height = 20.dp)
            .clip(RoundedCornerShape(5.dp))
            .background(colors.surfaceLow)
            .then(
                Modifier.border(1.dp, colors.outlineSubtle, RoundedCornerShape(5.dp)),
            ),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            code,
            style = ManisType.data.copy(fontSize = 10.5.sp, fontWeight = FontWeight.SemiBold),
            color = colors.textSecondary,
            maxLines = 1,
        )
    }
}

/** The status colour a measured latency earns, following the documented bands. */
fun latitudeTone(milliseconds: Int?, colors: ManisColors): Color = when {
    milliseconds == null -> colors.textTertiary
    milliseconds < 120 -> colors.statusSuccess
    milliseconds <= 350 -> colors.statusWarning
    else -> colors.statusError
}

private fun latencyBars(milliseconds: Int?): Int = when {
    milliseconds == null -> 0
    milliseconds < 120 -> 3
    milliseconds <= 350 -> 2
    else -> 1
}

/** Mono latency plus three signal bars; unknown latency renders a neutral dash. */
@Composable
fun ManisLatency(milliseconds: Int?, modifier: Modifier = Modifier) {
    val colors = LocalManisColors.current
    val color = latitudeTone(milliseconds, colors)
    val bars = latencyBars(milliseconds)
    Row(
        modifier = modifier,
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Row(
            modifier = Modifier.height(11.dp),
            verticalAlignment = Alignment.Bottom,
            horizontalArrangement = Arrangement.spacedBy(2.dp),
        ) {
            listOf(4.dp, 7.dp, 11.dp).forEachIndexed { index, height ->
                Box(
                    Modifier
                        .width(2.5.dp)
                        .height(height)
                        .clip(RoundedCornerShape(1.dp))
                        .background(if (index < bars) color else color.copy(alpha = 0.22f)),
                )
            }
        }
        Text(
            milliseconds?.let { "$it ms" } ?: "—",
            style = ManisType.data,
            color = color,
            maxLines = 1,
        )
    }
}

/** One segment of the Route Truth Chain. */
@Composable
private fun RouteSegment(label: String, tone: ManisTone, mono: Boolean, dot: Boolean = false) {
    val toneColors = toneColors(tone)
    Row(
        modifier = Modifier
            .height(22.dp)
            .clip(RoundedCornerShape(ManisDimens.radiusBadge))
            .background(toneColors.background)
            .then(
                if (tone == ManisTone.Neutral) {
                    Modifier.border(1.dp, LocalManisColors.current.outlineSubtle, RoundedCornerShape(ManisDimens.radiusBadge))
                } else {
                    Modifier
                },
            )
            .padding(horizontal = ManisDimens.spaceSm),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(5.dp),
    ) {
        if (dot) {
            Box(Modifier.size(6.dp).clip(CircleShape).background(toneColors.foreground))
        }
        Text(
            label,
            style = if (mono) ManisType.data.copy(fontSize = 11.sp) else ManisType.metadata,
            color = toneColors.foreground,
            fontWeight = FontWeight.Medium,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

/**
 * The signature observed route: `[rule] -> [policy/rule group] -> [exit · latency]`.
 *
 * Royal Violet marks the matched rule only. The middle group segment is omitted when no rule
 * group or user policy group applies (never showing internal runtime groups). A direct exit
 * renders neutral `直连`; a rejected route renders the error tone.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun ManisRouteChain(
    rule: String,
    group: String?,
    exit: String?,
    latency: Int?,
    modifier: Modifier = Modifier,
    direct: Boolean = false,
    rejected: Boolean = false,
) {
    val colors = LocalManisColors.current
    FlowRow(
        modifier = modifier,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        RouteSegment(rule, ManisTone.Route, mono = true)
        if (!group.isNullOrBlank()) {
            Text(
                "→",
                modifier = Modifier.align(Alignment.CenterVertically),
                style = ManisType.metadata,
                color = colors.textTertiary,
            )
            RouteSegment(group, ManisTone.Neutral, mono = false)
        }
        when {
            rejected -> {
                Text(
                    "→",
                    modifier = Modifier.align(Alignment.CenterVertically),
                    style = ManisType.metadata,
                    color = colors.textTertiary,
                )
                RouteSegment("REJECT", ManisTone.Error, mono = true)
            }
            direct -> {
                Text(
                    "→",
                    modifier = Modifier.align(Alignment.CenterVertically),
                    style = ManisType.metadata,
                    color = colors.textTertiary,
                )
                RouteSegment("直连", ManisTone.Neutral, mono = false)
            }
            !exit.isNullOrBlank() -> {
                Text(
                    "→",
                    modifier = Modifier.align(Alignment.CenterVertically),
                    style = ManisType.metadata,
                    color = colors.textTertiary,
                )
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    RouteSegment(exit, ManisTone.Success, mono = false, dot = true)
                    if (latency != null) {
                        ManisLatency(latency, Modifier)
                    }
                }
            }
            else -> {
                Text(
                    "→",
                    modifier = Modifier.align(Alignment.CenterVertically),
                    style = ManisType.metadata,
                    color = colors.textTertiary,
                )
                RouteSegment("无路由信息", ManisTone.Neutral, mono = false)
            }
        }
    }
}

/** A subscription quota meter, always paired with its mono usage text by the caller. */
@Composable
fun ManisUsageMeter(fraction: Float, modifier: Modifier = Modifier) {
    val colors = LocalManisColors.current
    Box(
        modifier = modifier
            .height(5.dp)
            .clip(RoundedCornerShape(9999.dp))
            .background(colors.surfaceHigh),
    ) {
        Box(
            Modifier
                .fillMaxWidth(fraction.coerceIn(0f, 1f))
                .fillMaxSize()
                .clip(RoundedCornerShape(9999.dp))
                .background(colors.actionPrimary),
        )
    }
}

/** A page header: 48dp band, optional brand mark, one title, trailing controls. */
@Composable
fun ManisPageHeader(
    title: String,
    modifier: Modifier = Modifier,
    brand: Boolean = false,
    trailing: @Composable RowScope.() -> Unit = {},
) {
    val colors = LocalManisColors.current
    Row(
        modifier = modifier
            .fillMaxWidth()
            .height(ManisDimens.chromeHeight)
            .padding(horizontal = ManisDimens.spaceLg),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(9.dp),
    ) {
        if (brand) {
            Box(
                Modifier
                    .size(26.dp)
                    .clip(RoundedCornerShape(7.dp))
                    .background(colors.textPrimary),
                contentAlignment = Alignment.Center,
            ) {
                Image(
                    painter = painterResource(R.drawable.ic_brand),
                    contentDescription = null,
                    modifier = Modifier.size(18.dp),
                )
            }
        }
        Text(
            title,
            modifier = Modifier.weight(1f),
            style = ManisType.pageTitle.copy(fontSize = 19.sp, fontWeight = FontWeight.Bold),
            color = colors.textPrimary,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        trailing()
    }
}

/** The theme control: a sun while dark (switch to light), a moon while light. 48dp target. */
@Composable
fun ManisThemeToggle(darkTheme: Boolean, onToggle: () -> Unit, modifier: Modifier = Modifier) {
    val colors = LocalManisColors.current
    Box(
        modifier = modifier
            .size(ManisDimens.minTouch)
            .clip(CircleShape)
            .clickable(onClick = onToggle),
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            painter = painterResource(
                if (darkTheme) R.drawable.ic_light_mode else R.drawable.ic_dark_mode,
            ),
            contentDescription = if (darkTheme) "浅色" else "深色",
            modifier = Modifier.size(20.dp),
            tint = colors.textSecondary,
        )
    }
}

/**
 * The precision segmented control used for route mode and filters: a recessed track, an elevated
 * selected thumb, equal widths, one 40dp row.
 */
@Composable
fun ManisSegmented(
    options: List<String>,
    selectedIndex: Int,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    onSelect: (Int) -> Unit,
) {
    val colors = LocalManisColors.current
    val dark = colors.surfaceBase.luminance() < 0.5f
    Row(
        modifier = modifier
            .height(40.dp)
            .clip(RoundedCornerShape(ManisDimens.radiusControl))
            .background(colors.surfaceLow)
            .border(1.dp, colors.outlineSubtle, RoundedCornerShape(ManisDimens.radiusControl))
            .padding(3.dp),
        horizontalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        options.forEachIndexed { index, option ->
            val selected = index == selectedIndex
            Box(
                modifier = Modifier
                    .weight(1f)
                    .fillMaxSize()
                    .clip(RoundedCornerShape(7.dp))
                    .background(
                        if (selected) {
                            if (dark) colors.surfaceHigh else colors.surfaceElevated
                        } else {
                            Color.Transparent
                        },
                    )
                    .clickable(enabled = enabled) { onSelect(index) },
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    option,
                    style = ManisType.label,
                    color = if (selected) colors.textPrimary else colors.textSecondary,
                    fontWeight = if (selected) FontWeight.SemiBold else FontWeight.Medium,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
}

