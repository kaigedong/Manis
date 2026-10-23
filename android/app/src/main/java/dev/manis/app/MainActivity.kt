package dev.manis.app

import android.app.Activity
import android.content.Intent
import android.net.VpnService
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.isSystemInDarkTheme
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
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay

class MainActivity : ComponentActivity() {
    private val requestVpnPermission =
        registerForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
            if (result.resultCode == Activity.RESULT_OK) {
                startVpn()
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
        var active by remember { mutableStateOf(ManisVpnService.active) }
        var status by remember { mutableStateOf("未连接") }
        var url by rememberSaveable { mutableStateOf(SubscriptionStore.url(activity) ?: "") }

        LaunchedEffect(Unit) {
            while (true) {
                active = ManisVpnService.active
                status = when {
                    ManisVpnService.lastError != null -> ManisVpnService.lastError!!
                    active -> "已连接"
                    else -> "未连接"
                }
                delay(400)
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
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(20.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.SpaceBetween,
                    ) {
                        Column(Modifier.weight(1f)) {
                            Text("VPN 代理", style = MaterialTheme.typography.titleMedium)
                            Text(
                                status,
                                style = MaterialTheme.typography.bodySmall,
                                color = if (ManisVpnService.lastError != null) {
                                    MaterialTheme.colorScheme.error
                                } else {
                                    MaterialTheme.colorScheme.onSurfaceVariant
                                },
                            )
                        }
                        Switch(
                            checked = active,
                            onCheckedChange = { enabled ->
                                if (enabled) requestVpn() else stopVpn()
                            },
                        )
                    }
                }

                OutlinedTextField(
                    value = url,
                    onValueChange = {
                        url = it
                        SubscriptionStore.save(activity, it)
                    },
                    modifier = Modifier.fillMaxWidth(),
                    label = { Text("订阅地址") },
                    placeholder = { Text("https://example.com/subscription") },
                    singleLine = true,
                )

                val abi = android.os.Build.SUPPORTED_ABIS.firstOrNull() ?: "unknown"
                Text(
                    "设备 ABI：$abi。Mihomo 内核随 APK 打包并以 GPL-3.0 分发；" +
                        "修改订阅后重新开关一次 VPN 生效。",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Spacer(Modifier.height(8.dp))
            }
        }
    }

    private fun requestVpn() {
        val url = SubscriptionStore.url(this)
        if (url == null) {
            return
        }
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
