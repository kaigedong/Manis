package dev.manis.app.ui

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

/** The semantic colors of the GPUI desktop application (GPUI_IMPLEMENTATION.md). */
data class ManisColors(
    val surfaceBase: Color,
    val surfaceLow: Color,
    val surfaceHigh: Color,
    val surfaceChrome: Color,
    val textPrimary: Color,
    val textSecondary: Color,
    val textTertiary: Color,
    val outlineSubtle: Color,
    val outlineStrong: Color,
    val actionPrimary: Color,
    val onActionPrimary: Color,
    val routeTrace: Color,
    val statusSuccess: Color,
    val statusWarning: Color,
    val statusError: Color,
    val focusRing: Color,
)

private val LightManisColors = ManisColors(
    surfaceBase = Color(0xFFF4F7F5),
    surfaceLow = Color(0xFFEDF2EF),
    surfaceHigh = Color(0xFFFFFFFF),
    surfaceChrome = Color(0xFFE7EEEA),
    textPrimary = Color(0xFF152321),
    textSecondary = Color(0xFF5F6E69),
    textTertiary = Color(0xFF84918D),
    outlineSubtle = Color(0xFFCBD6D2),
    outlineStrong = Color(0xFF9FAFA9),
    actionPrimary = Color(0xFF176C62),
    onActionPrimary = Color(0xFFFFFFFF),
    routeTrace = Color(0xFFD46642),
    statusSuccess = Color(0xFF24795F),
    statusWarning = Color(0xFFA96620),
    statusError = Color(0xFFB54F49),
    focusRing = Color(0xFF0B6FBB),
)

private val DarkManisColors = ManisColors(
    surfaceBase = Color(0xFF0E1715),
    surfaceLow = Color(0xFF111D1A),
    surfaceHigh = Color(0xFF172521),
    surfaceChrome = Color(0xFF13211E),
    textPrimary = Color(0xFFE3EEEA),
    textSecondary = Color(0xFFA4B4AE),
    textTertiary = Color(0xFF7D8E88),
    outlineSubtle = Color(0xFF2B3D37),
    outlineStrong = Color(0xFF435851),
    actionPrimary = Color(0xFF79D7C6),
    onActionPrimary = Color(0xFF082A24),
    routeTrace = Color(0xFFF39B75),
    statusSuccess = Color(0xFF79D7B0),
    statusWarning = Color(0xFFEFB96E),
    statusError = Color(0xFFEF8C84),
    focusRing = Color(0xFF82C8FF),
)

/** The spacing, radii and control sizes of the GPUI desktop application. */
object ManisDimens {
    val spaceXs: Dp = 4.dp
    val spaceSm: Dp = 8.dp
    val spaceMd: Dp = 12.dp
    val spaceLg: Dp = 16.dp
    val spaceXl: Dp = 24.dp
    val spaceXxl: Dp = 32.dp

    val radiusControl: Dp = 8.dp
    val radiusRow: Dp = 8.dp
    val radiusPane: Dp = 12.dp

    val controlCompact: Dp = 34.dp
    val controlStandard: Dp = 38.dp
    val pointerTarget: Dp = 32.dp
    val touchTarget: Dp = 44.dp

    val iconSmall: Dp = 16.dp
    val iconStandard: Dp = 18.dp

    /** The compact window's icon rail (WindowSizeClass::Compact). */
    val compactRail: Dp = 56.dp
}

val LocalManisColors = staticCompositionLocalOf { LightManisColors }

private fun ManisColors.toLightScheme() = lightColorScheme(
    primary = actionPrimary,
    onPrimary = onActionPrimary,
    primaryContainer = surfaceLow,
    onPrimaryContainer = textPrimary,
    secondary = surfaceChrome,
    onSecondary = textPrimary,
    tertiary = routeTrace,
    onTertiary = onActionPrimary,
    background = surfaceBase,
    onBackground = textPrimary,
    surface = surfaceHigh,
    onSurface = textPrimary,
    surfaceVariant = surfaceLow,
    onSurfaceVariant = textSecondary,
    outline = outlineSubtle,
    outlineVariant = outlineSubtle,
    error = statusError,
    onError = onActionPrimary,
)

private fun ManisColors.toDarkScheme() = darkColorScheme(
    primary = actionPrimary,
    onPrimary = onActionPrimary,
    primaryContainer = surfaceLow,
    onPrimaryContainer = textPrimary,
    secondary = surfaceChrome,
    onSecondary = textPrimary,
    tertiary = routeTrace,
    onTertiary = onActionPrimary,
    background = surfaceBase,
    onBackground = textPrimary,
    surface = surfaceHigh,
    onSurface = textPrimary,
    surfaceVariant = surfaceLow,
    onSurfaceVariant = textSecondary,
    outline = outlineSubtle,
    outlineVariant = outlineSubtle,
    error = statusError,
    onError = onActionPrimary,
)

/** Applies the GPUI palette to the whole application. */
@Composable
fun ManisTheme(
    darkTheme: Boolean = isSystemInDarkTheme(),
    content: @Composable () -> Unit,
) {
    val colors = if (darkTheme) DarkManisColors else LightManisColors
    CompositionLocalProvider(LocalManisColors provides colors) {
        MaterialTheme(
            colorScheme = if (darkTheme) colors.toDarkScheme() else colors.toLightScheme(),
            content = content,
        )
    }
}
