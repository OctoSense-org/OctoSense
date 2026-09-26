package dev.makepad.octosense.agent;

import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.ServiceConnection;
import android.content.pm.PackageManager;
import android.os.Bundle;
import android.os.Handler;
import android.os.IBinder;
import android.os.Looper;
import android.os.RemoteException;
import android.os.SystemClock;
import dev.makepad.octosense.settingsbroker.IAppBatterySettings;

/** Optional system-owned battery policy adapter. Failed binds are retried by visible-page reads. */
final class AppBatteryClient implements AutoCloseable {
    private final Context context;
    private final Handler main=new Handler(Looper.getMainLooper());
    private volatile IAppBatterySettings service;
    private boolean bound,closed;
    private long lastAttempt;
    AppBatteryClient(Context context) {this.context=context;main.post(this::bind);}
    private void bind() {
        if(bound||closed||SystemClock.elapsedRealtime()-lastAttempt<5000) return;
        lastAttempt=SystemClock.elapsedRealtime();
        if(context.getPackageManager().checkSignatures(context.getPackageName(),"dev.makepad.octosense.settingsbroker")!=PackageManager.SIGNATURE_MATCH) return;
        try {bound=context.bindService(new Intent().setComponent(new ComponentName("dev.makepad.octosense.settingsbroker",
                "dev.makepad.octosense.settingsbroker.OctoSenseAppBatteryService")),connection,Context.BIND_AUTO_CREATE);}
        catch(SecurityException unavailable) {bound=false;}
    }
    private final ServiceConnection connection=new ServiceConnection() {
        @Override public void onServiceConnected(ComponentName name,IBinder binder) {service=IAppBatterySettings.Stub.asInterface(binder);}
        @Override public void onServiceDisconnected(ComponentName name) {service=null;}
        @Override public void onBindingDied(ComponentName name) {unbind();}
        @Override public void onNullBinding(ComponentName name) {unbind();}
    };
    private void unbind() {if(bound) context.unbindService(connection);bound=false;service=null;}
    Bundle snapshot(long id,String pkg) throws RemoteException {
        dev.makepad.octosense.battery.AppBatteryContract.packageName(pkg);
        if(id<=0)throw new IllegalArgumentException("Positive request ID required");
        IAppBatterySettings current=service;if(current==null){main.post(this::bind);return null;}
        return current.snapshot(id,pkg);
    }
    String set(String pkg,String key,String mode) throws RemoteException {
        dev.makepad.octosense.battery.AppBatteryContract.packageName(pkg);
        dev.makepad.octosense.battery.AppBatteryContract.key(key);
        dev.makepad.octosense.battery.AppBatteryContract.Mode.parse(mode);
        IAppBatterySettings current=service;return current==null?"app_battery_unavailable":current.set(pkg,key,mode);
    }
    @Override public void close() {closed=true;main.removeCallbacksAndMessages(null);unbind();}
}
