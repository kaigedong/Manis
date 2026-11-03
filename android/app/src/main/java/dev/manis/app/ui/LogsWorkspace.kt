package dev.manis.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.unit.dp
import java.io.File

/** The GPUI logs workspace: the kernel log with search, refresh and copy. */
@Composable
fun LogsWorkspace(onMessage: (String) -> Unit) {
    val colors = LocalManisColors.current
    val context = LocalContext.current
    val clipboard = LocalClipboardManager.current
    var lines by remember { mutableStateOf(readKernelLog(context)) }
    var query by remember { mutableStateOf("") }

    val filtered = remember(lines, query) {
        if (query.isBlank()) {
            lines
        } else {
            lines.filter { it.contains(query, ignoreCase = true) }
        }
    }

    Column(Modifier.fillMaxSize().background(colors.surfaceBase)) {
        Row(
            Modifier
                .fillMaxWidth()
                .padding(horizontal = ManisDimens.spaceMd, vertical = ManisDimens.spaceLg),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
        ) {
            Column(Modifier.weight(1f)) {
                Text("日志", style = ManisType.pageTitle, color = colors.textPrimary)
                ManisMeta("${filtered.size} 行 · 内核日志来自本机 controller")
            }
            ManisButton("刷新", onClick = {
                lines = readKernelLog(context)
                onMessage("已刷新日志")
            })
            ManisButton("复制", primary = true, onClick = {
                clipboard.setText(AnnotatedString(lines.joinToString("\n")))
                onMessage("日志已复制到剪贴板")
            })
        }
        ManisDivider()
        Column(
            Modifier
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(ManisDimens.spaceMd),
            verticalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
        ) {
            ManisTextField(value = query, onValueChange = { query = it }, placeholder = "搜索日志")
            ManisCard {
                if (filtered.isEmpty()) {
                    ManisMeta(if (lines.isEmpty()) "暂无日志：连接一次 VPN 后生成。" else "没有符合当前筛选的结果")
                } else {
                    for (line in filtered.takeLast(400)) {
                        Text(
                            line,
                            style = ManisType.metadata.copy(
                                fontFamily = androidx.compose.ui.text.font.FontFamily.Monospace,
                            ),
                            color = logLineColor(line),
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun logLineColor(line: String): Color {
    val colors = LocalManisColors.current
    return when {
        line.contains("ERROR", true) || line.contains("FATAL", true) -> colors.statusError
        line.contains("WARN", true) -> colors.statusWarning
        line.contains("DEBUG", true) -> colors.logDebug
        line.contains("TRACE", true) -> colors.logTrace
        else -> colors.textSecondary
    }
}

private fun readKernelLog(context: android.content.Context): List<String> {
    val file = File(context.filesDir, "mihomo/core.log")
    if (!file.isFile) return emptyList()
    return runCatching { file.readLines().takeLast(300) }.getOrDefault(listOf("日志读取失败"))
}
