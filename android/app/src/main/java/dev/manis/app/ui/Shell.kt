package dev.manis.app.ui

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.manis.app.R

/** The window size classes: compact phones get the bottom bar, the wider two get the rail. */
enum class SizeClass { Compact, Medium, Wide }

val LocalSizeClass = compositionLocalOf { SizeClass.Compact }

/** The five primary workspaces, in navigation order. */
enum class PrimaryWorkspace(val label: String, val iconRes: Int) {
    Nodes("节点", R.drawable.ic_globe),
    RoutingRules("规则", R.drawable.ic_map),
    Activity("活动", R.drawable.ic_pie_chart),
    Logs("日志", R.drawable.ic_terminal),
    Configuration("设置", R.drawable.ic_sliders),
}

/** The four workspaces that live above the settings entry in the rail. */
private val railEntries = PrimaryWorkspace.entries - PrimaryWorkspace.Configuration

/**
 * The application shell. The page owns its header, so the shell carries no top chrome: a phone
 * gets the 64dp five-tab bottom bar, and 600dp and wider gets the rail the desktop uses, with the
 * settings entry and the theme control pinned to its foot.
 */
@Composable
fun ManisShell(
    workspace: PrimaryWorkspace,
    onWorkspace: (PrimaryWorkspace) -> Unit,
    darkTheme: Boolean,
    onToggleTheme: () -> Unit,
    content: @Composable () -> Unit,
) {
    val colors = LocalManisColors.current
    BoxWithConstraints(Modifier.fillMaxSize()) {
        val sizeClass = when {
            maxWidth < 600.dp -> SizeClass.Compact
            maxWidth < 840.dp -> SizeClass.Medium
            else -> SizeClass.Wide
        }
        CompositionLocalProvider(LocalSizeClass provides sizeClass) {
            // The sides and the status bar are padded here; the bottom inset belongs to the bar and
            // the rail, whose own fills must reach the screen edge behind the gesture indicator.
            val insets = Modifier.windowInsetsPadding(
                WindowInsets.safeDrawing.only(
                    WindowInsetsSides.Top + WindowInsetsSides.Horizontal,
                ),
            )
            if (sizeClass == SizeClass.Compact) {
                Column(
                    Modifier
                        .fillMaxSize()
                        .background(colors.surfaceBase)
                        .then(insets),
                ) {
                    Box(Modifier.fillMaxWidth().weight(1f)) { content() }
                    BottomTabBar(workspace, onWorkspace)
                }
            } else {
                Row(
                    Modifier
                        .fillMaxSize()
                        .background(colors.surfaceBase)
                        .then(insets),
                ) {
                    NavigationRail(workspace, onWorkspace, sizeClass, darkTheme, onToggleTheme)
                    Box(Modifier.weight(1f).fillMaxHeight()) { content() }
                }
            }
        }
    }
}

/**
 * The phone navigation: 64dp of chrome with five equal tabs. The selected tab is a cobalt glyph in
 * an accent pill with a semibold label; every tab clears the 48dp touch minimum.
 */
