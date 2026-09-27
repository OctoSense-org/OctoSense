package com.android.permissioncontroller.octosense;

import android.app.Application;
import android.app.AppOpsManager;
import android.content.Context;
import android.content.pm.ApplicationInfo;
import android.content.pm.PackageInfo;
import android.content.pm.PackageManager;
import android.os.Handler;
import android.os.Looper;
import android.os.Process;
import android.os.UserHandle;
import androidx.lifecycle.Observer;
import androidx.lifecycle.ViewModelStore;
import com.android.permissioncontroller.permission.ui.Category;
import com.android.permissioncontroller.permission.ui.model.AppPermissionGroupsViewModel;
import com.android.permissioncontroller.permission.ui.model.AppPermissionViewModel;
import com.android.permissioncontroller.permission.ui.model.AppPermissionViewModel.ButtonState;
import com.android.permissioncontroller.permission.ui.model.AppPermissionViewModel.ButtonType;
import com.android.permissioncontroller.permission.utils.v35.MultiDeviceUtils;
import dev.makepad.octosense.permissions.PermissionsSettingsContract;
import dev.makepad.octosense.permissions.PermissionsSettingsContract.Choice;
import dev.makepad.octosense.permissions.PermissionsSettingsContract.Group;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;
import java.util.Map;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicReference;
import java.util.function.Consumer;

/** Native policy projection. Observers and ViewModels live exclusively on the main thread. */
final class NativePermissionModel {
    static final long READ_MS=5000;
    private final Context context;
    private final Handler main=new Handler(Looper.getMainLooper());
    NativePermissionModel(Context context){this.context=context.getApplicationContext();}
    static final class Row {
        Group group;String name,label,category,subtitle,target;
    }
    static final class Option {
        final Choice choice;String label;final boolean selected,enabled;
        Option(Choice choice,boolean selected,boolean enabled){this.choice=choice;label=choice.label;this.selected=selected;this.enabled=enabled&&choice.actionable()&&!selected;}
    }
    static final class State {
        String pkg,identity,rawState,label,fingerprint,generation,detail;
        Group group;boolean exists,groupAvailable;
        final List<Row> rows=new ArrayList<>();final List<Option> choices=new ArrayList<>();
        Option option(Choice choice){for(Option item:choices)if(item.choice==choice)return item;return null;}
    }
    static String hash(String value){return PermissionsSettingsContract.hash(value);}
    private static String text(CharSequence value){return PermissionsSettingsContract.text(value,256);}

    State read(String pkg,Group group)throws Exception{
        if(Looper.myLooper()==Looper.getMainLooper())throw new IllegalStateException("Permission reads cannot block main");
        CountDownLatch done=new CountDownLatch(1);AtomicBoolean finished=new AtomicBoolean();
        AtomicReference<State> result=new AtomicReference<>();AtomicReference<Session> active=new AtomicReference<>();
        main.post(()->{
            if(finished.get())return;
            Session session=new Session(pkg,group,state->{if(finished.compareAndSet(false,true)){result.set(state);done.countDown();}Session current=active.get();if(current!=null)current.close();});
            active.set(session);session.start();
        });
        try{
            if(!done.await(READ_MS,TimeUnit.MILLISECONDS))throw new IllegalStateException("Native permission model timed out");
            State state=result.get();if(state==null)throw new IllegalStateException("Native permission model unavailable");return state;
        }finally{
            finished.set(true);main.post(()->{Session current=active.get();if(current!=null)current.close();});
        }
    }

