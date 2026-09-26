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
import com.android.systemui.octosense.ISensorSettings;

/** Optional role-owned SystemUI adapter. Failed binds are retried by visible-page reads. */
final class SensorSettingsClient implements AutoCloseable {
    private final Context context;
    private final Handler main=new Handler(Looper.getMainLooper());
    private volatile ISensorSettings service;
    private boolean bound,closed;
    private long lastAttempt;
    SensorSettingsClient(Context context) {this.context=context;main.post(this::bind);}
    private void bind() {
        if(bound||closed||SystemClock.elapsedRealtime()-lastAttempt<5000) return;
        lastAttempt=SystemClock.elapsedRealtime();
        if(context.getPackageManager().checkSignatures(context.getPackageName(),"com.android.systemui")!=PackageManager.SIGNATURE_MATCH) return;
        try {bound=context.bindService(new Intent().setComponent(new ComponentName("com.android.systemui",
                "com.android.systemui.octosense.OctoSenseSensorSettingsService")),connection,Context.BIND_AUTO_CREATE);}
        catch(SecurityException unavailable) {bound=false;}
    }
    private final ServiceConnection connection=new ServiceConnection() {
        @Override public void onServiceConnected(ComponentName name,IBinder binder) {service=ISensorSettings.Stub.asInterface(binder);}
        @Override public void onServiceDisconnected(ComponentName name) {service=null;}
        @Override public void onBindingDied(ComponentName name) {unbind();}
        @Override public void onNullBinding(ComponentName name) {unbind();}
    };
    private void unbind() {if(bound) context.unbindService(connection);bound=false;service=null;}
    Bundle snapshot() {
        ISensorSettings current=service;
        if(current==null) {main.post(this::bind);return null;}
        try {Bundle result=current.snapshot();return result!=null&&result.getBoolean("ok")?result:null;}
        catch(RemoteException|SecurityException unavailable) {return null;}
    }
    boolean apply(int sensor,boolean allowed) {
        if(sensor!=1&&sensor!=2) throw new IllegalArgumentException("Unknown sensor");
        ISensorSettings current=service;if(current==null) return false;
        try {Bundle result=current.setAccess(sensor,allowed);return result!=null&&result.getBoolean("ok");}
        catch(RemoteException|SecurityException unavailable) {return false;}
    }
    @Override public void close() {closed=true;main.removeCallbacksAndMessages(null);unbind();}
}
