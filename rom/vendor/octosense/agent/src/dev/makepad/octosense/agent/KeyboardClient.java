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
import dev.makepad.octosense.settingsbroker.IKeyboardSettings;

/** Optional system-owned keyboard adapter. Failed binds are retried by visible-page reads. */
final class KeyboardClient implements AutoCloseable {
    private final Context context;
    private final Handler main=new Handler(Looper.getMainLooper());
    private volatile IKeyboardSettings service;
    private boolean bound,closed;
    private long lastAttempt;
    KeyboardClient(Context context) {this.context=context;main.post(this::bind);}
    private void bind() {
        if(bound||closed||SystemClock.elapsedRealtime()-lastAttempt<5000) return;
        lastAttempt=SystemClock.elapsedRealtime();
        if(context.getPackageManager().checkSignatures(context.getPackageName(),"dev.makepad.octosense.settingsbroker")!=PackageManager.SIGNATURE_MATCH) return;
        try {bound=context.bindService(new Intent().setComponent(new ComponentName("dev.makepad.octosense.settingsbroker",
                "dev.makepad.octosense.settingsbroker.OctoSenseKeyboardService")),connection,Context.BIND_AUTO_CREATE);}
        catch(SecurityException unavailable) {bound=false;}
    }
    private final ServiceConnection connection=new ServiceConnection() {
        @Override public void onServiceConnected(ComponentName name,IBinder binder) {service=IKeyboardSettings.Stub.asInterface(binder);}
        @Override public void onServiceDisconnected(ComponentName name) {service=null;}
        @Override public void onBindingDied(ComponentName name) {unbind();}
        @Override public void onNullBinding(ComponentName name) {unbind();}
    };
    private void unbind() {if(bound) context.unbindService(connection);bound=false;service=null;}
    Bundle snapshot(long id,String query,int offset)throws RemoteException {
        dev.makepad.octosense.keyboards.KeyboardContract.read(id,query,offset);
        IKeyboardSettings current=service;if(current==null){main.post(this::bind);return null;}
        return current.snapshot(id,query,offset);
    }
    Bundle prepare(long id,String key,String target,String operation)throws RemoteException {
        dev.makepad.octosense.keyboards.KeyboardContract.flow(id,key,target,operation);
        IKeyboardSettings current=service;return current==null?null:current.prepare(id,key,target,operation);
    }
    @Override public void close() {closed=true;main.removeCallbacksAndMessages(null);unbind();}
}
