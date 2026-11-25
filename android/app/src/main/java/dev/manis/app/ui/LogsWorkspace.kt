package dev.manis.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import java.io.File

/** One parsed kernel log line. */
private data class LogEntry(val time: String, val level: String, val message: String)

/** The level filter, in the order the pills are laid out. */
private val LogLevels = listOf("全部", "ERROR", "WARN", "INFO", "DEBUG")

/** The width every level tag holds, so a stack of log lines keeps one text column. */
private val LevelTagWidth = 48.dp

/**
 * The logs workspace: the kernel log as a monospace stream, with a search field, a scrollable row
 * of level pills and a copy action.
 */
@Composable
fun LogsWorkspace(onMessage: (String) -> Unit) {
    val colors = LocalManisColors.current
    val context = LocalContext.current
    val clipboard = LocalClipboardManager.current
    var raw by remember { mutableStateOf(emptyList<String>()) }
    var query by remember { mutableStateOf("") }
    var level by remember { mutableStateOf(LogLevels.first()) }
    var loading by remember { mutableStateOf(true) }
    var reloads by remember { mutableStateOf(0) }

    // The log tails itself: poll the file and only re-read when it actually grew.
    LaunchedEffect(reloads) {
        loading = true
        var stamp = Long.MIN_VALUE
        while (true) {
            val file = File(context.filesDir, "mihomo/core.log")
            val current = if (file.isFile) file.length() xor file.lastModified() else 0L
            if (current != stamp) {
                stamp = current
                raw = withContext(Dispatchers.IO) { readKernelLog(context) }
                loading = false
            }
            delay(2_000)
        }
    }

    val filtered = remember(raw, query, level) {
        val entries = raw.map(::parseLogLine)
        val matched = entries.filter { entry ->
            matchesLevel(entry.level, level) &&
                (query.isBlank() || entry.message.contains(query, ignoreCase = true) ||
                    entry.level.contains(query, ignoreCase = true))
        }
        // Newest first, like tailing the file: the newest line stays at the top.
        matched.asReversed().take(200)
    }

    Column(Modifier.fillMaxSize().background(colors.surfaceBase)) {
        ManisPageHeader(
            title = "内核日志",
            trailing = {
                ManisSectionAction("复制") {
                    clipboard.setText(AnnotatedString(raw.joinToString("\n")))
                    onMessage("日志已复制到剪贴板")
                }
            },
        )
        LazyColumn(
            modifier = Modifier.fillMaxSize(),
            contentPadding = PaddingValues(
                start = ManisDimens.spaceLg,
                end = ManisDimens.spaceLg,
                bottom = ManisDimens.spaceXl,
            ),
            verticalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
        ) {
            item(key = "logs-search") {
                ManisSearchField(
                    value = query,
                    onValueChange = { query = it },
                    placeholder = "筛选内核日志",
                )
            }
            item(key = "logs-levels") {
                // Horizontal: five levels never fit a 320dp phone, and wrapping would hide one.
                Row(
                    Modifier
                        .fillMaxWidth()
                        .horizontalScroll(rememberScrollState()),
                    horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceXs),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    for (option in LogLevels) {
                        ManisChip(
                            label = option,
                            selected = option == level,
                            onClick = { level = option },
                        )
                    }
                }
            }
            item(key = "logs-meta") {
                ManisMeta(
                    when {
                        loading -> "正在读取日志…"
                        else -> "${filtered.size} 条日志 · 最新在最上 · 敏感信息已隐藏"
                    },
                )
            }
            if (filtered.isEmpty()) {
                item(key = "logs-empty") {
                    ManisCard {
                        ManisMeta(
                            when {
                                loading -> "正在读取日志…"
                                raw.isEmpty() -> "暂无日志：连接一次 VPN 后生成。"
                                else -> "没有符合当前筛选的结果"
                            },
                        )
                    }
                }
            } else {
                itemsIndexed(filtered, key = { index, _ -> index }) { _, entry ->
                    LogLine(entry)
                }
            }
        }
    }
}

