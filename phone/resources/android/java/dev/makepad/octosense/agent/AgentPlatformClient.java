package dev.makepad.octosense.agent;

import android.app.Activity;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.ServiceConnection;
import android.content.pm.PackageManager;
import android.os.Bundle;
import android.os.IBinder;
import android.os.RemoteException;
import android.util.Log;

import java.util.ArrayList;
import java.util.List;
import java.util.function.Consumer;
import org.json.JSONObject;

/**
 * The launcher's binding to the OctoSense ROM's agent platform. Present only
 * on the ROM (the package is platform-signed and absent elsewhere), so every
 * caller keeps its Home-app fallback: this client reports "absent" and the
 * shell carries on with UsageStats and the bridge.
 */
public final class AgentPlatformClient {
    public static final String PACKAGE = "dev.makepad.octosense.agent";
    static final String TAG = "OctoSenseAgentClient";

    /** One recent task, as the platform lists it. */
    public static final class Task {
        public final int id; public final String pkg; public final String label; public final boolean visible; public final long lastActive;
        Task(int id, String pkg, String label, boolean visible, long lastActive) {
            this.id = id; this.pkg = pkg; this.label = label; this.visible = visible; this.lastActive = lastActive;
        }
    }

    private final Activity activity;
    private final Consumer<String> onState;
    private volatile IAgentPlatform platform;
    private boolean bound;
    private volatile List<String> capabilities = new ArrayList<>();

    public AgentPlatformClient(Activity activity, Consumer<String> onState) {
        this.activity = activity; this.onState = onState;
    }

    /** True when the ROM ships the platform and it is signed like us. */
    public boolean available() {
        PackageManager pm = activity.getPackageManager();
        try {
            pm.getPackageInfo(PACKAGE, 0);
        } catch (PackageManager.NameNotFoundException e) {
            return false;
        }
        return pm.checkSignatures(activity.getPackageName(), PACKAGE) == PackageManager.SIGNATURE_MATCH;
    }

    public boolean connected() { return platform != null; }
    public boolean has(String capability) { return platform != null && capabilities.contains(capability); }

    public void bind() {
        if (bound || !available()) { if (!bound) onState.accept("absent"); return; }
        try {
            bound = activity.bindService(new Intent("dev.makepad.octosense.agent.AGENT_PLATFORM")
                    .setComponent(new ComponentName(PACKAGE, PACKAGE + ".AgentPlatformService")),
                    connection, Context.BIND_AUTO_CREATE);
            if (!bound) onState.accept("bind_failed");
        } catch (SecurityException e) {
            onState.accept("denied");
        }
    }

    public void unbind() {
        if (bound) { activity.unbindService(connection); bound = false; }
        platform = null;
    }

    private final ServiceConnection connection = new ServiceConnection() {
        @Override public void onServiceConnected(ComponentName name, IBinder binder) {
            IAgentPlatform candidate = IAgentPlatform.Stub.asInterface(binder);
            try {
                Bundle caps = candidate.getCapabilities();
                ArrayList<String> list = caps.getStringArrayList("capabilities");
                capabilities = list == null ? new ArrayList<>() : list;
                platform = candidate;
                onState.accept("connected");
            } catch (RemoteException | SecurityException e) {
                Log.w(TAG, "handshake failed", e);
                platform = null;
                onState.accept("handshake_failed");
            }
        }
        @Override public void onServiceDisconnected(ComponentName name) { platform = null; onState.accept("disconnected"); }
        @Override public void onNullBinding(ComponentName name) { platform = null; onState.accept("absent"); }
    };

    /** Recent tasks, most recent first, the launcher's own excluded; empty when the capability is absent. */
    public List<Task> recentTasks(int max) {
        ArrayList<Task> out = new ArrayList<>();
        if (!has("tasks")) return out;
        try {
            Bundle r = platform.getTasks(max + 1);
            @SuppressWarnings("deprecation")
            ArrayList<Bundle> tasks = r.getParcelableArrayList("tasks");
            if (!r.getBoolean("ok") || tasks == null) return out;
            for (Bundle t : tasks) {
                String pkg = t.getString("package");
                if (pkg == null || pkg.equals(activity.getPackageName())) continue;
                out.add(new Task(t.getInt("id"), pkg, t.getString("label", pkg), t.getBoolean("visible"), t.getLong("lastActive")));
                if (out.size() >= max) break;
            }
        } catch (RemoteException | SecurityException e) {
            Log.w(TAG, "tasks failed", e);
        }
        return out;
    }

    /** Brings a task forward; false when the platform refused or is absent. */
    public boolean startTask(int taskId) {
        if (!has("apps")) return false;
        try { return platform.startTask(taskId).getBoolean("ok"); }
        catch (RemoteException | SecurityException e) { return false; }
    }

    /** Opens the system shade through the platform; false when absent. */
    public boolean expandNotifications() {
        if (!has("statusbar")) return false;
        try { return platform.expandNotifications().getBoolean("ok"); }
        catch (RemoteException | SecurityException e) { return false; }
    }