    Session observe(String pkg,Group group,Consumer<State> callback){
        if(Looper.myLooper()!=Looper.getMainLooper())throw new IllegalStateException("Permission observer must run on main");
        return new Session(pkg,group,callback);
    }
    final class Session implements AutoCloseable {
        private final String pkg;private final Group selected;private final Consumer<State> callback;
        private final ViewModelStore store=new ViewModelStore();private boolean closed,detailStarted,detailEmitted;
        private AppPermissionGroupsViewModel inventory;AppPermissionViewModel model;
        private Observer<Map<Category,List<AppPermissionGroupsViewModel.GroupUiInfo>>> inventoryObserver;
        private Observer<Map<ButtonType,ButtonState>> choiceObserver;
        Session(String pkg,Group selected,Consumer<State> callback){this.pkg=pkg;this.selected=selected;this.callback=callback;}
        void start(){
            if(closed)return;
            try{
                State initial=packageState(pkg,selected);if(!initial.exists){callback.accept(initial);return;}
                inventory=new AppPermissionGroupsViewModel(pkg,UserHandle.SYSTEM,android.os.SystemClock.elapsedRealtime());store.put("inventory",inventory);
                inventoryObserver=value->{if(closed||inventory.getPackagePermGroupsLiveData().isStale())return;
                    if(value==null){callback.accept(null);return;}
                    if(selected==null){publish();return;}
                    if(!detailStarted){
                        boolean present=false;for(List<AppPermissionGroupsViewModel.GroupUiInfo> rows:value.values())for(AppPermissionGroupsViewModel.GroupUiInfo row:rows)
                            if(selected.nativeName.equals(row.getGroupName())&&MultiDeviceUtils.isDefaultDeviceId(row.getPersistentDeviceId()))present=true;
                        if(!present){publish();return;}
                        detailStarted=true;
                        model=new AppPermissionViewModel((Application)context,pkg,selected.nativeName,UserHandle.SYSTEM,android.os.SystemClock.elapsedRealtime(),MultiDeviceUtils.getDefaultDevicePersistentDeviceId());store.put("group",model);
                        // Mirror the native fragment: a newly emitted non-null button map
                        // is usable even when aggregate isStale remains true. The native
                        // static "no safety rationale" value is initialized before active
                        // observers and never clears that flag. Inventory freshness and
                        // the model's actual emitted choices remain mandatory.
                        choiceObserver=choices->{if(closed)return;detailEmitted=choices!=null;
                            if(detailEmitted)publish();else if(!model.getButtonStateLiveData().isStale())callback.accept(null);};
                        model.getButtonStateLiveData().observeForever(choiceObserver);
                    }else publish();
                };
                inventory.getPackagePermGroupsLiveData().observeForever(inventoryObserver);
            }catch(Exception unavailable){callback.accept(null);}
        }
        State current(){
            if(closed||inventory==null||inventory.getPackagePermGroupsLiveData().isStale()||inventory.getPackagePermGroupsLiveData().getValue()==null)return null;
            if(selected!=null&&detailStarted&&(model==null||!detailEmitted||model.getButtonStateLiveData().getValue()==null))return null;
            try{return assemble(pkg,selected,inventory.getPackagePermGroupsLiveData().getValue(),model);}catch(Exception unavailable){return null;}
        }
        private void publish(){State state=current();if(state!=null)callback.accept(state);}
        @Override public void close(){if(closed)return;closed=true;
            if(inventory!=null&&inventoryObserver!=null)inventory.getPackagePermGroupsLiveData().removeObserver(inventoryObserver);
            if(model!=null&&choiceObserver!=null)model.getButtonStateLiveData().removeObserver(choiceObserver);
            store.clear();
        }
    }

