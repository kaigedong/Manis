package dev.manis.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.clickable
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.unit.dp

/** A GPUI pane card: overlay surface, one-pixel outline, 12dp radius. */
@Composable
fun ManisCard(
    modifier: Modifier = Modifier,
    content: @Composable ColumnScope.() -> Unit,
) {
    val colors = LocalManisColors.current
    Column(
        modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(ManisDimens.radiusPane))
            .background(colors.surfaceOverlay)
            .border(1.dp, colors.outlineSubtle, RoundedCornerShape(ManisDimens.radiusPane))
            .padding(ManisDimens.spaceMd),
        verticalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
        content = content,
    )
}

/** A GPUI section heading. */
@Composable
fun ManisSectionTitle(text: String) {
    Text(text, style = ManisType.sectionTitle, color = LocalManisColors.current.textPrimary)
}

/** Secondary metadata text. */
@Composable
fun ManisMeta(text: String, color: Color? = null) {
    Text(
        text,
        style = ManisType.metadata,
        color = color ?: LocalManisColors.current.textTertiary,
    )
}

/** A GPUI control-sized button: 34dp tall, 8dp radius, label type. */
@Composable
fun ManisButton(
    label: String,
    modifier: Modifier = Modifier,
    primary: Boolean = false,
    enabled: Boolean = true,
    onClick: () -> Unit,
) {
    val colors = LocalManisColors.current
    Row(
        modifier
            .height(ManisDimens.controlCompact)
            .clip(RoundedCornerShape(ManisDimens.radiusControl))
            .background(
                when {
                    !enabled -> colors.surfaceLow
                    primary -> colors.actionPrimary
                    else -> colors.buttonBg
                },
            )
            .clickable(enabled = enabled, onClick = onClick)
            .padding(horizontal = ManisDimens.spaceMd),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.Center,
    ) {
        Text(
            label,
            style = ManisType.label,
            color = when {
                !enabled -> colors.textTertiary
                primary -> colors.actionOnPrimary
                else -> colors.textPrimary
            },
        )
    }
}

/** A selectable GPUI list row: title, metadata and an optional trailing text. */
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
            .padding(horizontal = ManisDimens.spaceMd, vertical = ManisDimens.spaceSm),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
    ) {
        leading?.invoke(this)
        Column(Modifier.weight(1f)) {
            Text(
                title,
                style = ManisType.body,
                color = if (selected) colors.actionPrimary else colors.textPrimary,
            )
            if (meta != null) ManisMeta(meta)
        }
        if (trailing != null) {
            Text(trailing, style = ManisType.metadata, color = colors.textTertiary)
        }
    }
}

/** A one-pixel divider between list rows. */
@Composable
fun ManisDivider() {
    Row(
        Modifier
            .fillMaxWidth()
            .height(1.dp)
            .background(LocalManisColors.current.outlineSubtle),
    ) {}
}

/** A GPUI toggle: a 36x20 track with a 16dp thumb. */
@Composable
fun ManisToggle(checked: Boolean, onCheckedChange: (Boolean) -> Unit) {
    val colors = LocalManisColors.current
    Row(
        Modifier
            .height(20.dp)
            .width(36.dp)
            .clip(RoundedCornerShape(10.dp))
            .background(if (checked) colors.actionPrimary else colors.outlineSubtle)
            .clickable { onCheckedChange(!checked) }
            .padding(2.dp),
        horizontalArrangement = if (checked) Arrangement.End else Arrangement.Start,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Row(
            Modifier
                .size(16.dp)
                .clip(RoundedCornerShape(8.dp))
                .background(colors.surfaceOverlay),
        ) {}
    }
}

/** A GPUI chip: 26dp tall, 8dp radius, action-soft when selected. */
@Composable
fun ManisChip(label: String, selected: Boolean, onClick: () -> Unit) {
    val colors = LocalManisColors.current
    Row(
        Modifier
            .height(26.dp)
            .clip(RoundedCornerShape(ManisDimens.radiusControl))
            .background(if (selected) colors.actionSoft else colors.surfaceLow)
            .clickable(onClick = onClick)
            .padding(horizontal = ManisDimens.spaceSm),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            label,
            style = ManisType.metadata,
            color = if (selected) colors.actionPrimary else colors.textSecondary,
        )
    }
}

