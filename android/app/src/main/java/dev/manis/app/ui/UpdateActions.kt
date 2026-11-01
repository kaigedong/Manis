package dev.manis.app.ui

import android.content.ClipData
import android.content.Context
import android.content.Intent
import android.net.Uri
import androidx.core.content.FileProvider
import java.io.File

internal const val RELEASE_PAGE = "https://github.com/kaigedong/Manis/releases/tag/latest"

internal fun launchInstaller(context: Context, file: File, onMessage: (String) -> Unit) {
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

internal fun openRelease(context: Context, url: String, onMessage: (String) -> Unit) {
    runCatching {
        context.startActivity(
            Intent(Intent.ACTION_VIEW, Uri.parse(url))
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
        )
    }.onFailure {
        onMessage("无法打开浏览器：${it.message ?: "未知错误"}")
    }
}
