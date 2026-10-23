package dev.manis.app.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Card
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
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
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import dev.manis.app.core.ControllerApi
import dev.manis.app.core.CoreController
import dev.manis.app.core.GroupInfo
import kotlinx.coroutines.launch

@Composable
fun ProxiesScreen(onMessage: (String) -> Unit) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val snapshot = RuntimeState.snapshot
    val active = RuntimeState.active
    var expanded by remember { mutableStateOf(setOf(GLOBAL_GROUP)) }
    var measured by remember { mutableStateOf<Map<String, Int>>(emptyMap()) }
    var testing by remember { mutableStateOf<String?>(null) }

    fun refresh() {
        scope.launch {
            ControllerApi.configure(CoreController.secret)
            runCatching { ControllerApi.snapshot() }.onSuccess { RuntimeState.snapshot = it }
        }
    }

    fun select(group: String, name: String) {
        scope.launch {
            ControllerApi.configure(CoreController.secret)
            runCatching { ControllerApi.select(group, name) }
                .onSuccess {
                    onMessage("已切换到 $name")
                    runCatching { ControllerApi.snapshot() }.onSuccess { RuntimeState.snapshot = it }
                }
                .onFailure { onMessage(it.message ?: "切换失败") }
        }
    }

    fun test(group: GroupInfo) {
        scope.launch {
            testing = group.name
            ControllerApi.configure(CoreController.secret)
            runCatching { ControllerApi.testGroup(group.name) }
                .onSuccess {
                    measured = measured + it
                    onMessage("${group.name} 测速完成")
                }
                .onFailure { onMessage(it.message ?: "测速失败") }
            testing = null
        }
    }

    Column(
        Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(20.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        ScreenTitle("节点", "列表来自运行中的内核，点选立即生效")
        ApplyBar()

        if (!active) {
            Card(Modifier.fillMaxWidth()) {
                Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text("尚未连接", style = MaterialTheme.typography.titleMedium)
                    Text(
                        "连接后这里会显示订阅里的节点、策略组和延迟。",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
            return@Column
        }

        val groups = snapshot?.groups.orEmpty()
        if (groups.isEmpty()) {
            Text("正在读取节点…", color = MaterialTheme.colorScheme.onSurfaceVariant)
        }

        for (group in groups) {
            val open = group.name in expanded
            Card(Modifier.fillMaxWidth()) {
                Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Row(
                        Modifier.fillMaxWidth(),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Column(Modifier.weight(1f)) {
                            Text(
                                group.name,
                                style = MaterialTheme.typography.titleMedium,
                                fontWeight = FontWeight.SemiBold,
                            )
                            Text(
                                "${typeLabel(group.type)} · ${group.all.size} 个节点",
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                        TextButton(
                            onClick = { test(group) },
                            enabled = testing == null && group.all.isNotEmpty(),
                        ) {
                            Text(if (testing == group.name) "测速中…" else "测速")
                        }
                        TextButton(onClick = {
                            expanded = if (open) expanded - group.name else expanded + group.name
                        }) {
                            Text(if (open) "收起" else "展开")
                        }
                    }

                    val current = group.now
                    if (current != null) {
                        Text(
                            "当前：$current",
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.primary,
                        )
                    }

                    if (open) {
                        HorizontalDivider()
                        for (name in group.all) {
                            val delay = measured[name] ?: snapshot?.nodes?.get(name)?.delay
                            Row(
                                Modifier
                                    .fillMaxWidth()
                                    .clickable { select(group.name, name) }
                                    .padding(vertical = 10.dp),
                                verticalAlignment = Alignment.CenterVertically,
                            ) {
                                Text(
                                    name,
                                    Modifier.weight(1f),
                                    style = MaterialTheme.typography.bodyMedium,
                                )
                                if (name == current) {
                                    Text(
                                        "当前",
                                        style = MaterialTheme.typography.labelSmall,
                                        color = MaterialTheme.colorScheme.primary,
                                    )
                                }
                                Text(
                                    delayText(delay),
                                    Modifier.padding(start = 12.dp),
                                    style = MaterialTheme.typography.bodySmall,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                                )
                            }
                        }
                    }
                }
            }
        }

        TextButton(onClick = { refresh() }) { Text("刷新") }
    }
}

private const val GLOBAL_GROUP = "__MANIS_GLOBAL__"

private fun typeLabel(type: String): String = when (type) {
    "Selector" -> "手动选择"
    "URLTest" -> "自动测速"
    "Fallback" -> "故障转移"
    "LoadBalance" -> "负载均衡"
    else -> type
}

internal fun delayText(delay: Int?): String = when {
    delay == null -> "—"
    delay >= 1_000 -> "%.1fs".format(delay / 1_000f)
    else -> "${delay}ms"
}
