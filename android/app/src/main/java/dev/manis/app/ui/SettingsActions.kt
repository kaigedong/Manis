package dev.manis.app.ui

import android.content.Context
import dev.manis.app.core.Native
import dev.manis.app.data.MobileConfig
import dev.manis.app.data.PROXY_DNS_SYSTEM
import dev.manis.app.data.Workspace
import java.io.File

internal fun readLog(context: Context): List<String> {
    val file = File(context.filesDir, "mihomo/core.log")
    if (!file.isFile) return listOf("还没有日志：连接一次 VPN 后生成。")
    return runCatching {
        file.readLines().takeLast(300)
    }.getOrDefault(listOf("日志读取失败"))
}

internal fun proxyDnsLabel(mode: String, custom: List<String>, discovered: List<String>): String = when {
    custom.isNotEmpty() -> "自定义 DoH（${custom.size} 条）"
    discovered.isNotEmpty() -> "机场 DNS（自动 ${discovered.size} 条）"
    mode == PROXY_DNS_SYSTEM -> "本机 DNS"
    else -> "加密 DNS（DoH）"
}

/**
 * Replaces the workspace with a pasted backup.
 *
 * The document is compiled by the same Rust renderer the core uses, so a broken or empty workspace
 * is rejected here instead of after the VPN restarts. Returns null on success, otherwise a message.
 */
internal fun importWorkspace(context: Context, text: String): String? {
    if (text.isBlank()) return "剪贴板里没有配置"
    val imported = runCatching { MobileConfig.fromJson(text) }
        .getOrElse { return "剪贴板内容不是 Manis 配置" }
    if (imported.subscriptions.none { it.enabled } && imported.nodes.none { it.enabled }) {
        return "导入的配置里没有启用的订阅或节点"
    }
    val compiled = runCatching {
        Native.renderConfig(imported.toJson(), 3)
    }.getOrElse { error -> return error.message ?: "配置校验失败" }
    if (compiled.isBlank()) return "配置校验失败"

    Workspace.update { imported }
    applyWorkspace(context)
    return null
}
