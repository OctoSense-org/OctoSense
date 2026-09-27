package dev.makepad.octosense;

import android.content.Context;
import android.os.SystemClock;
import dev.makepad.octosense.agent.AgentPlatformClient;
import dev.makepad.octosense.display.DisplaySettingsBackend;
import dev.makepad.octosense.display.DisplaySettingsContract;
import org.json.JSONObject;

/** Pins writes to the helper that produced the observed snapshot; no fallback replay. */
final class DisplaySettingsClient {
    private final AgentPlatformClient agent;
    private final DisplaySettingsBackend local;
    private String observed;
    private long at=-1;
    private boolean remote;
    DisplaySettingsClient(Context context,AgentPlatformClient agent){this.agent=agent;local=new DisplaySettingsBackend(context,null,()->false);}
    JSONObject snapshot(long id)throws Exception {
        observed=null;at=-1;JSONObject state=agent.displaySnapshot(id);remote=state!=null;
        if(state==null)state=local.snapshot(id);
        observed=DisplaySettingsContract.key(state.getString("key"));at=SystemClock.elapsedRealtime();return state;
    }
    String set(String key,DisplaySettingsContract.Setting setting,Object value)throws Exception {
        DisplaySettingsContract.key(key);String encoded=DisplaySettingsContract.encode(setting,value);
        if(!remote||!key.equals(observed)||!DisplaySettingsContract.recent(at,SystemClock.elapsedRealtime()))return "display_target_changed";
        at=-1;return agent.displaySet(key,setting,encoded);
    }
}
