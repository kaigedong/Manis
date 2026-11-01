package dev.manis.app.ui

import android.content.ClipData
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.provider.Settings
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.core.content.FileProvider
import dev.manis.app.data.AvailableUpdate
import dev.manis.app.data.UpdateChecker
import dev.manis.app.data.UpdateManager
import dev.manis.app.data.UpdateState
import java.io.File

private const val RELEASE_PAGE = "https://github.com/kaigedong/Manis/releases/tag/latest"

/**
 * In-app update flow: check the rolling GitHub release, download its APK, then turn the action
 * into an installer button. Nothing is installed until the user taps 安装 and the downloaded file
 * has matched the release checksum.
 */
@Composable
fun UpdateSection(onMessage: (String) -> Unit) {
    val context = LocalContext.current
    val currentVersion = remember {
        UpdateChecker.installedVersion(context).ifBlank { "未知" }
    }
    val state by UpdateManager.state.collectAsState()
    var pendingInstall by remember { mutableStateOf<File?>(null) }

    val installPermission = rememberLauncherForActivityResult(
        ActivityResultContracts.StartActivityForResult(),
    ) {
        val file = pendingInstall
        pendingInstall = null
        if (file == null) return@rememberLauncherForActivityResult
        if (context.packageManager.canRequestPackageInstalls()) {
            launchInstaller(context, file, onMessage)
        } else {
            onMessage("仍未允许安装未知应用")
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

    SectionCard("应用更新") {
        Text(
            "当前版本 $currentVersion",
            style = MaterialTheme.typography.bodyMedium,
        )
        when (val current = state) {
            UpdateState.Idle -> {
                TextButton(onClick = { UpdateManager.check(context) }) { Text("检查更新") }
            }

            UpdateState.Checking -> {
                Row(
                    horizontalArrangement = Arrangement.spacedBy(10.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    CircularProgressIndicator(Modifier.size(18.dp), strokeWidth = 2.dp)
                    Text("正在检查 GitHub Release…")
                }
            }

            UpdateState.UpToDate -> {
                Text("已是最新版本")
                TextButton(onClick = { UpdateManager.check(context) }) { Text("重新检查") }
            }

            is UpdateState.Available -> {
                Text(
                    "发现新版本 ${current.update.version}",
                    style = MaterialTheme.typography.bodyLarge,
                )
                if (current.update.size > 0) {
                    Text(
                        "安装包 ${formatSize(current.update.size)}",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                Row {
                    TextButton(onClick = {
                        UpdateManager.download(context, current.update)
                    }) { Text("下载") }
                    TextButton(onClick = {
                        openRelease(context, current.update.releaseUrl, onMessage)
                    }) { Text("查看 Release") }
                }
            }

            is UpdateState.Downloading -> {
                Text("正在下载 ${current.update.version}…")
                LinearProgressIndicator(
                    progress = { current.progress / 100f },
                    modifier = Modifier.fillMaxWidth(),
                )
                Text(
                    "${current.progress}%",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }

            is UpdateState.Ready -> {
                Text(
                    "新版本 ${current.update.version} 已下载",
                    style = MaterialTheme.typography.bodyLarge,
                )
                TextButton(onClick = {
                    if (current.file.isFile) {
                        install(current.file)
                    } else {
                        UpdateManager.download(context, current.update)
                        onMessage("更新包已被系统清理，正在重新下载")
                    }
                }) { Text("安装") }
            }

            is UpdateState.Failed -> {
                Text(
                    current.message,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.error,
                )
                Row {
                    TextButton(onClick = {
                        val update = current.update
                        if (update == null) {
                            UpdateManager.check(context)
                        } else {
                            UpdateManager.download(context, update)
                        }
                    }) { Text(if (current.update == null) "重新检查" else "重试下载") }
                    TextButton(onClick = {
                        openRelease(context, RELEASE_PAGE, onMessage)
                    }) { Text("打开 Release 页面") }
                }
            }
        }
        Text(
            "从 GitHub Release 下载官方 APK，校验同名 .sha256 后才会启动系统安装器。",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

private fun launchInstaller(context: Context, file: File, onMessage: (String) -> Unit) {
    runCatching {
        val uri = FileProvider.getUriForFile(
            context,
            "${context.packageName}.fileprovider",
            file,
        )
        val intent = Intent(Intent.ACTION_VIEW).apply {
            setDataAndType(uri, "application/vnd.android.package-archive")
            clipData = ClipData.newRawUri("", uri)
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
        }
        context.startActivity(intent)
    }.onFailure {
        onMessage("无法启动安装器：${it.message ?: "未知错误"}")
    }
}

private fun openRelease(context: Context, url: String, onMessage: (String) -> Unit) {
    runCatching {
        context.startActivity(
            Intent(Intent.ACTION_VIEW, Uri.parse(url))
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
        )
    }.onFailure {
        onMessage("无法打开浏览器：${it.message ?: "未知错误"}")
    }
}

private fun formatSize(bytes: Long): String =
    if (bytes >= 1024 * 1024) {
        "%.1f MB".format(bytes / 1024.0 / 1024.0)
    } else {
        "${bytes / 1024} KB"
    }
