package dev.makepad.octosense.accounts;

import android.Manifest;
import android.accounts.Account;
import android.accounts.AccountManager;
import android.accounts.AuthenticatorDescription;
import android.app.KeyguardManager;
import android.app.admin.DevicePolicyManager;
import android.content.ContentResolver;
import android.content.Context;
import android.content.Intent;
import android.content.SyncAdapterType;
import android.content.pm.PackageManager;
import android.content.pm.ProviderInfo;
import android.os.Bundle;
import android.os.Process;
import android.os.SystemClock;
import android.os.UserManager;
import android.provider.Settings;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Comparator;
import java.util.LinkedHashMap;
import dev.makepad.octosense.accounts.AccountsSettingsContract.Observations;
import java.util.List;
import java.util.Map;
import org.json.JSONArray;
import org.json.JSONObject;
import dev.makepad.octosense.accounts.AccountsSettingsContract.SyncAction;

/** Current-user observations. A non-system caller's inventory is always labelled limited. */
public final class AccountsSettingsBackend {
    private final Context context;
    private final AccountManager manager;
    private final boolean full;
    private static final long OBSERVATION_TTL_MS=30000;
    private final Observations observedAccounts=new Observations(128,OBSERVATION_TTL_MS,SystemClock::elapsedRealtime),
            observedProviders=new Observations(128,OBSERVATION_TTL_MS,SystemClock::elapsedRealtime),
            observedAuthorities=new Observations(8192,OBSERVATION_TTL_MS,SystemClock::elapsedRealtime);
    private long masterObservedAt=-1;
    private boolean freshMaster() {long now=SystemClock.elapsedRealtime();return masterObservedAt>=0&&now>=masterObservedAt&&now-masterObservedAt<=OBSERVATION_TTL_MS;}
    public AccountsSettingsBackend(Context context) {
        this.context=context;manager=AccountManager.get(context);
        // The narrow SettingsBroker route runs as system UID, guarded to its current unlocked user.
        // GET_ACCOUNTS_PRIVILEGED alone can still be overridden by stored visibility.
        full=Process.myUid()==Process.SYSTEM_UID;
    }
    private boolean permission(String permission) {return context.checkSelfPermission(permission)==PackageManager.PERMISSION_GRANTED;}
    public boolean unlocked() {
        UserManager users=context.getSystemService(UserManager.class);KeyguardManager lock=context.getSystemService(KeyguardManager.class);
        return users!=null&&users.isUserUnlocked()&&lock!=null&&!lock.isKeyguardLocked();
    }
    private boolean canModify(String type) {
        UserManager users=context.getSystemService(UserManager.class);DevicePolicyManager policy=context.getSystemService(DevicePolicyManager.class);
        if(!unlocked()||users==null||policy==null||users.hasUserRestriction(UserManager.DISALLOW_MODIFY_ACCOUNTS)) return false;
        try {String[] disabled=policy.getAccountTypesWithManagementDisabled();return disabled!=null&&!Arrays.asList(disabled).contains(type);}
        catch(SecurityException unavailable) {return false;}
    }
    private int scope() {return context.getApplicationInfo().uid/100000;}
    private String accountKey(Account account) {return AccountsSettingsContract.fingerprint("account",scope(),account.type,account.name);}
    private String providerKey(AuthenticatorDescription auth) {
        return AccountsSettingsContract.fingerprint("provider",scope(),auth.type,auth.packageName);
    }
    private String authorityKey(Account account,SyncAdapterType adapter) {
        return AccountsSettingsContract.fingerprint("authority",scope(),accountKey(account),adapter.authority,adapter.accountType);
    }
    private static String text(CharSequence value) {
        if(value==null) return "";StringBuilder out=new StringBuilder();value.toString().codePoints().limit(512)
                .forEach(codepoint->out.appendCodePoint(Character.isISOControl(codepoint)?' ':codepoint));return out.toString();
    }
    private String label(AuthenticatorDescription auth) {
        if(auth==null) return "";
        try {CharSequence label=context.getPackageManager().getText(auth.packageName,auth.labelId,null);if(label!=null) return text(label);}
        catch(RuntimeException unavailable) {}return text(auth.type);
    }
    private List<AuthenticatorDescription> providers() {
        List<AuthenticatorDescription> result=new ArrayList<>(Arrays.asList(manager.getAuthenticatorTypes()));
        result.sort(Comparator.comparing((AuthenticatorDescription auth)->label(auth)).thenComparing(auth->auth.type));return result;
    }
    private AuthenticatorDescription provider(String type) {
        for(AuthenticatorDescription auth:providers()) if(auth.type.equals(type)) return auth;return null;
    }
    private List<Account> accounts() {
        List<Account> result=new ArrayList<>(Arrays.asList(manager.getAccounts()));
        result.sort(Comparator.comparing((Account account)->account.type).thenComparing(account->account.name));return result;
    }
    public synchronized Account observedAccount(String key) {
        AccountsSettingsContract.key(key);if(!observedAccounts.contains(key)) return null;
        return resolveConfirmationAccount(key);
    }
    /** Fresh lookup for the unexported confirmation continuation; this does not grant command authority. */
    public synchronized Account resolveConfirmationAccount(String key) {
        AccountsSettingsContract.key(key);if(!unlocked()) return null;
        for(Account account:accounts()) if(accountKey(account).equals(key)) return account;return null;
    }
    private String visibility(boolean available) {return !available?"unavailable":full?"full":"limited";}
    private Boolean masterSync() {
        if(!unlocked()||!permission(Manifest.permission.READ_SYNC_SETTINGS)) return null;
        try {return ContentResolver.getMasterSyncAutomatically();}catch(SecurityException|IllegalStateException unavailable) {return null;}
    }
    private boolean canSync() {return unlocked()&&permission(Manifest.permission.WRITE_SYNC_SETTINGS);}
    private boolean resolves(Intent intent) {return intent.resolveActivity(context.getPackageManager())!=null;}
    private Intent accountAdd(AuthenticatorDescription auth) {
        // Pinned Settings.AccountPreferenceBase.ACCOUNT_TYPES_FILTER_KEY.
        return new Intent(Settings.ACTION_ADD_ACCOUNT).setPackage("com.android.settings").putExtra("account_types",new String[]{auth.type});
    }
    public synchronized Intent addIntent(String key) {
        AccountsSettingsContract.key(key);if(!observedProviders.contains(key)) return null;for(AuthenticatorDescription auth:providers()) if(providerKey(auth).equals(key)) {
            Intent intent=accountAdd(auth);if(canModify(auth.type)&&resolves(intent)) return intent;return null;
        }return null;
    }
    public Intent accessIntent() {
        if(full||!unlocked()) return null;
        Intent intent=AccountManager.newChooseAccountIntent(null,null,null,"Choose an account visible to OctoSense",null,null,null);
        return resolves(intent)?intent:null;
    }
    public synchronized JSONObject snapshot(long id) throws Exception {
        if(id<=0) throw new IllegalArgumentException("Positive request ID required");
        boolean available=unlocked();List<Account> accounts=new ArrayList<>();List<AuthenticatorDescription> providers=new ArrayList<>();
        if(available) try {accounts=accounts();providers=providers();}catch(SecurityException|IllegalStateException unavailable) {available=false;}
        if(!available) {accounts.clear();providers.clear();}
        observedAccounts.clear();observedProviders.clear();long now=SystemClock.elapsedRealtime();
        JSONArray accountRows=new JSONArray(),providerRows=new JSONArray();
        for(int index=0;index<Math.min(accounts.size(),AccountsSettingsContract.MAX_ACCOUNTS);index++) {
            Account account=accounts.get(index);observedAccounts.add(accountKey(account));accountRows.put(new JSONObject().put("key",accountKey(account)).put("name",text(account.name))
                    .put("type",text(account.type)).put("label",label(provider(account.type))));
        }
        for(int index=0;index<Math.min(providers.size(),AccountsSettingsContract.MAX_PROVIDERS);index++) {
            AuthenticatorDescription auth=providers.get(index);observedProviders.add(providerKey(auth));providerRows.put(new JSONObject().put("key",providerKey(auth)).put("type",text(auth.type))
                    .put("label",label(auth)).put("can_add",canModify(auth.type)&&resolves(accountAdd(auth))));
        }
        Boolean master=masterSync();masterObservedAt=master==null?-1:now;
        return new JSONObject().put("schema",1).put("request_id",id).put("visibility",visibility(available))
                .put("master_sync",master==null?JSONObject.NULL:master).put("can_set_master_sync",master!=null&&canSync())
                .put("can_request_access",accessIntent()!=null).put("accounts",accountRows).put("accounts_total",accounts.size())
                .put("accounts_truncated",accounts.size()>AccountsSettingsContract.MAX_ACCOUNTS).put("providers",providerRows).put("providers_total",providers.size())
                .put("providers_truncated",providers.size()>AccountsSettingsContract.MAX_PROVIDERS);
    }
    private List<SyncAdapterType> authorities(Account account) {
        Map<String,SyncAdapterType> result=new LinkedHashMap<>();
        for(SyncAdapterType adapter:ContentResolver.getSyncAdapterTypes()) if(adapter.accountType.equals(account.type)&&adapter.isUserVisible()) result.put(adapter.authority,adapter);
        List<SyncAdapterType> rows=new ArrayList<>(result.values());rows.sort(Comparator.comparing(adapter->adapter.authority));return rows;
    }
    private String authorityLabel(SyncAdapterType adapter) {
        try {ProviderInfo provider=context.getPackageManager().resolveContentProvider(adapter.authority,0);if(provider!=null) return text(provider.loadLabel(context.getPackageManager()));}
        catch(RuntimeException unavailable) {}return text(adapter.authority);
    }
    private Boolean automatic(Account account,String authority) {
        if(!permission(Manifest.permission.READ_SYNC_SETTINGS)) return null;
        try {return ContentResolver.getSyncAutomatically(account,authority);}catch(SecurityException|IllegalStateException unavailable) {return null;}
    }
    private Boolean syncable(Account account,String authority) {
        if(!permission(Manifest.permission.READ_SYNC_SETTINGS)) return null;
        try {int value=ContentResolver.getIsSyncable(account,authority);return value<0?null:value>0;}catch(SecurityException|IllegalStateException unavailable) {return null;}
    }
    private Boolean activity(Account account,String authority,boolean pending) {
        if(!permission(Manifest.permission.READ_SYNC_STATS)) return null;
        try {return pending?ContentResolver.isSyncPending(account,authority):ContentResolver.isSyncActive(account,authority);}
        catch(SecurityException|IllegalStateException unavailable) {return null;}
    }
    private JSONArray actions(Account account,SyncAdapterType adapter) {
        JSONArray actions=new JSONArray();if(!canSync()) return actions;
        if(Boolean.TRUE.equals(syncable(account,adapter.authority))) {
            if(automatic(account,adapter.authority)!=null) actions.put("auto");actions.put("sync_now");
        }
        if(Boolean.TRUE.equals(activity(account,adapter.authority,true))||Boolean.TRUE.equals(activity(account,adapter.authority,false))) actions.put("cancel");
        return actions;
    }
    private static Object observed(Boolean value) {return value==null?JSONObject.NULL:value;}
    public boolean canRemove(Account account) {return full&&account!=null&&canModify(account.type);}
    public synchronized JSONObject details(long id,String key) throws Exception {
        if(id<=0) throw new IllegalArgumentException("Positive request ID required");AccountsSettingsContract.key(key);
        boolean available=unlocked();Account account=null;
        if(available) try {account=resolveConfirmationAccount(key);}catch(SecurityException|IllegalStateException unavailable) {available=false;}
        JSONObject result=new JSONObject().put("schema",1).put("request_id",id).put("key",key).put("exists",account!=null)
                .put("visibility",visibility(available)).put("can_remove",canRemove(account));
        List<SyncAdapterType> authorities=new ArrayList<>();boolean authorityAccess=account!=null&&permission(Manifest.permission.READ_SYNC_SETTINGS);
        if(authorityAccess) try {authorities=authorities(account);}catch(SecurityException|IllegalStateException unavailable) {authorityAccess=false;}
        JSONArray rows=new JSONArray();
        if(account!=null) {
            observedAccounts.add(key);
            result.put("name",text(account.name)).put("type",text(account.type)).put("label",label(provider(account.type)));
            for(int index=0;index<Math.min(authorities.size(),AccountsSettingsContract.MAX_AUTHORITIES);index++) {
                SyncAdapterType adapter=authorities.get(index);observedAuthorities.add(authorityKey(account,adapter));rows.put(new JSONObject().put("key",authorityKey(account,adapter)).put("authority",text(adapter.authority))
                        .put("label",authorityLabel(adapter)).put("automatic",observed(automatic(account,adapter.authority))).put("syncable",observed(syncable(account,adapter.authority)))
                        .put("active",observed(activity(account,adapter.authority,false))).put("pending",observed(activity(account,adapter.authority,true))).put("actions",actions(account,adapter)));
            }
        }
        return result.put("authorities_available",authorityAccess).put("authorities",rows).put("authorities_total",authorities.size())
                .put("truncated",authorities.size()>AccountsSettingsContract.MAX_AUTHORITIES);
    }
    public synchronized String master(boolean enabled) {
        if(!freshMaster()||!canSync()||masterSync()==null) return "sync_unavailable";
        try {ContentResolver.setMasterSyncAutomatically(enabled);return Boolean.valueOf(enabled).equals(masterSync())?"sync_applied":"sync_requested";}
        catch(SecurityException|IllegalStateException unavailable) {return "sync_unavailable";}
    }
    public synchronized String sync(String key,String authorityKey,SyncAction action,Boolean enabled) {
        AccountsSettingsContract.key(key);AccountsSettingsContract.key(authorityKey);AccountsSettingsContract.syncValue(action,enabled);
        try {
            Account account=observedAccount(key);if(account==null||!observedAuthorities.contains(authorityKey)) return "account_target_changed";
            for(SyncAdapterType adapter:authorities(account)) if(authorityKey(account,adapter).equals(authorityKey)) {
                JSONArray actions=actions(account,adapter);boolean allowed=false;
                for(int index=0;index<actions.length();index++) if(action.wire.equals(actions.optString(index))) allowed=true;
                if(!allowed) return "sync_unavailable";
                switch(action) {
                    case AUTO: ContentResolver.setSyncAutomatically(account,adapter.authority,enabled);
                        return enabled.equals(automatic(account,adapter.authority))?"sync_applied":"sync_requested";
                    case SYNC_NOW: Bundle extras=new Bundle();extras.putBoolean(ContentResolver.SYNC_EXTRAS_MANUAL,true);extras.putBoolean(ContentResolver.SYNC_EXTRAS_EXPEDITED,true);
                        ContentResolver.requestSync(account,adapter.authority,extras);return "sync_requested";
                    case CANCEL: ContentResolver.cancelSync(account,adapter.authority);return "sync_requested";
                    default:throw new IllegalArgumentException("Unknown sync action");
                }
            }
            return "account_target_changed";
        }catch(SecurityException|IllegalStateException unavailable) {return "sync_unavailable";}
    }
}
