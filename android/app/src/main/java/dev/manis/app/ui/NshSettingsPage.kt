package dev.manis.app.ui

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.provider.Settings
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.compose.foundation.layout.fillMaxSize
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.viewinterop.AndroidView
import com.longbridge.nsh.NshView
import dev.manis.app.data.PROXY_DNS_DOH
import dev.manis.app.data.UpdateManager
import dev.manis.app.data.UpdateState
import dev.manis.app.data.Workspace
import dev.manis.app.nsh.buildSettingsData
import java.io.File
import kotlinx.coroutines.launch
import org.json.JSONArray
import org.json.JSONObject

@Composable
internal fun NshSettingsPage(onMessage: (String) -> Unit, modifier: Modifier = Modifier) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val config = Workspace.config.value
    val updateState by UpdateManager.state.collectAsState()
    val data = remember(config, updateState) {
        buildSettingsData(context, config, updateState)
    }
    var pendingInstall by remember { mutableStateOf<File?>(null) }

    val installPermission = rememberLauncherForActivityResult(
        ActivityResultContracts.StartActivityForResult(),
    ) {
        val file = pendingInstall
        pendingInstall = null
        if (file != null) {
            if (context.packageManager.canRequestPackageInstalls()) {
                launchInstaller(context, file, onMessage)
            } else {
                onMessage("仍未允许安装未知应用")
            }
        }
    }

    fun install(file: File) {
        if (!context.packageManager.canRequestPackageInstalls()) {
            pendingInstall = file
            val intent = Intent(
                Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,
                Uri.parse("package:${context.packageName}"),
            )
            runCatching { installPermission.launch(intent) }.onFailure {
                pendingInstall = null
                onMessage("系统没有开放「安装未知应用」设置页，请手动在系统设置中允许")
            }
        } else {
            launchInstaller(context, file, onMessage)
        }
    }

    AndroidView(
        modifier = modifier.fillMaxSize(),
        factory = { viewContext ->
            NshView(viewContext).apply {
                setErrorListener { onMessage("NSH: $it") }
                setParams(mapOf("page" to "settings", "data" to data, "embedded" to true))
                handle("settings.reload") { _, reply ->
                    scope.launch {
                        applyWorkspace(context)
                        reply.resolve(replyPayload(context, "已重新载入配置"))
                    }
                }
                handle("settings.logs") { _, reply ->
                    reply.resolve(replyPayload(context, logs = readLog(context)))
                }
                handle("settings.copyLogs") { payload, reply ->
                    val values = payload as? Map<*, *>
                    val lines = (values?.get("lines") as? List<*>)
                        ?.mapNotNull { it as? String }
                        .orEmpty()
                    val clipboard = context.getSystemService(ClipboardManager::class.java)
                    clipboard?.setPrimaryClip(ClipData.newPlainText("Manis 内核日志", lines.joinToString("\n")))
                    reply.resolve(replyPayload(context, "日志已复制"))
                }
                handle("settings.copyBackup") { _, reply ->
                    val clipboard = context.getSystemService(ClipboardManager::class.java)
                    clipboard?.setPrimaryClip(
                        ClipData.newPlainText("Manis 配置", Workspace.config.value.toJson()),
                    )
                    reply.resolve(replyPayload(context, "配置已复制到剪贴板"))
                }
                handle("settings.importBackup") { _, reply ->
                    val clipboard = context.getSystemService(ClipboardManager::class.java)
                    val text = clipboard?.primaryClip
                        ?.takeIf { it.itemCount > 0 }
                        ?.getItemAt(0)
                        ?.coerceToText(context)
                        ?.toString()
                        .orEmpty()
                    val result = importWorkspace(context, text)
                    reply.resolve(replyPayload(context, result ?: "已从剪贴板导入并生效"))
                }
                handle("settings.dns.set") { payload, reply ->
                    val values = payload as? Map<*, *>
                    val mode = values?.get("mode") as? String
                    val servers = (values?.get("servers") as? List<*>)
                        ?.mapNotNull { (it as? String)?.trim()?.takeIf(String::isNotEmpty) }
                        .orEmpty()
                    Workspace.update {
                        it.copy(
                            proxyDns = mode ?: PROXY_DNS_DOH,
                            proxyDnsServers = servers,
                        )
                    }
                    scope.launch {
                        applyWorkspace(context)
                        reply.resolve(replyPayload(context, "节点域名解析已更新并重新载入"))
                    }
                }
                handle("settings.update.check") { _, reply ->
                    UpdateManager.check(context)
                    reply.resolve(replyPayload(context, "正在检查更新"))
                }
                handle("settings.update.download") { _, reply ->
                    val state = UpdateManager.state.value
                    val update = when (state) {
                        is UpdateState.Available -> state.update
                        is UpdateState.Failed -> state.update
                        else -> null
                    }
                    if (update == null) {
                        reply.resolve(replyPayload(context, "当前没有可下载的更新"))
                    } else {
                        UpdateManager.download(context, update)
                        reply.resolve(replyPayload(context, "正在下载更新"))
                    }
                }
                handle("settings.update.install") { _, reply ->
                    val state = UpdateManager.state.value
                    if (state is UpdateState.Ready && state.file.isFile) {
                        install(state.file)
                        reply.resolve(replyPayload(context, "正在启动安装器"))
                    } else {
                        reply.resolve(replyPayload(context, "更新包还没有下载完成"))
                    }
                }
                handle("settings.update.release") { _, reply ->
                    val state = UpdateManager.state.value
                    val url = when (state) {
                        is UpdateState.Available -> state.update.releaseUrl
                        is UpdateState.Downloading -> state.update.releaseUrl
                        is UpdateState.Ready -> state.update.releaseUrl
                        is UpdateState.Failed -> state.update?.releaseUrl
                        else -> null
                    } ?: RELEASE_PAGE
                    openRelease(context, url, onMessage)
                    reply.resolve(replyPayload(context, "已打开 Release 页面"))
                }
                if (!loadAssetBundle("nsh")) onMessage("NSH 设置页加载失败")
            }
        },
        update = { view ->
            view.send(
                "settings.updated",
                mapOf(
                    "data" to buildSettingsData(
                        context,
                        Workspace.config.value,
                        UpdateManager.state.value,
                    ),
                ),
            )
        },
        onRelease = { view -> view.destroy() },
    )
}

private fun NshView.replyPayload(
    context: Context,
    message: String? = null,
    logs: List<String>? = null,
): Map<String, Any?> = buildMap {
    put("data", buildSettingsData(context, Workspace.config.value, UpdateManager.state.value))
    if (message != null) put("message", message)
    if (logs != null) put("logs", JSONArray(logs))
}
