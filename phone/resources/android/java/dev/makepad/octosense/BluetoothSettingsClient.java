package dev.makepad.octosense;

import android.content.Context;
import android.content.Intent;
import android.net.Uri;
import android.provider.Settings;
import android.os.SystemClock;
import dev.makepad.octosense.agent.AgentPlatformClient;
import dev.makepad.octosense.bluetooth.BluetoothSettingsBackend;
import dev.makepad.octosense.bluetooth.BluetoothSettingsContract;
import java.util.HashSet;
import org.json.JSONArray;
import org.json.JSONObject;

/** Worker-owned Bluetooth routing; renderer target keys never become MAC addresses. */
final class BluetoothSettingsClient implements AutoCloseable {
    private final Context context;
    private final AgentPlatformClient agent;
    private final BluetoothSettingsBackend local;
    private final HashSet<String> observed=new HashSet<>();
    private long observedAt;
    BluetoothSettingsClient(Context context,AgentPlatformClient agent) {
        this.context=context;this.agent=agent;local=new BluetoothSettingsBackend(context,false,null);
    }
    JSONObject snapshot(long id) throws Exception {
        observed.clear();observedAt=0;JSONObject result=agent.bluetoothSnapshot(id);
        if(result==null) result=local.snapshot(id);
        JSONArray rows=result.getJSONArray("devices");
        if(rows.length()>BluetoothSettingsContract.MAX_DEVICES) throw new IllegalArgumentException("Bluetooth snapshot too large");
        for(int i=0;i<rows.length();i++) observed.add(BluetoothSettingsContract.key(rows.getJSONObject(i).getString("key")));
        observedAt=SystemClock.elapsedRealtime();return result;
    }
    String enabled(boolean value) throws Exception {return agent.bluetoothEnabled(value);}
    String scan(boolean value) throws Exception {return agent.has("bluetooth_settings_v1")?agent.bluetoothScan(value):local.scan(value);}
    String name(String value) throws Exception {return agent.bluetoothName(BluetoothSettingsContract.name(value));}
    private boolean current(String key) {
        BluetoothSettingsContract.key(key);return observed.contains(key)&&SystemClock.elapsedRealtime()-observedAt<=20000;
    }
    String device(String key,BluetoothSettingsContract.Action action) throws Exception {
        if(!current(key)) return "bluetooth_target_changed";
        String result=agent.bluetoothDevice(key,action);observed.clear();observedAt=0;return result;
    }
    String sharing(String key,String kind,String value) throws Exception {
        BluetoothSettingsContract.sharingKind(kind);BluetoothSettingsContract.sharingValue(value);
        if(!current(key)) return "bluetooth_target_changed";
        return agent.bluetoothSharing(key,kind,value);
    }
    Intent access() {return new Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS,Uri.fromParts("package",context.getPackageName(),null));}
    @Override public void close() {local.close();observed.clear();}
}
