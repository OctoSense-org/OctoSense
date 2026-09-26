package dev.makepad.octosense.dndfixture;

import android.app.AutomaticZenRule;
import android.app.NotificationManager;
import android.content.ComponentName;
import android.os.Parcel;
import android.os.Parcelable;
import android.util.Base64;
import java.util.Map;
import java.util.TreeMap;
import org.json.JSONObject;

/** Public-API observation only; the caller owns any temporary read permission. */
public final class DndNativeSnapshot {
    private static Object nullable(Object value){return value==null?JSONObject.NULL:value;}
    private static Object component(ComponentName value){return value==null?JSONObject.NULL:value.flattenToString();}
    private static String parcel(Parcelable value){
        Parcel data=Parcel.obtain();
        try{value.writeToParcel(data,0);return Base64.encodeToString(data.marshall(),Base64.NO_WRAP);}
        finally{data.recycle();}
    }
    private static JSONObject policy(NotificationManager.Policy policy)throws Exception{
        return new JSONObject().put("categories",policy.priorityCategories)
                .put("calls",policy.priorityCallSenders).put("messages",policy.priorityMessageSenders)
                .put("conversations",policy.priorityConversationSenders)
                .put("visual_effects",policy.suppressedVisualEffects).put("parcel",parcel(policy));
    }
    public static JSONObject read(NotificationManager manager)throws Exception{
        JSONObject result=new JSONObject().put("filter",manager.getCurrentInterruptionFilter())
                .put("policy",policy(manager.getNotificationPolicy()))
                .put("consolidated_policy",policy(manager.getConsolidatedNotificationPolicy()));
        JSONObject rules=new JSONObject();
        for(Map.Entry<String,AutomaticZenRule> entry:new TreeMap<>(manager.getAutomaticZenRules()).entrySet()){
            AutomaticZenRule rule=entry.getValue();
            rules.put(entry.getKey(),new JSONObject().put("name",rule.getName()).put("enabled",rule.isEnabled())
                    .put("owner",component(rule.getOwner())).put("configuration",component(rule.getConfigurationActivity()))
                    .put("condition",nullable(rule.getConditionId())).put("filter",rule.getInterruptionFilter())
                    .put("created",rule.getCreationTime()).put("type",rule.getType())
                    .put("trigger",nullable(rule.getTriggerDescription())).put("icon",rule.getIconResId())
                    .put("manual_invocation",rule.isManualInvocationAllowed())
                    .put("zen_policy",rule.getZenPolicy()==null?JSONObject.NULL:parcel(rule.getZenPolicy()))
                    .put("device_effects",rule.getDeviceEffects()==null?JSONObject.NULL:parcel(rule.getDeviceEffects()))
                    .put("parcel",parcel(rule)));
        }
        return result.put("rules",rules);
    }
}
