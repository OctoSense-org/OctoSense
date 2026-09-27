package dev.makepad.octosense;

import android.content.Context;
import android.os.SystemClock;
import dev.makepad.octosense.agent.AgentPlatformClient;
import dev.makepad.octosense.network.NetworkSettingsBackend;
import dev.makepad.octosense.network.NetworkSettingsContract;
import org.json.JSONObject;

final class NetworkSettingsClient {
    private final AgentPlatformClient agent;
    private final NetworkSettingsBackend local;
    private String observed;
    private long at=-1;
    private boolean remote,airplane,saver,dns;
    NetworkSettingsClient(Context context,AgentPlatformClient agent) {this.agent=agent;local=new NetworkSettingsBackend(context,false,null);}
    JSONObject snapshot(long id) throws Exception {
        observed=null;at=-1;airplane=saver=dns=false;
        JSONObject state=agent.networkSnapshot(id);remote=state!=null;if(state==null) state=local.snapshot(id);
        observed=NetworkSettingsContract.key(state.getString("key"));at=SystemClock.elapsedRealtime();
        airplane=state.getJSONObject("airplane").getBoolean("can_set");saver=state.getJSONObject("data_saver").getBoolean("can_set");dns=state.getJSONObject("private_dns").getBoolean("can_set");
        return state;
    }
    private boolean fresh(String key) {NetworkSettingsContract.key(key);return remote&&key.equals(observed)&&NetworkSettingsContract.recent(at,SystemClock.elapsedRealtime());}
    String airplane(String key,boolean enabled) throws Exception {
        if(!fresh(key)||!airplane) return "network_target_changed";at=-1;return agent.networkAirplane(key,enabled);
    }
    String dataSaver(String key,boolean enabled) throws Exception {
        if(!fresh(key)||!saver) return "network_target_changed";at=-1;return agent.networkDataSaver(key,enabled);
    }
    String privateDns(String key,String mode,String hostname) throws Exception {
        String normalized=NetworkSettingsContract.dnsHostname(mode,hostname);
        if(!fresh(key)||!dns) return "network_target_changed";at=-1;return agent.networkPrivateDns(key,mode,normalized);
    }
}