/** The compact back control used by the drill-in panes. */
@Composable
fun ManisBackButton(onBack: () -> Unit) {
    val colors = LocalManisColors.current
    Row(
        Modifier
            .size(ManisDimens.controlStandard)
            .clip(RoundedCornerShape(ManisDimens.radiusControl))
            .background(colors.buttonBg)
            .clickable(onClick = onBack),
        horizontalArrangement = Arrangement.Center,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text("‹", style = ManisType.sectionTitle, color = colors.textPrimary)
    }
}

/** A single-line GPUI text field. */
@Composable
fun ManisTextField(
    value: String,
    onValueChange: (String) -> Unit,
    placeholder: String = "",
) {
    val colors = LocalManisColors.current
    Box(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(ManisDimens.radiusControl))
            .background(colors.surfaceOverlay)
            .border(1.dp, colors.outlineStrong, RoundedCornerShape(ManisDimens.radiusControl))
            .padding(horizontal = ManisDimens.spaceMd, vertical = ManisDimens.spaceSm),
    ) {
        if (value.isEmpty()) {
            Text(placeholder, style = ManisType.body, color = colors.textTertiary)
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
}

/** A label/value row in a GPUI pane. */
@Composable
fun ManisInfoRow(label: String, value: String) {
    val colors = LocalManisColors.current
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text(label, style = ManisType.metadata, color = colors.textSecondary)
        Spacer(Modifier.weight(1f))
        Text(value, style = ManisType.body, color = colors.textPrimary)
    }
}

/** A borderless text action, as used at the end of GPUI rows. */
@Composable
fun ManisTextAction(label: String, onClick: () -> Unit) {
    val colors = LocalManisColors.current
    Row(
        Modifier
            .clip(RoundedCornerShape(ManisDimens.radiusControl))
            .clickable(onClick = onClick)
            .padding(horizontal = ManisDimens.spaceSm, vertical = ManisDimens.spaceXs),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(label, style = ManisType.metadata, color = colors.actionPrimary)
    }
}

/** A GPUI progress track: 4dp tall, action-primary fill. */
@Composable
fun ManisProgress(fraction: Float) {
    val colors = LocalManisColors.current
    Row(
        Modifier
            .fillMaxWidth()
            .height(4.dp)
            .clip(RoundedCornerShape(2.dp))
            .background(colors.surfaceLow),
    ) {
        Row(
            Modifier
                .fillMaxWidth(fraction.coerceIn(0f, 1f))
                .height(4.dp)
                .background(colors.actionPrimary),
        ) {}
    }
}

/** A GPUI status toast shown above the status bar. */
@Composable
fun ManisToast(message: String, modifier: Modifier = Modifier) {
    val colors = LocalManisColors.current
    Row(
        modifier
            .clip(RoundedCornerShape(ManisDimens.radiusControl))
            .background(colors.surfaceChrome)
            .border(1.dp, colors.outlineSubtle, RoundedCornerShape(ManisDimens.radiusControl))
            .padding(horizontal = ManisDimens.spaceMd, vertical = ManisDimens.spaceSm),
    ) {
        Text(message, style = ManisType.body, color = colors.textPrimary)
    }
}

/** A GPUI dialog: a scrim and a centred pane with a titled header and a footer. */
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
            .background(Color(0x66000000))
            .clickable(onClick = onDismiss),
        contentAlignment = Alignment.Center,
    ) {
        Column(
            Modifier
                .fillMaxWidth()
                .padding(ManisDimens.spaceLg)
                .clip(RoundedCornerShape(ManisDimens.radiusPane))
                .background(colors.surfaceOverlay)
                .border(1.dp, colors.outlineSubtle, RoundedCornerShape(ManisDimens.radiusPane))
                .clickable(
                    interactionSource = remember { MutableInteractionSource() },
                    indication = null,
                ) {},
        ) {
            Column(Modifier.padding(ManisDimens.spaceLg)) {
                Text(title, style = ManisType.sectionTitle, color = colors.textPrimary)
                if (subtitle != null) ManisMeta(subtitle)
            }
            ManisDivider()
            Column(
                Modifier
                    .padding(ManisDimens.spaceLg)
                    .verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
                content = content,
            )
            ManisDivider()
            Row(
                Modifier
                    .fillMaxWidth()
                    .padding(ManisDimens.spaceMd),
                horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceXs, Alignment.End),
                content = actions,
            )
        }
    }
}
