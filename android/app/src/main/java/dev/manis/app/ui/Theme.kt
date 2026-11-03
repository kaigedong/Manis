package dev.manis.app.ui

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/** The exact palette of the GPUI desktop theme (`crates/manis-ui/src/theme.rs`). */
data class ManisColors(
    val windowBackdrop: Color,
    val surfaceBase: Color,
    val surfaceLow: Color,
    val surfaceHigh: Color,
    val surfaceOverlay: Color,
    val surfaceChrome: Color,
    val textPrimary: Color,
    val textSecondary: Color,
    val textTertiary: Color,
    val outlineSubtle: Color,
    val outlineStrong: Color,
    val actionPrimary: Color,
    val actionOnPrimary: Color,
    val actionSoft: Color,
    val buttonBg: Color,
    val buttonHover: Color,
    val focusRing: Color,
    val routeTrace: Color,
    val routeSoft: Color,
    val statusSuccess: Color,
    val statusWarning: Color,
    val statusError: Color,
    val logInfo: Color,
    val logDebug: Color,
    val logTrace: Color,
)

private val LightManisColors = ManisColors(
    windowBackdrop = Color(0xFFFAFBFC),
    surfaceBase = Color(0xFFFAFBFC),
    surfaceLow = Color(0xFFF0F3F5),
    surfaceHigh = Color(0xFFE9EEF2),
    surfaceOverlay = Color(0xFFFFFFFF),
    surfaceChrome = Color(0xFFE9EEF2),
    textPrimary = Color(0xFF0D0D0D),
    textSecondary = Color(0xFF5D5D5D),
    textTertiary = Color(0xFF666666),
    outlineSubtle = Color(0xFFDDE3E8),
    outlineStrong = Color(0xFF78828C),
    actionPrimary = Color(0xFF176C62),
    actionOnPrimary = Color(0xFFFFFFFF),
    actionSoft = Color(0xFFE3F0ED),
    buttonBg = Color(0xFFEDF1F4),
    buttonHover = Color(0xFFE2E8ED),
    focusRing = Color(0xFF2563EB),
    routeTrace = Color(0xFF923B0F),
    routeSoft = Color(0xFFFFF5F0),
    statusSuccess = Color(0xFF00692A),
    statusWarning = Color(0xFF875500),
    statusError = Color(0xFFB42318),
    logInfo = Color(0xFF1D4ED8),
    logDebug = Color(0xFF7E22CE),
    logTrace = Color(0xFF0E7490),
)

private val DarkManisColors = ManisColors(
    windowBackdrop = Color(0xFF15191E),
    surfaceBase = Color(0xFF15191E),
    surfaceLow = Color(0xFF1C2229),
    surfaceHigh = Color(0xFF242C35),
    surfaceOverlay = Color(0xFF242C35),
    surfaceChrome = Color(0xFF1C2229),
    textPrimary = Color(0xFFF3F3F3),
    textSecondary = Color(0xFFB9B9B9),
    textTertiary = Color(0xFFABABAB),
    outlineSubtle = Color(0xFF35404C),
    outlineStrong = Color(0xFF858F9B),
    actionPrimary = Color(0xFF79D7C6),
    actionOnPrimary = Color(0xFF0D0D0D),
    actionSoft = Color(0xFF203B35),
    buttonBg = Color(0xFF29333E),
    buttonHover = Color(0xFF354251),
    focusRing = Color(0xFF8CB4FF),
    routeTrace = Color(0xFFFFB790),
    routeSoft = Color(0xFF362820),
    statusSuccess = Color(0xFF73D499),
    statusWarning = Color(0xFFF0C36A),
    statusError = Color(0xFFFF9A96),
    logInfo = Color(0xFF93C5FD),
    logDebug = Color(0xFFD8B4FE),
    logTrace = Color(0xFF67E8F9),
)

/** The GPUI text roles (`TextRole`). */
object ManisType {
    val pageTitle = TextStyle(fontSize = 24.sp, lineHeight = 32.sp, fontWeight = FontWeight.SemiBold)
    val sectionTitle = TextStyle(fontSize = 16.sp, lineHeight = 22.sp, fontWeight = FontWeight.SemiBold)
    val body = TextStyle(fontSize = 13.sp, lineHeight = 20.sp)
    val label = TextStyle(fontSize = 12.sp, lineHeight = 16.sp, fontWeight = FontWeight.Medium)
    val data = TextStyle(fontSize = 12.sp, lineHeight = 18.sp, fontWeight = FontWeight.Medium)
    val metadata = TextStyle(fontSize = 11.sp, lineHeight = 16.sp)
}

/** The GPUI spacing, radius and control metrics (`Space`, `Radius`, `ControlSize`, `LayoutMetric`). */
object ManisDimens {
    val spaceXs = 4.dp
    val spaceSm = 8.dp
    val spaceMd = 12.dp
    val spaceLg = 16.dp
    val spaceXl = 24.dp

    val radiusControl = 8.dp
    val radiusRow = 8.dp
    val radiusPane = 12.dp

    val controlCompact = 34.dp
    val controlStandard = 38.dp
    val controlIcon = 32.dp

    /** The application chrome: `ControlSize::Standard.height() + Space::Md`. */
    val chromeHeight = 50.dp
    val statusBarHeight = 28.dp

    /** The navigation widths of the three window size classes. */
    val navCompact = 56.dp
    val navMedium = 66.dp
    val navWide = 220.dp
}

val LocalManisColors = staticCompositionLocalOf { LightManisColors }

private fun ManisColors.toScheme(dark: Boolean) = if (dark) {
    darkColorScheme(
        primary = actionPrimary,
        onPrimary = actionOnPrimary,
        primaryContainer = actionSoft,
        onPrimaryContainer = textPrimary,
        secondary = surfaceHigh,
        onSecondary = textPrimary,
        tertiary = routeTrace,
        onTertiary = actionOnPrimary,
        background = surfaceBase,
        onBackground = textPrimary,
        surface = surfaceOverlay,
        onSurface = textPrimary,
        surfaceVariant = surfaceLow,
        onSurfaceVariant = textSecondary,
        outline = outlineSubtle,
        outlineVariant = outlineSubtle,
        error = statusError,
        onError = actionOnPrimary,
    )
} else {
    lightColorScheme(
        primary = actionPrimary,
        onPrimary = actionOnPrimary,
        primaryContainer = actionSoft,
        onPrimaryContainer = textPrimary,
        secondary = surfaceHigh,
        onSecondary = textPrimary,
        tertiary = routeTrace,
        onTertiary = actionOnPrimary,
        background = surfaceBase,
        onBackground = textPrimary,
        surface = surfaceOverlay,
        onSurface = textPrimary,
        surfaceVariant = surfaceLow,
        onSurfaceVariant = textSecondary,
        outline = outlineSubtle,
        outlineVariant = outlineSubtle,
        error = statusError,
        onError = actionOnPrimary,
    )
}

/** The user's explicit theme choice; null follows the system. */
object UiPrefs {
    var dark = mutableStateOf<Boolean?>(null)
}

/** Applies the GPUI palette and typography to the whole application. */
@Composable
fun ManisTheme(
    darkTheme: Boolean = UiPrefs.dark.value ?: isSystemInDarkTheme(),
    content: @Composable () -> Unit,
) {
    val colors = if (darkTheme) DarkManisColors else LightManisColors
    CompositionLocalProvider(LocalManisColors provides colors) {
        MaterialTheme(
            colorScheme = colors.toScheme(darkTheme),
            content = content,
        )
    }
}
