package dev.makepad.octosense;

import android.content.Context;
import android.os.SystemClock;
import dev.makepad.octosense.agent.AgentPlatformClient;
import dev.makepad.octosense.updates.UpdatesSettingsContract;
import dev.makepad.octosense.updates.UpdatesSettingsSnapshot;
import java.util.HashSet;
import org.json.JSONObject;
import org.json.JSONArray;

/** No updater service means observed versions only. Never replay a mutation. */
final class UpdatesSettingsClient {
    private final Context context;
    private final AgentPlatformClient agent;
    private final HashSet<String> capabilities=new HashSet<>();
    private String offerKey,rebootKey;
    private long observed=-1;
    UpdatesSettingsClient(Context context,AgentPlatformClient agent) {this.context=context;this.agent=agent;}
    JSONObject snapshot(long id) throws Exception {
        capabilities.clear();offerKey=null;rebootKey=null;observed=-1;
        JSONObject state=agent.updatesSnapshot(id);
        if(state==null) return UpdatesSettingsSnapshot.unavailable(context,id,"unavailable");
        JSONArray caps=state.getJSONArray("capabilities");
        if(caps.length()>4) throw new IllegalArgumentException("Invalid update capabilities");
        for(int i=0;i<caps.length();i++) capabilities.add(caps.getString(i));
        JSONObject offer=state.optJSONObject("offer");
        if(offer!=null) offerKey=UpdatesSettingsContract.key(offer.getString("key"));
        if(state.has("reboot_key")) rebootKey=UpdatesSettingsContract.key(state.getString("reboot_key"));
        observed=SystemClock.elapsedRealtime();return state;
    }
    private boolean allowed(String capability) {
        return UpdatesSettingsContract.recent(observed,SystemClock.elapsedRealtime(),UpdatesSettingsContract.OBSERVATION_TTL_MS)&&capabilities.contains(capability);
    }
    String check() throws Exception {
        if(!allowed("check")) return "update_unavailable";
        capabilities.clear();observed=-1;return agent.updatesCheck();
    }
    String install(String key,String part) throws Exception {
        UpdatesSettingsContract.key(key);UpdatesSettingsContract.part(part);
        if(!key.equals(offerKey)||!allowed("install_"+part)) return "update_target_changed";
        capabilities.clear();observed=-1;return agent.updatesInstall(key,part);
    }
    String reboot(String key) throws Exception {
        UpdatesSettingsContract.key(key);
        if(!key.equals(rebootKey)||!allowed("reboot")) return "update_target_changed";
        capabilities.clear();observed=-1;return agent.updatesReboot(key);
    }
}
