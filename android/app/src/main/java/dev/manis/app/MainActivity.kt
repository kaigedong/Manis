package dev.manis.app

import android.app.Activity
import android.content.Intent
import android.net.VpnService
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.clickable
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import dev.manis.app.core.ControllerApi
import dev.manis.app.core.CoreController
import dev.manis.app.core.GroupState
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

class MainActivity : ComponentActivity() {
    private var pendingError by mutableStateOf<String?>(null)

    private val requestVpnPermission =
        registerForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
            if (result.resultCode == Activity.RESULT_OK) {
                startVpn()
            } else {
                pendingError = "已取消 VPN 授权"
            }
        }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            val dark = isSystemInDarkTheme()
            MaterialTheme(colorScheme = if (dark) darkColorScheme() else lightColorScheme()) {
                Surface(Modifier.fillMaxSize()) {
                    ManisScreen()
                }
            }
        }
    }

    @Composable
    private fun ManisScreen() {
        val activity = this@MainActivity
        var url by rememberSaveable { mutableStateOf(SubscriptionStore.url(activity) ?: "") }
        var savedUrl by remember { mutableStateOf(SubscriptionStore.url(activity)) }
        var active by remember { mutableStateOf(ManisVpnService.active) }
        var coreError by remember { mutableStateOf<String?>(null) }
        var group by remember { mutableStateOf<GroupState?>(null) }
        var showNodes by remember { mutableStateOf(false) }
        val scope = rememberCoroutineScope()
        val error = pendingError ?: ManisVpnService.lastError ?: coreError
        val saved = url.isNotBlank() && url.trim() == savedUrl

        fun persist() {
            SubscriptionStore.save(activity, url)
            savedUrl = SubscriptionStore.url(activity)
        }

        suspend fun refreshNodes() {
            ControllerApi.configure(CoreController.secret)
            runCatching { ControllerApi.group() }.onSuccess { group = it }
        }

        LaunchedEffect(Unit) {
            var tick = 0
            while (true) {
                active = ManisVpnService.active
                coreError = CoreController.lastError
                if (active) {
                    if (group == null || tick % 4 == 0) {
                        refreshNodes()
                    }
                } else if (group != null) {
                    group = null
                }
                tick += 1
                delay(800)
            }
        }

        Scaffold { insets ->
            Column(
                modifier = Modifier
                    .padding(insets)
                    .padding(horizontal = 24.dp, vertical = 20.dp)
                    .fillMaxSize()
                    .verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                Column {
                    Text("Manis", style = MaterialTheme.typography.headlineLarge)
                    Text(
                        "Android 实验版",
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }

                Card(Modifier.fillMaxWidth()) {
                    Column(Modifier.padding(20.dp)) {
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Column(Modifier.weight(1f)) {
                                Text("VPN 代理", style = MaterialTheme.typography.titleMedium)
                                Text(
                                    statusText(active, group),
                                    style = MaterialTheme.typography.bodySmall,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                                )
                            }
                            Switch(
                                checked = active,
                                onCheckedChange = { enabled ->
                                    pendingError = null
                                    if (enabled) {
                                        if (url.isBlank()) {
                                            pendingError = "先填写订阅地址"
                                        } else {
                                            persist()
                                            prepareVpn()
                                        }
                                    } else {
                                        stopVpn()
                                    }
                                },
                            )
                        }

                        if (active || group != null) {
                            Spacer(Modifier.height(12.dp))
                            HorizontalDivider()
                            Spacer(Modifier.height(4.dp))
                            Row(
                                modifier = Modifier.fillMaxWidth(),
                                verticalAlignment = Alignment.CenterVertically,
                            ) {
                                Text(
                                    "当前节点",
                                    style = MaterialTheme.typography.bodyMedium,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                                )
                                Text(
                                    group?.now ?: "未选择",
                                    modifier = Modifier
                                        .weight(1f)
                                        .padding(horizontal = 12.dp),
                                    style = MaterialTheme.typography.bodyMedium,
                                )
                                TextButton(
                                    onClick = {
                                        scope.launch {
                                            refreshNodes()
                                            showNodes = true
                                        }
                                    },
                                    enabled = group?.all?.isNotEmpty() == true,
                                ) {
                                    Text("选择")
                                }
                            }
                        }
                    }
                }

                if (error != null) {
                    Text(
                        error,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.error,
                    )
                }

                OutlinedTextField(
                    value = url,
                    onValueChange = {
                        url = it
                        pendingError = null
                    },
                    modifier = Modifier.fillMaxWidth(),
                    label = { Text("订阅地址") },
                    placeholder = { Text("https://example.com/subscription") },
                    singleLine = true,
                    keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
                    keyboardActions = KeyboardActions(onDone = { persist() }),
                    supportingText = {
                        Text(
                            when {
                                url.isBlank() -> "粘贴 Clash 格式的订阅链接"
                                saved -> "已保存 · 重新开关 VPN 生效"
                                else -> "有未保存的修改"
                            },
                        )
                    },
                )

                Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    Button(
                        onClick = { persist() },
                        enabled = !saved && url.isNotBlank(),
                    ) {
                        Text(if (saved) "已保存" else "保存订阅")
                    }
                    if (active) {
                        TextButton(onClick = {
                            pendingError = null
                            stopVpn()
                        }) {
                            Text("断开")
                        }
                    }
                }

                val abi = android.os.Build.SUPPORTED_ABIS.firstOrNull() ?: "unknown"
                Text(
                    "设备 ABI：$abi。Mihomo 内核随 APK 打包并以 GPL-3.0 分发。",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }

        if (showNodes) {
            NodeDialog(
                group = group,
                onSelect = { name ->
                    scope.launch {
                        pendingError = null
                        runCatching {
                            ControllerApi.configure(CoreController.secret)
                            ControllerApi.select(name)
                            refreshNodes()
                            showNodes = false
                        }.onFailure {
                            pendingError = it.message ?: "切换节点失败"
                        }
                    }
                },
                onDismiss = { showNodes = false },
            )
        }
    }

    @Composable
    private fun NodeDialog(
        group: GroupState?,
        onSelect: (String) -> Unit,
        onDismiss: () -> Unit,
    ) {
        Dialog(onDismissRequest = onDismiss) {
            Surface(
                shape = MaterialTheme.shapes.large,
                tonalElevation = 6.dp,
            ) {
                Column(
                    Modifier
                        .fillMaxWidth()
                        .heightIn(max = 520.dp)
                        .padding(20.dp),
                ) {
                    Text("选择节点", style = MaterialTheme.typography.titleLarge)
                    Spacer(Modifier.height(8.dp))
                    val nodes = group?.all.orEmpty()
                    if (nodes.isEmpty()) {
                        Text(
                            "暂无节点。等订阅加载完成，或检查订阅地址后重新连接。",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    } else {
                        LazyColumn(Modifier.weight(1f, fill = false)) {
                            items(nodes) { name ->
                                Row(
                                    modifier = Modifier
                                        .fillMaxWidth()
                                        .clickable { onSelect(name) }
                                        .padding(vertical = 12.dp),
                                    verticalAlignment = Alignment.CenterVertically,
                                ) {
                                    Column(Modifier.weight(1f)) {
                                        Text(name, style = MaterialTheme.typography.bodyLarge)
                                        if (name == group?.now) {
                                            Text(
                                                "当前",
                                                style = MaterialTheme.typography.labelSmall,
                                                color = MaterialTheme.colorScheme.primary,
                                            )
                                        }
                                    }
                                    Text(
                                        delayText(group?.delays?.get(name)),
                                        style = MaterialTheme.typography.bodySmall,
                                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                                    )
                                }
                            }
                        }
                    }
                    TextButton(
                        onClick = onDismiss,
                        modifier = Modifier.align(Alignment.End),
                    ) {
                        Text("关闭")
                    }
                }
            }
        }
    }

    private fun statusText(active: Boolean, group: GroupState?): String = when {
        !active -> "未连接"
        group == null -> "已连接 · 正在加载节点"
        group.now != null -> "已连接 · ${group.now}"
        group.all.isEmpty() -> "已连接 · 订阅没有节点"
        else -> "已连接 · 未选择节点"
    }

    private fun delayText(delay: Int?): String = when {
        delay == null -> "—"
        delay >= 1_000 -> "%.1fs".format(delay / 1_000f)
        else -> "${delay}ms"
    }

    private fun prepareVpn() {
        val prepare = VpnService.prepare(this)
        if (prepare != null) {
            requestVpnPermission.launch(prepare)
        } else {
            startVpn()
        }
    }

    private fun startVpn() {
        startForegroundService(
            Intent(this, ManisVpnService::class.java).setAction(ManisVpnService.ACTION_START),
        )
    }

    private fun stopVpn() {
        startService(
            Intent(this, ManisVpnService::class.java).setAction(ManisVpnService.ACTION_STOP),
        )
    }
}
