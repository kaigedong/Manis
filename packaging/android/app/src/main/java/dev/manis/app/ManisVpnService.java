package dev.manis.app;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.content.Intent;
import android.net.VpnService;
import android.os.Build;
import android.os.ParcelFileDescriptor;

public final class ManisVpnService extends VpnService {
    public static final String ACTION_START = "dev.manis.app.action.START_VPN";
    public static final String ACTION_STOP = "dev.manis.app.action.STOP_VPN";
    private static final String CHANNEL_ID = "manis-vpn";
    private static final int NOTIFICATION_ID = 1;
    private ParcelFileDescriptor tunnel;

    @Override
    public int onStartCommand(Intent intent, int flags, int startId) {
        if (intent != null && ACTION_STOP.equals(intent.getAction())) {
            closeTunnel();
            stopForeground(true);
            stopSelf();
            return START_NOT_STICKY;
        }
        startForegroundNow();
        try {
            establishTunnel();
        } catch (Exception error) {
            closeTunnel();
            stopForeground(true);
            stopSelf();
            ManisActivity.nativeVpnFailed();
        }
        return START_NOT_STICKY;
    }

    @Override
    public void onRevoke() {
        closeTunnel();
        stopForeground(true);
        stopSelf();
        ManisActivity.nativeVpnFailed();
        super.onRevoke();
    }

    @Override
    public void onDestroy() {
        closeTunnel();
        super.onDestroy();
    }

    private void establishTunnel() throws Exception {
        Builder builder = new Builder()
                .setSession("Manis")
                .setMtu(1500)
                .addAddress("172.19.0.1", 30)
                .addAddress("fd00:1::1", 126)
                .addRoute("0.0.0.0", 0)
                .addRoute("::", 0)
                .addDnsServer("172.19.0.2")
                .addDisallowedApplication(getPackageName());
        tunnel = builder.establish();
        if (tunnel == null) {
            throw new IllegalStateException("Android VPN interface could not be established");
        }
        int fd = tunnel.detachFd();
        tunnel = null;
        ManisActivity.nativeVpnStarted(fd);
    }

    private void startForegroundNow() {
        NotificationManager manager = getSystemService(NotificationManager.class);
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            manager.createNotificationChannel(new NotificationChannel(
                    CHANNEL_ID, "Manis VPN", NotificationManager.IMPORTANCE_LOW));
        }
        PendingIntent launch = PendingIntent.getActivity(
                this,
                0,
                new Intent(this, ManisActivity.class),
                PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
        Notification notification = new Notification.Builder(this, CHANNEL_ID)
                .setSmallIcon(android.R.drawable.stat_sys_warning)
                .setContentTitle("Manis is protecting your connection")
                .setContentText("VPN routing is active")
                .setContentIntent(launch)
                .setOngoing(true)
                .build();
        if (Build.VERSION.SDK_INT >= 34) {
            startForeground(NOTIFICATION_ID, notification,
                    android.content.pm.ServiceInfo.FOREGROUND_SERVICE_TYPE_SYSTEM_EXEMPTED);
        } else {
            startForeground(NOTIFICATION_ID, notification);
        }
    }

    private void closeTunnel() {
        if (tunnel != null) {
            try {
                tunnel.close();
            } catch (Exception ignored) {
            }
            tunnel = null;
        }
    }
}