/** One log line: clock and level on the first line, the body wrapping under them in mono. */
@Composable
private fun LogLine(entry: LogEntry) {
    val colors = LocalManisColors.current
    Column(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(ManisDimens.radiusRow))
            .background(colors.surfaceElevated)
            .padding(horizontal = ManisDimens.spaceMd, vertical = ManisDimens.spaceSm),
        verticalArrangement = Arrangement.spacedBy(3.dp),
    ) {
        Row(
            Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
        ) {
            Text(
                entry.time,
                style = ManisType.data.copy(fontSize = 11.5.sp),
                color = colors.textTertiary,
                maxLines = 1,
            )
            LevelTag(entry.level)
        }
        Text(
            entry.message,
            style = ManisType.data.copy(fontSize = 12.5.sp),
            color = colors.textSecondary,
        )
    }
}

/** The 48dp level tag: soft fill plus its word, so colour is never the only signal. */
@Composable
private fun LevelTag(level: String) {
    val colors = LocalManisColors.current
    val foreground = levelColor(level, colors)
    val background = when (level.lowercase()) {
        "error", "fatal" -> colors.statusErrorSoft
        "warning", "warn" -> colors.statusWarningSoft
        "debug" -> colors.routeSoft
        "trace" -> colors.surfaceLow
        else -> colors.logInfo.copy(alpha = 0.12f)
    }
    Box(
        Modifier
            .width(LevelTagWidth)
            .height(20.dp)
            .clip(RoundedCornerShape(ManisDimens.radiusBadge))
            .background(background),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            level.uppercase().take(5),
            style = ManisType.data.copy(fontSize = 10.5.sp, fontWeight = androidx.compose.ui.text.font.FontWeight.SemiBold),
            color = foreground,
            maxLines = 1,
            overflow = TextOverflow.Clip,
        )
    }
}

/** Does one parsed level belong to the pill the user picked? */
private fun matchesLevel(level: String, filter: String): Boolean = when (filter) {
    LogLevels.first() -> true
    "ERROR" -> level.lowercase() in setOf("error", "fatal")
    "WARN" -> level.lowercase() in setOf("warn", "warning")
    else -> level.equals(filter, ignoreCase = true)
}

private fun levelColor(level: String, colors: ManisColors): Color = when (level.lowercase()) {
    "error", "fatal" -> colors.statusError
    "warning", "warn" -> colors.statusWarning
    "debug" -> colors.logDebug
    "trace" -> colors.logTrace
    else -> colors.logInfo
}

private val LOG_TIME = Regex("time=\"([^\"]*)\"")
private val LOG_LEVEL = Regex("level=\"?([A-Za-z]+)\"?")
private val LOG_MESSAGE = Regex("msg=\"(.*)\"$")

/** Parses the core's `time="…" level=… msg="…"` format, falling back to the raw line. */
private fun parseLogLine(line: String): LogEntry = runCatching {
    val time = LOG_TIME.find(line)?.groupValues?.get(1).orEmpty()
    val level = LOG_LEVEL.find(line)?.groupValues?.get(1) ?: "info"
    val message = LOG_MESSAGE.find(line)?.groupValues?.get(1) ?: line
    LogEntry(formatTime(time), level, displayMessage(message))
}.getOrElse { LogEntry("--:--:--", "info", displayMessage(line)) }

/** Replaces internal identifiers with the labels the rest of the app shows. */
private fun displayMessage(message: String): String =
    message.replace("__MANIS_GLOBAL__", "全局")

private fun formatTime(time: String): String {
    if (time.isBlank()) return "--:--:--"
    val clock = time.substringAfter('T', "")
    return if (clock.length >= 8) clock.take(8) else time.takeLast(8)
}

/** Reads only the tail of the kernel log, so a large file cannot exhaust the heap. */
private fun readKernelLog(context: android.content.Context): List<String> {
    val file = File(context.filesDir, "mihomo/core.log")
    if (!file.isFile) return emptyList()
    return runCatching {
        val maxBytes = 512 * 1024
        val length = file.length()
        val from = (length - maxBytes).coerceAtLeast(0)
        java.io.RandomAccessFile(file, "r").use { handle ->
            handle.seek(from)
            val bytes = ByteArray((length - from).toInt())
            handle.readFully(bytes)
            bytes.toString(Charsets.UTF_8)
                .lineSequence()
                .drop(if (from > 0) 1 else 0)
                .filter { it.isNotBlank() }
                .toList()
                .takeLast(300)
        }
    }.getOrDefault(listOf("日志读取失败"))
}
