package dev.manis.app.ui

import android.content.Context
import dev.manis.app.core.ControllerApi
import dev.manis.app.core.CoreController
import dev.manis.app.data.MobileConfig
import dev.manis.app.data.SubscriptionCache
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** Downloads enabled subscriptions and asks a live core to refresh them. */
internal suspend fun refreshSubscriptions(context: Context, config: MobileConfig): String {
    val cached = withContext(Dispatchers.IO) { SubscriptionCache.refresh(context, config) }
    val parts = mutableListOf<String>()
    if (cached.downloaded > 0) {
        parts += if (RuntimeState.active) {
            "已更新 ${cached.downloaded} 个订阅（${cached.nodes} 个节点）"
        } else {
            "已下载 ${cached.downloaded} 个订阅（${cached.nodes} 个节点），连接后生效"
        }
    } else if (cached.nodes > 0) {
        parts += "沿用本地缓存（${cached.nodes} 个节点）"
    }
    if (cached.failures.isNotEmpty()) {
        parts += "失败：${cached.failures.first()}"
        if (cached.failures.size > 1) parts += "另有 ${cached.failures.size - 1} 个订阅失败"
    }
    if (RuntimeState.active) {
        ControllerApi.configure(CoreController.secret)
        runCatching { ControllerApi.refreshSubscriptions() }
            .onSuccess { parts += it }
            .onFailure {
                // The downloaded cache is already on disk; restarting the core makes it use it.
                applyWorkspace(context)
                parts += "内核刷新失败，已用本地缓存重新载入"
            }
    }
    return parts.joinToString("；").ifBlank { "没有启用的订阅" }
}
