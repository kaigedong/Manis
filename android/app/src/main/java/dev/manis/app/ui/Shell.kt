package dev.manis.app.ui

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.FilledTonalIconButton
import androidx.compose.material3.Icon
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.NavigationBarItemDefaults
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.dp
import dev.manis.app.R

/** The GPUI window size classes: labels appear only in the wide layout. */
enum class SizeClass { Compact, Medium, Wide }

val LocalSizeClass = androidx.compose.runtime.compositionLocalOf { SizeClass.Compact }

enum class StatusTone { Ok, Route, Error }

/** The five primary workspaces, in navigation order. */
enum class PrimaryWorkspace(val label: String, val iconRes: Int) {
    Nodes("节点", R.drawable.ic_globe),
    RoutingRules("分流规则", R.drawable.ic_map),
    Activity("网络活动", R.drawable.ic_pie_chart),
    Logs("日志", R.drawable.ic_terminal),
    Configuration("配置", 0),
}

/**
 * The application shell: a Material 3 top app bar, the workspace, a status strip and the
 * navigation. Phones put the navigation in a bottom bar; wider classes keep the rail.
 */
@Composable
fun ManisShell(
    workspace: PrimaryWorkspace,
    onWorkspace: (PrimaryWorkspace) -> Unit,
    proxyLabel: String,
    onProxyToggle: () -> Unit,
    routingLabel: String,
    onRoutingCycle: () -> Unit,
    darkTheme: Boolean,
    onToggleTheme: () -> Unit,
    statusText: String,
    statusTone: StatusTone,
    downloadText: String,
    uploadText: String,
    content: @Composable () -> Unit,
) {
    val colors = LocalManisColors.current
    androidx.compose.foundation.layout.BoxWithConstraints(Modifier.fillMaxSize()) {
        val sizeClass = when {
            maxWidth < 900.dp -> SizeClass.Compact
            maxWidth < 1280.dp -> SizeClass.Medium
            else -> SizeClass.Wide
        }
        androidx.compose.runtime.CompositionLocalProvider(LocalSizeClass provides sizeClass) {
            Column(
                Modifier
                    .fillMaxSize()
                    .background(colors.surfaceBase)
                    .windowInsetsPadding(WindowInsets.safeDrawing),
            ) {
                Chrome(
                    proxyLabel = proxyLabel,
                    onProxyToggle = onProxyToggle,
                    routingLabel = routingLabel,
                    onRoutingCycle = onRoutingCycle,
                    darkTheme = darkTheme,
                    onToggleTheme = onToggleTheme,
                    navWidth = navWidth(sizeClass),
                )
                if (sizeClass == SizeClass.Compact) {
                    Box(Modifier.fillMaxWidth().weight(1f)) { content() }
                    StatusStrip(statusText, statusTone, downloadText, uploadText)
                    BottomNavigation(workspace, onWorkspace)
                } else {
                    Row(Modifier.fillMaxWidth().weight(1f)) {
                        Navigation(workspace, onWorkspace, sizeClass)
                        Box(Modifier.weight(1f).fillMaxHeight()) { content() }
                    }
                    StatusStrip(statusText, statusTone, downloadText, uploadText)
                }
            }
        }
    }
}

private fun navWidth(sizeClass: SizeClass) = when (sizeClass) {
    SizeClass.Compact -> ManisDimens.navCompact
    SizeClass.Medium -> ManisDimens.navMedium
    SizeClass.Wide -> ManisDimens.navWide
}

@Composable
private fun Chrome(
    proxyLabel: String,
    onProxyToggle: () -> Unit,
    routingLabel: String,
    onRoutingCycle: () -> Unit,
    darkTheme: Boolean,
    onToggleTheme: () -> Unit,
    navWidth: androidx.compose.ui.unit.Dp,
) {
    val colors = LocalManisColors.current
    Row(
        Modifier
            .fillMaxWidth()
            .height(ManisDimens.chromeHeight)
            .background(colors.surfaceBase)
            .padding(horizontal = ManisDimens.spaceLg),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
    ) {
        Box(Modifier.width(navWidth - ManisDimens.spaceLg), contentAlignment = Alignment.CenterStart) {
            Surface(
                shape = RoundedCornerShape(12.dp),
                color = colors.surfaceHigh,
            ) {
                Image(
                    painter = painterResource(R.drawable.ic_brand),
                    contentDescription = "Manis",
                    modifier = Modifier.size(36.dp).padding(4.dp),
                )
            }
        }
        Spacer(Modifier.weight(1f))
        ThemeToggle(darkTheme, onToggleTheme)
        ChromePill(proxyLabel, onProxyToggle, emphasized = true)
        ChromePill(routingLabel, onRoutingCycle)
    }
}

/** The theme control: a sun while dark (switch to light), a moon while light. */
@Composable
private fun ThemeToggle(darkTheme: Boolean, onToggle: () -> Unit) {
    FilledTonalIconButton(
        onClick = onToggle,
        modifier = Modifier.size(ManisDimens.controlCompact),
        shape = CircleShape,
    ) {
        Icon(
            painter = painterResource(
                if (darkTheme) R.drawable.ic_light_mode else R.drawable.ic_dark_mode,
            ),
            contentDescription = if (darkTheme) "浅色" else "深色",
            modifier = Modifier.size(20.dp),
        )
    }
}

