package dev.makepad.octosense;

import android.app.AlertDialog;
import android.content.ComponentName;
import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.content.ServiceConnection;
import android.content.SharedPreferences;
import android.content.res.Configuration;
import android.view.HapticFeedbackConstants;
import android.view.View;
import android.view.Window;
import android.content.pm.LauncherActivityInfo;
import android.content.pm.LauncherApps;
import android.content.pm.PackageManager;
import android.content.pm.ShortcutInfo;
import android.graphics.Bitmap;
import android.graphics.Canvas;
import android.graphics.drawable.Drawable;
import android.os.Bundle;
import android.os.Handler;
import android.os.IBinder;
import android.os.Looper;
import android.os.RemoteException;
import android.os.UserHandle;
import android.os.UserManager;
import android.provider.Settings;
import dev.makepad.android.MakepadActivity;
import dev.makepad.android.MakepadNative;
import dev.makepad.octosense.contracts.ISystemBridge;
import dev.makepad.octosense.contracts.ISystemBridgeCallback;
import dev.makepad.octosense.contracts.Protocol;
import java.io.File;
import java.io.FileOutputStream;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.util.ArrayList;
import java.util.Collections;
import java.util.Comparator;
import java.util.HashMap;
import java.util.HashSet;
import java.util.Map;
import java.util.LinkedHashMap;
import java.util.UUID;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.RejectedExecutionException;
import java.util.concurrent.ThreadPoolExecutor;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;
import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

/** Public launcher client and asynchronous bridge adapter in the Home process. */
public final class MakepadAppExtension implements MakepadActivity.ApplicationExtension {
    private final MakepadActivity activity;
    private final Handler main=new Handler(Looper.getMainLooper());
    private final ThreadPoolExecutor worker=new ThreadPoolExecutor(1,1,0,TimeUnit.MILLISECONDS,
            new ArrayBlockingQueue<>(32),r -> new Thread(r,"OctoSenseAndroid"),new ThreadPoolExecutor.AbortPolicy());
    private final LauncherApps launcher;
    private final UserManager users;
    private volatile NativeWidgets widgets;
    private final HomeGeometryClient homeGeometry;
    private final NativeReplyComposer replyComposer;
    private final NotificationAppIdentity notificationIdentity;
    private Bundle lastBridgeSnapshot;
    private boolean widgetsVisible,replyVisible;
    // Worker-owned, opaque handles from the authenticated bridge snapshot.
    private final Map<String,String[]> replyTargets=new HashMap<>();
    private final Map<Long,String> replyCommands=new HashMap<>();
    private final boolean validationBuild;
    private java.io.Closeable validationRemote;
    private final String session=UUID.randomUUID().toString();
    private final AtomicBoolean catalogQueued=new AtomicBoolean();
    private final AtomicBoolean queueRecovery=new AtomicBoolean();
    private final AtomicBoolean queueCommandLoss=new AtomicBoolean();
    private final AtomicBoolean recoveryScheduled=new AtomicBoolean();
    private final Map<String,LauncherActivityInfo> apps=new HashMap<>();
    private final Map<String,ShortcutInfo> shortcuts=new HashMap<>();
    private ISystemBridge bridge;
    private long catalogRevision;
    private LauncherPlacements placements;
    private File placementFile;
    private long placementRevision;
    private AlertDialog placementDialog;
    private volatile JSONObject placementSnapshot=new JSONObject();
    private volatile boolean launcherFixtureAvailable;
    private volatile String observedBridgeState="disconnected";
    private volatile JSONArray observedNotificationPresentation=new JSONArray();
    private volatile JSONObject observedNotificationRenderer=new JSONObject();
    private volatile JSONObject observedLauncherRenderer=new JSONObject();
    private long commandId;
    private String bridgeEpoch="";
    private long bridgeRevision=-1;
    private boolean bound;
    private volatile boolean destroyed;
    private boolean resumed;
    private int reconnectAttempt;
    /** The shell's appearance, for the system-bar icons and the native dialogs. */
    private volatile boolean shellDark;
    /** Which first-use hints the person has already found (mobile_hints.rs). */
    private final SharedPreferences hints;
    /** The shell's hit regions as accessibility nodes (ShellAccessibility.java). */
    private final ShellAccessibility accessibility;
    private final LinkedHashMap<String,String[]> outbound=new LinkedHashMap<>();
    private boolean flushScheduled;
    private boolean resyncNeeded;
    private boolean commandOutcomeUncertain;

