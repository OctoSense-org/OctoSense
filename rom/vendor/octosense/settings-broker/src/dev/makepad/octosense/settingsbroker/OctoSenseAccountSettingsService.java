package dev.makepad.octosense.settingsbroker;

import android.app.ActivityManager;
import android.accounts.Account;
import android.app.PendingIntent;
import android.app.Service;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.net.Uri;
import android.os.Binder;
import android.os.IBinder;
import android.os.Process;
import android.os.UserHandle;
import java.util.Arrays;
import dev.makepad.octosense.accounts.AccountsSettingsBackend;
import dev.makepad.octosense.accounts.AccountsSettingsContract;
import dev.makepad.octosense.accounts.AccountsSettingsContract.SyncAction;

/** System-UID account access exposed only to the signed, current-user ROM helper. */
public final class OctoSenseAccountSettingsService extends Service {
    private AccountsSettingsBackend accountBackend;
    @Override public void onCreate() {super.onCreate();accountBackend=new AccountsSettingsBackend(this);}
    private void caller() {
        int uid=Binder.getCallingUid();String[] packages=getPackageManager().getPackagesForUid(uid);
        if(UserHandle.getUserId(uid)!=UserHandle.myUserId()||packages==null||!Arrays.asList(packages).contains("dev.makepad.octosense.agent")
                ||getPackageManager().checkSignatures(uid,Process.myUid())!=PackageManager.SIGNATURE_MATCH)
            throw new SecurityException("Caller is not the trusted Settings helper");
    }
    private AccountsSettingsBackend backend() {
        AccountsSettingsBackend backend=accountBackend;
        if(ActivityManager.getCurrentUser()!=UserHandle.myUserId()||!backend.unlocked()) throw new SecurityException("Current unlocked user required");
        return backend;
    }
    private final IAccountSettings.Stub binder=new IAccountSettings.Stub() {
        @Override public String snapshot(long id) {
            caller();long identity=Binder.clearCallingIdentity();try {return backend().snapshot(id).toString();}
            catch(SecurityException denied) {throw denied;}catch(Exception unavailable) {return null;}finally {Binder.restoreCallingIdentity(identity);}
        }
        @Override public String details(long id,String key) {
            caller();AccountsSettingsContract.key(key);long identity=Binder.clearCallingIdentity();try {return backend().details(id,key).toString();}
            catch(SecurityException denied) {throw denied;}catch(Exception unavailable) {return null;}finally {Binder.restoreCallingIdentity(identity);}
        }
        @Override public String masterSync(boolean enabled) {
            caller();long identity=Binder.clearCallingIdentity();try {return backend().master(enabled);}finally {Binder.restoreCallingIdentity(identity);}
        }
        @Override public String sync(String key,String authorityKey,String action,boolean enabled) {
            caller();AccountsSettingsContract.key(key);AccountsSettingsContract.key(authorityKey);SyncAction parsed=SyncAction.parse(action);
            long identity=Binder.clearCallingIdentity();try {return backend().sync(key,authorityKey,parsed,parsed==SyncAction.AUTO?enabled:null);}finally {Binder.restoreCallingIdentity(identity);}
        }
        @Override public PendingIntent addition(String providerKey) {
            caller();AccountsSettingsContract.key(providerKey);long identity=Binder.clearCallingIdentity();
            try {
                Intent intent=backend().addIntent(providerKey);if(intent==null) return null;
                intent.setIdentifier("octosense-account-add:"+providerKey);
                return PendingIntent.getActivity(OctoSenseAccountSettingsService.this,providerKey.hashCode(),intent,
                        PendingIntent.FLAG_ONE_SHOT|PendingIntent.FLAG_IMMUTABLE|PendingIntent.FLAG_CANCEL_CURRENT);
            }finally {Binder.restoreCallingIdentity(identity);}
        }
        @Override public PendingIntent removal(String key) {
            caller();AccountsSettingsContract.key(key);long identity=Binder.clearCallingIdentity();
            try {
                AccountsSettingsBackend backend=backend();Account account=backend.observedAccount(key);
                if(!backend.canRemove(account)||account.getAccessId()==null) return null;
                String incarnation=AccountsSettingsContract.fingerprint("account-incarnation",UserHandle.myUserId(),key,account.getAccessId());
                Intent intent=new Intent(OctoSenseAccountSettingsService.this,OctoSenseRemoveAccountActivity.class)
                        .setData(Uri.parse("octosense-account-remove:"+key)).putExtra("account_key",key).putExtra("account_incarnation",incarnation);
                return PendingIntent.getActivity(OctoSenseAccountSettingsService.this,0,intent,
                        PendingIntent.FLAG_ONE_SHOT|PendingIntent.FLAG_IMMUTABLE|PendingIntent.FLAG_CANCEL_CURRENT);
            }finally {Binder.restoreCallingIdentity(identity);}
        }
    };
    @Override public IBinder onBind(Intent intent) {return binder;}
}
