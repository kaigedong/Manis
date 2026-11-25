package dev.manis.app.ui

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/**
 * Manis UI 2.0 — "Obsidian & Alabaster · Swiss Precision Routing".
 *
 * These values are the Android projection of `design-system/manis/tokens.json`, the single source
 * of truth shared with the desktop GPUI application. The old mint Material 3 palette and the icy
 * blue route colour are gone: the warm alabaster / obsidian neutral ladder, the electric cobalt
 * action accent and the royal violet route accent replace them everywhere.
 */
data class ManisColors(
    val windowBackdrop: Color,
    val surfaceBase: Color,
    val surfaceLow: Color,
    val surfaceHigh: Color,
    val surfaceOverlay: Color,
    val surfaceChrome: Color,
    /** Opaque content cards: policy groups, subscriptions, settings groups. */
    val surfaceElevated: Color,
    val textPrimary: Color,
    val textSecondary: Color,
    val textTertiary: Color,
    val outlineSubtle: Color,
    val outlineStrong: Color,
    val actionPrimary: Color,
    val actionPrimaryHover: Color,
    val actionPrimaryActive: Color,
    val actionOnPrimary: Color,
    val actionSoft: Color,
    val buttonBg: Color,
    val buttonHover: Color,
    val buttonActive: Color,
    val focusRing: Color,
    val routeTrace: Color,
    val routeSoft: Color,
    val statusSuccess: Color,
    val statusSuccessSoft: Color,
    val statusWarning: Color,
    val statusWarningSoft: Color,
    val statusError: Color,
    val statusErrorSoft: Color,
    val dangerFill: Color,
    val dangerHover: Color,
    val dangerOn: Color,
    val logInfo: Color,
    val logDebug: Color,
    val logTrace: Color,
    val scrim: Color,
)

private val LightManisColors = ManisColors(
    windowBackdrop = Color(0xFFF7F6F3),
    surfaceBase = Color(0xFFF7F6F3),
    surfaceLow = Color(0xFFF1EFEA),
    surfaceHigh = Color(0xFFE6E3DC),
    surfaceOverlay = Color(0xFFFFFFFF),
    surfaceChrome = Color(0xFFEFECE6),
    surfaceElevated = Color(0xFFFFFFFF),
    textPrimary = Color(0xFF141413),
    textSecondary = Color(0xFF4E4D49),
    textTertiary = Color(0xFF5E5C57),
    outlineSubtle = Color(0xFFE5E2DC),
    outlineStrong = Color(0xFF8B8881),
    actionPrimary = Color(0xFF2B4EFF),
    actionPrimaryHover = Color(0xFF1F3EE0),
    actionPrimaryActive = Color(0xFF1732BD),
    actionOnPrimary = Color(0xFFFFFFFF),
    actionSoft = Color(0xFFEEF2FF),
    buttonBg = Color(0xFFECEAE4),
    buttonHover = Color(0xFFE2DFD7),
    buttonActive = Color(0xFFD7D3C9),
    focusRing = Color(0xFF2B4EFF),
    routeTrace = Color(0xFF6D28D9),
    routeSoft = Color(0xFFF3EEFF),
    statusSuccess = Color(0xFF0E7A4B),
    statusSuccessSoft = Color(0xFFE8F7F0),
    statusWarning = Color(0xFF9A5504),
    statusWarningSoft = Color(0xFFFEF5E7),
    statusError = Color(0xFFC81E1E),
    statusErrorSoft = Color(0xFFFEF2F2),
    dangerFill = Color(0xFFC81E1E),
    dangerHover = Color(0xFFA61717),
    dangerOn = Color(0xFFFFFFFF),
    logInfo = Color(0xFF1D4ED8),
    logDebug = Color(0xFF6D28D9),
    logTrace = Color(0xFF0F766E),
    scrim = Color(0x59141413),
)

