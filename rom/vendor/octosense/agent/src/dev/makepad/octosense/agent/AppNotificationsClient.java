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
import dev.makepad.octosense.settingsbroker.IAppNotifications;

/** Optional system-owned notification settings adapter. Failed binds are retried by visible-page reads. */
final class AppNotificationsClient implements AutoCloseable {
    private final Context context;
    private final Handler main=new Handler(Looper.getMainLooper());
    private volatile IAppNotifications service;
    private boolean bound,closed;
    private long lastAttempt;
    AppNotificationsClient(Context context) {this.context=context;main.post(this::bind);}
    private void bind() {
        if(bound||closed||SystemClock.elapsedRealtime()-lastAttempt<5000) return;
        lastAttempt=SystemClock.elapsedRealtime();
        if(context.getPackageManager().checkSignatures(context.getPackageName(),"dev.makepad.octosense.settingsbroker")!=PackageManager.SIGNATURE_MATCH) return;
        try {bound=context.bindService(new Intent().setComponent(new ComponentName("dev.makepad.octosense.settingsbroker",
                "dev.makepad.octosense.settingsbroker.OctoSenseAppNotificationsService")),connection,Context.BIND_AUTO_CREATE);}
        catch(SecurityException unavailable) {bound=false;}
    }
    private final ServiceConnection connection=new ServiceConnection() {
        @Override public void onServiceConnected(ComponentName name,IBinder binder) {service=IAppNotifications.Stub.asInterface(binder);}
        @Override public void onServiceDisconnected(ComponentName name) {service=null;}
        @Override public void onBindingDied(ComponentName name) {unbind();}
        @Override public void onNullBinding(ComponentName name) {unbind();}
    };
    private void unbind() {if(bound) context.unbindService(connection);bound=false;service=null;}
    Bundle snapshot(long id,String pkg,int offset,String generation) throws RemoteException {
        dev.makepad.octosense.notifications.AppNotificationsContract.packageName(pkg);
        dev.makepad.octosense.notifications.AppNotificationsContract.page(id,offset,generation);
        IAppNotifications current=service;if(current==null){main.post(this::bind);return null;}
        return current.snapshot(id,pkg,offset,generation);
    }
    String set(String pkg,String key,String target,String action,String value) throws RemoteException {
        dev.makepad.octosense.notifications.AppNotificationsContract.packageName(pkg);
        dev.makepad.octosense.notifications.AppNotificationsContract.key(key);
        dev.makepad.octosense.notifications.AppNotificationsContract.key(target);
        dev.makepad.octosense.notifications.AppNotificationsContract.value(dev.makepad.octosense.notifications.AppNotificationsContract.Action.parse(action),value);
        IAppNotifications current=service;return current==null?"notifications_unavailable":current.set(pkg,key,target,action,value);
    }
    @Override public void close() {closed=true;main.removeCallbacksAndMessages(null);unbind();}
}