    public MakepadAppExtension(MakepadActivity activity) {
        this.activity=activity;
        launcher=activity.getSystemService(LauncherApps.class);
        users=activity.getSystemService(UserManager.class);
        homeGeometry=new HomeGeometryClient(activity,this::offer,this::emit);
        replyComposer=new NativeReplyComposer(activity,(token,handle,text) -> offer(() ->
                emit("notification.reply.submit",json("token",token,"handle",handle,"reply",text))),
                visible -> {replyVisible=visible;updateNativeCoverage();});
        boolean validation=false;
        try {
            Bundle metadata=activity.getPackageManager().getApplicationInfo(activity.getPackageName(),PackageManager.GET_META_DATA).metaData;
            validation=metadata!=null&&metadata.getBoolean("octosense.validation",false);
        } catch(Exception ignored) {}
        validationBuild=validation;
        notificationIdentity=new NotificationAppIdentity(activity,new File(activity.getCacheDir(),
                validation&&(dev.makepad.octosense.validation.NotificationUiFixture.active()
                    ||dev.makepad.octosense.validation.NotificationFlowFixture.active())?"notification-validation-icons":"notification-icons"));
        if(validation&&dev.makepad.octosense.validation.NotificationUiFixture.active()) {
            dev.makepad.octosense.validation.NotificationUiFixture.attach(state -> {
                if(!offer(() -> {
                    bridgeEpoch="notification-fixture:"+session;
                    publishBridgeSnapshot(bridgeEpoch,++bridgeRevision,state);
                })) throw new IllegalStateException("Fixture worker is full");
            });
        }
        boolean launcherTest=validation&&dev.makepad.octosense.validation.LauncherUiFixture.active();
        placementFile=validation&&(launcherTest||(activity.getIntent()!=null&&activity.getIntent().getBooleanExtra("octosense.placement_test",false)))
                ?new File(activity.getCacheDir(),"home-geometry-placements.json"):new File(activity.getFilesDir(),"launcher-placements.json");
        boolean widgetTest=validation&&activity.getIntent()!=null&&activity.getIntent().getBooleanExtra("octosense.widget_test",false);
        widgets=new NativeWidgets(activity,this::offer,reason -> result(0,Protocol.UNCERTAIN,reason),
                model -> emit("launcher.widgets",model),widgetTest?0x4f4356:NativeWidgets.HOST_ID,
                widgetTest?new File(activity.getCacheDir(),"widget-ui-validation.json"):new File(activity.getFilesDir(),"launcher-widgets.json"));
        widgets.setVisibilityListener(visible -> {widgetsVisible=visible;updateNativeCoverage();});
        if(validation&&(launcherTest||(activity.getIntent()!=null&&activity.getIntent().getBooleanExtra("--remote",false)))) {
            try {validationRemote=new dev.makepad.octosense.validation.ValidationRemote(activity,
                    () -> widgets.show(),() -> widgets.hide(),() -> widgets.validationState().put("home_integration",homeGeometry.validationState()).put("launcher",validationLauncherState()),this::validationWindow);}
            catch(Exception e) {android.util.Log.e("OctoSenseValidation","Remote startup failed",e);}
        }
        // All package/profile queries and bitmap work execute on one worker.
        launcher.registerCallback(packageCallback,main);
        IntentFilter profileEvents=new IntentFilter();
        for(String action:new String[]{Intent.ACTION_MANAGED_PROFILE_AVAILABLE,Intent.ACTION_MANAGED_PROFILE_UNAVAILABLE,
                Intent.ACTION_MANAGED_PROFILE_UNLOCKED,Intent.ACTION_MANAGED_PROFILE_ADDED,Intent.ACTION_MANAGED_PROFILE_REMOVED,Intent.ACTION_USER_UNLOCKED}) profileEvents.addAction(action);
        if(android.os.Build.VERSION.SDK_INT>=33) activity.registerReceiver(profileCallback,profileEvents,Context.RECEIVER_EXPORTED);
        else activity.registerReceiver(profileCallback,profileEvents);
        hints=activity.getSharedPreferences("octosense-hints",Context.MODE_PRIVATE);
        accessibility=new ShellAccessibility(activity,index -> emit("a11y.activate",json("index",index)));
        activity.getApplicationOverlay().addView(accessibility,new android.widget.FrameLayout.LayoutParams(
                android.view.ViewGroup.LayoutParams.MATCH_PARENT,android.view.ViewGroup.LayoutParams.MATCH_PARENT));
        activity.registerComponentCallbacks(new android.content.ComponentCallbacks() {
            @Override public void onConfigurationChanged(Configuration configuration) { offer(MakepadAppExtension.this::emitUiMode); }
            @Override public void onLowMemory() {}
        });
        refreshCatalog();
        onIntent(activity.getIntent());
    }
    /** A native dialog in the shell's appearance rather than the device default. */
    private AlertDialog.Builder dialog(boolean dark) {
        int theme=dark?android.R.style.Theme_DeviceDefault_Dialog_Alert:android.R.style.Theme_DeviceDefault_Light_Dialog_Alert;
        return new AlertDialog.Builder(activity,theme);
    }
    /** A committed shell gesture or a long press: the platform's own haptic, honouring the system touch-feedback setting. */
    private void haptic(String kind) {
        if(destroyed||activity.isFinishing()) return;
        Window window=activity.getWindow();
        View view=window==null?null:window.getDecorView();
        if(view==null) return;
        int constant;
        switch(kind) {
            case "long_press": constant=HapticFeedbackConstants.LONG_PRESS; break;
            case "confirm": constant=android.os.Build.VERSION.SDK_INT>=30?HapticFeedbackConstants.CONFIRM:HapticFeedbackConstants.CONTEXT_CLICK; break;
            default: constant=HapticFeedbackConstants.CLOCK_TICK; break;
        }
        view.performHapticFeedback(constant);
    }
    /**
     * Edge-to-edge: the system bars are transparent over the shell's own
     * wallpaper and their icons follow the shell's appearance, so the shell
     * draws no second status bar and no second navigation pill. Makepad
     * reports the bars as safe-area insets; the shell lays out inside them.
     */
    private void applyWindowChrome() {
        if(destroyed||activity.isFinishing()) return;
        Window window=activity.getWindow();
        if(window==null||android.os.Build.VERSION.SDK_INT<30) return;
        window.setDecorFitsSystemWindows(false);
        window.setStatusBarColor(android.graphics.Color.TRANSPARENT);
        window.setNavigationBarColor(android.graphics.Color.TRANSPARENT);
        window.setNavigationBarContrastEnforced(false);
        android.view.WindowInsetsController controller=window.getInsetsController();
        if(controller!=null) {
            int mask=android.view.WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS|android.view.WindowInsetsController.APPEARANCE_LIGHT_NAVIGATION_BARS;
            controller.setSystemBarsAppearance(shellDark?0:mask,mask);
        }
    }
    private void emitUiMode() {
        Configuration configuration=activity.getResources().getConfiguration();
        boolean night=(configuration.uiMode&Configuration.UI_MODE_NIGHT_MASK)==Configuration.UI_MODE_NIGHT_YES;
        emit("launcher.ui_mode",json("dark",night,"font_scale_percent",Math.round(configuration.fontScale*100f)));
    }
    private void emitHints() {
        JSONArray seen=new JSONArray();
        for(String key:new String[]{"search","shade","recents"}) if(hints.getBoolean(key,false)) seen.put(key);
        emit("launcher.hints",json("seen",seen));
    }
    private boolean offer(Runnable task) {
        if(destroyed) return false;
        try { worker.execute(() -> {
            if(destroyed) return;
            recoverDelivery();
            try { task.run(); } finally { recoverDelivery(); }
        }); return true; }
        catch(RejectedExecutionException e) { queueRecovery.set(true); scheduleRecovery(); return false; }
    }
    private void scheduleRecovery() {
        if(!destroyed && recoveryScheduled.compareAndSet(false,true)) main.post(this::enqueueRecovery);
    }
    private void enqueueRecovery() {
        if(destroyed) {recoveryScheduled.set(false);return;}
        try {
            worker.execute(() -> {
                recoveryScheduled.set(false);
                if(!destroyed) recoverDelivery();
            });
        } catch(RejectedExecutionException e) {main.postDelayed(this::enqueueRecovery,50);}
    }
    private void recoverDelivery() {
        boolean droppedState=queueRecovery.getAndSet(false);
        boolean droppedCommand=queueCommandLoss.getAndSet(false);
        if(droppedState || droppedCommand) {
            resyncNeeded=true;
            commandOutcomeUncertain|=droppedCommand;
            scheduleFlush();
        }
    }
    private void emit(String channel,JSONObject payload) {
        if(destroyed) return;
        String data=payload.toString();
        if(data.getBytes(StandardCharsets.UTF_8).length>Protocol.MAX_PACKET_BYTES) { resyncNeeded=true; scheduleFlush(); return; }
        if(outbound.isEmpty() && MakepadNative.onAndroidIntegrationEvent(channel,data)) return;
        String key=channel;
        if(channel.endsWith(".result")) key+=":"+payload.optLong("id",0);
        if(channel.equals("launcher.catalog")) key+=":"+payload.optLong("chunk",0);
        outbound.put(key,new String[]{channel,data});
        if(outbound.size()>192) {
            String[] lost=outbound.remove(outbound.keySet().iterator().next());
            resyncNeeded=true; commandOutcomeUncertain|=lost[0].endsWith(".result");
        }
        scheduleFlush();
    }
    private void scheduleFlush() {
        if(flushScheduled || destroyed) return;
        flushScheduled=true;
        main.postDelayed(this::tryFlushOnMain,32);
    }
    private void tryFlushOnMain() {
        if(destroyed) return;
        if(!offer(this::flushEvents)) main.postDelayed(this::tryFlushOnMain,50);
    }
    private void flushEvents() {
        flushScheduled=false;
        if(resyncNeeded) {
            JSONObject signal=json("command_outcome_uncertain",commandOutcomeUncertain);
            if(!MakepadNative.onAndroidIntegrationEvent("integration.resync",signal.toString())) {scheduleFlush();return;}
            resyncNeeded=false;commandOutcomeUncertain=false;
        }
        java.util.Iterator<Map.Entry<String,String[]>> iterator=outbound.entrySet().iterator();
        while(iterator.hasNext()) {
            String[] packet=iterator.next().getValue();
            if(!MakepadNative.onAndroidIntegrationEvent(packet[0],packet[1])) break;
            iterator.remove();
        }
        if(!outbound.isEmpty()) scheduleFlush();
    }
    private static JSONObject json(Object... fields) {
        JSONObject value=new JSONObject();
        try { for(int i=0;i<fields.length;i+=2) value.put((String)fields[i],fields[i+1]); }
        catch(JSONException e) { throw new IllegalArgumentException(e); }
        return value;
    }
    private void result(long id,int status,String reason) { emit("launcher.result",json("id",id,"status",status,"reason",reason)); }
    private void updateNativeCoverage() {homeGeometry.setCovered(widgetsVisible||replyVisible);}
    private android.view.Window validationWindow() {
        android.view.Window pin=ShortcutPinActivity.validationWindow();
        if(pin!=null) return pin;
        return placementDialog!=null&&placementDialog.isShowing()?placementDialog.getWindow():activity.getWindow();
    }
    private JSONObject validationLauncherState() throws JSONException {
        JSONObject state=new JSONObject().put("epoch",session).put("placements",new JSONObject(placementSnapshot.toString()))
                .put("fixture_available",launcherFixtureAvailable).put("bridge_state",observedBridgeState);
        JSONArray items=new JSONArray();
        if(placementDialog!=null&&placementDialog.isShowing()) {
            android.widget.ListView list=placementDialog.getListView();
            for(int index=0;index<list.getChildCount();index++) {
                android.view.View view=list.getChildAt(index);int[] position=new int[2];view.getLocationInWindow(position);
                items.put(json("label",String.valueOf(list.getItemAtPosition(list.getFirstVisiblePosition()+index)),
                        "x",position[0]+view.getWidth()/2,"y",position[1]+view.getHeight()/2));
            }
        }
        return state.put("menu",items).put("pin",ShortcutPinActivity.validationState())
                .put("notification_fixture",dev.makepad.octosense.validation.NotificationUiFixture.active())
                .put("notification_presentation",observedNotificationPresentation).put("notification_renderer",observedNotificationRenderer)
                .put("reply_editor",replyComposer.validationState())
                .put("renderer",observedLauncherRenderer);
    }
    private void closePlacementMenu() {if(placementDialog!=null) {placementDialog.dismiss();placementDialog=null;}}
    private void updateReplyTargets(Bundle state) {
        replyTargets.clear();
        ArrayList<Bundle> notices=state.getParcelableArrayList("notifications");
        if(notices!=null) for(Bundle notice:notices) {
            ArrayList<Bundle> actions=notice.getParcelableArrayList("actions");
            if(actions!=null) for(Bundle action:actions) if(action.getBoolean("reply")) {
                String handle=action.getString("handle","");
                if(!handle.isEmpty()) replyTargets.put(handle,new String[]{notice.getString("title",""),action.getString("label","Reply")});
            }
        }
        HashSet<String> current=new HashSet<>(replyTargets.keySet());
        main.post(() -> replyComposer.updateTargets(current));
    }
    @Override public void command(String channel,String payload) {
        if(validationBuild&&"validation.ui".equals(channel)) {
            try {observedNotificationRenderer=new JSONObject(payload);} catch(JSONException ignored) {}
            return;
        }
        if(validationBuild&&"validation.launcher_ui".equals(channel)) {
            try {observedLauncherRenderer=new JSONObject(payload);} catch(JSONException ignored) {}
            return;
        }
        // Local view geometry has no worker/Binder/shell round trip. The
        // renderer coalesces unchanged layouts; native state is cached already.
        if("widgets.layout".equals(channel)) {widgets.layout(payload);return;}
        if("home.layout".equals(channel)) {homeGeometry.layout(payload);return;}
        if("a11y.layout".equals(channel)) {accessibility.layout(payload);return;}
        if(!offer(() -> {
            long id=0;
            String resultChannel="bridge".equals(channel) ? "bridge.result" : "launcher.result";
            try {
                JSONObject command=new JSONObject(payload);
                id=command.optLong("id",0);
                if("launcher".equals(channel)) launcherCommand(command);
                else if("bridge".equals(channel)) bridgeCommand(command);
                else result(command.optLong("id",0),Protocol.UNSUPPORTED,"unknown_channel");
            } catch(JSONException|IllegalArgumentException e) { emit(resultChannel,json("id",id,"status",Protocol.INVALID_ARGUMENT,"reason","invalid_command")); }
            catch(SecurityException e) { emit(resultChannel,json("id",id,"status",Protocol.ACCESS_DENIED,"reason","permission_denied")); }
            catch(Exception e) { emit(resultChannel,json("id",id,"status",Protocol.UNCERTAIN,"reason","operation_failed")); }
        })) {
            // Existing queued work publishes an explicit recovery event. It is
            // retained if JNI is also full; no automatic command replay occurs.
            queueCommandLoss.set(true);
            scheduleRecovery();
        }
    }
    private void launcherCommand(JSONObject command) throws Exception {
        String operation=command.getString("operation"); long id=command.optLong("id",0);
        switch(operation) {
            case "catalog": refreshCatalog(); break;
            case "widgets_snapshot": widgets.refresh();break;
            case "haptic": { String kind=command.optString("kind","tick"); main.post(() -> haptic(kind)); break; }
            case "system_bars": shellDark=command.optBoolean("dark",false); main.post(this::applyWindowChrome); break;
            case "hint_seen": {
                String hint=command.optString("hint","");
                if(!hint.isEmpty()&&hint.length()<32) hints.edit().putBoolean(hint,true).apply();
                break;
            }
            case "menu": placementMenu(command.getString("app"),command.optString("hosted_label",""),command.optBoolean("dark",false)); break;
            // A drag on the home page: the whole new order, or a drop on a dock slot.
            case "reorder": case "dock": {
                try {
                    if(operation.equals("dock")) placements().dock(command.getString("app"),command.getInt("slot"));
                    else {
                        JSONArray items=command.getJSONArray("order");ArrayList<String> next=new ArrayList<>();
                        for(int index=0;index<items.length();index++) next.add(items.getString(index));
                        placements().reorder(next);
                    }
                    publishPlacements();
                    result(id,Protocol.COMPLETED,"placed");
                } catch(IllegalArgumentException e) {result(id,Protocol.INVALID_ARGUMENT,"home_placement_limit_or_identity");}
                catch(Exception e) {
                    placements=null;
                    result(id,Protocol.UNCERTAIN,"home_placement_storage_unavailable");
                    try {publishPlacements();} catch(Exception ignored) {}
                }
                break;
            }
            case "home_menu": main.post(() -> {
                if(destroyed || activity.isFinishing()) return;
                dialog(command.optBoolean("dark",false)).setTitle("Home").setItems(new String[]{"Widgets","Wallpaper","System setup"},(dialog,which) -> {
                    if(which==0) widgets.show();
                    else if(which==2) dev.makepad.octosense.contracts.SystemSettings.open(activity,"access");
                    else try {activity.startActivity(new Intent(Intent.ACTION_SET_WALLPAPER));}
                    catch(android.content.ActivityNotFoundException e) {
                        new AlertDialog.Builder(activity).setMessage("No wallpaper picker is installed.").setPositiveButton("OK",null).show();
                    }
                }).setNegativeButton("Cancel",null).show();
            });break;
            case "launch": {
                String identity=command.getString("app");
                LauncherActivityInfo app=apps.get(identity);
                if(app==null) { result(id,Protocol.EXPIRED_HANDLE,"app_unavailable"); break; }
                if(!launcher.isActivityEnabled(app.getComponentName(),app.getUser())) { result(id,Protocol.PREREQUISITE_MISSING,"app_disabled_or_profile_locked"); break; }
                launcher.startMainActivity(app.getComponentName(),app.getUser(),null,null);
                result(id,Protocol.COMPLETED,"launch_dispatched"); break;
            }
            case "shortcut": {
                ShortcutInfo shortcut=shortcuts.get(command.getString("app"));
                if(shortcut==null) { result(id,Protocol.EXPIRED_HANDLE,"shortcut_unavailable"); break; }
                UserHandle user=shortcut.getUserHandle();
                if(!users.isUserUnlocked(user)||users.isQuietModeEnabled(user)) {
                    result(id,Protocol.PREREQUISITE_MISSING,"profile_locked");break;
                }
                // Catalog callbacks can race a tap. Resolve the exact pin on
                // this worker again before dispatching the launch.
                LauncherApps.ShortcutQuery query=new LauncherApps.ShortcutQuery().setPackage(shortcut.getPackage())
                        .setShortcutIds(java.util.Collections.singletonList(shortcut.getId()))
                        .setQueryFlags(LauncherApps.ShortcutQuery.FLAG_MATCH_PINNED);
                try {
                    java.util.List<ShortcutInfo> current=launcher.getShortcuts(query,user);
                    if(current==null||current.isEmpty()) {result(id,Protocol.EXPIRED_HANDLE,"shortcut_unavailable");refreshCatalog();break;}
                    shortcut=current.get(0);
                    if(!shortcut.isEnabled()) {result(id,Protocol.PREREQUISITE_MISSING,"shortcut_disabled");refreshCatalog();break;}
                    launcher.startShortcut(shortcut,null,null);result(id,Protocol.COMPLETED,"launch_dispatched");
                } catch(android.content.ActivityNotFoundException error) {
                    result(id,Protocol.EXPIRED_HANDLE,"shortcut_unavailable");refreshCatalog();
                } catch(IllegalStateException error) {
                    result(id,Protocol.PREREQUISITE_MISSING,"profile_locked");refreshCatalog();
                }
                break;
            }
            case "settings": case "bridge_settings": case "system_settings": {
                String destination=operation.equals("settings") ? "home" : operation.equals("bridge_settings") ? "access" : command.getString("destination");
                main.post(() -> {
                    if(destroyed || activity.isFinishing()) return;
                    boolean opened=dev.makepad.octosense.contracts.SystemSettings.open(activity,destination);
                    result(id,opened ? Protocol.COMPLETED : Protocol.UNSUPPORTED,opened ? "settings_opened" : "setting_unavailable");
                }); break;
            }
            default: result(id,Protocol.UNSUPPORTED,"unknown_launcher_operation");
        }
    }
    private void refreshCatalog() {
        homeGeometry.catalogChanged();
        if(!catalogQueued.compareAndSet(false,true)) return;
        if(!offer(() -> {
            catalogQueued.set(false);
            try { loadCatalog(); }
            catch(Exception e) { result(0,Protocol.ACCESS_DENIED,"catalog_access_unavailable"); }
        })) catalogQueued.set(false);
    }
    private static String appId(ComponentName component,long user) { return "android:"+user+":"+component.flattenToString(); }
    private static String shortcutDisabledMessage(ShortcutInfo shortcut) {
        if(shortcut.isEnabled()) return "";
        CharSequence message=shortcut.getDisabledMessage();
        String text=message==null?"":message.toString().trim();
        if(text.isEmpty()) return "This shortcut was disabled by its app.";
        return text.codePointCount(0,text.length())>512?text.substring(0,text.offsetByCodePoints(0,512)):text;
    }
    private void loadCatalog() throws Exception {
        notificationIdentity.invalidate();
        long geometryToken=homeGeometry.catalogToken();
        ArrayList<JSONObject> models=new ArrayList<>(); Map<String,LauncherActivityInfo> fresh=new HashMap<>();
        Map<String,ShortcutInfo> freshShortcuts=new HashMap<>(); HashSet<String> iconFiles=new HashSet<>();
        for(UserHandle profile:launcher.getProfiles()) {
            long user=users.getSerialNumberForUser(profile);
            boolean unlocked=users.isUserUnlocked(profile)&&!users.isQuietModeEnabled(profile);
            for(LauncherActivityInfo app:launcher.getActivityList(null,profile)) {
                if(Protocol.HOME_PACKAGE.equals(app.getComponentName().getPackageName())) continue;
                String id=appId(app.getComponentName(),user); fresh.put(id,app);
                String icon=cacheIcon(app,id); if(!icon.isEmpty()) iconFiles.add(icon);
                boolean suspended=(app.getApplicationInfo().flags & android.content.pm.ApplicationInfo.FLAG_SUSPENDED)!=0;
                models.add(json("id",id,"label",app.getLabel().toString(),"component",app.getComponentName().flattenToString(),
                        "user",user,"icon",icon,"locked",!unlocked,"suspended",suspended,"shortcut",false));
            }
            if(launcher.hasShortcutHostPermission()) {
                LauncherApps.ShortcutQuery query=new LauncherApps.ShortcutQuery().setQueryFlags(
                        LauncherApps.ShortcutQuery.FLAG_MATCH_PINNED);
                java.util.List<ShortcutInfo> pinned=launcher.getShortcuts(query,profile);
                if(pinned!=null) for(ShortcutInfo shortcut:pinned) {
                    String id="android-shortcut:"+user+":"+shortcut.getPackage()+":"+shortcut.getId();
                    freshShortcuts.put(id,shortcut);
                    String icon=cacheShortcutIcon(shortcut,id);if(!icon.isEmpty()) iconFiles.add(icon);
                    models.add(json("id",id,"label",String.valueOf(shortcut.getShortLabel()),"component",shortcut.getPackage(),
                            "user",user,"icon",icon,"locked",!unlocked,"suspended",!shortcut.isEnabled(),"shortcut",true,
                            "disabled_message",shortcutDisabledMessage(shortcut)));
                }
            }
        }
        models.sort(Comparator.comparing(value -> value.optString("label").toLowerCase(java.util.Locale.ROOT)));
        apps.clear(); apps.putAll(fresh); shortcuts.clear(); shortcuts.putAll(freshShortcuts);
        launcherFixtureAvailable=fresh.values().stream().anyMatch(app -> app.getComponentName().getClassName()
                .equals("dev.makepad.octosense.bridge.validation.LauncherFixtureActivity"));
        try {placements=null;publishPlacements();}
        catch(Exception e) {result(0,Protocol.UNCERTAIN,"home_placement_storage_unavailable");}
        long revision=++catalogRevision;
        outbound.keySet().removeIf(key -> key.startsWith("launcher.catalog:"));
        int chunks=Math.max(1,(models.size()+63)/64);
        for(int chunk=0;chunk<chunks;chunk++) {
            JSONArray entries=new JSONArray();
            for(int i=chunk*64;i<Math.min(models.size(),(chunk+1)*64);i++) entries.put(models.get(i));
            emit("launcher.catalog",json("epoch",session,"revision",revision,"chunk",chunk,"chunks",chunks,"apps",entries));
        }
        homeGeometry.catalogPublished(revision,geometryToken);
        File[] old=new File(activity.getCacheDir(),"launcher-icons").listFiles();
        if(old!=null) for(File file:old) if(!iconFiles.contains(file.getAbsolutePath())) file.delete();
        if(lastBridgeSnapshot!=null) publishBridgeSnapshot(bridgeEpoch,bridgeRevision,lastBridgeSnapshot);
    }
    private LauncherPlacements placements() throws Exception {
        if(placements==null) placements=new LauncherPlacements(placementFile);
        return placements;
    }
    private void publishPlacements() throws Exception {
        JSONObject model=placements().snapshot();
        model.put("epoch",session).put("revision",++placementRevision);
        placementSnapshot=model;
        emit("launcher.placements",model);
    }
    private void placementMenu(String identity,String hostedLabel,boolean dark) throws Exception {
        // Hosted labels come from the in-process Rust launchable-app catalog.
        // Android component/profile identities still resolve through LauncherApps.
        boolean hosted=LauncherPlacements.isHosted(identity)&&!hostedLabel.isEmpty()&&hostedLabel.length()<=256;
        if(LauncherPlacements.isHosted(identity)&&!hosted) {result(0,Protocol.EXPIRED_HANDLE,"app_unavailable");return;}
        LauncherActivityInfo app=apps.get(identity);ShortcutInfo shortcut=shortcuts.get(identity);
        LauncherPlacements store=placements();
        if(!hosted && app==null && shortcut==null && !store.isPlaced(identity)) {result(0,Protocol.EXPIRED_HANDLE,"app_unavailable");return;}
        String label=hosted?hostedLabel:app!=null?app.getLabel().toString():shortcut!=null?String.valueOf(shortcut.getShortLabel()):"Unavailable app";
        boolean favorite=store.isFavorite(identity);boolean docked=store.isDocked(identity);
        ArrayList<String> labels=new ArrayList<>();ArrayList<Integer> actions=new ArrayList<>();
        if(favorite || hosted || app!=null || shortcut!=null) {labels.add(favorite?"Remove from Home":"Add to Home");actions.add(favorite?-1:-2);}
        if(hosted || app!=null || shortcut!=null) for(int slot=0;slot<4;slot++) {labels.add("Place in dock position "+(slot+1));actions.add(slot);}
        if(docked) {labels.add("Remove from dock");actions.add(-3);}
        // An installed Android app also offers what its own launcher would.
        boolean androidApp=!hosted && app!=null;
        boolean removable=androidApp && (app.getApplicationInfo().flags & android.content.pm.ApplicationInfo.FLAG_SYSTEM)==0;
        if(androidApp) {labels.add("App info");actions.add(-4);}
        if(removable) {labels.add("Uninstall");actions.add(-5);}
        main.post(() -> {
            if(destroyed || !resumed || activity.isFinishing()) return;
            closePlacementMenu();
            placementDialog=dialog(dark).setTitle(label).setItems(labels.toArray(new String[0]),(dialog,which) -> {
                int action=actions.get(which);
                if(action==-4) {
                    try {launcher.startAppDetailsActivity(app.getComponentName(),app.getUser(),null,null);}
                    catch(Exception e) {result(0,Protocol.UNSUPPORTED,"setting_unavailable");}
                    return;
                }
                if(action==-5) {
                    try {
                        Intent uninstall=new Intent(Intent.ACTION_DELETE,android.net.Uri.parse("package:"+app.getComponentName().getPackageName()));
                        uninstall.putExtra(Intent.EXTRA_USER,app.getUser());
                        activity.startActivity(uninstall);
                    } catch(Exception e) {result(0,Protocol.UNSUPPORTED,"operation_failed");}
                    return;
                }
                if(!offer(() -> {
                    try {
                        if(action>=0 || action==-2) {
                            if(!hosted && !apps.containsKey(identity) && !shortcuts.containsKey(identity)) {result(0,Protocol.EXPIRED_HANDLE,"app_unavailable");return;}
                        }
                        if(action>=0) placements().dock(identity,action);
                        else if(action==-3) placements().undock(identity);
                        else placements().favorite(identity,action==-2);
                        publishPlacements();
                    } catch(IllegalArgumentException e) {result(0,Protocol.INVALID_ARGUMENT,"home_placement_limit_or_identity");}
                    catch(Exception e) {
                        placements=null;
                        result(0,Protocol.UNCERTAIN,"home_placement_storage_unavailable");
                        try {publishPlacements();} catch(Exception ignored) {}
                    }
                })) {queueCommandLoss.set(true);scheduleRecovery();}
            }).setNegativeButton("Cancel",null).show();
        });
    }
    private String cacheIcon(LauncherActivityInfo app,String identity) {
        try {
            int density=activity.getResources().getDisplayMetrics().densityDpi;
            long changed=activity.getPackageManager().getPackageInfo(app.getComponentName().getPackageName(),0).lastUpdateTime;
            return cacheIcon(app.getBadgedIcon(density),identity,changed);
        } catch(Exception e) {return "";}
    }
    private String cacheShortcutIcon(ShortcutInfo shortcut,String identity) {
        try {
            int density=activity.getResources().getDisplayMetrics().densityDpi;
            long changed=activity.getPackageManager().getPackageInfo(shortcut.getPackage(),0).lastUpdateTime;
            return cacheIcon(launcher.getShortcutBadgedIconDrawable(shortcut,density),identity+":"+changed,shortcut.getLastChangedTimestamp());
        } catch(Exception e) {return "";}
    }
    private String cacheIcon(Drawable icon,String identity,long changed) {
        if(icon==null) return "";
        try {
            int density=activity.getResources().getDisplayMetrics().densityDpi;
            byte[] digest=MessageDigest.getInstance("SHA-256").digest((identity+":"+changed+":"+density).getBytes(StandardCharsets.UTF_8));
            StringBuilder key=new StringBuilder(); for(byte value:digest) key.append(String.format("%02x",value & 255));
            File directory=new File(activity.getCacheDir(),"launcher-icons"); directory.mkdirs();
            File file=new File(directory,key+".png");
            if(!file.exists()) {
                int size=Math.min(192,Math.max(48,Math.round(60*activity.getResources().getDisplayMetrics().density)));
                Bitmap bitmap=Bitmap.createBitmap(size,size,Bitmap.Config.ARGB_8888);
                icon.setBounds(0,0,size,size); icon.draw(new Canvas(bitmap));
                File partial=new File(directory,key+".partial");
                try(FileOutputStream output=new FileOutputStream(partial)) { bitmap.compress(Bitmap.CompressFormat.PNG,100,output); }
                bitmap.recycle();
                if(!partial.renameTo(file)) { partial.delete(); return ""; }
            }
            return file.getAbsolutePath();
        } catch(Exception e) { return ""; }
    }
    private void bridgePackageChanged(String name,UserHandle user) {
        if(!Protocol.BRIDGE_PACKAGE.equals(name)||!android.os.Process.myUserHandle().equals(user)) return;
        main.post(() -> {
            if(destroyed) return;
            // A failed bind while the service is disabled has no connection
            // for Android to restart. A package/component change is the event
            // that permits a fresh authenticated bind, without command replay.
            reconnectAttempt=0;
            reconnect();
        });
    }
    private final LauncherApps.Callback packageCallback=new LauncherApps.Callback() {
        @Override public void onPackageRemoved(String p,UserHandle u) { refreshCatalog();bridgePackageChanged(p,u); }
        @Override public void onPackageAdded(String p,UserHandle u) { refreshCatalog();bridgePackageChanged(p,u); }
        @Override public void onPackageChanged(String p,UserHandle u) { refreshCatalog();bridgePackageChanged(p,u); }
        @Override public void onPackagesAvailable(String[] p,UserHandle u,boolean replacing) { refreshCatalog();for(String name:p) bridgePackageChanged(name,u); }
        @Override public void onPackagesUnavailable(String[] p,UserHandle u,boolean replacing) { refreshCatalog();for(String name:p) bridgePackageChanged(name,u); }
        @Override public void onPackagesSuspended(String[] p,UserHandle u) { refreshCatalog(); }
        @Override public void onPackagesUnsuspended(String[] p,UserHandle u) { refreshCatalog(); }
        @Override public void onShortcutsChanged(String p,java.util.List<ShortcutInfo> s,UserHandle u) { refreshCatalog(); }
    };
    private final BroadcastReceiver profileCallback=new BroadcastReceiver() {
        @Override public void onReceive(Context context,Intent intent) {refreshCatalog();widgets.refresh();}
    };
    private static JSONObject bundleJson(Bundle bundle) throws JSONException {
        JSONObject result=new JSONObject();
        for(String key:bundle.keySet()) {
            Object value=bundle.get(key);
            if(value instanceof Bundle) result.put(key,bundleJson((Bundle)value));
            else if(value instanceof ArrayList) {
                JSONArray list=new JSONArray();
                for(Object item:(ArrayList<?>)value) if(item instanceof Bundle) list.put(bundleJson((Bundle)item));
                result.put(key,list);
            } else if(value instanceof String || value instanceof Boolean || value instanceof Number) result.put(key,value);
        }
        return result;
    }
    private void bridgeState(String state,String reason) {
        observedBridgeState=state;
        if(!"connected".equals(state)) {
            lastBridgeSnapshot=null;
            replyTargets.clear();replyCommands.clear();main.post(replyComposer::disconnected);
        }
        emit("bridge.connection",json("state",state,"reason",reason));
    }
    private void bindBridge() {
        // bindService runs on Android's activity thread; handshakes never do.
        main.post(() -> {
            // The opt-in protocol fixture owns this UID's single subscription.
            if(dev.makepad.octosense.validation.BridgeInstrumentation.notificationRoundtripTest) return;
            if(destroyed || bound || !resumed) return;
            try {
                PackageManager pm=activity.getPackageManager();
                if(pm.checkSignatures(activity.getPackageName(),Protocol.BRIDGE_PACKAGE)!=PackageManager.SIGNATURE_MATCH) {
                    offer(() -> bridgeState("unavailable","bridge_missing_or_certificate_mismatch")); return;
                }
                bound=activity.bindService(new Intent().setComponent(new ComponentName(Protocol.BRIDGE_PACKAGE,
                        Protocol.BRIDGE_PACKAGE+".SystemBridgeService")),bridgeConnection,android.content.Context.BIND_AUTO_CREATE);
                if(!bound) offer(() -> bridgeState("unavailable","bridge_bind_failed"));
            } catch(SecurityException e) { offer(() -> bridgeState("denied","bridge_bind_denied")); }
        });
    }
    private final ServiceConnection bridgeConnection=new ServiceConnection() {
        @Override public void onServiceConnected(ComponentName component,IBinder binder) {
            offer(() -> {
                try {
                    ISystemBridge candidate=ISystemBridge.Stub.asInterface(binder);
                    if(candidate.getProtocolInfo().getInt("major")!=Protocol.MAJOR) { bridge=null; bridgeState("incompatible","protocol_major_mismatch"); return; }
                    bridge=candidate; bridgeEpoch=""; bridgeRevision=-1;
                    candidate.subscribe(session,bridgeCallback);
                    reconnectAttempt=0; bridgeState("connected","");
                } catch(RemoteException|SecurityException e) { bridge=null; bridgeState("disconnected","bridge_handshake_failed"); }
            });
        }
        @Override public void onServiceDisconnected(ComponentName component) {
            offer(() -> { bridge=null; bridgeRevision=-1; bridgeState("disconnected","bridge_process_died"); });
        }
        @Override public void onBindingDied(ComponentName component) { reconnect(); }
        @Override public void onNullBinding(ComponentName component) { reconnect(); }
    };
    private void reconnect() {
        main.post(() -> {
            if(bound) { activity.unbindService(bridgeConnection); bound=false; }
            offer(() -> { bridge=null; bridgeState("disconnected","bridge_binding_died"); });
            if(resumed && !destroyed && reconnectAttempt<5) {
                int delay=250 << reconnectAttempt++; main.postDelayed(this::bindBridge,delay);
            }
        });
    }
    private final ISystemBridgeCallback.Stub bridgeCallback=new ISystemBridgeCallback.Stub() {
        @Override public void onSnapshot(String epoch,long revision,Bundle state) {
            Bundle retained=validationBuild?dev.makepad.octosense.validation.NotificationFlowFixture.filter(state):state;
            if(!offer(() -> {
                if(validationBuild&&dev.makepad.octosense.validation.NotificationUiFixture.active()) return;
                if(epoch.equals(bridgeEpoch) && revision<bridgeRevision) return;
                bridgeEpoch=epoch; bridgeRevision=revision;
                publishBridgeSnapshot(epoch,revision,retained);
            })) requestResync();
        }
        @Override public void onDelta(String epoch,long revision,Bundle patch) { requestResync(); }
        @Override public void onResyncRequired(String epoch) { requestResync(); }
        @Override public void onCommandResult(String s,long id,int status,String reason) {
            if(!session.equals(s)) return;
            if(!offer(() -> {
                String token=replyCommands.get(id);
                if(token!=null) {
                    if(status!=Protocol.ACCEPTED) replyCommands.remove(id);
                    main.post(() -> replyComposer.complete(token,status));
                }
                emit("bridge.result",json("id",id,"status",status,"reason",reason));
            })) {
                queueCommandLoss.set(true);
                scheduleRecovery();
            }
        }
    };
    private void publishBridgeSnapshot(String epoch,long revision,Bundle state) {
        lastBridgeSnapshot=state;
        updateReplyTargets(state);
        try {
            JSONObject model=bundleJson(notificationIdentity.decorate(state));
            JSONObject event=NotificationAppIdentity.fitSnapshot(json("epoch",epoch,"revision",revision,"state",model));
            observedNotificationPresentation=model.getJSONArray("notifications");
            emit("bridge.snapshot",event);
        }
        catch(JSONException e) {bridgeState("incompatible","snapshot_model_invalid");}
    }
    private void requestResync() { offer(() -> { try { if(bridge!=null) bridge.requestSnapshot(session); } catch(RemoteException e) { bridge=null; bridgeState("disconnected","snapshot_request_failed"); } }); }
    private void bridgeCommand(JSONObject command) throws Exception {
        long id=command.optLong("id",++commandId); Protocol.requireCommand(id);
        if(bridge==null) {
            if("reply_send".equals(command.optString("operation"))) {
                String token=command.optString("token");main.post(() -> replyComposer.complete(token,Protocol.DISCONNECTED));
            }
            emit("bridge.result",json("id",id,"status",Protocol.DISCONNECTED,"reason","bridge_unavailable")); return;
        }
        switch(command.getString("operation")) {
            case "wifi": bridge.setWifiEnabled(session,id,command.getBoolean("enabled")); break;
            case "bluetooth": bridge.setBluetoothEnabled(session,id,command.getBoolean("enabled")); break;
            case "torch": bridge.setTorchEnabled(session,id,command.getBoolean("enabled")); break;
            case "rotation": bridge.setRotationLocked(session,id,command.getBoolean("enabled")); break;
            case "brightness": bridge.setBrightness(session,id,(float)command.getDouble("value"),command.optBoolean("automatic",false)); break;
            case "volume": bridge.setVolume(session,id,(float)command.getDouble("value")); break;
            case "dnd": bridge.setInterruptionFilter(session,id,command.getBoolean("enabled")?2:1); break;
            case "battery_saver": bridge.setBatterySaver(session,id,command.getBoolean("enabled")); break;
            case "dismiss": bridge.dismissNotification(session,id,command.getString("handle")); break;
            case "dismiss_all": bridge.dismissAllNotifications(session,id); break;
            case "action": bridge.invokeNotificationAction(session,id,command.getString("handle"),command.has("reply")?command.getString("reply"):null); break;
            case "reply": {
                String handle=command.getString("handle");String[] target=replyTargets.get(handle);
                if(target==null) {emit("bridge.result",json("id",id,"status",Protocol.EXPIRED_HANDLE,"reason","notification_reply_expired"));break;}
                main.post(() -> {if(resumed&&!destroyed) {widgets.hide();replyComposer.open(handle,target[0],target[1]);}});break;
            }
            case "reply_send": {
                String token=command.getString("token"),handle=command.getString("handle"),text=command.getString("reply");
                if(!replyComposer.matchesSubmission(token,handle,text)) break;
                if(!replyTargets.containsKey(handle)) {main.post(() -> replyComposer.complete(token,Protocol.EXPIRED_HANDLE));break;}
                if(replyCommands.size()>=16) {main.post(() -> replyComposer.complete(token,Protocol.QUEUE_FULL));break;}
                replyCommands.put(id,token);
                try {bridge.invokeNotificationAction(session,id,handle,text);}
                catch(Exception e) {replyCommands.remove(id);main.post(() -> replyComposer.complete(token,Protocol.UNCERTAIN));throw e;}
                break;
            }
            case "snapshot": bridge.requestSnapshot(session); break;
            default: emit("bridge.result",json("id",id,"status",Protocol.UNSUPPORTED,"reason","unknown_bridge_operation"));
        }
    }
    @Override public void onResume() {
        resumed=true; homeGeometry.onResume(); widgets.onResume(); refreshCatalog(); bindBridge(); requestResync();
        main.post(this::applyWindowChrome);
        offer(() -> {emitUiMode();emitHints();flushEvents();});
    }
    @Override public void onPause() { resumed=false;closePlacementMenu();replyComposer.close(); homeGeometry.onPause(); widgets.onPause(); }
    @Override public boolean onActivityResult(int request,int result,Intent data) {return widgets.onActivityResult(request,result,data);}
    @Override public boolean onBackPressed() {return replyComposer.close()||widgets.hide();}
    @Override public void onIntent(Intent intent) {
        // singleInstance Home can already exist when the shell explicitly
        // launches an owned validation activity. Enable its test host here too.
        if(validationBuild&&validationRemote==null&&intent!=null&&intent.getBooleanExtra("--remote",false)) {
            if(intent.getBooleanExtra("octosense.placement_test",false)) offer(() -> {
                placementFile=new File(activity.getCacheDir(),"home-geometry-placements.json");placements=null;
                try {publishPlacements();} catch(Exception e) {result(0,Protocol.UNCERTAIN,"validation_placements_unavailable");}
            });
            if(intent.getBooleanExtra("octosense.widget_test",false)) {
                widgets.onDestroy();
                widgets=new NativeWidgets(activity,this::offer,reason -> result(0,Protocol.UNCERTAIN,reason),
                        model -> emit("launcher.widgets",model),0x4f4356,new File(activity.getCacheDir(),"widget-ui-validation.json"));
                widgets.setVisibilityListener(visible -> {widgetsVisible=visible;updateNativeCoverage();});
                if(resumed) widgets.onResume();
            }
            try {validationRemote=new dev.makepad.octosense.validation.ValidationRemote(activity,
                    () -> widgets.show(),() -> widgets.hide(),() -> widgets.validationState().put("home_integration",homeGeometry.validationState()).put("launcher",validationLauncherState()),this::validationWindow);}
            catch(Exception e) {android.util.Log.e("OctoSenseValidation","Remote startup failed",e);}
        }
        if(intent!=null && intent.hasCategory(Intent.CATEGORY_HOME)) {replyComposer.close();widgets.hide();homeGeometry.invalidate();}
    }
    @Override public void onDestroy() {
        closePlacementMenu();
        replyComposer.close();
        homeGeometry.onDestroy();
        widgets.onDestroy();
        if(validationRemote!=null) try {validationRemote.close();} catch(java.io.IOException ignored) {}
        launcher.unregisterCallback(packageCallback);
        activity.unregisterReceiver(profileCallback);
        if(bound) { activity.unbindService(bridgeConnection); bound=false; }
        // The service removes subscriptions on final unbind, even if this
        // activity is recreated while the Home process and Binder remain alive.
        destroyed=true; main.removeCallbacksAndMessages(null); worker.shutdownNow();
    }
}