private val DarkManisColors = ManisColors(
    windowBackdrop = Color(0xFF0D0D0F),
    surfaceBase = Color(0xFF0D0D0F),
    surfaceLow = Color(0xFF17171B),
    surfaceHigh = Color(0xFF25252C),
    surfaceOverlay = Color(0xFF212127),
    surfaceChrome = Color(0xFF141417),
    surfaceElevated = Color(0xFF1C1C21),
    textPrimary = Color(0xFFF4F4F6),
    textSecondary = Color(0xFFB4B4BE),
    textTertiary = Color(0xFF9A9AA6),
    outlineSubtle = Color(0xFF2B2B33),
    outlineStrong = Color(0xFF787886),
    actionPrimary = Color(0xFF6B8AFF),
    actionPrimaryHover = Color(0xFF859FFF),
    actionPrimaryActive = Color(0xFFA1B5FF),
    actionOnPrimary = Color(0xFF0A0D18),
    actionSoft = Color(0xFF1B2342),
    buttonBg = Color(0xFF26262E),
    buttonHover = Color(0xFF31313B),
    buttonActive = Color(0xFF3D3D4A),
    focusRing = Color(0xFF859FFF),
    routeTrace = Color(0xFFC4B5FD),
    routeSoft = Color(0xFF271F3D),
    statusSuccess = Color(0xFF4ADE80),
    statusSuccessSoft = Color(0xFF122B20),
    statusWarning = Color(0xFFFBBF24),
    statusWarningSoft = Color(0xFF2E2312),
    statusError = Color(0xFFF87171),
    statusErrorSoft = Color(0xFF331719),
    dangerFill = Color(0xFFC81E1E),
    dangerHover = Color(0xFFA61717),
    dangerOn = Color(0xFFFFFFFF),
    logInfo = Color(0xFF93C5FD),
    logDebug = Color(0xFFD8B4FE),
    logTrace = Color(0xFF5EEAD4),
    scrim = Color(0x80000000),
)

/** The mobile type scale. All measured data uses [ManisType.data], which is monospaced. */
object ManisType {
    val pageTitle = TextStyle(fontSize = 24.sp, lineHeight = 32.sp, fontWeight = FontWeight.SemiBold)
    val displayMetric = TextStyle(fontSize = 26.sp, lineHeight = 32.sp, fontWeight = FontWeight.SemiBold)
    val dialogTitle = TextStyle(fontSize = 20.sp, lineHeight = 26.sp, fontWeight = FontWeight.SemiBold)
    val sectionTitle = TextStyle(fontSize = 16.sp, lineHeight = 22.sp, fontWeight = FontWeight.SemiBold)
    val body = TextStyle(fontSize = 14.5.sp, lineHeight = 21.sp)
    val label = TextStyle(fontSize = 14.sp, lineHeight = 20.sp, fontWeight = FontWeight.Medium)
    val data = TextStyle(
        fontSize = 13.sp,
        lineHeight = 18.sp,
        fontWeight = FontWeight.Medium,
        fontFamily = FontFamily.Monospace,
        fontFeatureSettings = "tnum",
    )
    val metadata = TextStyle(fontSize = 12.sp, lineHeight = 16.sp)
}

/** Spacing, shapes and control metrics on the 4dp grid. */
object ManisDimens {
    val spaceXs = 4.dp
    val spaceSm = 8.dp
    val spaceMd = 12.dp
    val spaceLg = 16.dp
    val spaceXl = 24.dp
    val space2xl = 32.dp

    val radiusBadge = 6.dp
    val radiusControl = 10.dp
    val radiusRow = 10.dp
    val radiusCard = 14.dp
    /** Kept as the historical name used by shared components: same 14dp card step. */
    val radiusPane = radiusCard
    val radiusSheet = 20.dp

    val controlCompact = 40.dp
    val controlStandard = 48.dp
    val controlIcon = 40.dp
    /** WCAG minimum touch target; every interactive row and icon clears it. */
    val minTouch = 48.dp

    /** Section headers and rule rows share one height, so the list keeps a steady step. */
    val rowHeight = 56.dp
    /** The square that holds one icon control, a grip or an order number. */
    val iconTarget = 44.dp
    /** The glyph inside [iconTarget]; the target carries the rest as touch room. */
    val iconGlyph = 20.dp
    /** The switch column: one width for headers and rows, so the two line up. */
    val switchWidth = 52.dp

    val chromeHeight = 48.dp
    val statusBarHeight = 32.dp
    val bottomNavigationHeight = 64.dp

    val navCompact = 56.dp
    val navMedium = 68.dp
    val navWide = 232.dp
}

val LocalManisColors = staticCompositionLocalOf { LightManisColors }

