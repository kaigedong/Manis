package dev.manis.app

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.net.VpnService
import android.os.Build
import dev.manis.app.core.CoreController

class ManisVpnService : VpnService() {
    companion object {
        const val ACTION_START = "dev.manis.app.action.START_VPN"
        const val ACTION_STOP = "dev.manis.app.action.STOP_VPN"
        private const val CHANNEL_ID = "manis-vpn"
        private const val NOTIFICATION_ID = 1

        @Volatile
        var active: Boolean = false
            private set

        @Volatile
        var lastError: String? = null
            private set
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_STOP) {
            shutdown()
            return START_NOT_STICKY
        }

        startForegroundNow()
        val subscription = SubscriptionStore.url(this)
        if (subscription == null) {
            fail("请先在 Manis 里填写订阅地址")
            return START_NOT_STICKY
        }

        return try {
            val descriptor = establish()
            if (CoreController.start(this, subscription, descriptor)) {
                active = true
                lastError = null
            } else {
                fail(CoreController.lastError ?: "内核启动失败")
            }
            START_NOT_STICKY
        } catch (error: Throwable) {
            fail(error.message ?: "VPN 启动失败")
            START_NOT_STICKY
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
    private fun establish(): Int {
        val descriptor = Builder()
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
        return descriptor.detachFd()
    }

    private fun fail(message: String) {
        lastError = message
        shutdown()
    }

    private fun shutdown() {
        if (active || CoreController.running) {
            CoreController.stop()
        }
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
