package dev.manis.app.ui

import androidx.compose.foundation.background
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
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp

/** A Material 3 filled card: tonal surface, 16dp corners, no hairline border. */
@Composable
fun ManisCard(
    modifier: Modifier = Modifier,
    content: @Composable ColumnScope.() -> Unit,
) {
    val colors = LocalManisColors.current
    Surface(
        modifier = modifier.fillMaxWidth(),
        shape = RoundedCornerShape(ManisDimens.radiusPane),
        color = colors.surfaceOverlay,
    ) {
        Column(
            Modifier.padding(ManisDimens.spaceLg),
            verticalArrangement = Arrangement.spacedBy(ManisDimens.spaceMd),
            content = content,
        )
    }
}

@Composable
fun ManisSectionTitle(text: String) {
    Text(text, style = ManisType.sectionTitle, color = LocalManisColors.current.textPrimary)
}

@Composable
fun ManisMeta(text: String, color: Color? = null) {
    Text(
        text,
        style = ManisType.metadata,
        color = color ?: LocalManisColors.current.textTertiary,
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
    val contentPadding = PaddingValues(horizontal = ManisDimens.spaceXl)
    val colors = ButtonDefaults.buttonColors(
        containerColor = MaterialTheme.colorScheme.primary,
        contentColor = MaterialTheme.colorScheme.onPrimary,
    )
    val tonalColors = ButtonDefaults.filledTonalButtonColors(
        containerColor = MaterialTheme.colorScheme.surfaceContainerHigh,
        contentColor = MaterialTheme.colorScheme.onSurface,
    )
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
fun ManisChip(label: String, selected: Boolean, onClick: () -> Unit) {
    val colors = LocalManisColors.current
    FilterChip(
        selected = selected,
        onClick = onClick,
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
fun ManisTextAction(label: String, onClick: () -> Unit) {
    TextButton(
        onClick = onClick,
        shape = CircleShape,
        contentPadding = PaddingValues(horizontal = ManisDimens.spaceMd),
    ) {
        Text(label, style = ManisType.label)
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
                Row(
                    Modifier
                        .fillMaxWidth()
                        .padding(ManisDimens.spaceLg),
                    horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm, Alignment.End),
                    content = actions,
                )
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
