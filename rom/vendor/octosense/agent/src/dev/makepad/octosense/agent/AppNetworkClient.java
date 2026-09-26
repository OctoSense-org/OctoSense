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
import dev.makepad.octosense.settingsbroker.IAppNetworkSettings;

/** Optional system-owned app network policy. Reads retry failed service bindings. */
final class AppNetworkClient implements AutoCloseable {
    private final Context context;
    private final Handler main=new Handler(Looper.getMainLooper());
    private volatile IAppNetworkSettings service;
    private boolean bound,closed;
    private long lastAttempt;
    AppNetworkClient(Context context) {this.context=context;main.post(this::bind);}
    private void bind() {
        if(bound||closed||SystemClock.elapsedRealtime()-lastAttempt<5000) return;
        lastAttempt=SystemClock.elapsedRealtime();
        if(context.getPackageManager().checkSignatures(context.getPackageName(),"dev.makepad.octosense.settingsbroker")!=PackageManager.SIGNATURE_MATCH) return;
        try {bound=context.bindService(new Intent().setComponent(new ComponentName("dev.makepad.octosense.settingsbroker",
                "dev.makepad.octosense.settingsbroker.OctoSenseAppNetworkService")),connection,Context.BIND_AUTO_CREATE);}
        catch(SecurityException unavailable) {bound=false;}
    }
    private final ServiceConnection connection=new ServiceConnection() {
        @Override public void onServiceConnected(ComponentName name,IBinder binder) {service=IAppNetworkSettings.Stub.asInterface(binder);}
        @Override public void onServiceDisconnected(ComponentName name) {service=null;}
        @Override public void onBindingDied(ComponentName name) {unbind();}
        @Override public void onNullBinding(ComponentName name) {unbind();}
    };
    private void unbind() {if(bound) context.unbindService(connection);bound=false;service=null;}
    private interface Operation {Bundle run(IAppNetworkSettings service) throws RemoteException;}
    private Bundle request(Operation operation) {
        IAppNetworkSettings current=service;
        if(current==null){main.post(this::bind);return null;}
        try{return operation.run(current);}catch(RemoteException|SecurityException unavailable){return null;}
    }
    Bundle snapshot(long id,String pkg) {return request(service->service.snapshot(id,pkg));}
    Bundle set(String pkg,String key,String field,boolean enabled) {return request(service->service.set(pkg,key,field,enabled));}
    @Override public void close() {closed=true;main.removeCallbacksAndMessages(null);unbind();}
}