    public JSONObject dateTimeSnapshot(long id) throws Exception {
        IAgentPlatform service=platform;if(service==null||!has("date_time_settings_v1")) return null;
        Bundle result=service.getDateTimeSnapshot(id);
        if(result==null||!result.getBoolean("ok")) return null;
        return new JSONObject(result.getString("json",""));
    }
    public JSONObject soundsSnapshot(long id,String type,String key,int offset) throws Exception {
        dev.makepad.octosense.sounds.SoundSettingsContract.Type.parse(type);
        dev.makepad.octosense.sounds.SoundSettingsContract.offset(offset,key);
        IAgentPlatform service=platform;if(service==null||!has("sounds_settings_v1")) return null;
        Bundle result=service.getSoundsSnapshot(id,type,key,offset);
        if(result==null||!result.getBoolean("ok")) throw new SecurityException("Sound settings unavailable");
        return new JSONObject(result.getString("json",""));
    }
    private String soundReason(Bundle result) {
        if(result==null)return "sound_unavailable";
        String reason=result.getString("reason","sound_unavailable");
        switch(reason) {
            case "sound_applied":case "sound_preview_started":case "sound_preview_requested":case "sound_silent":case "sound_stopped":
            case "sound_restricted":case "sound_target_changed":case "sound_unconfirmed":return reason;
            case "keyguard":case "wrong_user":return "sound_restricted";
            default:return "sound_unavailable";
        }
    }
    public String soundAction(String type,String key,String target,boolean save) throws Exception {
        dev.makepad.octosense.sounds.SoundSettingsContract.Type.parse(type);
        dev.makepad.octosense.sounds.SoundSettingsContract.key(key);dev.makepad.octosense.sounds.SoundSettingsContract.key(target);
        IAgentPlatform service=platform;if(service==null||!has("sounds_settings_v1"))return "sound_unavailable";
        return soundReason(save?service.saveSound(type,key,target):service.previewSound(type,key,target));
    }
    public String soundStop() throws Exception {
        IAgentPlatform service=platform;return service==null||!has("sounds_settings_v1")?"sound_stopped":soundReason(service.stopSound());
    }
    /** Capture the live Binder before unbinding; lifecycle cleanup cannot wait on the UI thread. */
    public void stopSoundInBackground() {
        IAgentPlatform service=platform;if(service==null||!has("sounds_settings_v1"))return;
        Thread stop=new Thread(() -> {try{service.stopSound();}catch(Exception ignored){}},"OctoSenseSoundStop");
        stop.setDaemon(true);stop.start();
    }
    public JSONObject notificationHistory(long id,String key,int offset) throws Exception {
        dev.makepad.octosense.notifications.NotificationHistoryContract.request(id,key,offset);
        IAgentPlatform service=platform;if(service==null||!has("notification_history_v1")) return null;
        Bundle result=service.getNotificationHistory(id,key,offset);
        if(result==null||!result.getBoolean("ok")) return null;
        return new JSONObject(result.getString("json",""));
    }
    public String setDateTime(String key,String action,String value,String occurrence) throws Exception {
        IAgentPlatform service=platform;if(service==null||!has("date_time_settings_v1")) return "time_unavailable";
        Bundle result=service.setDateTime(key,action,value,occurrence);
        return result==null?"time_unavailable":result.getString("reason","time_unavailable");
    }

    /** Settings owns the finite operation enum; scripts never receive this client. */
    public boolean applyDeviceSetting(dev.makepad.octosense.DeviceSetting setting, Object value) {
        IAgentPlatform service=platform;
        if(service==null||!has("settings")||setting==null||setting.audio()) return false;
        String stored=setting.storedValue(value);
        try {return service.putSetting(setting.table,setting.key,stored).getBoolean("ok");}
        catch(RemoteException|SecurityException e) {return false;}
    }