/** A tonal pill for the three chrome controls. */
@Composable
private fun ChromePill(label: String, onClick: () -> Unit, emphasized: Boolean = false) {
    val colors = LocalManisColors.current
    FilledTonalButton(
        onClick = onClick,
        modifier = Modifier.height(ManisDimens.controlCompact),
        shape = CircleShape,
        contentPadding = androidx.compose.foundation.layout.PaddingValues(
            horizontal = ManisDimens.spaceLg,
        ),
    ) {
        if (emphasized) {
            Box(
                Modifier
                    .size(8.dp)
                    .clip(CircleShape)
                    .background(colors.actionPrimary),
            )
            Spacer(Modifier.width(ManisDimens.spaceSm))
        }
        Text(label, style = ManisType.label, maxLines = 1)
    }
}

@Composable
private fun StatusStrip(
    text: String,
    tone: StatusTone,
    downloadText: String,
    uploadText: String,
) {
    val colors = LocalManisColors.current
    Row(
        Modifier
            .fillMaxWidth()
            .height(ManisDimens.statusBarHeight)
            .background(colors.surfaceBase)
            .padding(horizontal = ManisDimens.spaceLg),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
    ) {
        Box(
            Modifier
                .size(8.dp)
                .clip(CircleShape)
                .background(
                    when (tone) {
                        StatusTone.Ok -> colors.statusSuccess
                        StatusTone.Route -> colors.routeTrace
                        StatusTone.Error -> colors.statusError
                    },
                ),
        )
        Text(
            text,
            style = ManisType.metadata,
            color = if (tone == StatusTone.Ok) colors.textSecondary else colors.textTertiary,
            maxLines = 1,
        )
        Spacer(Modifier.weight(1f))
        Text("↓ $downloadText", style = ManisType.metadata, color = colors.textSecondary)
        Text("↑ $uploadText", style = ManisType.metadata, color = colors.textSecondary)
    }
}

/** The phone navigation: the Material 3 bottom bar. */
@Composable
private fun BottomNavigation(
    workspace: PrimaryWorkspace,
    onWorkspace: (PrimaryWorkspace) -> Unit,
) {
    val colors = LocalManisColors.current
    NavigationBar(
        containerColor = colors.surfaceChrome,
        contentColor = colors.textSecondary,
        tonalElevation = 0.dp,
        windowInsets = WindowInsets(0, 0, 0, 0),
    ) {
        for (entry in PrimaryWorkspace.entries) {
            val selected = entry == workspace
            NavigationBarItem(
                selected = selected,
                onClick = { onWorkspace(entry) },
                icon = {
                    WorkspaceIcon(
                        entry,
                        if (selected) colors.actionPrimary else colors.textSecondary,
                    )
                },
                label = {
                    Text(entry.label, style = ManisType.metadata, maxLines = 1)
                },
                colors = NavigationBarItemDefaults.colors(
                    selectedIconColor = colors.actionPrimary,
                    selectedTextColor = colors.actionPrimary,
                    unselectedIconColor = colors.textSecondary,
                    unselectedTextColor = colors.textSecondary,
                    indicatorColor = colors.actionSoft,
                ),
            )
        }
    }
}

@Composable
private fun WorkspaceIcon(entry: PrimaryWorkspace, tint: Color) {
    if (entry.iconRes != 0) {
        Icon(
            painter = painterResource(entry.iconRes),
            contentDescription = entry.label,
            modifier = Modifier.size(22.dp),
            tint = tint,
        )
    } else {
        Icon(
            imageVector = Icons.Filled.Settings,
            contentDescription = entry.label,
            modifier = Modifier.size(22.dp),
            tint = tint,
        )
    }
}

/** The rail used from the medium class up. */
@Composable
private fun Navigation(
    workspace: PrimaryWorkspace,
    onWorkspace: (PrimaryWorkspace) -> Unit,
    sizeClass: SizeClass,
) {
    val colors = LocalManisColors.current
    Column(
        Modifier
            .width(navWidth(sizeClass))
            .fillMaxHeight()
            .background(colors.surfaceBase)
            .padding(ManisDimens.spaceSm),
        verticalArrangement = Arrangement.spacedBy(ManisDimens.spaceXs),
    ) {
        for (entry in PrimaryWorkspace.entries) {
            val selected = entry == workspace
            val wide = sizeClass == SizeClass.Wide
            Surface(
                onClick = { onWorkspace(entry) },
                modifier = Modifier.fillMaxWidth().height(ManisDimens.controlStandard),
                shape = RoundedCornerShape(ManisDimens.radiusControl),
                color = if (selected) colors.actionSoft else Color.Transparent,
            ) {
                Row(
                    Modifier.padding(
                        horizontal = if (wide) ManisDimens.spaceMd else 0.dp,
                    ),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = if (wide) {
                        Arrangement.spacedBy(ManisDimens.spaceMd)
                    } else {
                        Arrangement.Center
                    },
                ) {
                    WorkspaceIcon(
                        entry,
                        if (selected) colors.actionPrimary else colors.textSecondary,
                    )
                    if (wide) {
                        Text(
                            entry.label,
                            style = ManisType.label,
                            color = if (selected) colors.actionPrimary else colors.textPrimary,
                        )
                    }
                }
            }
        }
    }
}
