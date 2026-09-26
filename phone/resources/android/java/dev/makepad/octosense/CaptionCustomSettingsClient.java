package dev.makepad.octosense;

import android.os.Binder;
import android.os.SystemClock;
import dev.makepad.octosense.agent.AgentPlatformClient;
import dev.makepad.octosense.controls.CaptionCustomContract;
import dev.makepad.octosense.controls.CaptionCustomSettings.Field;
import java.security.SecureRandom;
import java.util.EnumMap;
import java.util.HashSet;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.atomic.AtomicLong;
import java.util.concurrent.atomic.AtomicReference;
import java.util.function.BooleanSupplier;
import org.json.JSONArray;
import org.json.JSONObject;

/** Process authority is independent of an Activity instance and of Rust draw areas. */
final class CaptionCustomSettingsClient {
    private static final Binder OWNER=new Binder();
    private static final String SESSION=session();
    private static String session(){byte[] bytes=new byte[32];new SecureRandom().nextBytes(bytes);StringBuilder out=new StringBuilder();for(byte value:bytes)out.append(String.format(java.util.Locale.ROOT,"%02x",value&255));return out.toString();}
    private final AgentPlatformClient agent;private final BooleanSupplier foreground;
    private final AtomicLong fence=new AtomicLong(),active=new AtomicLong(),retired=new AtomicLong();
    private final AtomicReference<Observation> observed=new AtomicReference<>();
    private static final class Observation {final long visit,time;final Map<Field,Set<String>> choices;Observation(long visit,Map<Field,Set<String>> choices){this.visit=visit;this.choices=choices;time=SystemClock.elapsedRealtime();}}
    CaptionCustomSettingsClient(AgentPlatformClient agent,BooleanSupplier foreground){this.agent=agent;this.foreground=foreground;}
    /** Safe on the UI thread: only atomic retirement and a captured asynchronous Binder close. */
    void retireInBackground(){long visit=active.getAndSet(0);fence.incrementAndGet();observed.set(null);if(visit>0)closeInBackground(visit);}
    void closeInBackground(long visit){
        if(visit<=0)return;retired.accumulateAndGet(visit,Math::max);active.compareAndSet(visit,0);
        Observation value=observed.get();if(value!=null&&value.visit==visit)observed.compareAndSet(value,null);
        Runnable task=agent.captionCustomCloseTask(OWNER,SESSION,visit);Thread thread=new Thread(task,"OctoSenseCaptionClose");thread.setDaemon(true);thread.start();
    }
    JSONObject snapshot(long id,long visit)throws Exception{
        CaptionCustomContract.read(id,visit);long generation=fence.get();
        if(visit<=retired.get()||!foreground.getAsBoolean())return CaptionCustomContract.unavailable(id,visit);
        long old=active.getAndSet(visit);if(old>0&&old!=visit)closeInBackground(old);
        JSONObject state=agent.captionCustomSnapshot(id,OWNER,SESSION,visit);
        if(generation!=fence.get()||!foreground.getAsBoolean()||visit<=retired.get()){closeInBackground(visit);return CaptionCustomContract.unavailable(id,visit);}
        if(state==null){observed.set(null);return CaptionCustomContract.unavailable(id,visit);}
        observed.set(null);
        if(state.getInt("schema")!=1||state.getLong("request_id")!=id||state.getLong("visit")!=visit||!"caption_custom".equals(state.getString("page")))throw new IllegalStateException("Wrong caption observation");
        JSONArray rows=state.getJSONArray("controls");if(rows.length()!=Field.values().length)throw new IllegalStateException("Wrong caption row count");
        Map<Field,Set<String>> choices=new EnumMap<>(Field.class);
        for(int i=0;i<rows.length();i++){JSONObject row=rows.getJSONObject(i);Field field=Field.parse(row.getString("id"));if(choices.containsKey(field))throw new IllegalStateException("Repeated caption field");Set<String> values=new HashSet<>();JSONArray options=row.getJSONArray("options");if(options.length()>field.choices().length)throw new IllegalStateException("Oversized caption choices");for(int j=0;j<options.length();j++){String option=options.getString(j);field.validate(option);if(!values.add(option))throw new IllegalStateException("Repeated caption choice");}if(!values.isEmpty()&&(!"available".equals(row.getString("availability"))||row.isNull("value")||!state.getBoolean("scope_active")||!state.optBoolean("custom_selected",false)))throw new IllegalStateException("Unobserved caption authority");choices.put(field,values);}
        if(generation==fence.get()&&foreground.getAsBoolean()&&visit>retired.get())observed.set(new Observation(visit,choices));
        else{closeInBackground(visit);return CaptionCustomContract.unavailable(id,visit);}
        return state;
    }
    String select(long visit,String fieldName,String choice)throws Exception{
        Field field=Field.parse(fieldName);field.validate(choice);Observation state=observed.get();long now=SystemClock.elapsedRealtime();
        if(!foreground.getAsBoolean()||state==null||state.visit!=visit||active.get()!=visit||visit<=retired.get()||now<state.time||now-state.time>20000||!state.choices.get(field).contains(choice))return "control_unavailable";
        if(!observed.compareAndSet(state,null))return "control_unavailable";
        return agent.captionCustomSet(OWNER,SESSION,visit,fieldName,choice);
    }
}
