package dev.manis.app.ui

import android.content.ClipData
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.provider.Settings
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.FileProvider
import dev.manis.app.R
import dev.manis.app.data.UpdateChecker
import dev.manis.app.data.UpdateManager
import dev.manis.app.data.UpdateState
import java.io.File

private const val RELEASE_PAGE = "https://github.com/kaigedong/Manis/releases/tag/latest"

/**
 * The updates subpage: checks GitHub Release, downloads the APK with live progress, verifies
 * SHA-256 and launches the Android package installer.
 */
@Composable
fun UpdatesContent(onMessage: (String) -> Unit) {
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
        verticalArrangement = Arrangement.spacedBy(20.dp),
    ) {
        SettingsGroup(
            header = "版本与更新",
            footer = "从 GitHub Release 下载官方 APK，校验同名 .sha256 后才会启动系统安装器。",
        ) {
            SettingsValueRow("当前版本", value = currentVersion)
            SettingsInsetDivider(startIndent = ManisDimens.spaceLg)
            when (val current = state) {
                UpdateState.Idle -> {
                    SettingsValueRow("状态", value = "尚未检查")
                }
                UpdateState.Checking -> {
                    SettingsValueRow("状态", value = "正在检查…", valueColor = colors.actionPrimary)
                }
                UpdateState.UpToDate -> {
                    SettingsValueRow("状态", value = "已是最新版本", valueColor = colors.statusSuccess)
                }
                is UpdateState.Available -> {
                    SettingsValueRow("可用版本", value = current.update.version, valueColor = colors.actionPrimary)
                    if (current.update.size > 0) {
                        SettingsInsetDivider(startIndent = ManisDimens.spaceLg)
                        SettingsValueRow("安装包大小", value = formatSize(current.update.size))
                    }
                }
                is UpdateState.Downloading -> {
                    SettingsValueRow(
                        "正在下载",
                        value = "${current.update.version} · ${current.progress}%",
                        valueColor = colors.actionPrimary,
                    )
                    Column(
                        Modifier
                            .fillMaxWidth()
                            .padding(
                                start = ManisDimens.spaceLg,
                                end = ManisDimens.spaceLg,
                                bottom = ManisDimens.spaceLg,
                            ),
                    ) {
                        ManisProgress(current.progress / 100f)
                    }
                }
                is UpdateState.Ready -> {
                    SettingsValueRow("已下载", value = current.update.version, valueColor = colors.statusSuccess)
                }
                is UpdateState.Failed -> {
                    Column(
                        Modifier
                            .fillMaxWidth()
                            .padding(horizontal = ManisDimens.spaceLg, vertical = 12.dp),
                        verticalArrangement = Arrangement.spacedBy(4.dp),
                    ) {
                        Text(
                            "检查或下载失败",
                            style = ManisType.body.copy(fontSize = 15.sp),
                            color = colors.statusError,
                        )
                        Text(
                            current.message,
                            style = ManisType.metadata,
                            color = colors.textTertiary,
                        )
                    }
                }
            }
        }

        SettingsGroup(header = "操作") {
            when (val current = state) {
                UpdateState.Idle -> {
                    UpdateActionRow(
                        label = "检查更新",
                        accent = true,
                        onClick = { UpdateManager.check(context) },
                    )
                }
                UpdateState.Checking -> {
                    UpdateActionRow(
                        label = "正在检查 GitHub Release…",
                        accent = false,
                        enabled = false,
                        onClick = {},
                    )
                }
                UpdateState.UpToDate -> {
                    UpdateActionRow(
                        label = "重新检查",
                        accent = true,
                        onClick = { UpdateManager.check(context) },
                    )
                }
                is UpdateState.Available -> {
                    UpdateActionRow(
                        label = "下载更新（${current.update.version}）",
                        accent = true,
                        onClick = { UpdateManager.download(context, current.update) },
                    )
                    SettingsInsetDivider(startIndent = ManisDimens.spaceLg)
                    UpdateActionRow(
                        label = "查看 Release 页面",
                        accent = false,
                        onClick = { openRelease(context, current.update.releaseUrl, onMessage) },
                    )
                }
                is UpdateState.Downloading -> {
                    UpdateActionRow(
                        label = "下载中 ${current.progress}%",
                        accent = false,
                        enabled = false,
                        onClick = {},
                    )
                }
                is UpdateState.Ready -> {
                    UpdateActionRow(
                        label = "安装更新",
                        accent = true,
                        onClick = {
                            if (current.file.isFile) {
                                install(current.file)
                            } else {
                                UpdateManager.download(context, current.update)
                                onMessage("更新包已被系统清理，正在重新下载")
                            }
                        },
                    )
                }
                is UpdateState.Failed -> {
                    UpdateActionRow(
                        label = if (current.update == null) "重新检查" else "重试下载",
                        accent = true,
                        onClick = {
                            val update = current.update
                            if (update == null) {
                                UpdateManager.check(context)
                            } else {
                                UpdateManager.download(context, update)
                            }
                        },
                    )
                    SettingsInsetDivider(startIndent = ManisDimens.spaceLg)
                    UpdateActionRow(
                        label = "打开 Release 页面",
                        accent = false,
                        onClick = { openRelease(context, RELEASE_PAGE, onMessage) },
                    )
                }
            }
        }
    }
}

