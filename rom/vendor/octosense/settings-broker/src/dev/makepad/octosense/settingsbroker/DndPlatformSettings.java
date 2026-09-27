package dev.makepad.octosense.settingsbroker;

import android.app.ActivityManager;
import android.app.AutomaticZenRule;
import android.app.KeyguardManager;
import android.app.NotificationManager;
import android.content.Context;
import android.os.Parcel;
import android.os.Process;
import android.os.UserHandle;
import android.os.UserManager;
import android.service.notification.ZenModeConfig;
import android.util.Base64;
import dev.makepad.octosense.controls.DndMode;
import dev.makepad.octosense.dnd.DndSettingsBackend;
import dev.makepad.octosense.dnd.DndSettingsContract;
import java.util.ArrayList;
import java.util.Map;
import java.util.Objects;

/** Hidden NMS APIs stay in the existing system-UID, exact-caller-gated broker. */
final class DndPlatformSettings implements DndSettingsBackend.Platform {
    private final Context context;
    DndPlatformSettings(Context context) {this.context=context;}
    private boolean owner() {
        UserManager users=context.getSystemService(UserManager.class);KeyguardManager lock=context.getSystemService(KeyguardManager.class);
        return Process.myUid()==Process.SYSTEM_UID&&UserHandle.myUserId()==0&&ActivityManager.getCurrentUser()==0
            &&users!=null&&users.isUserUnlocked()&&lock!=null&&!lock.isKeyguardLocked();
    }
    private boolean writable() {
        UserManager users=context.getSystemService(UserManager.class);
        // NMS target-SDK compatibility otherwise preserves fields such as conversations.
        return owner()&&context.getApplicationInfo().targetSdkVersion>=30&&users.isAdminUser()
            &&!users.hasUserRestriction(UserManager.DISALLOW_ADJUST_VOLUME);
    }
    private NotificationManager manager() {return context.getSystemService(NotificationManager.class);}
    private static String mode(int zen) {DndMode mode=DndMode.fromZen(zen);return mode==null?null:mode.wire;}
    private static DndSettingsContract.Policy policy(NotificationManager.Policy p) {
        return p==null?null:new DndSettingsContract.Policy(p.priorityCategories,p.priorityCallSenders,p.priorityMessageSenders,p.suppressedVisualEffects,p.state,p.priorityConversationSenders);
    }
    private static String serialized(AutomaticZenRule rule) {
        Parcel parcel=Parcel.obtain();try {rule.writeToParcel(parcel,0);return Base64.encodeToString(parcel.marshall(),Base64.NO_WRAP);}finally{parcel.recycle();}
    }
    private static AutomaticZenRule copy(AutomaticZenRule rule) {
        Parcel parcel=Parcel.obtain();try {rule.writeToParcel(parcel,0);parcel.setDataPosition(0);return AutomaticZenRule.CREATOR.createFromParcel(parcel);}finally{parcel.recycle();}
    }
    private static String label(String name) {
        if(name==null)return "Unnamed rule";StringBuilder result=new StringBuilder();
        name.codePoints().filter(c->!Character.isISOControl(c)).limit(100).forEach(result::appendCodePoint);
        return result.toString().trim().isEmpty()?"Unnamed rule":result.toString();
    }
    private static final class NativeRule {final String id;final AutomaticZenRule rule;NativeRule(String id,AutomaticZenRule rule){this.id=id;this.rule=copy(rule);}}
    @Override public DndSettingsBackend.State read() throws Exception {
        DndSettingsBackend.State state=new DndSettingsBackend.State();
        state.modesApi=android.app.Flags.modesApi();state.modesUi=android.app.Flags.modesUi();
        if(!owner()||manager()==null)return state;
        NotificationManager nm=manager();ZenModeConfig config=nm.getZenModeConfig();
        if(config==null||config.user!=0)return state;
        state.available=true;state.writable=writable();state.currentMode=mode(nm.getZenMode());
        state.manualMode=config.manualRule!=null&&config.isManualActive()?mode(config.manualRule.zenMode):"off";
        state.policy=policy(nm.getNotificationPolicy());state.effectivePolicy=policy(nm.getConsolidatedNotificationPolicy());
        state.policyWritable=state.policy!=null&&(!state.modesUi||"off".equals(state.manualMode)||"priority".equals(state.manualMode));
        int repeat=context.getResources().getIdentifier("config_zen_repeat_callers_threshold","integer","android");
        if(repeat!=0)state.repeatMinutes=context.getResources().getInteger(repeat);
        Map<String,AutomaticZenRule> rules=nm.getAutomaticZenRules();
        if(rules.size()>1024)throw new IllegalStateException("Rule inventory exceeds safe bound");
        ArrayList<String> revisions=new ArrayList<>();
        rules.entrySet().stream().sorted(Map.Entry.comparingByKey()).forEach(entry->{
            AutomaticZenRule rule=entry.getValue();ZenModeConfig.ZenRule nativeState=config.automaticRules.get(entry.getKey());
            if(rule==null||nativeState==null)return;
            DndSettingsBackend.Rule row=new DndSettingsBackend.Rule();row.name=label(rule.getName());row.enabled=rule.isEnabled();row.active=DndRuleActivity.read(nativeState);
            row.target=DndSettingsContract.hash("rule",entry.getKey(),Long.toString(rule.getCreationTime()),String.valueOf(rule.getOwner()),nativeState.getPkg());
            row.revision=DndSettingsContract.hash(serialized(rule),Integer.toString(nativeState.userModifiedFields),Integer.toString(nativeState.zenPolicyUserModifiedFields),Integer.toString(nativeState.zenDeviceEffectsUserModifiedFields));
            row.nativeRule=new NativeRule(entry.getKey(),rule);
            ZenModeConfig.ScheduleInfo schedule=ZenModeConfig.tryParseScheduleConditionId(rule.getConditionId());
            boolean nativeTime=rule.getOwner()!=null&&DndScheduleOwner.matches(nativeState.getPkg(),rule.getOwner().getPackageName(),rule.getOwner().getClassName())
                &&(!state.modesApi||rule.getType()==AutomaticZenRule.TYPE_SCHEDULE_TIME||rule.getType()==AutomaticZenRule.TYPE_UNKNOWN);
            if(nativeTime&&schedule!=null)try {
                row.schedule=new DndSettingsContract.Schedule(row.name,schedule.days==null?new int[0]:schedule.days,schedule.startHour*60+schedule.startMinute,schedule.endHour*60+schedule.endMinute,schedule.exitAtAlarm,rule.isEnabled());
                row.editable=true;row.deletable=true;
            }catch(IllegalArgumentException invalid) { /* Malformed/unknown native schedule remains read-only. */ }
            revisions.add(row.target);revisions.add(row.revision);state.rules.add(row);
        });
        state.canCreate=nm.isSystemConditionProviderEnabled(ZenModeConfig.SCHEDULE_PATH)&&state.rules.size()<DndSettingsContract.MAX_RULES;
        revisions.add(state.policy==null?null:state.policy.fingerprint());revisions.add(state.manualMode);
        revisions.add(String.valueOf(state.writable));revisions.add(String.valueOf(state.modesApi));revisions.add(String.valueOf(state.modesUi));
        revisions.add(String.valueOf(state.canCreate));
        state.revision=DndSettingsContract.hash(revisions.toArray(new String[0]));
        if(!owner())return new DndSettingsBackend.State();return state;
    }
    private boolean current(DndSettingsBackend.State observed) throws Exception {
        return writable()&&observed.available&&Objects.equals(observed.revision,read().revision);
    }
    @Override public boolean policy(DndSettingsBackend.State before,DndSettingsContract.Policy value) throws Exception {
        if(!current(before)||!before.policyWritable)return false;
        // Preserve unedited categories, both sender fields, visual effects, state and conversations.
        manager().setNotificationPolicy(new NotificationManager.Policy(value.categories,value.calls,value.messages,value.effects,value.state,value.conversations),true);return true;
    }
    private static ZenModeConfig.ScheduleInfo schedule(DndSettingsContract.Schedule value) {
        ZenModeConfig.ScheduleInfo info=new ZenModeConfig.ScheduleInfo();info.days=value.days.clone();info.startHour=value.start/60;info.startMinute=value.start%60;
        info.endHour=value.end/60;info.endMinute=value.end%60;info.exitAtAlarm=value.exitAtAlarm;return info;
    }
    private static android.net.Uri condition(android.net.Uri original,DndSettingsContract.Schedule value) {
        android.net.Uri replacement=ZenModeConfig.toScheduleConditionId(schedule(value));
        if(original==null)return replacement;
        android.net.Uri.Builder builder=original.buildUpon().clearQuery();
        java.util.Set<String> owned=replacement.getQueryParameterNames();
        for(String key:original.getQueryParameterNames())if(!owned.contains(key))
            for(String item:original.getQueryParameters(key))builder.appendQueryParameter(key,item);
        for(String key:owned)for(String item:replacement.getQueryParameters(key))builder.appendQueryParameter(key,item);
        return builder.build();
    }
    @Override public boolean schedule(DndSettingsBackend.State before,DndSettingsBackend.Rule target,DndSettingsContract.Schedule value) throws Exception {
        if(!current(before))return false;
        if(target==null) {
            if(!before.canCreate)return false;
            AutomaticZenRule rule=new AutomaticZenRule(value.name,ZenModeConfig.getScheduleConditionProvider(),null,
                condition(null,value),null,NotificationManager.INTERRUPTION_FILTER_PRIORITY,value.enabled);
            // Let NMS initialize its native default: legacy inheritance, current policy with
            // MODES_API alone, framework default with MODES_UI. Never use consolidated policy.
            if(before.modesApi)rule.setType(AutomaticZenRule.TYPE_SCHEDULE_TIME);
            return manager().addAutomaticZenRule(rule,true)!=null;
        }
        if(!target.editable||target.schedule==null)return false;
        NativeRule nativeRule=(NativeRule)target.nativeRule;AutomaticZenRule rule=copy(nativeRule.rule);
        rule.setName(value.name);rule.setConditionId(condition(rule.getConditionId(),value));rule.setEnabled(value.enabled);
        return manager().updateAutomaticZenRule(nativeRule.id,rule,true);
    }
    @Override public boolean enabled(DndSettingsBackend.State before,DndSettingsBackend.Rule target,boolean value) throws Exception {
        if(!current(before)||!target.editable||target.schedule==null)return false;
        NativeRule nativeRule=(NativeRule)target.nativeRule;AutomaticZenRule rule=copy(nativeRule.rule);rule.setEnabled(value);
        return manager().updateAutomaticZenRule(nativeRule.id,rule,true);
    }
    @Override public boolean delete(DndSettingsBackend.State before,DndSettingsBackend.Rule target) throws Exception {
        return current(before)&&target.deletable&&target.schedule!=null&&manager().removeAutomaticZenRule(((NativeRule)target.nativeRule).id,true);
    }
}
