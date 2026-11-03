package dev.manis.app.ui

import androidx.compose.foundation.isSystemInDarkTheme
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
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/**
 * The semantic palette, mapped onto Material 3 roles from a teal seed. The names are kept from
 * the desktop port so the workspaces stay untouched, but every value follows the M3 tonal scale.
 */
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
    windowBackdrop = Color(0xFFF5FBF8),
    surfaceBase = Color(0xFFF5FBF8),
    surfaceLow = Color(0xFFEFF5F2),
    surfaceHigh = Color(0xFFE3EAE7),
    surfaceOverlay = Color(0xFFFFFFFF),
    surfaceChrome = Color(0xFFE9EFEC),
    textPrimary = Color(0xFF171D1B),
    textSecondary = Color(0xFF3F4946),
    textTertiary = Color(0xFF6F7976),
    outlineSubtle = Color(0xFFBEC9C4),
    outlineStrong = Color(0xFF6F7976),
    actionPrimary = Color(0xFF006A60),
    actionOnPrimary = Color(0xFFFFFFFF),
    actionSoft = Color(0xFFCCE8E2),
    buttonBg = Color(0xFFE3EAE7),
    buttonHover = Color(0xFFDDE4E1),
    focusRing = Color(0xFF006A60),
    routeTrace = Color(0xFF3E6374),
    routeSoft = Color(0xFFC1E8FB),
    statusSuccess = Color(0xFF2F6E4F),
    statusWarning = Color(0xFF8A5300),
    statusError = Color(0xFFBA1A1A),
    logInfo = Color(0xFF1D4ED8),
    logDebug = Color(0xFF7E22CE),
    logTrace = Color(0xFF0E7490),
)

private val DarkManisColors = ManisColors(
    windowBackdrop = Color(0xFF0E1513),
    surfaceBase = Color(0xFF0E1513),
    surfaceLow = Color(0xFF171D1B),
    surfaceHigh = Color(0xFF252B29),
    surfaceOverlay = Color(0xFF171D1B),
    surfaceChrome = Color(0xFF1B211F),
    textPrimary = Color(0xFFDDE4E0),
    textSecondary = Color(0xFFBEC9C4),
    textTertiary = Color(0xFF88938F),
    outlineSubtle = Color(0xFF3F4946),
    outlineStrong = Color(0xFF88938F),
    actionPrimary = Color(0xFF83D5C8),
    actionOnPrimary = Color(0xFF003731),
    actionSoft = Color(0xFF324B46),
    buttonBg = Color(0xFF252B29),
    buttonHover = Color(0xFF303634),
    focusRing = Color(0xFF83D5C8),
    routeTrace = Color(0xFFA5CCDF),
    routeSoft = Color(0xFF264C5C),
    statusSuccess = Color(0xFF8FD8B0),
    statusWarning = Color(0xFFF2C36B),
    statusError = Color(0xFFFFB4AB),
    logInfo = Color(0xFFA8C7FA),
    logDebug = Color(0xFFD8B4FE),
    logTrace = Color(0xFF67E8F9),
)

/** The Material 3 type scale, expressed with the app's role names. */
object ManisType {
    val pageTitle = TextStyle(fontSize = 26.sp, lineHeight = 32.sp, fontWeight = FontWeight.SemiBold)
    val dialogTitle = TextStyle(fontSize = 20.sp, lineHeight = 26.sp, fontWeight = FontWeight.SemiBold)
    val sectionTitle = TextStyle(fontSize = 16.sp, lineHeight = 24.sp, fontWeight = FontWeight.SemiBold)
    val body = TextStyle(fontSize = 14.sp, lineHeight = 20.sp)
    val label = TextStyle(fontSize = 14.sp, lineHeight = 20.sp, fontWeight = FontWeight.Medium)
    val data = TextStyle(fontSize = 13.sp, lineHeight = 18.sp, fontWeight = FontWeight.Medium)
    val metadata = TextStyle(fontSize = 12.sp, lineHeight = 16.sp)
}

