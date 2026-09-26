package dev.makepad.octosense;

import android.os.SystemClock;
import dev.makepad.octosense.agent.AgentPlatformClient;
import dev.makepad.octosense.applanguage.AppLanguageContract;
import java.util.HashSet;
import java.util.Set;
import java.util.function.BooleanSupplier;
import org.json.JSONArray;
import org.json.JSONObject;

/** Only choices in the current foreground page authorize one native locale selection. */
final class AppLanguageSettingsClient {
    private final AgentPlatformClient agent;private final BooleanSupplier foreground;
    private String pkg,key;private long observed=-1;private final Set<String> choices=new HashSet<>();
    AppLanguageSettingsClient(AgentPlatformClient agent,BooleanSupplier foreground){this.agent=agent;this.foreground=foreground;}
    synchronized void invalidate(){pkg=null;key=null;observed=-1;choices.clear();}
    synchronized JSONObject snapshot(long id,String packageName,String catalogKey,String parent,String query,int offset)throws Exception{
        AppLanguageContract.read(id,packageName,catalogKey,parent,query,offset);invalidate();
        if(!foreground.getAsBoolean())return AppLanguageContract.unavailable(id,packageName,query,"restricted","locked");
        JSONObject state=agent.appLanguageSnapshot(id,packageName,catalogKey,parent,query,offset);
        if(state==null||!foreground.getAsBoolean())return AppLanguageContract.unavailable(id,packageName,query,"unavailable","service_unavailable");
        if(state.getLong("request_id")!=id||!packageName.equals(state.getString("package"))||!query.equals(state.getString("query")))throw new IllegalStateException("Wrong language observation");
        if(!"available".equals(state.getString("availability")))return state;
        JSONArray rows=state.getJSONArray("rows");if(rows.length()>AppLanguageContract.PAGE_SIZE)throw new IllegalStateException("Oversized language page");
        if(state.getBoolean("can_set"))for(int i=0;i<rows.length();i++){JSONObject row=rows.getJSONObject(i);if("select".equals(row.getString("kind"))&&!choices.add(AppLanguageContract.key(row.getString("key"))))throw new IllegalStateException("Repeated language choice");}
        pkg=packageName;key=AppLanguageContract.key(state.getString("key"));observed=SystemClock.elapsedRealtime();return state;
    }
    synchronized String select(String packageName,String observedKey,String choice)throws Exception{
        AppLanguageContract.packageName(packageName);AppLanguageContract.key(observedKey);AppLanguageContract.key(choice);long now=SystemClock.elapsedRealtime();
        if(!foreground.getAsBoolean()||!packageName.equals(pkg)||!observedKey.equals(key)||observed<0||now<observed||now-observed>20000||!choices.contains(choice))return "app_language_target_changed";
        invalidate();return agent.appLanguageSet(packageName,observedKey,choice);
    }
}
