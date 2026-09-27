package dev.makepad.octosense;

import android.os.SystemClock;
import dev.makepad.octosense.systemlanguage.SystemLanguageContract;
import java.util.HashSet;
import java.util.Set;
import java.util.function.BooleanSupplier;
import org.json.JSONArray;
import org.json.JSONObject;

/** A foreground reviewed list may contain leaves observed on earlier pages of the same catalog. */
final class SystemLanguageSettingsClient {
    interface Bridge {JSONObject snapshot(long id,String key,String parent,String query,int offset)throws Exception;String apply(String key,String[] targets)throws Exception;}
    private final Bridge bridge;private final BooleanSupplier foreground;
    private String key;private long created;private boolean writable;private final Set<String> choices=new HashSet<>();
    SystemLanguageSettingsClient(Bridge bridge,BooleanSupplier foreground){this.bridge=bridge;this.foreground=foreground;}
    synchronized void invalidate(){key=null;choices.clear();writable=false;created=0;}
    synchronized JSONObject snapshot(long id,String requested,String parent,String query,int offset)throws Exception{
        SystemLanguageContract.read(id,requested,parent,query,offset);
        if(!foreground.getAsBoolean()){invalidate();return SystemLanguageContract.unavailable(id,query,"restricted","locked");}
        if(!requested.isEmpty()&&!requested.equals(key)){invalidate();return SystemLanguageContract.unavailable(id,query,"stale","target_changed");}
        JSONObject state=bridge.snapshot(id,requested,parent,query,offset);
        if(state==null||!foreground.getAsBoolean()){invalidate();return SystemLanguageContract.unavailable(id,query,"unavailable","service_unavailable");}
        if(state.getLong("request_id")!=id||!query.equals(state.getString("query")))throw new IllegalStateException("Wrong system language observation");
        if(!"available".equals(state.getString("availability"))){invalidate();return state;}
        String observed=SystemLanguageContract.key(state.getString("key"));JSONArray rows=state.getJSONArray("rows"),current=state.getJSONArray("current");
        if(rows.length()>SystemLanguageContract.PAGE_SIZE||current.length()<1||current.length()>SystemLanguageContract.MAX_CURRENT)throw new IllegalStateException("Invalid system language page");
        if(!observed.equals(key)){invalidate();key=observed;created=SystemClock.elapsedRealtime();}
        for(int i=0;i<current.length();i++)choices.add(SystemLanguageContract.key(current.getJSONObject(i).getString("target")));
        for(int i=0;i<rows.length();i++){JSONObject row=rows.getJSONObject(i);if("select".equals(row.getString("kind")))choices.add(SystemLanguageContract.key(row.getString("target")));}
        writable=state.getBoolean("can_apply");return state;
    }
    synchronized String apply(String observed,String[] targets)throws Exception{
        SystemLanguageContract.order(observed,targets);long now=SystemClock.elapsedRealtime();
        if(!foreground.getAsBoolean()||!writable||!observed.equals(key)||now<created||now-created>600000)return "languages_target_changed";
        for(String target:targets)if(!choices.contains(target))return "languages_target_changed";
        invalidate();return bridge.apply(observed,targets);
    }
}
