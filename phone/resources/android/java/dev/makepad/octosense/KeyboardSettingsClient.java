package dev.makepad.octosense;

import android.app.PendingIntent;
import android.os.SystemClock;
import dev.makepad.octosense.keyboards.KeyboardContract;
import dev.makepad.octosense.keyboards.KeyboardJson;
import dev.makepad.octosense.keyboards.KeyboardPolicy;
import java.util.HashMap;
import java.util.HashSet;
import java.util.Map;
import java.util.Set;
import java.util.function.BooleanSupplier;
import org.json.JSONArray;
import org.json.JSONObject;

/** Visible-page lease only; native service reobserves policy and owns the consent flow. */
final class KeyboardSettingsClient {
    interface Bridge {JSONObject snapshot(long id,String query,int offset)throws Exception;PendingIntent prepare(long id,String key,String target,String operation)throws Exception;}
    private final Bridge bridge;private final BooleanSupplier foreground;
    private String key;private long observed;private boolean picker;
    private final Map<String,Set<KeyboardPolicy.Action>> targets=new HashMap<>();
    KeyboardSettingsClient(Bridge bridge,BooleanSupplier foreground){this.bridge=bridge;this.foreground=foreground;}
    synchronized void invalidate(){key=null;observed=0;picker=false;targets.clear();}
    synchronized JSONObject snapshot(long id,String query,int offset)throws Exception{
        KeyboardContract.read(id,query,offset);invalidate();
        if(!foreground.getAsBoolean())return KeyboardJson.unavailable(id,query,"restricted");
        JSONObject state=bridge.snapshot(id,query,offset);
        if(state==null||!foreground.getAsBoolean())return KeyboardJson.unavailable(id,query,"unavailable");
        if(state.getLong("request_id")!=id||!query.equals(state.getString("query")))throw new IllegalStateException("Mismatched keyboard observation");
        if(!"available".equals(state.getString("availability")))return state;
        String observedKey=state.getString("key");KeyboardContract.key(observedKey);JSONArray rows=state.getJSONArray("rows");if(rows.length()>KeyboardPolicy.PAGE_SIZE)throw new IllegalStateException("Oversized keyboard page");
        for(int i=0;i<rows.length();i++){JSONObject row=rows.getJSONObject(i);String target=row.getString("target");KeyboardContract.key(target);Set<KeyboardPolicy.Action> actions=new HashSet<>();
            if(row.getBoolean("can_enable")&&!row.getBoolean("enabled"))actions.add(KeyboardPolicy.Action.ENABLE);
            if(row.getBoolean("can_disable")&&row.getBoolean("enabled"))actions.add(KeyboardPolicy.Action.DISABLE);
            if(row.getBoolean("can_settings"))actions.add(KeyboardPolicy.Action.SETTINGS);
            if(row.getBoolean("can_subtypes"))actions.add(KeyboardPolicy.Action.SUBTYPES);
            if(targets.put(target,actions)!=null)throw new IllegalStateException("Duplicate keyboard target");
        }
        key=observedKey;picker=state.getBoolean("can_choose_default");observed=SystemClock.elapsedRealtime();return state;
    }
    synchronized PendingIntent prepare(long id,String key,String target,String operation)throws Exception{
        KeyboardPolicy.Action action=KeyboardContract.flow(id,key,target,operation);long now=SystemClock.elapsedRealtime();
        if(!foreground.getAsBoolean()||!key.equals(this.key)||now<observed||now-observed>KeyboardPolicy.OBSERVED_MS)return null;
        if(action==KeyboardPolicy.Action.PICK_DEFAULT?!picker:!targets.getOrDefault(target,java.util.Collections.emptySet()).contains(action))return null;
        invalidate();return bridge.prepare(id,key,target,operation);
    }
}
