package dev.makepad.octosense.agent;

import android.app.PendingIntent;
import android.content.ComponentName;
import android.content.Context;
import android.os.RemoteException;
import dev.makepad.octosense.settingsbroker.IAccountSettings;
import dev.makepad.octosense.accounts.AccountsSettingsContract;
import dev.makepad.octosense.accounts.AccountsSettingsContract.SyncAction;

/** Optional full current-user account route. No fallback to helper-filtered accounts. */
final class AccountSettingsClient implements AutoCloseable {
    private final SettingsServiceConnection<IAccountSettings> connection;
    AccountSettingsClient(Context context) {
        connection = new SettingsServiceConnection<>(context, new ComponentName(
                "dev.makepad.octosense.settingsbroker", "dev.makepad.octosense.settingsbroker.OctoSenseAccountSettingsService"), IAccountSettings.Stub::asInterface);
    }
    private IAccountSettings current() {IAccountSettings current=connection.current();return current;}
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
    @Override public void close() {connection.close();}
}
