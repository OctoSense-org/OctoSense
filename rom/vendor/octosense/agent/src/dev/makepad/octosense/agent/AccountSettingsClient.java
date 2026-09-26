package dev.makepad.octosense.agent;

import android.app.PendingIntent;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.ServiceConnection;
import android.content.pm.PackageManager;
import android.os.Handler;
import android.os.IBinder;
import android.os.Looper;
import android.os.RemoteException;
import android.os.SystemClock;
import dev.makepad.octosense.settingsbroker.IAccountSettings;
import dev.makepad.octosense.accounts.AccountsSettingsContract;
import dev.makepad.octosense.accounts.AccountsSettingsContract.SyncAction;

/** Optional full current-user account route. No fallback to helper-filtered accounts. */
final class AccountSettingsClient implements AutoCloseable {
    private final Context context;
    private final Handler main=new Handler(Looper.getMainLooper());
    private volatile IAccountSettings service;
    private boolean bound,closed;
    private long lastAttempt;
    AccountSettingsClient(Context context) {this.context=context;main.post(this::bind);}
    private void bind() {
        if(bound||closed||SystemClock.elapsedRealtime()-lastAttempt<5000) return;
        lastAttempt=SystemClock.elapsedRealtime();
        if(context.getPackageManager().checkSignatures(context.getPackageName(),"dev.makepad.octosense.settingsbroker")!=PackageManager.SIGNATURE_MATCH) return;
        try {bound=context.bindService(new Intent().setComponent(new ComponentName("dev.makepad.octosense.settingsbroker",
                "dev.makepad.octosense.settingsbroker.OctoSenseAccountSettingsService")),connection,Context.BIND_AUTO_CREATE);}
        catch(SecurityException unavailable) {bound=false;}
    }
    private final ServiceConnection connection=new ServiceConnection() {
        @Override public void onServiceConnected(ComponentName name,IBinder binder) {service=IAccountSettings.Stub.asInterface(binder);}
        @Override public void onServiceDisconnected(ComponentName name) {service=null;}
        @Override public void onBindingDied(ComponentName name) {unbind();}
        @Override public void onNullBinding(ComponentName name) {unbind();}
    };
    private void unbind() {if(bound) context.unbindService(connection);bound=false;service=null;}
    private IAccountSettings current() {IAccountSettings current=service;if(current==null) main.post(this::bind);return current;}
    String snapshot(long id) {
        if(id<=0) throw new IllegalArgumentException("Positive request ID required");IAccountSettings current=current();if(current==null) return null;
        try {return current.snapshot(id);}catch(RemoteException|SecurityException unavailable) {return null;}
    }
    String details(long id,String key) {
        AccountsSettingsContract.key(key);if(id<=0) throw new IllegalArgumentException("Positive request ID required");IAccountSettings current=current();if(current==null) return null;
        try {return current.details(id,key);}catch(RemoteException|SecurityException unavailable) {return null;}
    }
    String master(boolean enabled) {
        IAccountSettings current=current();if(current==null) return "sync_unavailable";
        try {return current.masterSync(enabled);}catch(RemoteException|SecurityException unavailable) {return "sync_unavailable";}
    }
    String sync(String key,String authorityKey,SyncAction action,Boolean enabled) {
        AccountsSettingsContract.key(key);AccountsSettingsContract.key(authorityKey);AccountsSettingsContract.syncValue(action,enabled);
        IAccountSettings current=current();if(current==null) return "sync_unavailable";
        try {return current.sync(key,authorityKey,action.wire,Boolean.TRUE.equals(enabled));}catch(RemoteException|SecurityException unavailable) {return "sync_unavailable";}
    }
    PendingIntent addition(String providerKey) {
        AccountsSettingsContract.key(providerKey);IAccountSettings current=current();if(current==null) return null;
        try {return current.addition(providerKey);}catch(RemoteException|SecurityException unavailable) {return null;}
    }
    PendingIntent removal(String key) {
        AccountsSettingsContract.key(key);IAccountSettings current=current();if(current==null) return null;
        try {return current.removal(key);}catch(RemoteException|SecurityException unavailable) {return null;}
    }
    @Override public void close() {closed=true;main.removeCallbacksAndMessages(null);unbind();}
}
