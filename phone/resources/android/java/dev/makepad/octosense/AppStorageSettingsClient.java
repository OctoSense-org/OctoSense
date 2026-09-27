package dev.makepad.octosense;

import android.os.Bundle;
import android.os.SystemClock;
import dev.makepad.octosense.agent.AgentPlatformClient;
import dev.makepad.octosense.appstorage.AppStorageContract;
import java.util.HashSet;
import java.util.Set;
import java.util.function.BooleanSupplier;
import org.json.JSONArray;
import org.json.JSONObject;

/** Native flow tokens stay in Java; scripts receive only finite completion observations. */
final class AppStorageSettingsClient {
    private final AgentPlatformClient agent;private final BooleanSupplier foreground;
    private String pkg,key;private long observed=-1;private final Set<String> actions=new HashSet<>();
    AppStorageSettingsClient(AgentPlatformClient agent,BooleanSupplier foreground){this.agent=agent;this.foreground=foreground;}
    synchronized void invalidate(){pkg=null;key=null;observed=-1;actions.clear();}
    synchronized JSONObject snapshot(long id,String packageName)throws Exception{
        AppStorageContract.packageName(packageName);invalidate();
        if(!foreground.getAsBoolean())return AppStorageContract.unavailable(id,packageName);
        JSONObject state=agent.appStorageSnapshot(id,packageName);
        if(state==null||!foreground.getAsBoolean())return AppStorageContract.unavailable(id,packageName);
        if(state.getLong("request_id")!=id||!packageName.equals(state.getString("package")))throw new IllegalStateException("Wrong storage observation");
        if(!"available".equals(state.getString("availability")))return state;
        JSONArray offered=state.getJSONArray("actions");if(offered.length()>3)throw new IllegalStateException("Invalid storage actions");
        for(int i=0;i<offered.length();i++)if(!actions.add(AppStorageContract.Action.parse(offered.getString(i)).wire))throw new IllegalStateException("Repeated storage action");
        if(state.getBoolean("shared_uid")&&!actions.isEmpty())throw new IllegalStateException("Shared UID deletion offered");
        pkg=packageName;key=AppStorageContract.key(state.getString("key"));observed=SystemClock.elapsedRealtime();return state;
    }
    synchronized Bundle action(String packageName,String observedKey,String action)throws Exception{
        AppStorageContract.packageName(packageName);AppStorageContract.key(observedKey);AppStorageContract.Action.parse(action);
        long now=SystemClock.elapsedRealtime();
        if(!foreground.getAsBoolean()||!packageName.equals(pkg)||!observedKey.equals(key)||observed<0||now<observed||now-observed>20000||!actions.contains(action)){
            Bundle denied=new Bundle();denied.putBoolean("ok",false);denied.putString("reason","app_storage_target_changed");return denied;
        }
        invalidate();Bundle result=agent.appStorageAction(packageName,observedKey,action);
        if(result!=null)return result;
        Bundle unavailable=new Bundle();unavailable.putBoolean("ok",false);unavailable.putString("reason","app_storage_unavailable");return unavailable;
    }
}
