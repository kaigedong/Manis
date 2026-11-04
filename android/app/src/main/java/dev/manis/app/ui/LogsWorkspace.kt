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
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
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
import java.io.File

/** One parsed kernel log line. */
private data class LogEntry(val time: String, val level: String, val message: String)

/** The GPUI logs workspace: the kernel log with search, refresh and copy. */
@Composable
fun LogsWorkspace(onMessage: (String) -> Unit) {
    val colors = LocalManisColors.current
    val context = LocalContext.current
    val clipboard = LocalClipboardManager.current
    var raw by remember { mutableStateOf(readKernelLog(context)) }
    var query by remember { mutableStateOf("") }

    val entries = remember(raw) { raw.map(::parseLogLine) }
    val filtered = remember(entries, query) {
        if (query.isBlank()) {
            entries
        } else {
            entries.filter {
                it.message.contains(query, ignoreCase = true) ||
                    it.level.contains(query, ignoreCase = true)
            }
        }
    }

    Column(Modifier.fillMaxSize().background(colors.surfaceBase)) {
        Row(
            Modifier
                .fillMaxWidth()
                .padding(horizontal = ManisDimens.spaceLg, vertical = ManisDimens.spaceLg),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
        ) {
            Column(Modifier.weight(1f)) {
                Text("日志", style = ManisType.pageTitle, color = colors.textPrimary)
                ManisMeta("${filtered.size} 条日志 · 敏感信息已隐藏")
            }
            ManisButton("刷新", onClick = {
                raw = readKernelLog(context)
                onMessage("已刷新日志")
            })
            ManisButton("复制", primary = true, onClick = {
                clipboard.setText(AnnotatedString(raw.joinToString("\n")))
                onMessage("日志已复制到剪贴板")
            })
        }
        ManisDivider()
        Column(
            Modifier
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(ManisDimens.spaceLg),
            verticalArrangement = Arrangement.spacedBy(ManisDimens.spaceMd),
        ) {
            ManisTextField(value = query, onValueChange = { query = it }, placeholder = "搜索日志")
            if (filtered.isEmpty()) {
                ManisCard {
                    ManisMeta(if (raw.isEmpty()) "暂无日志：连接一次 VPN 后生成。" else "没有符合当前筛选的结果")
                }
            } else {
                ManisCard {
                    for ((index, entry) in filtered.takeLast(400).withIndex()) {
                        LogLine(entry)
                        if (index != filtered.takeLast(400).lastIndex) ManisDivider()
                    }
                }
            }
        }
    }
}

@Composable
private fun LogLine(entry: LogEntry) {
    val colors = LocalManisColors.current
    Column(Modifier.fillMaxWidth().padding(vertical = ManisDimens.spaceXs)) {
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
    LogEntry(formatTime(time), level, message)
}.getOrElse { LogEntry("--:--:--", "info", line) }

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