private fun ManisColors.toScheme(dark: Boolean) = if (dark) {
    darkColorScheme(
        primary = actionPrimary,
        onPrimary = actionOnPrimary,
        primaryContainer = actionSoft,
        onPrimaryContainer = textPrimary,
        secondary = textSecondary,
        onSecondary = surfaceBase,
        secondaryContainer = surfaceHigh,
        onSecondaryContainer = textPrimary,
        tertiary = routeTrace,
        onTertiary = Color(0xFF1A1226),
        tertiaryContainer = routeSoft,
        onTertiaryContainer = routeTrace,
        error = statusError,
        onError = Color(0xFF1A0B0B),
        errorContainer = statusErrorSoft,
        onErrorContainer = statusError,
        background = surfaceBase,
        onBackground = textPrimary,
        surface = surfaceBase,
        onSurface = textPrimary,
        surfaceVariant = surfaceHigh,
        onSurfaceVariant = textSecondary,
        surfaceContainerLowest = surfaceBase,
        surfaceContainerLow = surfaceLow,
        surfaceContainer = surfaceChrome,
        surfaceContainerHigh = surfaceElevated,
        surfaceContainerHighest = surfaceHigh,
        outline = outlineStrong,
        outlineVariant = outlineSubtle,
        inverseSurface = textPrimary,
        inverseOnSurface = surfaceBase,
        inversePrimary = Color(0xFF2B4EFF),
        scrim = scrim,
    )
} else {
    lightColorScheme(
        primary = actionPrimary,
        onPrimary = actionOnPrimary,
        primaryContainer = actionSoft,
        onPrimaryContainer = textPrimary,
        secondary = textSecondary,
        onSecondary = surfaceElevated,
        secondaryContainer = surfaceLow,
        onSecondaryContainer = textPrimary,
        tertiary = routeTrace,
        onTertiary = Color(0xFFFFFFFF),
        tertiaryContainer = routeSoft,
        onTertiaryContainer = routeTrace,
        error = statusError,
        onError = Color(0xFFFFFFFF),
        errorContainer = statusErrorSoft,
        onErrorContainer = statusError,
        background = surfaceBase,
        onBackground = textPrimary,
        surface = surfaceBase,
        onSurface = textPrimary,
        surfaceVariant = surfaceLow,
        onSurfaceVariant = textSecondary,
        surfaceContainerLowest = surfaceElevated,
        surfaceContainerLow = surfaceLow,
        surfaceContainer = surfaceChrome,
        surfaceContainerHigh = surfaceHigh,
        surfaceContainerHighest = surfaceHigh,
        outline = outlineStrong,
        outlineVariant = outlineSubtle,
        inverseSurface = textPrimary,
        inverseOnSurface = surfaceBase,
        inversePrimary = Color(0xFF6B8AFF),
        scrim = scrim,
    )
}

private val ManisShapes = Shapes(
    extraSmall = RoundedCornerShape(ManisDimens.radiusBadge),
    small = RoundedCornerShape(ManisDimens.radiusControl),
    medium = RoundedCornerShape(ManisDimens.radiusCard),
    large = RoundedCornerShape(ManisDimens.radiusSheet),
    extraLarge = RoundedCornerShape(ManisDimens.radiusSheet),
)

private val ManisTypography = Typography(
    headlineMedium = ManisType.pageTitle,
    titleLarge = ManisType.dialogTitle,
    titleMedium = ManisType.sectionTitle,
    titleSmall = ManisType.label,
    bodyMedium = ManisType.body,
    bodySmall = ManisType.metadata,
    labelLarge = ManisType.label,
    labelMedium = ManisType.data,
    labelSmall = ManisType.metadata,
)

/** The user's explicit theme choice; null follows the system. */
object UiPrefs {
    private const val STORE = "manis-ui"
    private const val DARK_KEY = "dark"
    private var prefs: android.content.SharedPreferences? = null

    var dark = mutableStateOf<Boolean?>(null)
        private set

    fun attach(context: android.content.Context) {
        if (prefs != null) return
        val store = context.getSharedPreferences(STORE, android.content.Context.MODE_PRIVATE)
        prefs = store
        dark.value = if (store.contains(DARK_KEY)) store.getBoolean(DARK_KEY, false) else null
    }

    fun setDark(value: Boolean?) {
        dark.value = value
        prefs?.edit()?.apply {
            if (value == null) remove(DARK_KEY) else putBoolean(DARK_KEY, value)
        }?.apply()
    }
}

/** Applies the Manis palette, type scale and shapes to the whole application. */
@Composable
fun ManisTheme(
    darkTheme: Boolean = UiPrefs.dark.value ?: isSystemInDarkTheme(),
    content: @Composable () -> Unit,
) {
    val colors = if (darkTheme) DarkManisColors else LightManisColors
    CompositionLocalProvider(LocalManisColors provides colors) {
        MaterialTheme(
            colorScheme = colors.toScheme(darkTheme),
            typography = ManisTypography,
            shapes = ManisShapes,
            content = content,
        )
    }
}
