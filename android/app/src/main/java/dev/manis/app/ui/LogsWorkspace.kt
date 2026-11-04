package dev.manis.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
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
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import java.io.File

/** One parsed kernel log line. */
private data class LogEntry(val time: String, val level: String, val message: String)

/** The logs workspace: the kernel log with search, refresh and copy. */
@Composable
fun LogsWorkspace(onMessage: (String) -> Unit) {
    val colors = LocalManisColors.current
    val context = LocalContext.current
    val clipboard = LocalClipboardManager.current
    var raw by remember { mutableStateOf(emptyList<String>()) }
    var query by remember { mutableStateOf("") }
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

    val filtered = remember(raw, query) {
        val entries = raw.map(::parseLogLine)
        val matched = if (query.isBlank()) {
            entries
        } else {
            entries.filter {
                it.message.contains(query, ignoreCase = true) ||
                    it.level.contains(query, ignoreCase = true)
            }
        }
        // Newest first, like tailing the file: the newest line stays at the top.
        matched.asReversed().take(200)
    }

    Column(Modifier.fillMaxSize().background(colors.surfaceBase)) {
        LazyColumn(
            modifier = Modifier.fillMaxSize(),
            contentPadding = androidx.compose.foundation.layout.PaddingValues(ManisDimens.spaceLg),
            verticalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
        ) {
            item(key = "search") {
                Row(
                    Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Box(Modifier.weight(1f)) {
                        ManisTextField(value = query, onValueChange = { query = it }, placeholder = "搜索日志")
                    }
                    ManisTextAction("刷新") {
                        reloads += 1
                        onMessage("已刷新日志")
                    }
                    ManisTextAction("复制") {
                        clipboard.setText(AnnotatedString(raw.joinToString("\n")))
                        onMessage("日志已复制到剪贴板")
                    }
                }
            }
            item(key = "meta") {
                ManisMeta(
                    when {
                        loading -> "正在读取日志…"
                        else -> "${filtered.size} 条日志 · 最新在最上 · 敏感信息已隐藏"
                    },
                )
            }
            if (filtered.isEmpty()) {
                item(key = "empty") {
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
                itemsIndexed(filtered, key = { index, _ -> index }) { index, entry ->
                    LogLine(entry, showDivider = index != filtered.lastIndex)
                }
            }
        }
    }
}

@Composable
private fun LogLine(entry: LogEntry, showDivider: Boolean) {
    val colors = LocalManisColors.current
    Column(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(ManisDimens.radiusRow))
            .background(colors.surfaceOverlay)
            .padding(horizontal = ManisDimens.spaceLg, vertical = ManisDimens.spaceMd),
    ) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
        ) {
            Text(
                entry.time,
                style = ManisType.metadata.copy(fontFamily = FontFamily.Monospace),
                color = colors.textTertiary,
            )
            LevelBadge(entry.level)
        }
        Text(
            entry.message,
            style = ManisType.metadata.copy(fontFamily = FontFamily.Monospace),
            color = colors.textSecondary,
            modifier = Modifier.padding(top = 2.dp),
        )
        if (showDivider) {
            // Spacing comes from the list arrangement; nothing to draw between rows.
        }
    }
}

@Composable
private fun LevelBadge(level: String) {
    val colors = LocalManisColors.current
    val tone: Color = when (level.lowercase()) {
        "error", "fatal" -> colors.statusError
        "warning", "warn" -> colors.statusWarning
        "debug" -> colors.logDebug
        "trace" -> colors.logTrace
        else -> colors.logInfo
    }
    Box(
        Modifier
            .width(52.dp)
            .clip(RoundedCornerShape(6.dp))
            .background(tone.copy(alpha = 0.14f))
            .padding(vertical = 1.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            level.uppercase(),
            style = ManisType.metadata.copy(fontFamily = FontFamily.Monospace),
            color = tone,
            maxLines = 1,
            overflow = TextOverflow.Clip,
        )
    }
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
