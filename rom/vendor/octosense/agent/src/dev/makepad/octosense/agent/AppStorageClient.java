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
import dev.makepad.octosense.settingsbroker.IAppStorageSettings;

/** Optional system-owned app storage adapter. Failed binds are retried by visible-page reads. */
final class AppStorageClient implements AutoCloseable {
    private final Context context;
    private final Handler main=new Handler(Looper.getMainLooper());
    private volatile IAppStorageSettings service;
    private boolean bound,closed;
    private long lastAttempt;
    AppStorageClient(Context context) {this.context=context;main.post(this::bind);}
    private void bind() {
        if(bound||closed||SystemClock.elapsedRealtime()-lastAttempt<5000) return;
        lastAttempt=SystemClock.elapsedRealtime();
        if(context.getPackageManager().checkSignatures(context.getPackageName(),"dev.makepad.octosense.settingsbroker")!=PackageManager.SIGNATURE_MATCH) return;
        try {bound=context.bindService(new Intent().setComponent(new ComponentName("dev.makepad.octosense.settingsbroker",
                "dev.makepad.octosense.settingsbroker.OctoSenseAppStorageService")),connection,Context.BIND_AUTO_CREATE);}
        catch(SecurityException unavailable) {bound=false;}
    }
    private final ServiceConnection connection=new ServiceConnection() {
        @Override public void onServiceConnected(ComponentName name,IBinder binder) {service=IAppStorageSettings.Stub.asInterface(binder);}
        @Override public void onServiceDisconnected(ComponentName name) {service=null;}
        @Override public void onBindingDied(ComponentName name) {unbind();}
        @Override public void onNullBinding(ComponentName name) {unbind();}
    };
    private void unbind() {if(bound) context.unbindService(connection);bound=false;service=null;}
    Bundle snapshot(long id,String pkg) throws RemoteException {
        dev.makepad.octosense.appstorage.AppStorageContract.packageName(pkg);
        if(id<=0)throw new IllegalArgumentException("Positive request ID required");
        IAppStorageSettings current=service;if(current==null){main.post(this::bind);return null;}
        return current.snapshot(id,pkg);
    }
    Bundle action(String pkg,String key,String action) throws RemoteException {
        dev.makepad.octosense.appstorage.AppStorageContract.packageName(pkg);
        dev.makepad.octosense.appstorage.AppStorageContract.key(key);
        dev.makepad.octosense.appstorage.AppStorageContract.Action.parse(action);
        IAppStorageSettings current=service;return current==null?null:current.action(pkg,key,action);
    }
    @Override public void close() {closed=true;main.removeCallbacksAndMessages(null);unbind();}
}