/** Spacing, shape and control metrics on a 4dp grid. */
object ManisDimens {
    val spaceXs = 4.dp
    val spaceSm = 8.dp
    val spaceMd = 12.dp
    val spaceLg = 16.dp
    val spaceXl = 24.dp
    val space2xl = 32.dp

    val radiusControl = 12.dp
    val radiusRow = 12.dp
    val radiusPane = 16.dp

    val controlCompact = 40.dp
    val controlStandard = 48.dp
    val controlIcon = 40.dp

    val chromeHeight = 64.dp
    val statusBarHeight = 44.dp
    val bottomNavigationHeight = 80.dp

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
        onPrimaryContainer = Color(0xFF9EF2E4),
        secondary = Color(0xFFB0CCC6),
        onSecondary = Color(0xFF1B3530),
        secondaryContainer = actionSoft,
        onSecondaryContainer = textPrimary,
        tertiary = routeTrace,
        onTertiary = Color(0xFF093543),
        tertiaryContainer = routeSoft,
        onTertiaryContainer = Color(0xFFC1E8FB),
        error = statusError,
        onError = Color(0xFF690005),
        errorContainer = Color(0xFF93000A),
        onErrorContainer = Color(0xFFFFDAD6),
        background = surfaceBase,
        onBackground = textPrimary,
        surface = surfaceBase,
        onSurface = textPrimary,
        surfaceVariant = Color(0xFF3F4946),
        onSurfaceVariant = textSecondary,
        surfaceContainerLowest = Color(0xFF090F0D),
        surfaceContainerLow = Color(0xFF171D1B),
        surfaceContainer = Color(0xFF1B211F),
        surfaceContainerHigh = Color(0xFF252B29),
        surfaceContainerHighest = Color(0xFF303634),
        outline = outlineStrong,
        outlineVariant = outlineSubtle,
        inverseSurface = Color(0xFFDDE4E0),
        inverseOnSurface = Color(0xFF2B3230),
        inversePrimary = Color(0xFF006A60),
        scrim = Color(0xFF000000),
    )
} else {
    lightColorScheme(
        primary = actionPrimary,
        onPrimary = actionOnPrimary,
        primaryContainer = Color(0xFF9EF2E4),
        onPrimaryContainer = Color(0xFF00201C),
        secondary = Color(0xFF4A635E),
        onSecondary = Color(0xFFFFFFFF),
        secondaryContainer = actionSoft,
        onSecondaryContainer = Color(0xFF05201C),
        tertiary = routeTrace,
        onTertiary = Color(0xFFFFFFFF),
        tertiaryContainer = routeSoft,
        onTertiaryContainer = Color(0xFF001F2A),
        error = statusError,
        onError = Color(0xFFFFFFFF),
        errorContainer = Color(0xFFFFDAD6),
        onErrorContainer = Color(0xFF410002),
        background = surfaceBase,
        onBackground = textPrimary,
        surface = surfaceBase,
        onSurface = textPrimary,
        surfaceVariant = Color(0xFFDAE5E0),
        onSurfaceVariant = textSecondary,
        surfaceContainerLowest = Color(0xFFFFFFFF),
        surfaceContainerLow = Color(0xFFEFF5F2),
        surfaceContainer = Color(0xFFE9EFEC),
        surfaceContainerHigh = Color(0xFFE3EAE7),
        surfaceContainerHighest = Color(0xFFDDE4E1),
        outline = outlineStrong,
        outlineVariant = outlineSubtle,
        inverseSurface = Color(0xFF2B3230),
        inverseOnSurface = Color(0xFFECF2EF),
        inversePrimary = Color(0xFF83D5C8),
        scrim = Color(0xFF000000),
    )
}

private val ManisShapes = Shapes(
    extraSmall = androidx.compose.foundation.shape.RoundedCornerShape(8.dp),
    small = androidx.compose.foundation.shape.RoundedCornerShape(12.dp),
    medium = androidx.compose.foundation.shape.RoundedCornerShape(16.dp),
    large = androidx.compose.foundation.shape.RoundedCornerShape(24.dp),
    extraLarge = androidx.compose.foundation.shape.RoundedCornerShape(28.dp),
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

/** Applies the Material 3 palette, type scale and shapes to the whole application. */
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
