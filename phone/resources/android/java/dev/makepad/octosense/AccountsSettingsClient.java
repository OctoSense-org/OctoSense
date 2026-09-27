package dev.makepad.octosense;

import android.app.PendingIntent;
import android.content.Context;
import android.content.Intent;
import android.os.SystemClock;
import dev.makepad.octosense.accounts.AccountsSettingsBackend;
import dev.makepad.octosense.accounts.AccountsSettingsContract;
import dev.makepad.octosense.accounts.AccountsSettingsContract.SyncAction;
import dev.makepad.octosense.agent.AgentPlatformClient;
import java.util.HashMap;
import java.util.HashSet;
import org.json.JSONArray;
import org.json.JSONObject;

/** One observed route at a time. A failed mutation is never replayed through a fallback. */
final class AccountsSettingsClient {
    static final class Flow {
        final Intent intent;final PendingIntent pending;
        Flow(Intent intent,PendingIntent pending) {this.intent=intent;this.pending=pending;}
    }
    private final AgentPlatformClient agent;
    private final AccountsSettingsBackend local;
    private final HashMap<String,Long> accounts=new HashMap<>();
    private final HashSet<String> providers=new HashSet<>(),authorities=new HashSet<>();
    private boolean remote;
    private long snapshotAt,detailsAt;
    private String selected;
    AccountsSettingsClient(Context context,AgentPlatformClient agent) {this.agent=agent;local=new AccountsSettingsBackend(context);}
    private static boolean recent(long at) {return at>0&&SystemClock.elapsedRealtime()-at<=20000;}
    JSONObject snapshot(long id) throws Exception {
        accounts.clear();providers.clear();authorities.clear();selected=null;snapshotAt=detailsAt=0;
        JSONObject state=agent.accountsSnapshot(id);remote=state!=null;if(state==null) state=local.snapshot(id);
        JSONArray rows=state.getJSONArray("accounts"),types=state.getJSONArray("providers");
        if(rows.length()>AccountsSettingsContract.MAX_ACCOUNTS||types.length()>AccountsSettingsContract.MAX_PROVIDERS) throw new IllegalArgumentException("Account inventory too large");
        long now=SystemClock.elapsedRealtime();
        for(int i=0;i<rows.length();i++) accounts.put(AccountsSettingsContract.key(rows.getJSONObject(i).getString("key")),now);
        for(int i=0;i<types.length();i++) providers.add(AccountsSettingsContract.key(types.getJSONObject(i).getString("key")));
        snapshotAt=now;return state;
    }
    private boolean account(String key) {AccountsSettingsContract.key(key);return recent(accounts.getOrDefault(key,0L));}
    JSONObject details(long id,String key) throws Exception {
        AccountsSettingsContract.key(key);
        // A read may refresh a previously selected target after native auth or
        // a pause. Only mutations require an unexpired observation.
        if(!accounts.containsKey(key)) throw new IllegalArgumentException("Account target changed");
        authorities.clear();selected=null;detailsAt=0;
        JSONObject state=remote?agent.accountDetails(id,key):local.details(id,key);
        if(state==null) throw new IllegalStateException("Account service unavailable");
        if(!key.equals(state.getString("key"))) throw new IllegalArgumentException("Wrong account response");
        JSONArray rows=state.getJSONArray("authorities");
        if(rows.length()>AccountsSettingsContract.MAX_AUTHORITIES) throw new IllegalArgumentException("Authority inventory too large");
        if(state.getBoolean("exists")) {
            for(int i=0;i<rows.length();i++) authorities.add(AccountsSettingsContract.key(rows.getJSONObject(i).getString("key")));
            selected=key;detailsAt=SystemClock.elapsedRealtime();accounts.put(key,detailsAt);
        } else accounts.remove(key);
        return state;
    }
    String master(boolean value) throws Exception {
        if(!recent(snapshotAt)) return "sync_unavailable";
        return remote?agent.accountsMaster(value):local.master(value);
    }
    String sync(String key,String authority,SyncAction action,Boolean enabled) throws Exception {
        AccountsSettingsContract.key(authority);AccountsSettingsContract.syncValue(action,enabled);
        if(!account(key)||!key.equals(selected)||!recent(detailsAt)||!authorities.contains(authority)) return "account_target_changed";
        return remote?agent.accountSync(key,authority,action,enabled):local.sync(key,authority,action,enabled);
    }
    Flow add(String key) throws Exception {
        AccountsSettingsContract.key(key);if(!recent(snapshotAt)||!providers.contains(key)) return null;
        return remote?new Flow(null,agent.accountAddition(key)):new Flow(local.addIntent(key),null);
    }
    Flow remove(String key) throws Exception {
        if(!remote||!account(key)||!key.equals(selected)||!recent(detailsAt)) return null;
        return new Flow(null,agent.accountRemoval(key));
    }
    Intent access() {return local.accessIntent();}
}