    private JSONObject accountJson(Bundle result) throws Exception {
        if(result==null) return null;
        if(!result.getBoolean("ok")) {
            if("accounts_unavailable".equals(result.getString("reason"))) return null;
            throw new SecurityException("Accounts unavailable");
        }
        return new JSONObject(result.getString("json",""));
    }
    public JSONObject updatesSnapshot(long id) throws Exception {
        IAgentPlatform service=platform;
        if(service==null||!has("updates_settings_v1")) return null;
        Bundle result=service.getUpdatesSnapshot(id);
        if(result==null||!result.getBoolean("ok")) return dev.makepad.octosense.updates.UpdatesSettingsSnapshot.unavailable(activity,id,"restricted");
        return new JSONObject(result.getString("json",""));
    }
    public JSONObject appNotificationsSnapshot(long id,String pkg,int offset,String generation)throws Exception {
        dev.makepad.octosense.notifications.AppNotificationsContract.packageName(pkg);dev.makepad.octosense.notifications.AppNotificationsContract.page(id,offset,generation);
        IAgentPlatform service=platform;if(service==null||!has("app_notifications_v1"))return null;
        Bundle result=service.getAppNotifications(id,pkg,offset,generation);
        if(result==null||!result.getBoolean("ok"))return dev.makepad.octosense.notifications.AppNotificationsContract.unavailable(id,pkg,"restricted");
        return new JSONObject(result.getString("json",""));
    }
    public JSONObject rolesSnapshot(long id,dev.makepad.octosense.roles.RolesSettingsContract.RoleId role,int offset,String generation)throws Exception {
        dev.makepad.octosense.roles.RolesSettingsContract.page(id,role,offset,generation);
        IAgentPlatform service=platform;if(service==null||!has("roles_settings_v1"))return null;
        Bundle result=service.getRolesSnapshot(id,role==null?null:role.wire,offset,generation);
        if(result==null||!result.getBoolean("ok"))return dev.makepad.octosense.roles.RolesSettingsContract.unavailable(id,role,"restricted");
        return new JSONObject(result.getString("json",""));
    }
    public android.app.PendingIntent roleConfirmation(dev.makepad.octosense.roles.RolesSettingsContract.RoleId role,String key,String target)throws Exception {
        dev.makepad.octosense.roles.RolesSettingsContract.key(key);dev.makepad.octosense.roles.RolesSettingsContract.key(target);
        IAgentPlatform service=platform;if(service==null||!has("roles_settings_v1"))return null;
        Bundle result=service.confirmRole(role.wire,key,target);if(result==null||!result.getBoolean("ok"))return null;
        android.app.PendingIntent pending=result.getParcelable("flow");
        return pending!=null&&pending.isImmutable()&&"com.android.permissioncontroller".equals(pending.getCreatorPackage())?pending:null;
    }
    public JSONObject permissionsSnapshot(long id,String pkg,dev.makepad.octosense.permissions.PermissionsSettingsContract.Group group,int offset,String generation)throws Exception {
        dev.makepad.octosense.permissions.PermissionsSettingsContract.page(id,pkg,group,offset,generation);
        IAgentPlatform service=platform;if(service==null||!has("permissions_settings_v1"))return null;
        Bundle result=service.getPermissionsSnapshot(id,pkg,group==null?null:group.wire,offset,generation);
        if(result==null||!result.getBoolean("ok"))return dev.makepad.octosense.permissions.PermissionsSettingsContract.unavailable(id,pkg,group,"unavailable");
        return new JSONObject(result.getString("json",""));
    }
    public JSONObject keyboardsSnapshot(long id,String query,int offset)throws Exception {
        dev.makepad.octosense.keyboards.KeyboardContract.read(id,query,offset);IAgentPlatform service=platform;if(service==null||!has("keyboards_v1"))return null;
        Bundle result=service.getKeyboardsSnapshot(id,query,offset);return result==null||!result.getBoolean("ok")?null:new JSONObject(result.getString("json",""));
    }
    public android.app.PendingIntent keyboardFlow(long id,String key,String target,String operation)throws Exception {
        dev.makepad.octosense.keyboards.KeyboardContract.flow(id,key,target,operation);IAgentPlatform service=platform;if(service==null||!has("keyboards_v1"))return null;
        Bundle result=service.prepareKeyboardFlow(id,key,target,operation);if(result==null||!result.getBoolean("ok"))return null;
        android.app.PendingIntent flow=result.getParcelable("flow");return flow!=null&&flow.isImmutable()&&flow.getCreatorUid()==android.os.Process.SYSTEM_UID&&"dev.makepad.octosense.settingsbroker".equals(flow.getCreatorPackage())?flow:null;
    }
    public JSONObject systemLanguagesSnapshot(long id,String key,String parent,String query,int offset)throws Exception {
        dev.makepad.octosense.systemlanguage.SystemLanguageContract.read(id,key,parent,query,offset);
        IAgentPlatform service=platform;if(service==null||!has("system_languages_v1"))return null;
        Bundle state=service.getSystemLanguagesSnapshot(id,key,parent,query,offset);
        return state==null||!state.getBoolean("ok")?null:new JSONObject(state.getString("json"));
    }
    public String systemLanguagesApply(String key,String[] targets)throws Exception {
        dev.makepad.octosense.systemlanguage.SystemLanguageContract.order(key,targets);
        IAgentPlatform service=platform;if(service==null||!has("system_languages_v1"))return "languages_unavailable";
        Bundle state=service.applySystemLanguages(key,targets);if(state==null)return "languages_unavailable";
        String reason=state.getString("reason","");
        switch(reason){case "languages_applied":case "languages_requested":return state.getBoolean("ok")?reason:"languages_unconfirmed";case "languages_target_changed":case "languages_restricted":case "languages_unconfirmed":case "languages_unavailable":return reason;default:return "languages_unconfirmed";}
    }
    public JSONObject appLanguageSnapshot(long id,String pkg,String key,String parent,String query,int offset)throws Exception {
        dev.makepad.octosense.applanguage.AppLanguageContract.read(id,pkg,key,parent,query,offset);
        IAgentPlatform service=platform;if(service==null||!has("app_language_v1"))return null;
        Bundle state=service.getAppLanguageSnapshot(id,pkg,key,parent,query,offset);
        return state==null||!state.getBoolean("ok")?null:new JSONObject(state.getString("json"));
    }
    public String appLanguageSet(String pkg,String key,String choice)throws Exception {
        dev.makepad.octosense.applanguage.AppLanguageContract.packageName(pkg);dev.makepad.octosense.applanguage.AppLanguageContract.key(key);dev.makepad.octosense.applanguage.AppLanguageContract.key(choice);
        IAgentPlatform service=platform;if(service==null||!has("app_language_v1"))return "app_language_unavailable";
        Bundle state=service.setAppLanguage(pkg,key,choice);if(state==null)return "app_language_unavailable";
        String reason=state.getString("reason","");
        switch(reason){
            case "app_language_applied":case "app_language_requested":return state.getBoolean("ok")?reason:"app_language_unconfirmed";
            case "app_language_target_changed":case "app_language_restricted":case "app_language_unconfirmed":case "app_language_unavailable":return reason;
            default:return "app_language_unconfirmed";
        }
    }
    public JSONObject appStorageSnapshot(long id,String pkg)throws Exception {
        dev.makepad.octosense.appstorage.AppStorageContract.packageName(pkg);
        if(id<=0)throw new IllegalArgumentException("Positive request ID required");
        IAgentPlatform service=platform;if(service==null||!has("app_storage_v1"))return null;
        Bundle result=service.getAppStorageSnapshot(id,pkg);
        return result==null||!result.getBoolean("ok")?null:new JSONObject(result.getString("json"));
    }
    public Bundle appStorageAction(String pkg,String key,String action)throws Exception {
        dev.makepad.octosense.appstorage.AppStorageContract.packageName(pkg);dev.makepad.octosense.appstorage.AppStorageContract.key(key);dev.makepad.octosense.appstorage.AppStorageContract.Action.parse(action);
        IAgentPlatform service=platform;Bundle clean=new Bundle();clean.putBoolean("ok",false);clean.putString("reason","app_storage_unavailable");
        if(service==null||!has("app_storage_v1"))return clean;
        Bundle result=service.applyAppStorage(pkg,key,action);if(result==null)return clean;
        String reason=result.getString("reason","");
        switch(reason){
            case "app_storage_requested":if(!result.getBoolean("ok")||action.equals("manage_space"))return clean;clean.putBoolean("ok",true);break;
            case "app_storage_flow_opened":
                android.app.PendingIntent pending=result.getParcelable("flow");
                if(!result.getBoolean("ok")||!action.equals("manage_space")||pending==null||!pending.isImmutable()||pending.getCreatorUid()!=android.os.Process.SYSTEM_UID||!"dev.makepad.octosense.settingsbroker".equals(pending.getCreatorPackage()))return clean;
                clean.putParcelable("flow",pending);clean.putBoolean("ok",true);break;
            case "app_storage_target_changed":case "app_storage_restricted":case "app_storage_busy":case "app_storage_unavailable":case "app_storage_unconfirmed":break;
            default:return clean;
        }
        clean.putString("reason",reason);return clean;
    }
    public JSONObject appBatterySnapshot(long id,String pkg)throws Exception {
        dev.makepad.octosense.battery.AppBatteryContract.packageName(pkg);
        IAgentPlatform service=platform;if(service==null||!has("app_battery_v1"))return null;
        Bundle result=service.getAppBatterySnapshot(id,pkg);
        return result==null||!result.getBoolean("ok")?null:new JSONObject(result.getString("json"));
    }
    public String appBatterySet(String pkg,String key,String mode)throws Exception {
        dev.makepad.octosense.battery.AppBatteryContract.packageName(pkg);dev.makepad.octosense.battery.AppBatteryContract.key(key);dev.makepad.octosense.battery.AppBatteryContract.Mode.parse(mode);
        IAgentPlatform service=platform;if(service==null||!has("app_battery_v1"))return "app_battery_unavailable";
        Bundle result=service.setAppBattery(pkg,key,mode);if(result==null)return "app_battery_unavailable";
        switch(result.getString("reason","")){case "app_battery_applied":case "app_battery_requested":case "app_battery_target_changed":case "app_battery_restricted":case "app_battery_partial":case "app_battery_unconfirmed":return result.getString("reason");default:return "app_battery_unavailable";}
    }
    public JSONObject appNetworkSnapshot(long id,String pkg)throws Exception {
        dev.makepad.octosense.appnetwork.AppNetworkContract.packageName(pkg);
        IAgentPlatform service=platform;if(service==null||!has("app_network_settings_v1"))return null;
        Bundle result=service.getAppNetworkSnapshot(id,pkg);
        return result==null||!result.getBoolean("ok")?null:new JSONObject(result.getString("json"));
    }
    public String appNetworkSet(String pkg,String key,dev.makepad.octosense.appnetwork.AppNetworkContract.Field field,boolean enabled)throws Exception {
        dev.makepad.octosense.appnetwork.AppNetworkContract.packageName(pkg);dev.makepad.octosense.appnetwork.AppNetworkContract.key(key);
        IAgentPlatform service=platform;if(service==null||!has("app_network_settings_v1"))return "app_network_unavailable";
        Bundle result=service.setAppNetwork(pkg,key,field.wire,enabled);if(result==null)return "app_network_unavailable";
        switch(result.getString("reason","")){case "app_network_applied":case "app_network_unchanged":case "app_network_target_changed":case "app_network_restricted":case "app_network_unconfirmed":return result.getString("reason");default:return "app_network_unavailable";}
    }
    public JSONObject dndSnapshot(long id,int offset,String generation)throws Exception {
        dev.makepad.octosense.dnd.DndSettingsContract.offset(offset,generation);
        IAgentPlatform service=platform;if(service==null||!has("dnd_settings_v1"))return null;
        Bundle result=service.getDndSnapshot(id,offset,generation);
        return result!=null&&result.getBoolean("ok")?new JSONObject(result.getString("json","")):null;
    }
    private String dndReason(Bundle result) {
        if(result==null)return "dnd_unavailable";
        switch(result.getString("reason","")){case "dnd_applied":case "dnd_requested":case "dnd_target_changed":case "dnd_restricted":return result.getString("reason");default:return "dnd_unavailable";}
    }
    public String dndPolicy(String key,dev.makepad.octosense.dnd.DndSettingsContract.Field field,String value)throws Exception {
        dev.makepad.octosense.dnd.DndSettingsContract.key(key);field.value(value);
        IAgentPlatform service=platform;return service==null||!has("dnd_settings_v1")?"dnd_unavailable":dndReason(service.setDndPolicy(key,field.wire,value));
    }
    public String dndSchedule(String key,String target,dev.makepad.octosense.dnd.DndSettingsContract.Schedule value)throws Exception {
        dev.makepad.octosense.dnd.DndSettingsContract.key(key);if(target!=null)dev.makepad.octosense.dnd.DndSettingsContract.key(target);
        IAgentPlatform service=platform;return service==null||!has("dnd_settings_v1")?"dnd_unavailable":dndReason(service.saveDndSchedule(key,target,value.name,value.days,value.start,value.end,value.exitAtAlarm,value.enabled));
    }
    public String dndEnabled(String key,String target,boolean enabled)throws Exception {
        dev.makepad.octosense.dnd.DndSettingsContract.key(key);dev.makepad.octosense.dnd.DndSettingsContract.key(target);
        IAgentPlatform service=platform;return service==null||!has("dnd_settings_v1")?"dnd_unavailable":dndReason(service.setDndRuleEnabled(key,target,enabled));
    }
    public String dndDelete(String key,String target)throws Exception {
        dev.makepad.octosense.dnd.DndSettingsContract.key(key);dev.makepad.octosense.dnd.DndSettingsContract.key(target);
        IAgentPlatform service=platform;return service==null||!has("dnd_settings_v1")?"dnd_unavailable":dndReason(service.deleteDndRule(key,target));
    }
    public android.app.PendingIntent permissionChoice(String pkg,dev.makepad.octosense.permissions.PermissionsSettingsContract.Group group,String key,String target)throws Exception {
        dev.makepad.octosense.permissions.PermissionsSettingsContract.packageName(pkg);dev.makepad.octosense.permissions.PermissionsSettingsContract.key(key);dev.makepad.octosense.permissions.PermissionsSettingsContract.key(target);
        IAgentPlatform service=platform;if(service==null||!has("permissions_settings_v1"))return null;
        Bundle result=service.requestPermissionChoice(pkg,group.wire,key,target);if(result==null||!result.getBoolean("ok"))return null;
        android.app.PendingIntent pending=result.getParcelable("flow");
        return pending!=null&&pending.isImmutable()&&"com.android.permissioncontroller".equals(pending.getCreatorPackage())?pending:null;
    }
    public String appNotificationsSet(String pkg,String key,String target,dev.makepad.octosense.notifications.AppNotificationsContract.Action action,String value)throws Exception {
        dev.makepad.octosense.notifications.AppNotificationsContract.packageName(pkg);dev.makepad.octosense.notifications.AppNotificationsContract.key(key);dev.makepad.octosense.notifications.AppNotificationsContract.key(target);dev.makepad.octosense.notifications.AppNotificationsContract.value(action,value);
        IAgentPlatform service=platform;if(service==null||!has("app_notifications_v1"))return "notifications_unavailable";
        Bundle result=service.setAppNotification(pkg,key,target,action.wire,value);if(result==null)return "notifications_unconfirmed";
        switch(result.getString("reason","")){case "notifications_applied":case "notifications_target_changed":case "notifications_restricted":case "notifications_partial":case "notifications_unconfirmed":return result.getString("reason");default:return "notifications_unavailable";}
    }
    public JSONObject displaySnapshot(long id) throws Exception {
        IAgentPlatform service=platform;if(service==null||!has("display_settings_v1"))return null;
        Bundle result=service.getDisplaySnapshot(id);
        if(result==null||!result.getBoolean("ok"))throw new SecurityException("Display settings unavailable");
        return new JSONObject(result.getString("json",""));
    }
    public String displaySet(String key,dev.makepad.octosense.display.DisplaySettingsContract.Setting setting,String value)throws Exception {
        dev.makepad.octosense.display.DisplaySettingsContract.key(key);dev.makepad.octosense.display.DisplaySettingsContract.value(setting,value);
        IAgentPlatform service=platform;if(service==null||!has("display_settings_v1"))return "display_unavailable";
        Bundle result=service.setDisplaySetting(key,setting.wire,value);if(result==null)return "display_unavailable";
        String reason=result.getString("reason","display_unavailable");
        switch(reason){case "display_applied":case "display_requested":case "display_target_changed":case "display_restricted":case "display_location_required":return reason;
            case "keyguard":case "wrong_user":return "display_restricted";default:return "display_unavailable";}
    }
    public JSONObject networkSnapshot(long id) throws Exception {
        IAgentPlatform service=platform;if(service==null||!has("network_settings_v1")) return null;
        Bundle result=service.getNetworkSnapshot(id);
        if(result==null||!result.getBoolean("ok")) throw new SecurityException("Network settings unavailable");
        return new JSONObject(result.getString("json",""));
    }
    private String networkReason(Bundle result) {
        if(result==null) return "network_unavailable";
        String reason=result.getString("reason","network_unavailable");
        switch(reason) {case "network_applied":case "network_requested":case "network_target_changed":return reason;default:return "network_unavailable";}
    }
    public String networkAirplane(String key,boolean value) throws RemoteException {
        dev.makepad.octosense.network.NetworkSettingsContract.key(key);
        IAgentPlatform service=platform;return service==null||!has("network_settings_v1")?"network_unavailable":networkReason(service.networkAirplane(key,value));
    }
    public String networkDataSaver(String key,boolean value) throws RemoteException {
        dev.makepad.octosense.network.NetworkSettingsContract.key(key);
        IAgentPlatform service=platform;return service==null||!has("network_settings_v1")?"network_unavailable":networkReason(service.networkDataSaver(key,value));
    }
    public String networkPrivateDns(String key,String mode,String hostname) throws RemoteException {
        dev.makepad.octosense.network.NetworkSettingsContract.key(key);
        String normalized=dev.makepad.octosense.network.NetworkSettingsContract.dnsHostname(mode,hostname);
        IAgentPlatform service=platform;return service==null||!has("network_settings_v1")?"network_unavailable":networkReason(service.networkPrivateDns(key,mode,normalized));
    }
    private String updateReason(Bundle result) {
        if(result==null) return "update_unavailable";
        String reason=result.getString("reason","update_unavailable");
        switch(reason) {
            case "update_check_requested":case "update_install_requested":case "update_reboot_requested":case "update_target_changed":return reason;
            default:return "update_unavailable";
        }
    }
    public String updatesCheck() throws RemoteException {
        IAgentPlatform service=platform;return service==null||!has("updates_settings_v1")?"update_unavailable":updateReason(service.checkReviewedUpdates());
    }
    public String updatesInstall(String key,String part) throws RemoteException {
        dev.makepad.octosense.updates.UpdatesSettingsContract.key(key);dev.makepad.octosense.updates.UpdatesSettingsContract.part(part);
        IAgentPlatform service=platform;return service==null||!has("updates_settings_v1")?"update_unavailable":updateReason(service.installReviewedUpdate(key,part));
    }
    public String updatesReboot(String key) throws RemoteException {
        dev.makepad.octosense.updates.UpdatesSettingsContract.key(key);
        IAgentPlatform service=platform;return service==null||!has("updates_settings_v1")?"update_unavailable":updateReason(service.rebootReviewedUpdate(key));
    }
    public JSONObject accountsSnapshot(long id) throws Exception {
        IAgentPlatform service=platform;return service==null||!has("accounts_settings_v1")?null:accountJson(service.getAccountsSnapshot(id));
    }
    public JSONObject accountDetails(long id,String key) throws Exception {
        dev.makepad.octosense.accounts.AccountsSettingsContract.key(key);
        IAgentPlatform service=platform;return service==null||!has("accounts_settings_v1")?null:accountJson(service.getAccountDetails(id,key));
    }
    private String syncReason(Bundle result) {
        if(result==null) return "sync_unavailable";String reason=result.getString("reason","sync_unavailable");
        switch(reason) {case "sync_applied":case "sync_requested":case "account_target_changed":return reason;default:return "sync_unavailable";}
    }
    public String accountsMaster(boolean value) throws RemoteException {
        IAgentPlatform service=platform;return service==null||!has("accounts_settings_v1")?"sync_unavailable":syncReason(service.setAccountsMasterSync(value));
    }
    public String accountSync(String key,String authority,dev.makepad.octosense.accounts.AccountsSettingsContract.SyncAction action,Boolean value) throws RemoteException {
        dev.makepad.octosense.accounts.AccountsSettingsContract.key(key);dev.makepad.octosense.accounts.AccountsSettingsContract.key(authority);
        dev.makepad.octosense.accounts.AccountsSettingsContract.syncValue(action,value);
        IAgentPlatform service=platform;return service==null||!has("accounts_settings_v1")?"sync_unavailable":syncReason(service.accountSync(key,authority,action.wire,Boolean.TRUE.equals(value)));
    }
    private android.app.PendingIntent accountFlow(Bundle result) {
        if(result==null||!result.getBoolean("ok")) return null;
        android.app.PendingIntent pending=result.getParcelable("pending");
        return pending!=null&&"dev.makepad.octosense.settingsbroker".equals(pending.getCreatorPackage())?pending:null;
    }
    public android.app.PendingIntent accountAddition(String key) throws RemoteException {
        dev.makepad.octosense.accounts.AccountsSettingsContract.key(key);
        IAgentPlatform service=platform;return service==null||!has("accounts_settings_v1")?null:accountFlow(service.accountAddition(key));
    }
    public android.app.PendingIntent accountRemoval(String key) throws RemoteException {
        dev.makepad.octosense.accounts.AccountsSettingsContract.key(key);
        IAgentPlatform service=platform;return service==null||!has("accounts_settings_v1")?null:accountFlow(service.accountRemoval(key));
    }
    public JSONObject bluetoothSnapshot(long id) throws Exception {
        IAgentPlatform service=platform;if(service==null||!has("bluetooth_settings_v1")) return null;
        Bundle result=service.getBluetoothSnapshot(id);
        if(result==null||!result.getBoolean("ok")) throw new SecurityException("Bluetooth observation unavailable");
        return new JSONObject(result.getString("json",""));
    }
    private String bluetoothReason(Bundle result) {
        if(result==null) return "bluetooth_unavailable";
        String reason=result.getString("reason","bluetooth_unavailable");
        switch(reason) {
            case "bluetooth_requested":case "bluetooth_name_applied":case "bluetooth_sharing_applied":case "bluetooth_target_changed":return reason;
            default:return "bluetooth_unavailable";
        }
    }
    public String bluetoothEnabled(boolean value) throws RemoteException {
        IAgentPlatform service=platform;return service==null||!has("bluetooth_settings_v1")?"bluetooth_unavailable":bluetoothReason(service.setBluetoothEnabled(value));
    }
    public String bluetoothScan(boolean value) throws RemoteException {
        IAgentPlatform service=platform;return service==null||!has("bluetooth_settings_v1")?"bluetooth_unavailable":bluetoothReason(service.scanBluetooth(value));
    }
    public String bluetoothName(String value) throws RemoteException {
        dev.makepad.octosense.bluetooth.BluetoothSettingsContract.name(value);
        IAgentPlatform service=platform;return service==null||!has("bluetooth_settings_v1")?"bluetooth_unavailable":bluetoothReason(service.setBluetoothName(value));
    }
    public String bluetoothDevice(String key,dev.makepad.octosense.bluetooth.BluetoothSettingsContract.Action action) throws RemoteException {
        dev.makepad.octosense.bluetooth.BluetoothSettingsContract.key(key);
        IAgentPlatform service=platform;return service==null||!has("bluetooth_settings_v1")?"bluetooth_unavailable":bluetoothReason(service.bluetoothDevice(key,action.wire));
    }
    public String bluetoothSharing(String key,String kind,String value) throws RemoteException {
        dev.makepad.octosense.bluetooth.BluetoothSettingsContract.key(key);
        dev.makepad.octosense.bluetooth.BluetoothSettingsContract.sharingKind(kind);
        dev.makepad.octosense.bluetooth.BluetoothSettingsContract.sharingValue(value);
        IAgentPlatform service=platform;return service==null||!has("bluetooth_settings_v1")?"bluetooth_unavailable":bluetoothReason(service.bluetoothSharing(key,kind,value));
    }
    /** Wi-Fi observations and finite operations; no credential-bearing results. */
    public JSONObject wifiSnapshot(long id) throws Exception {
        IAgentPlatform service=platform;
        if(service==null||!has("wifi_settings_v1")) return null;
        Bundle result=service.getWifiSnapshot(id);
        if(result==null||!result.getBoolean("ok")) throw new SecurityException("Wi-Fi observation unavailable");
        return new JSONObject(result.getString("json",""));
    }
    public String wifiEnabled(boolean enabled) throws RemoteException {
        IAgentPlatform service=platform;
        if(service==null||!has("wifi_settings_v1")) return "wifi_unavailable";
        return wifiReason(service.setWifiEnabled(enabled));
    }
    public String wifiScan() throws RemoteException {
        IAgentPlatform service=platform;
        if(service==null||!has("wifi_settings_v1")) return "wifi_unavailable";
        return wifiReason(service.scanWifi());
    }
    public String wifiNetwork(String key,dev.makepad.octosense.wifi.WifiSettingsContract.Action action) throws RemoteException {
        IAgentPlatform service=platform;
        if(service==null||!has("wifi_settings_v1")) return "wifi_unavailable";
        return wifiReason(service.wifiNetwork(dev.makepad.octosense.wifi.WifiSettingsContract.key(key),action.wire));
    }
    private String wifiReason(Bundle result) {
        if(result==null) return "wifi_unavailable";
        String reason=result.getString("reason","wifi_unavailable");
        switch(reason) {
            case "wifi_requested":case "wifi_scan_requested":case "wifi_scan_throttled":case "wifi_target_changed":return reason;
            default:return "wifi_unavailable";
        }
    }
    public Intent wifiConfiguration(String key) throws RemoteException {
        IAgentPlatform service=platform;
        if(service==null||!has("wifi_settings_v1")) return null;
        Bundle result=service.wifiConfiguration(dev.makepad.octosense.wifi.WifiSettingsContract.key(key));
        if(result==null||!result.getBoolean("ok")) return null;
        return result.getParcelable("intent");
    }
    public JSONObject captionLanguageSnapshot(long id,String key,String query,int offset)throws Exception{
        if(id<=0)throw new IllegalArgumentException("Invalid request");dev.makepad.octosense.controls.CaptionLocaleSettings.validateRead(key,query,offset);
        IAgentPlatform service=platform;if(service==null||!has("caption_language_v1"))return null;
        Bundle state=service.getCaptionLanguageSnapshot(id,key,query,offset);return state==null||!state.getBoolean("ok")?null:new JSONObject(state.getString("json"));
    }
    public String captionLanguageSet(String key,String choice)throws Exception{
        dev.makepad.octosense.controls.CaptionLocaleSettings.validateKey(key);dev.makepad.octosense.controls.CaptionLocaleSettings.validateKey(choice);
        IAgentPlatform service=platform;if(service==null||!has("caption_language_v1"))return "control_unavailable";
        Bundle state=service.setCaptionLanguage(key,choice);if(state==null)return "control_unavailable";String reason=state.getString("reason","");
        switch(reason){case "control_applied":case "control_requested":return state.getBoolean("ok")?reason:"control_unavailable";
            case "caption_language_changed":case "policy_restricted":case "control_unavailable":return reason;default:return "control_unavailable";}
    }
    public JSONObject captionCustomSnapshot(long id,IBinder owner,String session,long visit)throws Exception {
        dev.makepad.octosense.controls.CaptionCustomContract.read(id,visit);
        IAgentPlatform service=platform;if(service==null||!has("caption_custom_v1"))return null;
        Bundle state=service.getCaptionCustomSnapshot(id,owner,session,visit);return state==null||!state.getBoolean("ok")?null:new JSONObject(state.getString("json"));
    }
    public String captionCustomSet(IBinder owner,String session,long visit,String field,String value)throws Exception {
        dev.makepad.octosense.controls.CaptionCustomSettings.Field.parse(field).validate(value);
        IAgentPlatform service=platform;if(service==null||!has("caption_custom_v1"))return "control_unavailable";
        Bundle state=service.setCaptionCustom(owner,session,visit,field,value);if(state==null)return "control_unavailable";
        String reason=state.getString("reason","control_unavailable");switch(reason){case "control_applied":case "control_requested":return state.getBoolean("ok")?reason:"control_partial";case "control_partial":case "policy_restricted":return reason;default:return "control_unavailable";}
    }
    public Runnable captionCustomCloseTask(IBinder owner,String session,long visit){
        final IAgentPlatform captured=platform;return ()->{if(captured!=null)try{captured.closeCaptionCustom(owner,session,visit);}catch(RemoteException unavailable){/* Retirement is also enforced by Binder death and native TTL. */}};
    }
    public JSONObject controlsSnapshot(long id,dev.makepad.octosense.controls.SettingsControlsContract.Page page) throws Exception {
        IAgentPlatform service=platform;
        if(service==null||!has("settings_controls_v1")) return null;
        Bundle result=service.getControlsSnapshot(id,page.wire);
        if(result==null||!result.getBoolean("ok")) throw new SecurityException("Controls unavailable");
        return new JSONObject(result.getString("json",""));
    }
    public String setControl(dev.makepad.octosense.controls.SettingsControlsContract.Page page,
            dev.makepad.octosense.controls.SettingsControlsContract.Control control,String value) throws RemoteException {
        IAgentPlatform service=platform;control.validate(value);
        if(service==null||!has("settings_controls_v1")||control.page!=page) return "control_unavailable";
        Bundle result=service.setControl(page.wire,control.wire,value);
        if(result==null) return "control_unavailable";
        String reason=result.getString("reason","control_unavailable");
        switch(reason) {
            case "control_applied":case "control_requested":case "control_partial":case "policy_restricted":return reason;
            default:return "control_unavailable";
        }
    }

    /** Set Monet's preset seed while retaining unrelated per-user overlay choices. */
    public boolean applyThemePalette(String seed) {
        IAgentPlatform service = platform;
        if (service == null || !has("settings") || !seed.matches("[0-9a-fA-F]{6}")) return false;
        try {
            String key = "theme_customization_overlay_packages";
            Bundle old = service.getSetting("secure", key);
            if (!old.getBoolean("ok")) return false;
            String value = old.getString("value");
            JSONObject overlay = value == null || value.isEmpty() ? new JSONObject() : new JSONObject(value);
            overlay.put("android.theme.customization.system_palette", seed);
            overlay.put("android.theme.customization.accent_color", seed);
            overlay.put("android.theme.customization.color_source", "preset");
            overlay.put("android.theme.customization.theme_style", "TONAL_SPOT");
            return service.putSetting("secure", key, overlay.toString()).getBoolean("ok");
        } catch (Exception e) {
            Log.w(TAG, "Theme palette update failed");
            return false;
        }
    }
}
