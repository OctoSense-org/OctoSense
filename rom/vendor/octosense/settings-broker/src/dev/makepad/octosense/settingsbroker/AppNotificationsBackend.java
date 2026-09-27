package dev.makepad.octosense.settingsbroker;

import android.Manifest;
import android.app.ActivityManager;
import android.app.INotificationManager;
import android.app.KeyguardManager;
import android.app.NotificationChannel;
import android.app.NotificationChannelGroup;
import android.app.NotificationManager;
import android.content.Context;
import android.content.pm.PackageInfo;
import android.content.pm.PackageManager;
import android.net.Uri;
import android.os.Parcel;
import android.os.Parcelable;
import android.os.Process;
import android.os.SystemClock;
import android.os.UserHandle;
import android.os.UserManager;
import android.provider.Settings;
import android.util.Base64;
import dev.makepad.octosense.notifications.AppNotificationsContract;
import dev.makepad.octosense.notifications.AppNotificationsContract.Action;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Comparator;
import java.util.HashSet;
import java.util.List;
import java.util.Set;
import org.json.JSONArray;
import org.json.JSONObject;

/** Owner-only native notification settings. A copied channel preserves every unedited field. */
final class AppNotificationsBackend {
    private final Context context;
    private final AppNotificationsContract.Lease lease=new AppNotificationsContract.Lease();
    private final Set<String> observedTargets=new HashSet<>();
    private String observedPackage;
    AppNotificationsBackend(Context context){this.context=context;}
    private boolean owner(){UserManager users=context.getSystemService(UserManager.class);KeyguardManager lock=context.getSystemService(KeyguardManager.class);return Process.myUid()==Process.SYSTEM_UID&&UserHandle.myUserId()==0&&ActivityManager.getCurrentUser()==0&&users!=null&&users.isUserUnlocked()&&lock!=null&&!lock.isKeyguardLocked();}
    private boolean writePolicy(){UserManager users=context.getSystemService(UserManager.class);return owner()&&users.isAdminUser()&&!users.hasUserRestriction(UserManager.DISALLOW_APPS_CONTROL);}
    private INotificationManager manager(){return NotificationManager.getService();}
    private static String packed(Parcelable value){Parcel parcel=Parcel.obtain();try{value.writeToParcel(parcel,0);return Base64.encodeToString(parcel.marshall(),Base64.NO_WRAP);}finally{parcel.recycle();}}
    private static NotificationChannel copy(NotificationChannel value){Parcel parcel=Parcel.obtain();try{value.writeToParcel(parcel,0);parcel.setDataPosition(0);return NotificationChannel.CREATOR.createFromParcel(parcel);}finally{parcel.recycle();}}
    private static NotificationChannelGroup copy(NotificationChannelGroup value){Parcel parcel=Parcel.obtain();try{value.writeToParcel(parcel,0);parcel.setDataPosition(0);return NotificationChannelGroup.CREATOR.createFromParcel(parcel);}finally{parcel.recycle();}}
    private static String text(CharSequence value,int bound){return AppNotificationsContract.text(value,bound);}
    private static String hash(String value){return AppNotificationsContract.hash(value);}
    private static final class Row {
        String key,name,groupName;NotificationChannel channel;NotificationChannelGroup group;boolean enabled,effective,canSet,groupBlocked,linkedApp;
        final List<String> options=new ArrayList<>(),soundChanges=new ArrayList<>();
        JSONObject json()throws Exception{return new JSONObject().put("key",key).put("kind",channel==null?"group":"channel").put("name",name).put("group_name",groupName==null?JSONObject.NULL:groupName)
            .put("enabled",enabled).put("effective_enabled",effective).put("can_set",canSet).put("importance",channel==null||AppNotificationsContract.importance(channel.getImportance())==null?JSONObject.NULL:AppNotificationsContract.importance(channel.getImportance()))
            .put("importance_options",new JSONArray(options)).put("sound_change_options",new JSONArray(soundChanges))
            .put("sound",channel==null?JSONObject.NULL:channel.getSound()==null||Uri.EMPTY.equals(channel.getSound())?"Silent":"Configured sound")
            .put("linked_app",linkedApp).put("user_locked_importance",channel!=null&&(channel.getUserLockedFields()&NotificationChannel.USER_LOCKED_IMPORTANCE)!=0).put("group_blocked",groupBlocked);}
        String identity(){return key+":"+name+":"+groupName;}
        String fingerprint(){return identity()+":"+enabled+":"+effective+":"+canSet+":"+options+":"+(channel==null?packed(group):packed(channel));}
    }
    private static final class State {
        String pkg,label,appTarget,generation,fingerprint;int uid;boolean exists,enabled,requested,fixed,locked,suspended,canSet,onlyDefault,truncated;
        final List<Row> rows=new ArrayList<>();
        Row row(String key){for(Row row:rows)if(row.key.equals(key))return row;return null;}
        JSONObject app()throws Exception{return new JSONObject().put("key",appTarget).put("label",label).put("enabled",enabled).put("permission_requested",requested).put("permission_fixed",fixed).put("importance_locked",locked).put("suspended",suspended).put("can_set",canSet);}
    }
    private State read(String pkg)throws Exception {
        AppNotificationsContract.packageName(pkg);State state=new State();state.pkg=pkg;
        PackageManager packages=context.getPackageManager();PackageInfo info;
        try{info=packages.getPackageInfo(pkg,PackageManager.GET_PERMISSIONS|PackageManager.GET_SIGNING_CERTIFICATES);}catch(PackageManager.NameNotFoundException removed){state.generation=hash("missing:"+pkg);state.fingerprint=state.generation;return state;}
        state.exists=true;state.uid=info.applicationInfo.uid;
        if(UserHandle.getUserId(state.uid)!=0)throw new SecurityException("Wrong notification user");
        String signer="";if(info.signingInfo!=null)for(android.content.pm.Signature signature:info.signingInfo.getApkContentsSigners())signer+=hash(signature.toCharsString());
        String incarnation=pkg+":"+state.uid+":"+info.firstInstallTime+":"+info.lastUpdateTime+":"+info.getLongVersionCode()+":"+signer;
        state.appTarget=hash("app:"+incarnation);state.label=text(info.applicationInfo.loadLabel(packages),256);if(state.label.isEmpty())state.label=pkg;
        state.requested=info.requestedPermissions!=null&&Arrays.asList(info.requestedPermissions).contains(Manifest.permission.POST_NOTIFICATIONS);
        int flags=state.requested?packages.getPermissionFlags(Manifest.permission.POST_NOTIFICATIONS,pkg,UserHandle.SYSTEM):0;
        state.fixed=(flags&(PackageManager.FLAG_PERMISSION_SYSTEM_FIXED|PackageManager.FLAG_PERMISSION_POLICY_FIXED))!=0;
        state.suspended=packages.isPackageSuspended(pkg);
        INotificationManager manager=manager();if(manager==null)throw new IllegalStateException("Notification service unavailable");
        state.enabled=manager.areNotificationsEnabledForPackage(pkg,state.uid);
        state.locked=manager.isImportanceLocked(pkg,state.uid)||(info.applicationInfo.targetSdkVersion>32&&!state.requested);
        state.canSet=writePolicy()&&AppNotificationsContract.appWritable(true,state.requested,state.fixed,state.locked,state.suspended);
        state.onlyDefault=manager.onlyHasDefaultChannel(pkg,state.uid);
        List<NotificationChannelGroup> groups=new ArrayList<>(manager.getNotificationChannelGroupsForPackage(pkg,state.uid,false).getList());
        // PreferencesHelper includes a null-ID container for ungrouped channels.
        // It is not a stored group and must never acquire an actionable target.
        groups.removeIf(group->group.getId()==null);
        List<NotificationChannel> channels=manager.getNotificationChannelsForPackage(pkg,state.uid,false).getList();
        for(NotificationChannelGroup group:groups){Row row=new Row();row.group=group;row.key=hash(incarnation+":group:"+group.getId());row.name=text(group.getName(),256);if(row.name.isEmpty())row.name="Unnamed group";
            row.enabled=!group.isBlocked();row.effective=state.enabled&&row.enabled;
            row.canSet=writePolicy()&&!state.suspended&&!state.fixed&&(!state.locked||group.isBlocked());state.rows.add(row);}
        for(NotificationChannel channel:channels){if(channel.isDeleted())continue;Row row=new Row();row.channel=channel;row.key=hash(incarnation+":channel:"+channel.getId());row.name=text(channel.getName(),256);if(row.name.isEmpty())row.name="Unnamed channel";
            for(NotificationChannelGroup group:groups)if(group.getId().equals(channel.getGroup())){row.groupName=text(group.getName(),256);row.groupBlocked=group.isBlocked();break;}
            row.enabled=channel.getImportance()!=NotificationManager.IMPORTANCE_NONE;row.effective=state.enabled&&!row.groupBlocked&&row.enabled;
            row.linkedApp=state.onlyDefault&&NotificationChannel.DEFAULT_CHANNEL_ID.equals(channel.getId());
            row.canSet=writePolicy()&&!state.fixed&&AppNotificationsContract.channelToggleWritable(state.locked,channel.isBlockable(),row.enabled,state.suspended)&&AppNotificationsContract.importance(channel.getImportance())!=null&&(!row.linkedApp||state.canSet);
            // Pinned Settings only offers detailed importance while the parents and channel are on.
            if(row.canSet&&AppNotificationsContract.channelWritable(state.locked,channel.isBlockable(),state.suspended)&&state.enabled&&!row.groupBlocked&&row.enabled&&!NotificationChannel.DEFAULT_CHANNEL_ID.equals(channel.getId())){
                row.options.addAll(Arrays.asList("min","low","default","high"));
                if(channel.getImportance()<NotificationManager.IMPORTANCE_DEFAULT&&(channel.getSound()==null||Uri.EMPTY.equals(channel.getSound())))row.soundChanges.addAll(Arrays.asList("default","high"));
            }
            state.rows.add(row);
        }
        state.rows.sort(Comparator.comparing((Row row)->row.name,String.CASE_INSENSITIVE_ORDER).thenComparing(row->row.key));
        StringBuilder generation=new StringBuilder(incarnation),fingerprint=new StringBuilder(incarnation+":"+state.enabled+":"+state.requested+":"+flags+":"+state.locked+":"+state.suspended+":"+writePolicy());
        for(Row row:state.rows){generation.append('\n').append(row.identity());fingerprint.append('\n').append(row.fingerprint());}
        state.generation=hash(generation.toString());state.fingerprint=hash(fingerprint.toString());
        if(state.rows.size()>AppNotificationsContract.MAX_ROWS){state.truncated=true;state.rows.subList(AppNotificationsContract.MAX_ROWS,state.rows.size()).clear();}
        return state;
    }
    synchronized JSONObject snapshot(long id,String pkg,int offset,String expected)throws Exception {
        AppNotificationsContract.page(id,offset,expected);AppNotificationsContract.packageName(pkg);
        if(!owner()){lease.retire();observedTargets.clear();return AppNotificationsContract.unavailable(id,pkg,"restricted");}
        State state=read(pkg);if(!owner()){lease.retire();observedTargets.clear();return AppNotificationsContract.unavailable(id,pkg,"restricted");}
        boolean stale=expected!=null&&!expected.equals(state.generation)||offset>0&&offset>=state.rows.size();if(stale)offset=0;
        String key=lease.observe(state.fingerprint,SystemClock.elapsedRealtime());observedPackage=pkg;observedTargets.clear();JSONArray rows=new JSONArray();
        if(state.exists){observedTargets.add(state.appTarget);for(int i=offset;i<Math.min(state.rows.size(),offset+AppNotificationsContract.PAGE_SIZE);i++){Row row=state.rows.get(i);rows.put(row.json());observedTargets.add(row.key);}}
        return new JSONObject().put("schema",1).put("request_id",id).put("package",pkg).put("exists",state.exists).put("availability","available").put("key",key).put("app",state.exists?state.app():JSONObject.NULL)
            .put("generation",state.generation).put("offset",offset).put("total",state.rows.size()).put("truncated",state.truncated).put("stale",stale).put("rows",rows);
    }
    synchronized String apply(String pkg,String key,String target,Action action,String value)throws Exception {
        AppNotificationsContract.packageName(pkg);AppNotificationsContract.key(key);AppNotificationsContract.key(target);AppNotificationsContract.value(action,value);
        if(!writePolicy())return "notifications_restricted";
        if(!pkg.equals(observedPackage)||!observedTargets.contains(target))return "notifications_target_changed";
        State before=read(pkg);if(!before.exists)return "notifications_target_changed";Row row=before.row(target);
        boolean app=action==Action.APP_ENABLED;
        if(app?(!target.equals(before.appTarget)||!before.canSet):(row==null||!row.canSet))return "notifications_restricted";
        if(!app&&((action==Action.GROUP_ENABLED)!=(row.group!=null)))return "notifications_target_changed";
        if(action==Action.CHANNEL_IMPORTANCE&&!row.options.contains(value))return "notifications_restricted";
        if(!writePolicy()||!lease.claim(key,before.fingerprint,SystemClock.elapsedRealtime()))return "notifications_target_changed";
        final INotificationManager manager=manager();
        Row defaultRow=null;for(Row candidate:before.rows)if(candidate.linkedApp)defaultRow=candidate;
        if((app&&before.onlyDefault)||(action==Action.CHANNEL_ENABLED&&row.linkedApp)){
            if(defaultRow==null)return "notifications_unavailable";
            final Row linked=defaultRow;final boolean enabled=value.equals("on");
            final NotificationChannel edited=copy(linked.channel);
            edited.setImportance(enabled?NotificationManager.IMPORTANCE_UNSPECIFIED:NotificationManager.IMPORTANCE_NONE);
            return AppNotificationsContract.linkedWrite(new AppNotificationsContract.LinkedWrite(){
                public boolean allowed()throws Exception{
                    if(!writePolicy())return false;State fresh=read(pkg);Row current=fresh.row(linked.key);
                    return fresh.exists&&fresh.appTarget.equals(before.appTarget)&&fresh.canSet&&fresh.onlyDefault&&current!=null&&current.linkedApp&&current.canSet;
                }
                public void channel()throws Exception{manager.updateNotificationChannelForPackage(pkg,before.uid,edited);}
                public void app()throws Exception{manager.setNotificationsEnabledForPackage(pkg,before.uid,enabled);}
                public boolean confirmed()throws Exception{
                    if(!owner())return false;State after=read(pkg);Row actual=after.row(linked.key);
                    return after.exists&&after.appTarget.equals(before.appTarget)&&after.enabled==enabled&&actual!=null&&actual.channel.getImportance()==edited.getImportance();
                }
            });
        }
        NotificationChannel edited=null;
        try{
            if(app)manager.setNotificationsEnabledForPackage(pkg,before.uid,value.equals("on"));
            else if(action==Action.GROUP_ENABLED){NotificationChannelGroup group=copy(row.group);group.setBlocked(value.equals("off"));manager.updateNotificationChannelGroupForPackage(pkg,before.uid,group);}
            else{
                edited=copy(row.channel);
                if(action==Action.CHANNEL_ENABLED){if(value.equals("off")||edited.getImportance()==NotificationManager.IMPORTANCE_NONE)edited.setImportance(value.equals("off")?NotificationManager.IMPORTANCE_NONE:Math.max(edited.getOriginalImportance(),NotificationManager.IMPORTANCE_LOW));}
                else{int importance=AppNotificationsContract.importance(value);if(row.soundChanges.contains(value)){edited.setSound(Settings.System.DEFAULT_NOTIFICATION_URI,edited.getAudioAttributes());edited.lockFields(NotificationChannel.USER_LOCKED_SOUND);}edited.setImportance(importance);edited.lockFields(NotificationChannel.USER_LOCKED_IMPORTANCE);}
                manager.updateNotificationChannelForPackage(pkg,before.uid,edited);
            }
            if(!owner())return "notifications_unconfirmed";
            State after=read(pkg);if(!after.exists||!after.appTarget.equals(before.appTarget))return "notifications_target_changed";
            if(app)return after.enabled==value.equals("on")?"notifications_applied":"notifications_unconfirmed";
            Row actual=after.row(target);if(actual==null)return "notifications_target_changed";
            if(action==Action.CHANNEL_IMPORTANCE)return actual.channel!=null&&actual.channel.getImportance()==edited.getImportance()&&java.util.Objects.equals(actual.channel.getSound(),edited.getSound())&&(actual.channel.getUserLockedFields()&edited.getUserLockedFields())==edited.getUserLockedFields()?"notifications_applied":"notifications_unconfirmed";
            return actual.enabled==value.equals("on")?"notifications_applied":"notifications_unconfirmed";
        }catch(Exception failed){return "notifications_unconfirmed";}
    }
}
