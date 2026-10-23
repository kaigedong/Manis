package dev.manis.app.ui

import android.content.Context
import android.content.Intent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import dev.manis.app.ManisVpnService
import dev.manis.app.data.Workspace

fun startVpn(context: Context) {
    context.startForegroundService(
        Intent(context, ManisVpnService::class.java).setAction(ManisVpnService.ACTION_START),
    )
}

fun stopVpn(context: Context) {
    context.startService(
        Intent(context, ManisVpnService::class.java).setAction(ManisVpnService.ACTION_STOP),
    )
}

/** Tells the running service to rebuild the core from the current workspace. */
fun applyWorkspace(context: Context) {
    Workspace.pending.value = false
    context.startService(
        Intent(context, ManisVpnService::class.java).setAction(ManisVpnService.ACTION_RELOAD),
    )
}

@Composable
fun ScreenTitle(title: String, subtitle: String? = null) {
    Column {
        Text(title, style = MaterialTheme.typography.headlineSmall)
        if (subtitle != null) {
            Text(
                subtitle,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
fun SectionCard(
    title: String,
    trailing: (@Composable () -> Unit)? = null,
    content: @Composable () -> Unit,
) {
    Card(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Row(
                Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.SpaceBetween,
            ) {
                Text(title, style = MaterialTheme.typography.titleMedium)
                trailing?.invoke()
            }
            content()
        }
    }
}

@Composable
fun ApplyBar() {
    if (!Workspace.pending.value) return
    val context = androidx.compose.ui.platform.LocalContext.current
    Card(Modifier.fillMaxWidth()) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            Text("有未应用的更改", style = MaterialTheme.typography.bodyMedium)
            Row {
                TextButton(onClick = { Workspace.pending.value = false }) { Text("放弃") }
                Button(onClick = { applyWorkspace(context) }) { Text("应用") }
            }
        }
    }
}
