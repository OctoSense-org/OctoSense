package dev.makepad.octosense;

import android.os.SystemClock;
import dev.makepad.octosense.agent.AgentPlatformClient;
import dev.makepad.octosense.dnd.DndSettingsBackend;
import dev.makepad.octosense.dnd.DndSettingsContract;
import java.util.HashSet;
import java.util.Set;
import java.util.function.BooleanSupplier;
import org.json.JSONArray;
import org.json.JSONObject;

/** Only the focused trusted Settings host can claim an actually observed policy or rule. */
final class DndSettingsClient {
    private final AgentPlatformClient agent;private final BooleanSupplier foreground;
    private final DndSettingsBackend unavailable=new DndSettingsBackend(null,SystemClock::elapsedRealtime);
    private String key;private long observed=-1;private boolean create;
    private final Set<String> editable=new HashSet<>(),deletable=new HashSet<>(),fields=new HashSet<>();
    DndSettingsClient(AgentPlatformClient agent,BooleanSupplier foreground){this.agent=agent;this.foreground=foreground;}
    synchronized void invalidate(){key=null;observed=-1;create=false;editable.clear();deletable.clear();fields.clear();}
    synchronized JSONObject snapshot(long id,int offset,String generation)throws Exception {
        DndSettingsContract.offset(offset,generation);invalidate();
        if(!foreground.getAsBoolean())return unavailable.snapshot(id,offset,generation);
        JSONObject state=agent.dndSnapshot(id,offset,generation);
        if(state==null||!foreground.getAsBoolean())return unavailable.snapshot(id,offset,generation);
        if(state.getLong("request_id")!=id)throw new IllegalStateException("Mismatched DND observation");
        if(!"available".equals(state.getString("availability")))return state;
        key=DndSettingsContract.key(state.getString("key"));create=state.getBoolean("can_create");
        JSONArray policy=state.getJSONArray("policy"),rules=state.getJSONArray("rules");
        if(policy.length()!=DndSettingsContract.Field.values().length||rules.length()>20)throw new IllegalStateException("Oversized DND observation");
        for(int i=0;i<policy.length();i++){JSONObject row=policy.getJSONObject(i);DndSettingsContract.Field field=DndSettingsContract.Field.parse(row.getString("field"));if(row.getBoolean("can_set")){field.value(row.getString("value"));fields.add(field.wire);}}
        for(int i=0;i<rules.length();i++){JSONObject row=rules.getJSONObject(i);String target=DndSettingsContract.key(row.getString("target"));if(row.getBoolean("can_edit"))editable.add(target);if(row.getBoolean("can_delete"))deletable.add(target);}
        observed=SystemClock.elapsedRealtime();return state;
    }
    private boolean fresh(String key){DndSettingsContract.key(key);long now=SystemClock.elapsedRealtime();return foreground.getAsBoolean()&&key.equals(this.key)&&observed>=0&&now>=observed&&now-observed<=20000;}
    synchronized String policy(String key,DndSettingsContract.Field field,String value)throws Exception{
        field.value(value);if(!fresh(key)||!fields.contains(field.wire))return "dnd_target_changed";invalidate();return agent.dndPolicy(key,field,value);
    }
    synchronized String schedule(String key,String target,DndSettingsContract.Schedule value)throws Exception{
        if(target!=null)DndSettingsContract.key(target);if(!fresh(key)||(target==null?!create:!editable.contains(target)))return "dnd_target_changed";
        invalidate();return agent.dndSchedule(key,target,value);
    }
    synchronized String enabled(String key,String target,boolean enabled)throws Exception{
        DndSettingsContract.key(target);if(!fresh(key)||!editable.contains(target))return "dnd_target_changed";invalidate();return agent.dndEnabled(key,target,enabled);
    }
    synchronized String delete(String key,String target)throws Exception{
        DndSettingsContract.key(target);if(!fresh(key)||!deletable.contains(target))return "dnd_target_changed";invalidate();return agent.dndDelete(key,target);
    }
}
