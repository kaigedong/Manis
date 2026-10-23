package dev.manis.app.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import dev.manis.app.core.ControllerApi
import dev.manis.app.core.CoreController
import dev.manis.app.data.Workspace
import kotlinx.coroutines.launch

private val modes = listOf("rule" to "规则", "global" to "全局", "direct" to "直连")

private const val SAMPLE_RULES = """# Quantumult X 风格，策略可以是 DIRECT、REJECT 或任意策略组名
HOST-SUFFIX,cn,DIRECT
HOST-SUFFIX,baidu.com,DIRECT
HOST-KEYWORD,ads,REJECT
HOST-SUFFIX,example.com,DIRECT"""

@Composable
fun RulesScreen(onMessage: (String) -> Unit) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val config = Workspace.config.value
    var text by remember(config.rules) { mutableStateOf(config.rules) }
    val mode = RuntimeState.snapshot?.mode ?: config.mode

    Column(
        Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(20.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        ScreenTitle("规则", "与桌面版同一套 QX 规则解析和策略映射")
        ApplyBar()

        SectionCard("路由模式") {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                for ((value, label) in modes) {
                    FilterChip(
                        selected = mode == value,
                        onClick = {
                            scope.launch {
                                if (RuntimeState.active) {
                                    ControllerApi.configure(CoreController.secret)
                                    runCatching { ControllerApi.setMode(value) }
                                        .onSuccess {
                                            Workspace.setMode(value)
                                            onMessage("已切换为$label")
                                        }
                                        .onFailure { onMessage(it.message ?: "切换失败") }
                                } else {
                                    Workspace.setMode(value)
                                    onMessage("已保存，连接后生效")
                                }
                            }
                        },
                        label = { Text(label) },
                    )
                }
            }
            Text(
                "规则模式按下方规则分流；全局模式全部走当前节点；直连模式全部直连。",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }

        SectionCard(
            "QX 规则",
            trailing = {
                Row {
                    TextButton(onClick = { text = SAMPLE_RULES }) { Text("示例") }
                    TextButton(onClick = {
                        Workspace.update { it.copy(rules = text) }
                        onMessage("规则已保存，点「应用」生效")
                    }) { Text("保存") }
                }
            },
        ) {
            OutlinedTextField(
                value = text,
                onValueChange = { text = it },
                modifier = Modifier.fillMaxWidth(),
                minLines = 10,
                maxLines = 20,
                textStyle = MaterialTheme.typography.bodySmall.copy(fontFamily = FontFamily.Monospace),
                placeholder = { Text("HOST-SUFFIX,cn,DIRECT") },
            )
            Text(
                "支持 DOMAIN、DOMAIN-SUFFIX、DOMAIN-KEYWORD（HOST- 前缀等价）；" +
                    "未匹配的流量自动走全局组。",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }

        if (RuntimeState.warnings.isNotEmpty()) {
            SectionCard("解析提示") {
                for (warning in RuntimeState.warnings.take(8)) {
                    Text(
                        warning,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        }

        Text(
            RuntimeState.summary.ifBlank { "尚未连接，应用后可在「连接」页查看摘要" },
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}
