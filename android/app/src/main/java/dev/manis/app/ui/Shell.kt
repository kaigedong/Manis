package dev.manis.app.ui

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.Icon
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

/** The five GPUI primary workspaces, in navigation order. */
enum class StatusTone { Ok, Route, Error }

enum class PrimaryWorkspace(val label: String, val iconRes: Int) {
    Nodes("节点", R.drawable.ic_globe),
    RoutingRules("分流规则", R.drawable.ic_map),
    Activity("网络活动", R.drawable.ic_pie_chart),
    Logs("日志", R.drawable.ic_terminal),
    Configuration("配置", 0),
}

/**
 * The GPUI shell: the chrome bar, the workspace, the status bar and the navigation. Phones put
 * the navigation at the bottom, where a thumb reaches it; the wider classes keep the rail.
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
                    .background(colors.windowBackdrop)
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
                    StatusBar(statusText, statusTone, downloadText, uploadText)
                    BottomNavigation(workspace, onWorkspace)
                } else {
                    Row(Modifier.fillMaxWidth().weight(1f)) {
                        Navigation(workspace, onWorkspace, sizeClass)
                        Box(Modifier.weight(1f).fillMaxHeight()) { content() }
                    }
                    StatusBar(statusText, statusTone, downloadText, uploadText)
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
            .background(colors.surfaceChrome)
            .border(width = 1.dp, color = colors.outlineSubtle)
            .padding(horizontal = ManisDimens.spaceLg),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceMd),
    ) {
        Box(Modifier.width(navWidth - ManisDimens.spaceLg), contentAlignment = Alignment.CenterStart) {
            Image(
                painter = painterResource(R.drawable.ic_brand),
                contentDescription = "Manis",
                modifier = Modifier.size(24.dp).clip(RoundedCornerShape(6.dp)),
            )
        }
        Spacer(Modifier.weight(1f))
        ChromeButton(if (darkTheme) "浅色" else "深色", onToggleTheme)
        ChromeButton(proxyLabel, onProxyToggle)
        ChromeButton(routingLabel, onRoutingCycle)
    }
}

@Composable
private fun ChromeButton(label: String, onClick: () -> Unit) {
    val colors = LocalManisColors.current
    Box(
        Modifier
            .height(ManisDimens.controlCompact)
            .clip(RoundedCornerShape(ManisDimens.radiusControl))
            .background(colors.buttonBg)
            .clickable(onClick = onClick)
            .padding(horizontal = ManisDimens.spaceMd),
        contentAlignment = Alignment.Center,
    ) {
        Text(label, style = ManisType.label, color = colors.textPrimary)
    }
}

/** The phone navigation: one row of icon-and-label tabs under the status bar. */
@Composable
private fun BottomNavigation(
    workspace: PrimaryWorkspace,
    onWorkspace: (PrimaryWorkspace) -> Unit,
) {
    val colors = LocalManisColors.current
    Row(
        Modifier
            .fillMaxWidth()
            .height(ManisDimens.bottomNavigationHeight)
            .background(colors.surfaceChrome)
            .border(width = 1.dp, color = colors.outlineSubtle)
            .padding(horizontal = ManisDimens.spaceSm),
    ) {
        for (entry in PrimaryWorkspace.entries) {
            val selected = entry == workspace
            val tint = if (selected) colors.actionPrimary else colors.textSecondary
            Column(
                Modifier
                    .weight(1f)
                    .fillMaxHeight()
                    .clickable { onWorkspace(entry) }
                    .padding(vertical = ManisDimens.spaceXs),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.Center,
            ) {
                Box(
                    Modifier
                        .clip(RoundedCornerShape(ManisDimens.radiusControl))
                        .background(if (selected) colors.actionSoft else Color.Transparent)
                        .padding(horizontal = ManisDimens.spaceSm, vertical = 2.dp),
                ) {
                    if (entry.iconRes != 0) {
                        Icon(
                            painter = painterResource(entry.iconRes),
                            contentDescription = entry.label,
                            modifier = Modifier.size(20.dp),
                            tint = tint,
                        )
                    } else {
                        Icon(
                            imageVector = Icons.Filled.Settings,
                            contentDescription = entry.label,
                            modifier = Modifier.size(20.dp),
                            tint = tint,
                        )
                    }
                }
                Text(
                    entry.label,
                    style = ManisType.metadata,
                    color = tint,
                    maxLines = 1,
                )
            }
        }
    }
}

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
            NavigationItem(entry, entry == workspace, sizeClass == SizeClass.Wide) {
                onWorkspace(entry)
            }
        }
    }
}

@Composable
private fun NavigationItem(
    workspace: PrimaryWorkspace,
    selected: Boolean,
    showLabel: Boolean,
    onClick: () -> Unit,
) {
    val colors = LocalManisColors.current
    val tint = if (selected) colors.actionPrimary else colors.textSecondary
    Row(
        Modifier
            .fillMaxWidth()
            .height(ManisDimens.controlStandard)
            .clip(RoundedCornerShape(ManisDimens.radiusRow))
            .background(if (selected) colors.actionSoft else Color.Transparent)
            .clickable(onClick = onClick)
            .padding(horizontal = if (showLabel) ManisDimens.spaceSm else 0.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = if (showLabel) Arrangement.spacedBy(ManisDimens.spaceSm) else Arrangement.Center,
    ) {
        if (workspace.iconRes != 0) {
            Icon(
                painter = painterResource(workspace.iconRes),
                contentDescription = workspace.label,
                modifier = Modifier.size(18.dp),
                tint = tint,
            )
        } else {
            Icon(
                imageVector = Icons.Filled.Settings,
                contentDescription = workspace.label,
                modifier = Modifier.size(18.dp),
                tint = tint,
            )
        }
        if (showLabel) {
            Text(
                workspace.label,
                style = ManisType.body,
                color = if (selected) colors.actionPrimary else colors.textPrimary,
            )
        }
    }
}

@Composable
private fun StatusBar(text: String, tone: StatusTone, downloadText: String, uploadText: String) {
    val colors = LocalManisColors.current
    Row(
        Modifier
            .fillMaxWidth()
            .height(ManisDimens.statusBarHeight)
            .background(colors.surfaceChrome)
            .border(width = 1.dp, color = colors.outlineSubtle)
            .padding(horizontal = ManisDimens.spaceMd),
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
        Text(downloadText, style = ManisType.data, color = colors.textSecondary)
        Text(uploadText, style = ManisType.data, color = colors.textSecondary)
    }
}

