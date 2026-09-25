package dev.manis.app;

import android.app.Activity;
import android.content.pm.ActivityInfo;
import android.content.pm.PackageManager;
import android.content.Intent;
import android.net.VpnService;
import android.os.Build;
import android.os.Bundle;
import android.os.Bundle;
import android.app.NativeActivity;

public final class ManisActivity extends NativeActivity {
    private static final int VPN_PERMISSION_REQUEST = 7314;
    private static boolean nativeLibraryLoaded;

    public static native void nativeVpnStarted(int fd);
    public static native void nativeVpnFailed();

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        loadNativeLibraryForJni();
        super.onCreate(savedInstanceState);
    }

    private void loadNativeLibraryForJni() {
        if (nativeLibraryLoaded) {
            return;
        }
        try {
            ActivityInfo info = getPackageManager().getActivityInfo(
                    getComponentName(), PackageManager.GET_META_DATA);
            String libraryName = info.metaData.getString("android.app.lib_name");
            if (libraryName != null) {
                System.loadLibrary(libraryName);
                nativeLibraryLoaded = true;
            }
        } catch (PackageManager.NameNotFoundException error) {
            throw new IllegalStateException("Manis NativeActivity metadata is missing", error);
        } catch (UnsatisfiedLinkError alreadyLoaded) {
            // NativeActivity may have loaded the same library before this lifecycle callback.
            nativeLibraryLoaded = true;
        }
    }

    public void requestManisVpnPermission() {
        runOnUiThread(() -> {
            Intent permission = VpnService.prepare(this);
            if (permission != null) {
                startActivityForResult(permission, VPN_PERMISSION_REQUEST);
            } else {
                startVpnService();
            }
        });
    }

    public void stopManisVpnService() {
        runOnUiThread(() -> {
            Intent intent = new Intent(this, ManisVpnService.class);
            stopService(intent);
        });
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        if (requestCode == VPN_PERMISSION_REQUEST) {
            if (resultCode == Activity.RESULT_OK) {
                startVpnService();
            } else {
                nativeVpnFailed();
            }
        }
    }

    private void startVpnService() {
        Intent intent = new Intent(this, ManisVpnService.class);
        intent.setAction(ManisVpnService.ACTION_START);
        try {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                startForegroundService(intent);
            } else {
                startService(intent);
            }
        } catch (RuntimeException error) {
            nativeVpnFailed();
        }
    }
}
