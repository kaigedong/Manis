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

/** The five GPUI primary workspaces, in navigation order. */
enum class PrimaryWorkspace(val label: String, val iconRes: Int) {
    Nodes("节点", R.drawable.ic_globe),
    RoutingRules("分流规则", R.drawable.ic_map),
    Activity("网络活动", R.drawable.ic_pie_chart),
    Logs("日志", R.drawable.ic_terminal),
    Configuration("配置", 0),
}

/**
 * The GPUI shell: the chrome bar, the compact navigation rail, the workspace and the status bar.
 * Compact (under 900dp) hides the navigation labels, exactly like the desktop window does.
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
    statusOk: Boolean,
    downloadText: String,
    uploadText: String,
    content: @Composable () -> Unit,
) {
    val colors = LocalManisColors.current
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
        )
        Row(Modifier.fillMaxWidth().weight(1f)) {
            Navigation(workspace, onWorkspace)
            Box(Modifier.weight(1f).fillMaxHeight()) { content() }
        }
        StatusBar(statusText, statusOk, downloadText, uploadText)
    }
}

@Composable
private fun Chrome(
    proxyLabel: String,
    onProxyToggle: () -> Unit,
    routingLabel: String,
    onRoutingCycle: () -> Unit,
    darkTheme: Boolean,
    onToggleTheme: () -> Unit,
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
        Box(Modifier.width(ManisDimens.navCompact), contentAlignment = Alignment.CenterStart) {
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

@Composable
private fun Navigation(workspace: PrimaryWorkspace, onWorkspace: (PrimaryWorkspace) -> Unit) {
    val colors = LocalManisColors.current
    Column(
        Modifier
            .width(ManisDimens.navCompact)
            .fillMaxHeight()
            .background(colors.surfaceBase)
            .padding(ManisDimens.spaceSm),
        verticalArrangement = Arrangement.spacedBy(ManisDimens.spaceXs),
    ) {
        for (entry in PrimaryWorkspace.entries) {
            NavigationItem(entry, entry == workspace) { onWorkspace(entry) }
        }
    }
}

@Composable
private fun NavigationItem(
    workspace: PrimaryWorkspace,
    selected: Boolean,
    onClick: () -> Unit,
) {
    val colors = LocalManisColors.current
    val tint = if (selected) colors.actionPrimary else colors.textSecondary
    Box(
        Modifier
            .fillMaxWidth()
            .height(ManisDimens.controlStandard)
            .clip(RoundedCornerShape(ManisDimens.radiusRow))
            .background(if (selected) colors.actionSoft else Color.Transparent)
            .clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
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
    }
}

@Composable
private fun StatusBar(text: String, ok: Boolean, downloadText: String, uploadText: String) {
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
                .background(if (ok) colors.statusSuccess else colors.textTertiary),
        )
        Text(
            text,
            style = ManisType.metadata,
            color = if (ok) colors.textSecondary else colors.textTertiary,
            maxLines = 1,
        )
        Spacer(Modifier.weight(1f))
        Text(downloadText, style = ManisType.data, color = colors.textSecondary)
        Text(uploadText, style = ManisType.data, color = colors.textSecondary)
    }
}

