package dev.manis.app.ui

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.Home
import androidx.compose.material.icons.filled.List
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.Icon
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import dev.manis.app.ManisVpnService
import dev.manis.app.core.ControllerApi
import dev.manis.app.core.CoreController
import dev.manis.app.core.Snapshot
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

enum class Tab(val label: String, val icon: ImageVector) {
    Connect("连接", Icons.Filled.Home),
    Nodes("节点", Icons.Filled.List),
    Subscriptions("订阅", Icons.Filled.Refresh),
    Rules("规则", Icons.Filled.Edit),
    Settings("设置", Icons.Filled.Settings),
}

/** Runtime state polled from the service and the controller. */
object RuntimeState {
    var active by mutableStateOf(false)
    var snapshot by mutableStateOf<Snapshot?>(null)
    var error by mutableStateOf<String?>(null)
    var summary by mutableStateOf("")
    var controllerError by mutableStateOf<String?>(null)
    var warnings by mutableStateOf(listOf<String>())
}

@Composable
fun ManisApp() {
    var tab by rememberSaveable { mutableStateOf(Tab.Connect) }
    val snackbar = remember { SnackbarHostState() }
    val scope = rememberCoroutineScope()

    fun say(message: String) {
        scope.launch { snackbar.showSnackbar(message) }
    }

    LaunchedEffect(Unit) {
        while (true) {
            RuntimeState.active = ManisVpnService.active
            RuntimeState.error = ManisVpnService.lastError ?: CoreController.lastError
            RuntimeState.summary = CoreController.summary
            RuntimeState.warnings = CoreController.warnings
            if (RuntimeState.active) {
                ControllerApi.configure(CoreController.secret)
                runCatching { ControllerApi.snapshot() }
                    .onSuccess {
                        RuntimeState.snapshot = it
                        RuntimeState.controllerError = null
                    }
                    .onFailure {
                        RuntimeState.controllerError = it.message
                        android.util.Log.w("Manis", "controller snapshot failed", it)
                    }
            } else {
                RuntimeState.snapshot = null
            }
            delay(1_500)
        }
    }

    Scaffold(
        snackbarHost = { SnackbarHost(snackbar) },
        bottomBar = {
            NavigationBar {
                for (entry in Tab.entries) {
                    NavigationBarItem(
                        selected = tab == entry,
                        onClick = { tab = entry },
                        icon = { Icon(entry.icon, contentDescription = entry.label) },
                        label = { Text(entry.label) },
                    )
                }
            }
        },
    ) { insets ->
        Box(Modifier.fillMaxSize().padding(insets)) {
            when (tab) {
                Tab.Connect -> ConnectionScreen(onMessage = ::say, onNavigate = { tab = it })
                Tab.Nodes -> ProxiesScreen(onMessage = ::say)
                Tab.Subscriptions -> SubscriptionsScreen(onMessage = ::say)
                Tab.Rules -> RulesScreen(onMessage = ::say)
                Tab.Settings -> SettingsScreen(onMessage = ::say)
            }
        }
    }
}
