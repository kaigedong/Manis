package dev.manis.app.data

import android.content.Context
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.io.FileOutputStream
import java.net.HttpURLConnection
import java.net.URL
import java.security.MessageDigest

data class AvailableUpdate(
    val version: String,
    val apkUrl: String,
    val checksumUrl: String,
    val size: Long,
    val releaseUrl: String,
)

/**
 * Checks the rolling GitHub release and downloads its Android APK.
 *
 * The release is a prerelease tagged `latest`, so the regular `/releases/latest` endpoint would
 * skip it; the tag endpoint returns the same asset the Releases page shows. Downloads are checked
 * against the release's `.sha256` file before the installer is allowed to see them.
 */
internal object UpdateChecker {
    private const val RELEASE_API =
        "https://api.github.com/repos/kaigedong/Manis/releases/tags/latest"
    private const val USER_AGENT = "Manis-Android"
    private const val CONNECT_TIMEOUT_MS = 10_000
    private const val READ_TIMEOUT_MS = 15_000
    private const val DOWNLOAD_READ_TIMEOUT_MS = 60_000
    private val APK_NAME =
        Regex("""Manis-([0-9]+\.[0-9]+\.[0-9]+)-android-arm64-debug\.apk""")

    /** Installed version without the `-android` suffix, e.g. `0.1.103`. */
    fun installedVersion(context: Context): String =
        currentVersionName(context).substringBefore("-android")

    fun currentVersionName(context: Context): String =
        runCatching {
            @Suppress("DEPRECATION")
            context.packageManager.getPackageInfo(context.packageName, 0).versionName.orEmpty()
        }.getOrDefault("")

    /** Returns the newest published APK when it is newer than this installation. */
    suspend fun check(context: Context): Result<AvailableUpdate?> = withContext(Dispatchers.IO) {
        runCatching {
            val release = JSONObject(getText(RELEASE_API))
            val assets = release.optJSONArray("assets") ?: JSONArray()
            val current = installedVersion(context)
            var best: AvailableUpdate? = null
            for (index in 0 until assets.length()) {
                val asset = assets.optJSONObject(index) ?: continue
                val match = APK_NAME.matchEntire(asset.optString("name")) ?: continue
                val version = match.groupValues[1]
                if (!isNewer(version, current)) continue
                val apkUrl = asset.optString("browser_download_url")
                if (!apkUrl.startsWith("https://")) continue
                if (best == null || isNewer(version, best.version)) {
                    best = AvailableUpdate(
                        version = version,
                        apkUrl = apkUrl,
                        checksumUrl = "$apkUrl.sha256",
                        size = asset.optLong("size", 0L),
                        releaseUrl = release.optString(
                            "html_url",
                            "https://github.com/kaigedong/Manis/releases/tag/latest",
                        ),
                    )
                }
            }
            best
        }
    }

    fun cachedApk(context: Context, version: String): File? =
        File(context.cacheDir, "updates/Manis-$version-android-arm64-debug.apk")
            .takeIf { it.isFile }

    /**
     * Downloads and verifies one update, reporting 0..100 on the main dispatcher.
     *
     * The partial file never reaches the installer: it is renamed only after the SHA-256 check
     * succeeds, and any failure removes it.
     */
    suspend fun download(
        context: Context,
        update: AvailableUpdate,
        onProgress: (Int) -> Unit,
    ): File = withContext(Dispatchers.IO) {
        val directory = File(context.cacheDir, "updates").apply { mkdirs() }
        val target = File(directory, "Manis-${update.version}-android-arm64-debug.apk")
        val temporary = File(directory, "${target.name}.part")
        directory.listFiles()?.forEach { file ->
            if (file.name != target.name) file.delete()
        }

        val connection = URL(update.apkUrl).openConnection() as HttpURLConnection
        connection.connectTimeout = CONNECT_TIMEOUT_MS
        connection.readTimeout = DOWNLOAD_READ_TIMEOUT_MS
        connection.instanceFollowRedirects = true
        connection.setRequestProperty("User-Agent", USER_AGENT)
        try {
            val code = connection.responseCode
            if (code !in 200..299) error("下载失败：HTTP $code")
            val total = connection.contentLengthLong.takeIf { it > 0 } ?: update.size
            var copied = 0L
            var reported = -1
            connection.inputStream.use { input ->
                FileOutputStream(temporary).use { output ->
                    val buffer = ByteArray(64 * 1024)
                    while (true) {
                        val read = input.read(buffer)
                        if (read < 0) break
                        output.write(buffer, 0, read)
                        copied += read
                        if (total > 0) {
                            val percent = ((copied * 100) / total).toInt().coerceIn(0, 100)
                            if (percent != reported) {
                                reported = percent
                                withContext(Dispatchers.Main) { onProgress(percent) }
                            }
                        }
                    }
                }
            }
            if (total > 0 && copied != total) error("下载不完整：$copied/$total 字节")
            val expected = checksum(update.checksumUrl)
            val actual = sha256(temporary)
            if (!actual.equals(expected, ignoreCase = true)) {
                error("更新包校验失败，请稍后重试")
            }
            if (target.exists() && !target.delete()) error("无法替换旧的更新包")
            if (!temporary.renameTo(target)) {
                try {
                    temporary.copyTo(target, overwrite = true)
                } catch (error: Throwable) {
                    target.delete()
                    throw error
                }
                temporary.delete()
            }
            withContext(Dispatchers.Main) { onProgress(100) }
            target
        } finally {
            connection.disconnect()
            if (temporary.exists()) temporary.delete()
        }
    }

    internal fun isNewer(remote: String, current: String): Boolean {
        val remoteParts = versionParts(remote)
        val currentParts = versionParts(current)
        val count = maxOf(remoteParts.size, currentParts.size)
        for (index in 0 until count) {
            val remotePart = remoteParts.getOrElse(index) { 0 }
            val currentPart = currentParts.getOrElse(index) { 0 }
            if (remotePart != currentPart) return remotePart > currentPart
        }
        return false
    }

    private fun versionParts(value: String): List<Int> =
        value.trim()
            .removePrefix("v")
            .split('.')
            .map { part -> part.takeWhile(Char::isDigit).toIntOrNull() ?: 0 }

    private fun checksum(url: String): String {
        val body = getText(url)
        val value = Regex("""[0-9a-fA-F]{64}""").find(body)?.value
            ?: error("更新包缺少 SHA-256 校验文件")
        return value
    }

    private fun sha256(file: File): String {
        val digest = MessageDigest.getInstance("SHA-256")
        file.inputStream().use { input ->
            val buffer = ByteArray(64 * 1024)
            while (true) {
                val read = input.read(buffer)
                if (read < 0) break
                digest.update(buffer, 0, read)
            }
        }
        return digest.digest().joinToString("") { "%02x".format(it.toInt() and 0xff) }
    }

    private fun getText(url: String): String {
        val connection = URL(url).openConnection() as HttpURLConnection
        connection.connectTimeout = CONNECT_TIMEOUT_MS
        connection.readTimeout = READ_TIMEOUT_MS
        connection.instanceFollowRedirects = true
        connection.setRequestProperty("User-Agent", USER_AGENT)
        connection.setRequestProperty("Accept", "application/vnd.github+json")
        try {
            val code = connection.responseCode
            if (code !in 200..299) {
                val hint = if (code == 403 || code == 429) "（GitHub API 可能已限流）" else ""
                error("GitHub 返回 HTTP $code$hint")
            }
            return connection.inputStream.bufferedReader().use { it.readText() }
        } finally {
            connection.disconnect()
        }
    }
}
