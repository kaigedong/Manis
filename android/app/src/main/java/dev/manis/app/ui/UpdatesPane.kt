package dev.manis.app.ui

import android.content.ClipData
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.provider.Settings
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
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
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.core.content.FileProvider
import dev.manis.app.data.UpdateChecker
import dev.manis.app.data.UpdateManager
import dev.manis.app.data.UpdateState
import java.io.File

private const val RELEASE_PAGE = "https://github.com/kaigedong/Manis/releases/tag/latest"

/**
 * The about section: what this build is, the licences it ships under, and the app updater that
 * checks the rolling release, downloads and installs its APK.
 */
@Composable
fun AboutContent(onMessage: (String) -> Unit) {
    val colors = LocalManisColors.current
    val context = LocalContext.current
    val currentVersion = remember { UpdateChecker.installedVersion(context).ifBlank { "未知" } }
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

    Column(
        Modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(ManisDimens.spaceMd),
    ) {
        ManisCard {
            Text("关于", style = ManisType.label, color = colors.textPrimary)
            ManisInfoRow("版本", currentVersion)
            ManisMeta("Manis Android 实验版")
            ManisMeta(
                "Mihomo 内核 v1.19.33 + Android 兼容补丁，随 APK 以 GPL-3.0 分发；" +
                    "补丁与构建脚本见仓库 android/ 目录。",
            )
            ManisMeta("Manis 应用代码为 Apache-2.0。")
        }
        ManisCard {
            Text("应用更新", style = ManisType.label, color = colors.textPrimary)
            ManisInfoRow("当前版本", currentVersion)
            when (val current = state) {
                UpdateState.Idle -> ManisButton("检查更新", onClick = { UpdateManager.check(context) })
                UpdateState.Checking -> ManisMeta("正在检查 GitHub Release…")
                UpdateState.UpToDate -> {
                    ManisMeta("已是最新版本")
                    ManisButton("重新检查", onClick = { UpdateManager.check(context) })
                }
                is UpdateState.Available -> {
                    Text("发现新版本 ${current.update.version}", style = ManisType.body, color = colors.textPrimary)
                    if (current.update.size > 0) {
                        ManisMeta("安装包 ${formatSize(current.update.size)}")
                    }
                    Row(horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceXs)) {
                        ManisButton("下载", primary = true, onClick = {
                            UpdateManager.download(context, current.update)
                        })
                        ManisButton("查看 Release", onClick = {
                            openRelease(context, current.update.releaseUrl, onMessage)
                        })
                    }
                }
                is UpdateState.Downloading -> {
                    Text("正在下载 ${current.update.version}…", style = ManisType.body, color = colors.textPrimary)
                    ManisProgress(current.progress / 100f)
                    ManisMeta("${current.progress}%")
                }
                is UpdateState.Ready -> {
                    Text("新版本 ${current.update.version} 已下载", style = ManisType.body, color = colors.textPrimary)
                    ManisButton("安装", primary = true, onClick = {
                        if (current.file.isFile) {
                            install(current.file)
                        } else {
                            UpdateManager.download(context, current.update)
                            onMessage("更新包已被系统清理，正在重新下载")
                        }
                    })
                }
                is UpdateState.Failed -> {
                    Text(current.message, style = ManisType.metadata, color = colors.statusError)
                    Row(horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceXs)) {
                        ManisButton(if (current.update == null) "重新检查" else "重试下载", onClick = {
                            val update = current.update
                            if (update == null) UpdateManager.check(context) else UpdateManager.download(context, update)
                        })
                        ManisButton("打开 Release 页面", onClick = {
                            openRelease(context, RELEASE_PAGE, onMessage)
                        })
                    }
                }
            }
            ManisMeta("从 GitHub Release 下载官方 APK，校验同名 .sha256 后才会启动系统安装器。")
        }
    }
}

private fun launchInstaller(context: Context, file: File, onMessage: (String) -> Unit) {
    runCatching {
        val uri = FileProvider.getUriForFile(context, "${context.packageName}.fileprovider", file)
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
        context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url)).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
    }.onFailure {
        onMessage("无法打开浏览器：${it.message ?: "未知错误"}")
    }
}

private fun formatSize(bytes: Long): String =
    if (bytes >= 1024 * 1024) "%.1f MB".format(bytes / 1024.0 / 1024.0) else "${bytes / 1024} KB"
