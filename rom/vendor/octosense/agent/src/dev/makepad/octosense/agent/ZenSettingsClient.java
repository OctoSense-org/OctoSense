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
import dev.makepad.octosense.settingsbroker.IZenSettings;

/** Optional system-owned manual DND adapter. Failed binds are retried by visible-page reads. */
final class ZenSettingsClient implements AutoCloseable {
    private final Context context;
    private final Handler main=new Handler(Looper.getMainLooper());
    private volatile IZenSettings service;
    private boolean bound,closed;
    private long lastAttempt;
    ZenSettingsClient(Context context) {this.context=context;main.post(this::bind);}
    private void bind() {
        if(bound||closed||SystemClock.elapsedRealtime()-lastAttempt<5000) return;
        lastAttempt=SystemClock.elapsedRealtime();
        if(context.getPackageManager().checkSignatures(context.getPackageName(),"dev.makepad.octosense.settingsbroker")!=PackageManager.SIGNATURE_MATCH) return;
        try {bound=context.bindService(new Intent().setComponent(new ComponentName("dev.makepad.octosense.settingsbroker",
                "dev.makepad.octosense.settingsbroker.OctoSenseZenSettingsService")),connection,Context.BIND_AUTO_CREATE);}
        catch(SecurityException unavailable) {bound=false;}
    }
    private final ServiceConnection connection=new ServiceConnection() {
        @Override public void onServiceConnected(ComponentName name,IBinder binder) {service=IZenSettings.Stub.asInterface(binder);}
        @Override public void onServiceDisconnected(ComponentName name) {service=null;}
        @Override public void onBindingDied(ComponentName name) {unbind();}
        @Override public void onNullBinding(ComponentName name) {unbind();}
    };
    private void unbind() {if(bound) context.unbindService(connection);bound=false;service=null;}
    Bundle snapshot() {
        IZenSettings current=service;
        if(current==null) {main.post(this::bind);return null;}
        try {Bundle result=current.snapshot();return result!=null&&result.getBoolean("ok")?result:null;}
        catch(RemoteException|SecurityException unavailable) {return null;}
    }
    boolean apply(String mode) {
        dev.makepad.octosense.controls.DndMode.parse(mode);
        IZenSettings current=service;if(current==null) return false;
        try {return current.setMode(mode);}
        catch(RemoteException|SecurityException unavailable) {return false;}
    }
    private interface Operation {Bundle run(IZenSettings service) throws RemoteException;}
    private Bundle request(Operation operation) {
        IZenSettings current=service;
        if(current==null){main.post(this::bind);return null;}
        try{return operation.run(current);}catch(RemoteException|SecurityException unavailable){return null;}
    }
    Bundle settingsSnapshot(long id,int offset,String generation) {return request(service->service.settingsSnapshot(id,offset,generation));}
    Bundle policy(String key,String field,String value) {return request(service->service.policy(key,field,value));}
    Bundle schedule(String key,String target,String name,int[] days,int start,int end,boolean exitAtAlarm,boolean enabled) {return request(service->service.schedule(key,target,name,days,start,end,exitAtAlarm,enabled));}
    Bundle enabled(String key,String target,boolean enabled) {return request(service->service.enabled(key,target,enabled));}
    Bundle deleteRule(String key,String target) {return request(service->service.deleteRule(key,target));}
    @Override public void close() {closed=true;main.removeCallbacksAndMessages(null);unbind();}
}
