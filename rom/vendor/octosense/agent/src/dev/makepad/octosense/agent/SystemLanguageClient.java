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
import dev.makepad.octosense.settingsbroker.ISystemLanguageSettings;

/** Optional system-owned ordered system-language adapter. Failed binds are retried by visible-page reads. */
final class SystemLanguageClient implements AutoCloseable {
    private final Context context;
    private final Handler main=new Handler(Looper.getMainLooper());
    private volatile ISystemLanguageSettings service;
    private boolean bound,closed;
    private long lastAttempt;
    SystemLanguageClient(Context context) {this.context=context;main.post(this::bind);}
    private void bind() {
        if(bound||closed||SystemClock.elapsedRealtime()-lastAttempt<5000) return;
        lastAttempt=SystemClock.elapsedRealtime();
        if(context.getPackageManager().checkSignatures(context.getPackageName(),"dev.makepad.octosense.settingsbroker")!=PackageManager.SIGNATURE_MATCH) return;
        try {bound=context.bindService(new Intent().setComponent(new ComponentName("dev.makepad.octosense.settingsbroker",
                "dev.makepad.octosense.settingsbroker.OctoSenseSystemLanguageService")),connection,Context.BIND_AUTO_CREATE);}
        catch(SecurityException unavailable) {bound=false;}
    }
    private final ServiceConnection connection=new ServiceConnection() {
        @Override public void onServiceConnected(ComponentName name,IBinder binder) {service=ISystemLanguageSettings.Stub.asInterface(binder);}
        @Override public void onServiceDisconnected(ComponentName name) {service=null;}
        @Override public void onBindingDied(ComponentName name) {unbind();}
        @Override public void onNullBinding(ComponentName name) {unbind();}
    };
    private void unbind() {if(bound) context.unbindService(connection);bound=false;service=null;}
    Bundle snapshot(long id,String key,String parent,String query,int offset)throws RemoteException {
        dev.makepad.octosense.systemlanguage.SystemLanguageContract.read(id,key,parent,query,offset);
        ISystemLanguageSettings current=service;if(current==null){main.post(this::bind);return null;}
        return current.snapshot(id,key,parent,query,offset);
    }
    String apply(String key,String[] targets)throws RemoteException {
        dev.makepad.octosense.systemlanguage.SystemLanguageContract.order(key,targets);
        ISystemLanguageSettings current=service;return current==null?"languages_unavailable":current.apply(key,targets);
    }
    @Override public void close() {closed=true;main.removeCallbacksAndMessages(null);unbind();}
}
