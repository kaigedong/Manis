package dev.manis.app

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Intent
import android.net.VpnService
import android.os.Build
import android.os.ParcelFileDescriptor
import dev.manis.app.core.CoreController
import dev.manis.app.data.SubscriptionDns
import dev.manis.app.data.Workspace

class ManisVpnService : VpnService() {
    companion object {
        const val ACTION_START = "dev.manis.app.action.START_VPN"
        const val ACTION_STOP = "dev.manis.app.action.STOP_VPN"
        const val ACTION_RELOAD = "dev.manis.app.action.RELOAD_VPN"
        private const val CHANNEL_ID = "manis-vpn"
        private const val NOTIFICATION_ID = 1

        @Volatile
        var active: Boolean = false
            private set

        @Volatile
        var lastError: String? = null
            private set
    }

    private var tunnel: ParcelFileDescriptor? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        android.util.Log.d("Manis", "action=${intent?.action} tunnel=${tunnel != null} active=$active")
        when (intent?.action) {
            ACTION_STOP -> {
                shutdown()
                return START_NOT_STICKY
            }

            ACTION_RELOAD -> {
                reload()
                return START_NOT_STICKY
            }
        }

        startForegroundNow()
        val workspace = Workspace.config.value
        if (workspace.subscriptions.none { it.enabled } && workspace.nodes.none { it.enabled }) {
            fail("先添加一个订阅或节点")
            return START_NOT_STICKY
        }

        return try {
            if (tunnel == null) {
                tunnel = establish()
            }
            val descriptor = tunnel?.fd ?: error("VPN 接口不可用")
            if (CoreController.start(this, descriptor)) {
                active = true
                lastError = null
                learnSubscriptionDns()
            } else {
                fail(CoreController.lastError ?: "内核启动失败")
            }
            START_NOT_STICKY
        } catch (error: Throwable) {
            fail(error.message ?: "VPN 启动失败")
            START_NOT_STICKY
        }
    }

    /**
     * Reads the airport's own resolvers out of the subscription document and reloads once when they
     * change. Runs off the main thread so the first start never waits on the network.
     */
    private fun learnSubscriptionDns() {
        Thread({
            val changed = runCatching { SubscriptionDns.refresh(this) }.getOrDefault(false)
            val descriptor = tunnel?.fd
            if (changed && active && descriptor != null) {
                CoreController.reload(this, descriptor)
            }
        }, "manis-subscription-dns").start()
    }

    private fun reload() {
        val descriptor = tunnel?.fd ?: return
        if (!active) return
        startForegroundNow()
        if (CoreController.reload(this, descriptor)) {
            lastError = null
        } else {
            fail(CoreController.lastError ?: "配置应用失败")
        }
    }

    override fun onRevoke() {
        fail("系统撤销了 VPN 授权")
        super.onRevoke()
    }

    override fun onDestroy() {
        shutdown()
        super.onDestroy()
    }

    /**
     * Builds the interface Android routes through. Mihomo owns only the descriptor, so every
     * address, route and the DNS server are declared here; 172.19.0.2 sits inside the same /30 and
     * is answered by Mihomo's DNS hijack.
     */
    private fun establish(): ParcelFileDescriptor {
        return Builder()
            .setSession("Manis")
            .setMtu(1500)
            .addAddress("172.19.0.1", 30)
            .addAddress("fd00:1::1", 126)
            .addRoute("0.0.0.0", 0)
            .addRoute("::", 0)
            .addDnsServer("172.19.0.2")
            .addDisallowedApplication(packageName)
            .establish()
            ?: error("Android 未建立 VPN 接口")
    }

    private fun fail(message: String) {
        android.util.Log.w("Manis", "service fail: $message")
        lastError = message
        shutdown()
    }

    private fun shutdown() {
        CoreController.stop()
        runCatching { tunnel?.close() }
        tunnel = null
        active = false
        stopForeground(STOP_FOREGROUND_REMOVE)
        stopSelf()
    }

    private fun startForegroundNow() {
        val manager = getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(
            NotificationChannel(CHANNEL_ID, "Manis VPN", NotificationManager.IMPORTANCE_LOW),
        )
        val launch = PendingIntent.getActivity(
            this,
            0,
            Intent(this, MainActivity::class.java),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val notification: Notification = Notification.Builder(this, CHANNEL_ID)
            .setSmallIcon(android.R.drawable.stat_sys_warning)
            .setContentTitle("Manis 正在接管网络")
            .setContentText("VPN 路由已启用")
            .setContentIntent(launch)
            .setOngoing(true)
            .build()
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            startForeground(
                NOTIFICATION_ID,
                notification,
                android.content.pm.ServiceInfo.FOREGROUND_SERVICE_TYPE_SYSTEM_EXEMPTED,
            )
        } else {
            startForeground(NOTIFICATION_ID, notification)
        }
    }
}