@Composable
private fun BottomTabBar(workspace: PrimaryWorkspace, onWorkspace: (PrimaryWorkspace) -> Unit) {
    val colors = LocalManisColors.current
    Column(
        Modifier
            .fillMaxWidth()
            .background(colors.surfaceChrome)
            .navigationBarsPadding(),
    ) {
        HorizontalDivider(thickness = 1.dp, color = colors.outlineSubtle)
        Row(
            Modifier
                .fillMaxWidth()
                .height(ManisDimens.bottomNavigationHeight)
                .padding(horizontal = ManisDimens.spaceSm, vertical = 6.dp),
            horizontalArrangement = Arrangement.spacedBy(2.dp),
        ) {
            for (entry in PrimaryWorkspace.entries) {
                val selected = entry == workspace
                Box(
                    Modifier
                        .weight(1f)
                        .fillMaxHeight()
                        .clip(RoundedCornerShape(ManisDimens.spaceMd))
                        .clickable { onWorkspace(entry) },
                    contentAlignment = Alignment.Center,
                ) {
                    Column(
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.spacedBy(3.dp),
                    ) {
                        Box(
                            Modifier
                                .size(width = 44.dp, height = 26.dp)
                                .clip(CircleShape)
                                .background(
                                    if (selected) colors.actionSoft else Color.Transparent,
                                ),
                            contentAlignment = Alignment.Center,
                        ) {
                            WorkspaceIcon(
                                entry,
                                if (selected) colors.actionPrimary else colors.textTertiary,
                            )
                        }
                        Text(
                            entry.label,
                            style = ManisType.metadata.copy(
                                fontSize = 11.sp,
                                fontWeight = if (selected) {
                                    FontWeight.SemiBold
                                } else {
                                    FontWeight.Medium
                                },
                            ),
                            color = if (selected) colors.actionPrimary else colors.textTertiary,
                            maxLines = 1,
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun WorkspaceIcon(entry: PrimaryWorkspace, tint: Color) {
    Icon(
        painter = painterResource(entry.iconRes),
        contentDescription = entry.label,
        modifier = Modifier.size(22.dp),
        tint = tint,
    )
}

/**
 * The rail from 600dp up: 68dp of icons, or the desktop's 232dp with labels. Workspaces sit under
 * the brand; settings and the theme control close the rail at the bottom, as on the desktop.
 */
@Composable
private fun NavigationRail(
    workspace: PrimaryWorkspace,
    onWorkspace: (PrimaryWorkspace) -> Unit,
    sizeClass: SizeClass,
    darkTheme: Boolean,
    onToggleTheme: () -> Unit,
) {
    val colors = LocalManisColors.current
    val labelled = sizeClass == SizeClass.Wide
    Column(
        Modifier
            .width(if (labelled) ManisDimens.navWide else ManisDimens.navMedium)
            .fillMaxHeight()
            .background(colors.surfaceChrome)
            .navigationBarsPadding()
            .padding(horizontal = ManisDimens.spaceSm, vertical = ManisDimens.spaceSm),
        verticalArrangement = Arrangement.spacedBy(ManisDimens.spaceXs),
        horizontalAlignment = if (labelled) Alignment.Start else Alignment.CenterHorizontally,
    ) {
        Row(
            Modifier.padding(
                start = if (labelled) ManisDimens.spaceSm else 0.dp,
                top = ManisDimens.spaceXs,
                bottom = ManisDimens.spaceSm,
            ),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
        ) {
            Box(
                Modifier
                    .size(32.dp)
                    .clip(RoundedCornerShape(ManisDimens.spaceSm))
                    .background(colors.textPrimary),
                contentAlignment = Alignment.Center,
            ) {
                Image(
                    painter = painterResource(R.drawable.ic_brand),
                    contentDescription = null,
                    modifier = Modifier.size(22.dp),
                )
            }
            if (labelled) {
                Text(
                    "Manis",
                    style = ManisType.sectionTitle,
                    color = colors.textPrimary,
                    maxLines = 1,
                )
            }
        }
        for (entry in railEntries) {
            RailEntry(entry, entry == workspace, labelled) { onWorkspace(entry) }
        }
        Spacer(Modifier.weight(1f))
        RailEntry(
            PrimaryWorkspace.Configuration,
            workspace == PrimaryWorkspace.Configuration,
            labelled,
        ) { onWorkspace(PrimaryWorkspace.Configuration) }
        ManisThemeToggle(
            darkTheme = darkTheme,
            onToggle = onToggleTheme,
            modifier = Modifier.padding(start = if (labelled) ManisDimens.spaceXs else 0.dp),
        )
    }
}

@Composable
private fun RailEntry(
    entry: PrimaryWorkspace,
    selected: Boolean,
    labelled: Boolean,
    onClick: () -> Unit,
) {
    val colors = LocalManisColors.current
    Row(
        Modifier
            .fillMaxWidth()
            .height(ManisDimens.controlStandard)
            .clip(RoundedCornerShape(ManisDimens.radiusControl))
            .background(if (selected) colors.actionSoft else Color.Transparent)
            .clickable(onClick = onClick)
            .padding(horizontal = if (labelled) ManisDimens.spaceMd else 0.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = if (labelled) {
            Arrangement.spacedBy(ManisDimens.spaceMd)
        } else {
            Arrangement.Center
        },
    ) {
        WorkspaceIcon(entry, if (selected) colors.actionPrimary else colors.textTertiary)
        if (labelled) {
            Text(
                entry.label,
                style = ManisType.label,
                color = if (selected) colors.actionPrimary else colors.textPrimary,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}
