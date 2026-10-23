package dev.manis.app.ui

import android.app.Activity
import android.net.VpnService
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Card
import androidx.compose.material3.FilterChip
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import dev.manis.app.core.ControllerApi
import dev.manis.app.core.CoreController
import dev.manis.app.data.Workspace
import kotlinx.coroutines.launch

private val modes = listOf("rule" to "规则", "global" to "全局", "direct" to "直连")

@Composable
fun ConnectionScreen(
    onMessage: (String) -> Unit,
    onNavigate: (Tab) -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val config = Workspace.config.value
    val snapshot = RuntimeState.snapshot
    val active = RuntimeState.active
    val global = snapshot?.groups?.firstOrNull { it.name == "__MANIS_GLOBAL__" }
    val node = global?.now
    val mode = snapshot?.mode ?: config.mode
    var localError by remember { mutableStateOf<String?>(null) }

    val permission = rememberLauncherForActivityResult(
        ActivityResultContracts.StartActivityForResult(),
    ) { result ->
        if (result.resultCode == Activity.RESULT_OK) {
            startVpn(context)
        } else {
            localError = "已取消 VPN 授权"
        }
    }

    fun connect() {
        localError = null
        if (config.subscriptions.none { it.enabled } && config.nodes.none { it.enabled }) {
            localError = "先在「订阅」里添加一个订阅或节点"
            return
        }
        val prepare = VpnService.prepare(context)
        if (prepare != null) {
            permission.launch(prepare)
        } else {
            startVpn(context)
        }
    }

    Column(
        Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(20.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        ScreenTitle("连接", "手机与桌面共用同一份规则和渲染器")
        ApplyBar()

        Card(Modifier.fillMaxWidth()) {
            Column(Modifier.padding(20.dp)) {
                Row(
                    Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Column(Modifier.weight(1f)) {
                        Text(
                            if (active) "已连接" else "未连接",
                            style = MaterialTheme.typography.titleLarge,
                        )
                        Text(
                            when {
                                active && node != null -> node
                                active -> "正在加载节点"
                                else -> "打开开关建立 VPN"
                            },
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                    Switch(checked = active, onCheckedChange = { if (it) connect() else stopVpn(context) })
                }

                if (active) {
                    Spacer(Modifier.height(12.dp))
                    HorizontalDivider()
                    Row(
                        Modifier.fillMaxWidth(),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text("出口", color = MaterialTheme.colorScheme.onSurfaceVariant)
                        Text(
                            node ?: "未选择",
                            Modifier.weight(1f).padding(horizontal = 12.dp),
                        )
                        TextButton(onClick = { onNavigate(Tab.Nodes) }) { Text("切换") }
                    }
                }
            }
        }

        Text("路由模式", style = MaterialTheme.typography.titleMedium)
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            for ((value, label) in modes) {
                FilterChip(
                    selected = mode == value,
                    onClick = {
                        scope.launch {
                            if (!active) {
                                Workspace.setMode(value)
                                onMessage("模式已保存，连接后生效")
                                return@launch
                            }
                            ControllerApi.configure(CoreController.secret)
                            runCatching { ControllerApi.setMode(value) }
                                .onSuccess {
                                    Workspace.setMode(value)
                                    onMessage("模式已切换为$label")
                                }
                                .onFailure { onMessage(it.message ?: "切换失败") }
                        }
                    },
                    label = { Text(label) },
                )
            }
        }

        val error = localError ?: RuntimeState.error ?: RuntimeState.controllerError
        if (error != null) {
            Text(error, color = MaterialTheme.colorScheme.error)
        }

        if (RuntimeState.summary.isNotBlank()) {
            Text(
                RuntimeState.summary,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }

        if (RuntimeState.warnings.isNotEmpty()) {
            Card(Modifier.fillMaxWidth()) {
                Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    Text("配置提示", style = MaterialTheme.typography.titleSmall)
                    for (warning in RuntimeState.warnings.take(4)) {
                        Text(
                            warning,
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                    if (RuntimeState.warnings.size > 4) {
                        Text(
                            "以及另外 ${RuntimeState.warnings.size - 4} 条",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
            }
        }

        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            TextButton(onClick = {
                applyWorkspace(context)
                onMessage("已重新加载配置")
            }) { Text("重新载入配置") }
            if (active) {
                TextButton(onClick = { stopVpn(context) }) { Text("断开") }
            }
        }
    }
}