    private State packageState(String pkg,Group group)throws Exception{
        State state=new State();state.pkg=pkg;state.group=group;
        PackageManager pm=context.getPackageManager();PackageInfo info;
        try{info=pm.getPackageInfo(pkg,PackageManager.GET_PERMISSIONS|PackageManager.GET_SIGNING_CERTIFICATES);}
        catch(PackageManager.NameNotFoundException removed){state.exists=false;return state;}
        ApplicationInfo app=info.applicationInfo;
        if(app==null||(app.flags&ApplicationInfo.FLAG_INSTALLED)==0||!UserHandle.SYSTEM.equals(UserHandle.getUserHandleForUid(app.uid)))return state;
        state.exists=true;state.label=text(app.loadLabel(pm));if(state.label.isEmpty())state.label=pkg;
        StringBuilder identity=new StringBuilder(pkg+":"+app.uid+":"+info.firstInstallTime+":"+info.lastUpdateTime+":"+info.getLongVersionCode());
        if(info.signingInfo==null)throw new IllegalStateException("Permission target signer unavailable");
        ArrayList<String> signers=new ArrayList<>();for(android.content.pm.Signature signer:info.signingInfo.getApkContentsSigners())signers.add(hash(signer.toCharsString()));signers.sort(String::compareTo);for(String signer:signers)identity.append(':').append(signer);state.identity=hash(identity.toString());
        int policyFlags=app.flags&(ApplicationInfo.FLAG_INSTALLED|ApplicationInfo.FLAG_SUSPENDED|ApplicationInfo.FLAG_SYSTEM|ApplicationInfo.FLAG_UPDATED_SYSTEM_APP);
        StringBuilder raw=new StringBuilder(state.identity+":"+app.enabled+":"+policyFlags);
        AppOpsManager ops=context.getSystemService(AppOpsManager.class);if(ops==null)throw new IllegalStateException("AppOps unavailable");
        ArrayList<String> permissions=new ArrayList<>();if(info.requestedPermissions!=null)java.util.Collections.addAll(permissions,info.requestedPermissions);permissions.sort(String::compareTo);
        for(String permission:permissions){raw.append(':').append(permission).append(':').append(pm.checkPermission(permission,pkg)).append(':').append(pm.getPermissionFlags(permission,pkg,UserHandle.SYSTEM));
            String op=AppOpsManager.permissionToOp(permission);if(op!=null)raw.append(':').append(op).append(':').append(ops.unsafeCheckOpRawNoThrow(op,app.uid,pkg));}
        state.rawState=hash(raw.toString());return state;
    }
    private State assemble(String pkg,Group selected,Map<Category,List<AppPermissionGroupsViewModel.GroupUiInfo>> inventory,AppPermissionViewModel model)throws Exception{
        State state=packageState(pkg,selected);if(!state.exists)return state;
        for(Map.Entry<Category,List<AppPermissionGroupsViewModel.GroupUiInfo>> section:inventory.entrySet()){
            String category=section.getKey()==Category.ALLOWED?"allowed":section.getKey()==Category.ASK?"ask":section.getKey()==Category.DENIED?"denied":null;if(category==null)continue;
            for(AppPermissionGroupsViewModel.GroupUiInfo item:section.getValue()){
                if(!MultiDeviceUtils.isDefaultDeviceId(item.getPersistentDeviceId()))continue;
                Row row=new Row();row.name=item.getGroupName();row.group=Group.fromNative(row.name);row.category=category;
                try{row.label=text(context.getPackageManager().getPermissionGroupInfo(row.name,0).loadLabel(context.getPackageManager()));}
                catch(PackageManager.NameNotFoundException missingNativeOnlyLabel){row.label="";}
                if(row.label.isEmpty())row.label=row.group==null?PermissionsSettingsContract.text(row.name,256):row.group.label;
                switch(item.getSubtitle()){
                    case FOREGROUND_ONLY:row.subtitle="Only while using the app";break;
                    case BACKGROUND:row.subtitle="All the time";break;
                    case MEDIA_ONLY:row.subtitle="Media only";break;
                    case ALL_FILES:row.subtitle="All files";break;
                    default:break;
                }
                row.target=hash(state.identity+":"+row.name);state.rows.add(row);
            }
        }
        state.rows.sort(Comparator.comparing((Row row)->row.label,String.CASE_INSENSITIVE_ORDER).thenComparing(row->row.name));
        StringBuilder generation=new StringBuilder(state.identity),fingerprint=new StringBuilder(state.rawState);
        for(Row row:state.rows){generation.append(':').append(row.target).append(':').append(row.label);fingerprint.append(':').append(row.target).append(':').append(row.category).append(':').append(row.subtitle);if(row.group==selected&&selected!=null)state.groupAvailable=true;}
        state.generation=hash(generation.toString());
        if(selected!=null&&state.groupAvailable&&model!=null){
            Map<ButtonType,ButtonState> buttons=model.getButtonStateLiveData().getValue();
            add(state,buttons,ButtonType.ALLOW,Choice.ALLOW);add(state,buttons,ButtonType.ALLOW_ALWAYS,Choice.ALLOW_ALWAYS);
            add(state,buttons,ButtonType.ALLOW_FOREGROUND,Choice.ALLOW_FOREGROUND);add(state,buttons,ButtonType.ASK,Choice.ASK);
            add(state,buttons,ButtonType.ASK_ONCE,Choice.ONE_TIME);add(state,buttons,ButtonType.DENY,Choice.DENY);add(state,buttons,ButtonType.DENY_FOREGROUND,Choice.DENY_FOREGROUND);
            ButtonState accuracy=buttons.get(ButtonType.LOCATION_ACCURACY);
            if(accuracy!=null&&accuracy.isShown()){
                state.choices.add(new Option(Choice.PRECISE,accuracy.isChecked(),accuracy.isEnabled()));
                state.choices.add(new Option(Choice.APPROXIMATE,!accuracy.isChecked(),accuracy.isEnabled()));
            }
            kotlin.Pair<Integer,Integer> detail=model.getDetailResIdLiveData().getValue();
            if(detail!=null&&detail.getFirst()!=null&&detail.getFirst()!=0){state.detail=PermissionsSettingsContract.text(detail.getSecond()==null?context.getString(detail.getFirst()):context.getString(detail.getFirst(),detail.getSecond()),512);}
        }
        for(Option option:state.choices){
            String resource;
            switch(option.choice){
                case ALLOW:resource="app_permission_button_allow";break;
                case ALLOW_ALWAYS:resource="app_permission_button_allow_always";break;
                case ALLOW_FOREGROUND:resource="app_permission_button_allow_foreground";break;
                case ASK:resource="app_permission_button_ask";break;
                case DENY:case DENY_FOREGROUND:resource="app_permission_button_deny";break;
                case PRECISE:resource="app_permission_location_accuracy";break;
                default:resource=null;
            }
            if(resource!=null){int id=context.getResources().getIdentifier(resource,"string",context.getPackageName());if(id!=0){String label=text(context.getString(id));if(!label.isEmpty())option.label=label;}}
            fingerprint.append(':').append(option.choice.wire).append(':').append(option.selected).append(':').append(option.enabled);
        }
        state.fingerprint=hash(fingerprint.toString());return state;
    }
    private static void add(State state,Map<ButtonType,ButtonState> buttons,ButtonType type,Choice choice){ButtonState value=buttons.get(type);if(value!=null&&value.isShown())state.choices.add(new Option(choice,value.isChecked(),value.isEnabled()));}
}