/** Compact sheet content for "关于 Manis". */
@Composable
internal fun AboutSheetContent(onMessage: (String) -> Unit) {
    val colors = LocalManisColors.current
    val context = LocalContext.current
    val currentVersion = remember { UpdateChecker.installedVersion(context).ifBlank { "未知" } }

    SettingsInsetCard {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = ManisDimens.spaceLg, vertical = 16.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            Box(
                modifier = Modifier
                    .size(44.dp)
                    .clip(RoundedCornerShape(12.dp))
                    .background(colors.textPrimary),
                contentAlignment = Alignment.Center,
            ) {
                Image(
                    painter = painterResource(R.drawable.ic_brand),
                    contentDescription = null,
                    modifier = Modifier.size(30.dp),
                )
            }
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(
                    "Manis",
                    style = ManisType.sectionTitle.copy(fontSize = 16.5.sp),
                    color = colors.textPrimary,
                )
                ManisMeta("Android 实验版 · $currentVersion")
            }
        }
        SettingsInsetDivider(startIndent = ManisDimens.spaceLg)
        SettingsValueRow("内核", value = "Mihomo v1.19.33")
        SettingsInsetDivider(startIndent = ManisDimens.spaceLg)
        SettingsValueRow("应用协议", value = "Apache-2.0")
        SettingsInsetDivider(startIndent = ManisDimens.spaceLg)
        SettingsValueRow("内核协议", value = "GPL-3.0")
        SettingsInsetDivider(startIndent = ManisDimens.spaceLg)
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .heightIn(min = 54.dp)
                .clickable { openRelease(context, RELEASE_PAGE, onMessage) }
                .padding(horizontal = ManisDimens.spaceLg),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceMd),
        ) {
            Text(
                "查看 GitHub Release",
                modifier = Modifier.weight(1f),
                style = ManisType.body.copy(fontSize = 15.sp, fontWeight = FontWeight.Medium),
                color = colors.actionPrimary,
            )
            Icon(
                painter = painterResource(R.drawable.ic_chevron_right),
                contentDescription = null,
                modifier = Modifier.size(16.dp),
                tint = colors.textTertiary.copy(alpha = 0.6f),
            )
        }
    }
    Text(
        "Mihomo 内核含 Android 兼容补丁，随 APK 以 GPL-3.0 分发；补丁与构建脚本见仓库 android/ 目录。",
        style = ManisType.metadata.copy(fontSize = 12.5.sp, lineHeight = 18.sp),
        color = colors.textTertiary,
        modifier = Modifier.padding(horizontal = ManisDimens.spaceXs),
    )
}

@Composable
private fun UpdateActionRow(
    label: String,
    accent: Boolean,
    enabled: Boolean = true,
    onClick: () -> Unit,
) {
    val colors = LocalManisColors.current
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = 54.dp)
            .then(if (enabled) Modifier.clickable(onClick = onClick) else Modifier)
            .padding(horizontal = ManisDimens.spaceLg),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.Center,
    ) {
        Text(
            label,
            style = ManisType.body.copy(fontWeight = FontWeight.Medium),
            color = when {
                !enabled -> colors.textTertiary
                accent -> colors.actionPrimary
                else -> colors.textPrimary
            },
        )
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
