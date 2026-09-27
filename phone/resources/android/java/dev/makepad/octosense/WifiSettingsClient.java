package dev.makepad.octosense;

import android.content.Context;
import android.content.Intent;
import android.net.Uri;
import android.os.SystemClock;
import android.provider.Settings;
import dev.makepad.octosense.agent.AgentPlatformClient;
import dev.makepad.octosense.wifi.WifiSettingsBackend;
import dev.makepad.octosense.wifi.WifiSettingsContract;
import java.util.HashSet;
import org.json.JSONArray;
import org.json.JSONObject;

/** Worker-owned router; only the signature-gated ROM helper manages networks. */
final class WifiSettingsClient implements AutoCloseable {
    private final Context context;
    private final AgentPlatformClient agent;
    private final WifiSettingsBackend local;
    private final HashSet<String> observed=new HashSet<>();
    private long observedAt;
    WifiSettingsClient(Context context,AgentPlatformClient agent) {
        this.context=context;this.agent=agent;local=new WifiSettingsBackend(context,false);
    }
    JSONObject snapshot(long id) throws Exception {
        observed.clear();observedAt=0;
        JSONObject result=agent.wifiSnapshot(id);
        if(result==null) result=local.snapshot(id);
        JSONArray networks=result.getJSONArray("networks");
        if(networks.length()>WifiSettingsContract.MAX_NETWORKS) throw new IllegalArgumentException("Wi-Fi snapshot too large");
        for(int i=0;i<networks.length();i++) observed.add(WifiSettingsContract.key(networks.getJSONObject(i).getString("key")));
        observedAt=SystemClock.elapsedRealtime();return result;
    }
    String enabled(boolean value) throws Exception {return agent.wifiEnabled(value);}
    String scan() throws Exception {return agent.has("wifi_settings_v1")?agent.wifiScan():local.scan();}
    String network(String key,WifiSettingsContract.Action action) throws Exception {
        WifiSettingsContract.key(key);
        if(!observed.contains(key)||SystemClock.elapsedRealtime()-observedAt>20000) return "wifi_target_changed";
        String result=agent.wifiNetwork(key,action);observed.clear();observedAt=0;return result;
    }
    Intent configure(String key) throws Exception {
        WifiSettingsContract.key(key);
        if(!observed.contains(key)||SystemClock.elapsedRealtime()-observedAt>20000) return null;
        Intent intent=agent.has("wifi_settings_v1")?agent.wifiConfiguration(key):local.configureObserved(key);
        // Older helpers may have resolved this generic fallback before Home
        // advertised its entry alias. Keep credential configuration in Android.
        if(intent!=null&&Settings.ACTION_WIFI_SETTINGS.equals(intent.getAction()))
            intent=new Intent(intent).setComponent(null).setPackage("com.android.settings");
        return intent;
    }
    Intent access() {
        return new Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS,Uri.fromParts("package",context.getPackageName(),null));
    }
    @Override public void close() {local.close();observed.clear();}
}
