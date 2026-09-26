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
import java.util.concurrent.CountDownLatch;
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
    private dev.makepad.octosense.agent.AgentPlatformClient agent;
    private WifiSettingsClient wifiSettings;
    private BluetoothSettingsClient bluetoothSettings;
    private AccountsSettingsClient accountsSettings;
    private UpdatesSettingsClient updatesSettings;
    private NetworkSettingsClient networkSettings;
    private DisplaySettingsClient displaySettings;
    private SoundsSettingsClient soundsSettings;
    private AppNotificationsSettingsClient appNotificationsSettings;
    private RolesSettingsClient rolesSettings;
    private PermissionsSettingsClient permissionsSettings;
    private DndSettingsClient dndSettings;
    private AppNetworkSettingsClient appNetworkSettings;
    private AppBatterySettingsClient appBatterySettings;
    private AppStorageSettingsClient appStorageSettings;
    private AppLanguageSettingsClient appLanguageSettings;
    private volatile SystemLanguageSettingsClient systemLanguageSettings;
    private volatile KeyboardSettingsClient keyboardSettings;
    private volatile CaptionCustomSettingsClient captionCustomSettings;
    private volatile CaptionLanguageSettingsClient captionLanguageSettings;
    private AndroidAccessibilityPreferences accessibilityPreferences;
    private final CountDownLatch agentBinding=new CountDownLatch(1);
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
    private volatile boolean resumed;
    private volatile boolean windowFocused;
    private final android.view.ViewTreeObserver.OnWindowFocusChangeListener settingsFocusListener;
    private int reconnectAttempt;
    /** The shell's appearance, for the system-bar icons and the native dialogs. */
    private volatile boolean shellDark;
    /** Which first-use hints the person has already found (mobile_hints.rs). */
    private final SharedPreferences hints;
    /** The shell's hit regions as accessibility nodes (ShellAccessibility.java). */
    private final ShellAccessibility accessibility;
    private final SettingsAccessibility settingsAccessibility;
    private static final java.util.concurrent.atomic.AtomicLong NEXT_SETTINGS_ENTRY=new java.util.concurrent.atomic.AtomicLong(1);
    private final SettingsEntryContract.Delivery settingsEntryDelivery=new SettingsEntryContract.Delivery();
    private final LinkedHashMap<String,String[]> outbound=new LinkedHashMap<>();
    private boolean flushScheduled;
    private boolean resyncNeeded;
    private boolean commandOutcomeUncertain;

    public MakepadAppExtension(MakepadActivity activity) {
        this.activity=activity;
        windowFocused=activity.hasWindowFocus();
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
        settingsAccessibility=new SettingsAccessibility(activity,
                request -> offer(() -> emit("settings.a11y.action",request)),
                active -> accessibility.setImportantForAccessibility(active
                        ?android.view.View.IMPORTANT_FOR_ACCESSIBILITY_NO_HIDE_DESCENDANTS
                        :android.view.View.IMPORTANT_FOR_ACCESSIBILITY_YES),
                enabled -> offer(() -> emit("settings.a11y.enabled",json("schema",1,"enabled",enabled))));
        // The ROM's agent platform: real tasks for Recents when present, nothing lost when absent.
        agent=new dev.makepad.octosense.agent.AgentPlatformClient(activity,state -> {
            agentBinding.countDown();
            emit("agent.connection",json("state",state));
            if(state.equals("connected")) offer(this::publishRecentApps);
        });
        activity.getApplicationOverlay().addView(accessibility,new android.widget.FrameLayout.LayoutParams(
                android.view.ViewGroup.LayoutParams.MATCH_PARENT,android.view.ViewGroup.LayoutParams.MATCH_PARENT));
        activity.getApplicationOverlay().addView(settingsAccessibility,new android.widget.FrameLayout.LayoutParams(
                android.view.ViewGroup.LayoutParams.MATCH_PARENT,android.view.ViewGroup.LayoutParams.MATCH_PARENT));
        activity.registerComponentCallbacks(new android.content.ComponentCallbacks() {
            @Override public void onConfigurationChanged(Configuration configuration) { offer(MakepadAppExtension.this::emitUiMode); }
            @Override public void onLowMemory() {}
        });
        accessibilityPreferences=new AndroidAccessibilityPreferences(activity,main,()->offer(this::emitUiMode));
        settingsAccessibility.setWindowFocused(windowFocused);
        settingsFocusListener=focused -> {
            windowFocused=focused;
            if(destroyed)return;
            // Retire virtual actions immediately on the UI thread. Preserve
            // this edge in the worker packet even if focus changes again.
            settingsAccessibility.setWindowFocused(focused);
            if(!focused&&captionCustomSettings!=null)captionCustomSettings.retireInBackground();
            if(!focused)offer(()->{if(captionLanguageSettings!=null)captionLanguageSettings.invalidate();if(systemLanguageSettings!=null)systemLanguageSettings.invalidate();if(keyboardSettings!=null)keyboardSettings.invalidate();});
            offer(() -> emitUiMode(focused));
        };
        activity.getWindow().getDecorView().getViewTreeObserver().addOnWindowFocusChangeListener(settingsFocusListener);
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
    private boolean openUsageAccess() {
        try {
            Intent intent=new Intent(Settings.ACTION_USAGE_ACCESS_SETTINGS);
            intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            activity.startActivity(intent);
            return true;
        } catch(Exception e) { return false; }
    }
    private void emitUiMode() { emitUiMode(windowFocused); }
    private void emitUiMode(boolean focused) {
        Configuration configuration=activity.getResources().getConfiguration();
        boolean night=(configuration.uiMode&Configuration.UI_MODE_NIGHT_MASK)==Configuration.UI_MODE_NIGHT_YES;
        boolean reduceMotion=Settings.Global.getFloat(activity.getContentResolver(),Settings.Global.ANIMATOR_DURATION_SCALE,1f)==0f;
        emit("launcher.ui_mode",json("dark",night,"font_scale_percent",Math.round(configuration.fontScale*100f),"reduce_motion",reduceMotion,"activity_resumed",resumed,"activity_focused",focused,
                "accessibility_preferences",accessibilityPreferences==null?null:accessibilityPreferences.snapshot(configuration),
                "accessibility_enabled",settingsAccessibility!=null&&settingsAccessibility.enabledNow(),
                "theme",ThemeCatalog.get(activity).read(activity).json()));
    }
    /** Recently used Android apps for the shell's Recents, newest first, when usage access is granted. */
    @SuppressWarnings("deprecation")
    private void publishRecentApps() {
        boolean granted=false;
        ArrayList<String> ids=new ArrayList<>();
        if(agent!=null && agent.has("tasks")) {
            // The agent platform lists the real task stack, no usage access needed.
            for(dev.makepad.octosense.agent.AgentPlatformClient.Task task:agent.recentTasks(8)) {
                String found=null;
                for(Map.Entry<String,LauncherActivityInfo> app:apps.entrySet()) {
                    if(!app.getValue().getComponentName().getPackageName().equals(task.pkg)) continue;
                    if(found==null || app.getValue().getUser().equals(android.os.Process.myUserHandle())) found=app.getKey();
                }
                if(found!=null && !ids.contains(found)) ids.add(found);
            }
            emit("launcher.recent_apps",json("granted",true,"apps",new JSONArray(ids)));
            return;
        }
        try {
            android.app.AppOpsManager ops=(android.app.AppOpsManager)activity.getSystemService(Context.APP_OPS_SERVICE);
            // checkOpNoThrow, not unsafeCheckOpNoThrow: the latter arrived in
            // API 29, and calling it on Android 9 is a NoSuchMethodError that
            // the catch below does not see and that took the shell down at
            // its first resume. The two answer the same question.
            int mode=ops.checkOpNoThrow(android.app.AppOpsManager.OPSTR_GET_USAGE_STATS,android.os.Process.myUid(),activity.getPackageName());
            granted=mode==android.app.AppOpsManager.MODE_ALLOWED || (mode==android.app.AppOpsManager.MODE_DEFAULT
                    && activity.checkSelfPermission(android.Manifest.permission.PACKAGE_USAGE_STATS)==PackageManager.PERMISSION_GRANTED);
            if(granted) {
                android.app.usage.UsageStatsManager usage=(android.app.usage.UsageStatsManager)activity.getSystemService(Context.USAGE_STATS_SERVICE);
                long now=System.currentTimeMillis();
                android.app.usage.UsageEvents events=usage.queryEvents(now-24L*3600_000L,now);
                android.app.usage.UsageEvents.Event event=new android.app.usage.UsageEvents.Event();
                HashMap<String,Long> latest=new HashMap<>();
                while(events.getNextEvent(event)) {
                    int type=event.getEventType();
                    if(type==android.app.usage.UsageEvents.Event.ACTIVITY_RESUMED || type==android.app.usage.UsageEvents.Event.MOVE_TO_FOREGROUND)
                        latest.put(event.getPackageName(),event.getTimeStamp());
                }
                ArrayList<Map.Entry<String,Long>> order=new ArrayList<>(latest.entrySet());
                order.sort((a,b) -> Long.compare(b.getValue(),a.getValue()));
                long me=users.getSerialNumberForUser(android.os.Process.myUserHandle());
                for(Map.Entry<String,Long> entry:order) {
                    String pkg=entry.getKey();
                    if(pkg.equals(activity.getPackageName())) continue;
                    String found=null;
                    for(Map.Entry<String,LauncherActivityInfo> app:apps.entrySet()) {
                        if(!app.getValue().getComponentName().getPackageName().equals(pkg)) continue;
                        if(found==null || app.getValue().getUser().equals(android.os.Process.myUserHandle())) found=app.getKey();
                    }
                    if(found!=null && !ids.contains(found)) ids.add(found);
                    if(ids.size()>=8) break;
                }
                if(me<0) ids.clear();
            }
        } catch(Exception e) { ids.clear(); }
        emit("launcher.recent_apps",json("granted",granted,"apps",new JSONArray(ids)));
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
        if("settings.a11y.layout".equals(channel)) {
            if(resumed&&!destroyed)settingsAccessibility.layout(payload);else settingsAccessibility.clear();
            return;
        }
        if("settings.a11y.result".equals(channel)) {settingsAccessibility.result(payload);return;}
        if("settings.entry.ready".equals(channel)||"settings.entry.received".equals(channel)) {
            offer(() -> {
                try {
                    JSONObject packet=new JSONObject(payload);Object schema=packet.get("schema");
                    if(!(schema instanceof Integer)||((Integer)schema)!=1)return;
                    if("settings.entry.ready".equals(channel)) {
                        boolean first=!settingsEntryDelivery.isReady();
                        SettingsEntryContract.Entry entry=settingsEntryDelivery.ready();
                        // Bootstrap can also discard the first Activity and
                        // accessibility observations. Reconcile once, after
                        // native readiness; repeating it would create a
                        // ui_mode -> ready feedback loop.
                        if(first)emitUiMode();
                        sendSettingsEntry(entry);
                    }
                    else {
                        Object id=packet.get("id");
                        if(id instanceof Integer||id instanceof Long)settingsEntryDelivery.received(((Number)id).longValue());
                    }
                }catch(JSONException malformed) { /* The retained entry waits for a valid receipt. */ }
            });
            return;
        }
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
    private synchronized AccountsSettingsClient accountsSettings() {
        if(destroyed) throw new IllegalStateException("Activity destroyed");
        if(accountsSettings==null) accountsSettings=new AccountsSettingsClient(activity,agent);return accountsSettings;
    }
    private synchronized UpdatesSettingsClient updatesSettings() {
        if(destroyed) throw new IllegalStateException("Activity destroyed");
        if(updatesSettings==null) updatesSettings=new UpdatesSettingsClient(activity,agent);return updatesSettings;
    }
    private synchronized DisplaySettingsClient displaySettings() {
        if(destroyed)throw new IllegalStateException("Activity destroyed");
        if(displaySettings==null)displaySettings=new DisplaySettingsClient(activity,agent);return displaySettings;
    }
    private synchronized NetworkSettingsClient networkSettings() {
        if(destroyed) throw new IllegalStateException("Activity destroyed");
        if(networkSettings==null) networkSettings=new NetworkSettingsClient(activity,agent);return networkSettings;
    }
    private synchronized AppNotificationsSettingsClient appNotificationsSettings() {
        if(destroyed)throw new IllegalStateException("Activity destroyed");
        if(appNotificationsSettings==null)appNotificationsSettings=new AppNotificationsSettingsClient(agent,() -> resumed&&!destroyed&&!activity.isFinishing()&&activity.hasWindowFocus());
        return appNotificationsSettings;
    }
    private synchronized RolesSettingsClient rolesSettings(){
        if(destroyed)throw new IllegalStateException("Activity destroyed");
        if(rolesSettings==null)rolesSettings=new RolesSettingsClient(agent,() -> resumed&&!destroyed&&!activity.isFinishing()&&activity.hasWindowFocus());
        return rolesSettings;
    }
    private synchronized PermissionsSettingsClient permissionsSettings(){
        if(agent==null)throw new IllegalStateException("Agent client not initialized");
        if(permissionsSettings==null)permissionsSettings=new PermissionsSettingsClient(agent,() -> resumed&&!destroyed&&!activity.isFinishing()&&activity.hasWindowFocus());
        return permissionsSettings;
    }
    private synchronized CaptionLanguageSettingsClient captionLanguageSettings(){
        if(agent==null)throw new IllegalStateException("Agent client unavailable");
        if(captionLanguageSettings==null)captionLanguageSettings=new CaptionLanguageSettingsClient(new CaptionLanguageSettingsClient.Bridge(){
            @Override public JSONObject snapshot(long id,String key,String query,int offset)throws Exception{return agent.captionLanguageSnapshot(id,key,query,offset);}
            @Override public String select(String key,String choice)throws Exception{return agent.captionLanguageSet(key,choice);}
        },()->resumed&&!destroyed&&!activity.isFinishing()&&activity.hasWindowFocus());
        return captionLanguageSettings;
    }
    private synchronized CaptionCustomSettingsClient captionCustomSettings(){
        if(agent==null)throw new IllegalStateException("Agent client not initialized");
        if(captionCustomSettings==null)captionCustomSettings=new CaptionCustomSettingsClient(agent,()->resumed&&!destroyed&&!activity.isFinishing()&&activity.hasWindowFocus());
        return captionCustomSettings;
    }
    private static long captionVisit(Object value){if(!(value instanceof Integer||value instanceof Long)||((Number)value).longValue()<=0)throw new IllegalArgumentException("Invalid caption visit");return ((Number)value).longValue();}
    private synchronized KeyboardSettingsClient keyboardSettings(){
        if(agent==null)throw new IllegalStateException("Agent client unavailable");
        if(keyboardSettings==null)keyboardSettings=new KeyboardSettingsClient(new KeyboardSettingsClient.Bridge(){
            @Override public JSONObject snapshot(long id,String query,int offset)throws Exception{return agent.keyboardsSnapshot(id,query,offset);}
            @Override public android.app.PendingIntent prepare(long id,String key,String target,String operation)throws Exception{return agent.keyboardFlow(id,key,target,operation);}
        },()->resumed&&!destroyed&&!activity.isFinishing()&&activity.hasWindowFocus());
        return keyboardSettings;
    }
    private synchronized SystemLanguageSettingsClient systemLanguageSettings(){
        if(agent==null)throw new IllegalStateException("Agent client unavailable");
        if(systemLanguageSettings==null)systemLanguageSettings=new SystemLanguageSettingsClient(new SystemLanguageSettingsClient.Bridge(){
            @Override public JSONObject snapshot(long id,String key,String parent,String query,int offset)throws Exception{return agent.systemLanguagesSnapshot(id,key,parent,query,offset);}
            @Override public String apply(String key,String[] targets)throws Exception{return agent.systemLanguagesApply(key,targets);}
        },()->resumed&&!destroyed&&!activity.isFinishing()&&activity.hasWindowFocus());
        return systemLanguageSettings;
    }
    private synchronized AppLanguageSettingsClient appLanguageSettings(){
        if(agent==null)throw new IllegalStateException("Agent client not initialized");
        if(appLanguageSettings==null)appLanguageSettings=new AppLanguageSettingsClient(agent,()->resumed&&!destroyed&&!activity.isFinishing()&&activity.hasWindowFocus());
        return appLanguageSettings;
    }
    private synchronized AppStorageSettingsClient appStorageSettings(){
        if(agent==null)throw new IllegalStateException("Agent client not initialized");
        if(appStorageSettings==null)appStorageSettings=new AppStorageSettingsClient(agent,()->resumed&&!destroyed&&!activity.isFinishing()&&activity.hasWindowFocus());
        return appStorageSettings;
    }
    private synchronized AppBatterySettingsClient appBatterySettings(){
        if(agent==null)throw new IllegalStateException("Agent client not initialized");
        if(appBatterySettings==null)appBatterySettings=new AppBatterySettingsClient(agent,()->resumed&&!destroyed&&!activity.isFinishing()&&activity.hasWindowFocus());
        return appBatterySettings;
    }
    private synchronized AppNetworkSettingsClient appNetworkSettings(){
        if(agent==null)throw new IllegalStateException("Agent client not initialized");
        if(appNetworkSettings==null)appNetworkSettings=new AppNetworkSettingsClient(agent,()->resumed&&!destroyed&&!activity.isFinishing()&&activity.hasWindowFocus());
        return appNetworkSettings;
    }
    private synchronized DndSettingsClient dndSettings(){
        if(agent==null)throw new IllegalStateException("Agent client not initialized");
        if(dndSettings==null)dndSettings=new DndSettingsClient(agent,()->resumed&&!destroyed&&!activity.isFinishing()&&activity.hasWindowFocus());
        return dndSettings;
    }
    private static int dndInt(Object value,int min,int max){
        if(!(value instanceof Integer||value instanceof Long))throw new IllegalArgumentException("DND integer required");
        long number=((Number)value).longValue();if(number<min||number>max)throw new IllegalArgumentException("DND value out of range");return (int)number;
    }
    private static boolean dndBool(Object value){if(!(value instanceof Boolean))throw new IllegalArgumentException("DND boolean required");return (Boolean)value;}
    private static String dndOptionalString(Object value){if(value==null||value==JSONObject.NULL)return null;if(!(value instanceof String))throw new IllegalArgumentException("DND string required");return (String)value;}
    private void dndResult(long id,String reason){result(id,reason.equals("dnd_applied")||reason.equals("dnd_requested")?Protocol.COMPLETED:Protocol.UNCERTAIN,reason);}
    private synchronized SoundsSettingsClient soundsSettings() {
        if(destroyed)throw new IllegalStateException("Activity destroyed");
        if(soundsSettings==null)soundsSettings=new SoundsSettingsClient(activity,agent,() -> resumed&&!destroyed&&!activity.isFinishing()&&activity.hasWindowFocus());
        return soundsSettings;
    }
    private void soundResult(long id,String reason) {
        boolean accepted=reason.equals("sound_applied")||reason.equals("sound_preview_started")||reason.equals("sound_preview_requested")||reason.equals("sound_silent")||reason.equals("sound_stopped");
        result(id,accepted?Protocol.COMPLETED:Protocol.UNCERTAIN,reason);
    }
    private void networkResult(long id,String reason) {result(id,reason.equals("network_applied")||reason.equals("network_requested")?Protocol.COMPLETED:Protocol.UNCERTAIN,reason);}
    private void updateResult(long id,String reason) {
        boolean accepted=reason.equals("update_check_requested")||reason.equals("update_install_requested")||reason.equals("update_reboot_requested");
        result(id,accepted?Protocol.COMPLETED:Protocol.UNCERTAIN,reason);
    }
    private void syncResult(long id,String reason) {result(id,reason.equals("sync_applied")||reason.equals("sync_requested")?Protocol.COMPLETED:Protocol.UNCERTAIN,reason);}
    private synchronized BluetoothSettingsClient bluetoothSettings() {
        if(destroyed) throw new IllegalStateException("Activity destroyed");
        if(bluetoothSettings==null) bluetoothSettings=new BluetoothSettingsClient(activity,agent);
        return bluetoothSettings;
    }
    private void bluetoothResult(long id,String reason) {
        boolean accepted=reason.equals("bluetooth_requested")||reason.equals("bluetooth_name_applied")||reason.equals("bluetooth_sharing_applied");
        result(id,accepted?Protocol.COMPLETED:Protocol.UNCERTAIN,reason);
    }
    private static String stringField(JSONObject command,String field) throws JSONException {
        Object value=command.get(field);if(!(value instanceof String)) throw new IllegalArgumentException("String required");return (String)value;
    }
    private synchronized WifiSettingsClient wifiSettings() {
        if(destroyed) throw new IllegalStateException("Activity destroyed");
        if(wifiSettings==null) wifiSettings=new WifiSettingsClient(activity,agent);
        return wifiSettings;
    }
    private void wifiResult(long id,String reason) {
        boolean accepted=reason.equals("wifi_requested")||reason.equals("wifi_scan_requested");
        result(id,accepted?Protocol.COMPLETED:Protocol.UNCERTAIN,reason);
    }
    private void launcherCommand(JSONObject command) throws Exception {
        String operation=command.getString("operation"); long id=command.optLong("id",0);
        switch(operation) {
            case "catalog": refreshCatalog(); break;
            case "widgets_snapshot": widgets.refresh();break;
            case "haptic": { String kind=command.optString("kind","tick"); main.post(() -> haptic(kind)); break; }
            case "recent_apps": publishRecentApps(); break;
            case "system_bars": shellDark=command.optBoolean("dark",false); main.post(this::applyWindowChrome); break;
            case "theme_snapshot": emitUiMode();break;
            case "display_snapshot": {Protocol.requireCommand(id);emit("launcher.display_state",displaySettings().snapshot(id));break;}
            case "display_density":case "display_night": {
                Protocol.requireCommand(id);
                dev.makepad.octosense.display.DisplaySettingsContract.Setting setting="display_density".equals(operation)
                    ?dev.makepad.octosense.display.DisplaySettingsContract.Setting.DENSITY
                    :dev.makepad.octosense.display.DisplaySettingsContract.Setting.parse(stringField(command,"setting"));
                if("display_night".equals(operation)&&setting==dev.makepad.octosense.display.DisplaySettingsContract.Setting.DENSITY)throw new IllegalArgumentException("Night Light setting required");
                String reason=displaySettings().set(stringField(command,"key"),setting,command.get("display_density".equals(operation)?"choice":"value"));
                result(id,reason.equals("display_applied")||reason.equals("display_requested")?Protocol.COMPLETED:Protocol.UNCERTAIN,reason);break;
            }
            case "network_snapshot": {Protocol.requireCommand(id);emit("launcher.network_state",networkSettings().snapshot(id));break;}
            case "network_airplane": {Protocol.requireCommand(id);networkResult(id,networkSettings().airplane(stringField(command,"key"),dev.makepad.octosense.network.NetworkSettingsContract.enabled(command.get("enabled"))));break;}
            case "network_data_saver": {Protocol.requireCommand(id);networkResult(id,networkSettings().dataSaver(stringField(command,"key"),dev.makepad.octosense.network.NetworkSettingsContract.enabled(command.get("enabled"))));break;}
            case "network_private_dns": {
                Protocol.requireCommand(id);String mode=stringField(command,"mode");
                String hostname=dev.makepad.octosense.network.NetworkSettingsContract.dnsHostname(mode,command.has("hostname")?command.get("hostname"):null);
                networkResult(id,networkSettings().privateDns(stringField(command,"key"),mode,hostname));break;
            }
            case "updates_snapshot": {Protocol.requireCommand(id);emit("launcher.updates_state",updatesSettings().snapshot(id));break;}
            case "updates_check": {Protocol.requireCommand(id);updateResult(id,updatesSettings().check());break;}
            case "updates_install": {Protocol.requireCommand(id);updateResult(id,updatesSettings().install(stringField(command,"offer_key"),stringField(command,"part")));break;}
            case "updates_reboot": {Protocol.requireCommand(id);updateResult(id,updatesSettings().reboot(stringField(command,"reboot_key")));break;}
            case "accounts_snapshot": {Protocol.requireCommand(id);emit("launcher.accounts_state",accountsSettings().snapshot(id));break;}
            case "account_details": {Protocol.requireCommand(id);emit("launcher.account_details",accountsSettings().details(id,stringField(command,"key")));break;}
            case "accounts_master_sync": {
                Protocol.requireCommand(id);syncResult(id,accountsSettings().master(dev.makepad.octosense.accounts.AccountsSettingsContract.enabled(command.get("enabled"))));break;
            }
            case "account_sync": {
                Protocol.requireCommand(id);dev.makepad.octosense.accounts.AccountsSettingsContract.SyncAction action=dev.makepad.octosense.accounts.AccountsSettingsContract.SyncAction.parse(stringField(command,"action"));
                Boolean value=dev.makepad.octosense.accounts.AccountsSettingsContract.syncValue(action,command.has("enabled")?command.get("enabled"):null);
                syncResult(id,accountsSettings().sync(stringField(command,"key"),stringField(command,"authority_key"),action,value));break;
            }
            case "account_add": {Protocol.requireCommand(id);openAccountFlow(id,accountsSettings().add(stringField(command,"provider_key")));break;}
            case "account_remove": {Protocol.requireCommand(id);openAccountFlow(id,accountsSettings().remove(stringField(command,"key")));break;}
            case "accounts_access": {Protocol.requireCommand(id);openSettingsAppIntent(id,accountsSettings().access());break;}
            case "bluetooth_snapshot": {Protocol.requireCommand(id);emit("launcher.bluetooth_state",bluetoothSettings().snapshot(id));break;}
            case "bluetooth_enabled": {
                Protocol.requireCommand(id);bluetoothResult(id,bluetoothSettings().enabled(dev.makepad.octosense.bluetooth.BluetoothSettingsContract.enabled(command.get("enabled"))));break;
            }
            case "bluetooth_scan": {
                Protocol.requireCommand(id);bluetoothResult(id,bluetoothSettings().scan(dev.makepad.octosense.bluetooth.BluetoothSettingsContract.enabled(command.get("enabled"))));break;
            }
            case "bluetooth_name": {Protocol.requireCommand(id);bluetoothResult(id,bluetoothSettings().name(stringField(command,"name")));break;}
            case "bluetooth_device": {
                Protocol.requireCommand(id);bluetoothResult(id,bluetoothSettings().device(stringField(command,"key"),
                        dev.makepad.octosense.bluetooth.BluetoothSettingsContract.Action.parse(stringField(command,"action"))));break;
            }
            case "bluetooth_sharing": {
                Protocol.requireCommand(id);bluetoothResult(id,bluetoothSettings().sharing(stringField(command,"key"),stringField(command,"kind"),stringField(command,"value")));break;
            }
            case "bluetooth_access": {Protocol.requireCommand(id);openSettingsAppIntent(id,bluetoothSettings().access());break;}
            case "keyboards_snapshot": {Protocol.requireCommand(id);emit("launcher.keyboards_state",keyboardSettings().snapshot(id,stringField(command,"query"),dndInt(command.get("offset"),0,127)));break;}
            case "keyboard_flow": {Protocol.requireCommand(id);openKeyboardFlow(id,keyboardSettings().prepare(id,stringField(command,"key"),stringField(command,"target"),stringField(command,"operation")));break;}
            case "system_languages_snapshot": {Protocol.requireCommand(id);emit("launcher.system_languages_state",systemLanguageSettings().snapshot(id,stringField(command,"key"),stringField(command,"parent"),stringField(command,"query"),dndInt(command.get("offset"),0,4095)));break;}
            case "system_languages_apply": {
                Protocol.requireCommand(id);JSONArray values=command.getJSONArray("order");if(values.length()<1||values.length()>128)throw new IllegalArgumentException("Invalid language order");String[] targets=new String[values.length()];for(int i=0;i<targets.length;i++){Object target=values.get(i);if(!(target instanceof String))throw new IllegalArgumentException("Invalid language target");targets[i]=(String)target;}
                String reason=systemLanguageSettings().apply(stringField(command,"key"),targets);result(id,reason.equals("languages_applied")||reason.equals("languages_requested")?Protocol.COMPLETED:Protocol.UNCERTAIN,reason);break;
            }
            case "caption_language_snapshot": {Protocol.requireCommand(id);emit("launcher.caption_language",captionLanguageSettings().snapshot(id,stringField(command,"key"),stringField(command,"query"),dndInt(command.get("offset"),0,1024)));break;}
            case "caption_language_set": {Protocol.requireCommand(id);String reason=captionLanguageSettings().select(stringField(command,"key"),stringField(command,"choice"));result(id,reason.equals("control_applied")||reason.equals("control_requested")?Protocol.COMPLETED:Protocol.UNCERTAIN,reason);break;}
            case "caption_custom_snapshot": {Protocol.requireCommand(id);emit("launcher.caption_custom_state",captionCustomSettings().snapshot(id,captionVisit(command.get("visit"))));break;}
            case "caption_custom_set": {Protocol.requireCommand(id);String reason=captionCustomSettings().select(captionVisit(command.get("visit")),stringField(command,"field"),stringField(command,"value"));result(id,reason.equals("control_applied")||reason.equals("control_requested")?Protocol.COMPLETED:Protocol.UNCERTAIN,reason);break;}
            case "caption_custom_close": {Protocol.requireCommand(id);captionCustomSettings().closeInBackground(captionVisit(command.get("visit")));break;}
            case "controls_snapshot": {
                Protocol.requireCommand(id);Object page=command.get("page");
                if(!(page instanceof String)) throw new IllegalArgumentException("Invalid controls page");
                dev.makepad.octosense.controls.SettingsControlsContract.Page parsed=dev.makepad.octosense.controls.SettingsControlsContract.Page.parse((String)page);
                JSONObject snapshot=agent.controlsSnapshot(id,parsed);
                if(snapshot==null) snapshot=new dev.makepad.octosense.controls.SettingsControlsBackend(activity,false,null).snapshot(id,parsed);
                emit("launcher.controls_state",snapshot);break;
            }
            case "sounds_snapshot": {
                Protocol.requireCommand(id);Object raw=command.opt("key");
                if(raw!=null&&raw!=JSONObject.NULL&&!(raw instanceof String))throw new IllegalArgumentException("Invalid sound key");
                String key=raw instanceof String?(String)raw:null;Object page=command.get("offset");
                if(!(page instanceof Integer||page instanceof Long))throw new IllegalArgumentException("Invalid sound offset");
                long offset=((Number)page).longValue();if(offset<0||offset>=dev.makepad.octosense.sounds.SoundSettingsContract.MAX_ROWS)throw new IllegalArgumentException("Invalid sound offset");
                emit("launcher.sounds_state",soundsSettings().snapshot(id,stringField(command,"type"),key,(int)offset));break;
            }
            case "sound_preview":case "sound_save": {
                Protocol.requireCommand(id);soundResult(id,soundsSettings().action(stringField(command,"type"),stringField(command,"key"),stringField(command,"target"),"sound_save".equals(operation)));break;
            }
            case "sound_stop": {Protocol.requireCommand(id);soundResult(id,soundsSettings().stop());break;}
            case "sounds_access": {Protocol.requireCommand(id);openSettingsAppIntent(id,soundsSettings().access());break;}
            case "app_notifications_snapshot": {
                Protocol.requireCommand(id);Object raw=command.opt("generation"),page=command.get("offset");
                if(raw!=null&&raw!=JSONObject.NULL&&!(raw instanceof String))throw new IllegalArgumentException("Invalid notification generation");
                if(!(page instanceof Integer||page instanceof Long))throw new IllegalArgumentException("Invalid notification offset");
                long offset=((Number)page).longValue();if(offset<0||offset>=dev.makepad.octosense.notifications.AppNotificationsContract.MAX_ROWS)throw new IllegalArgumentException("Invalid notification offset");
                emit("launcher.app_notifications_state",appNotificationsSettings().snapshot(id,stringField(command,"package"),(int)offset,raw instanceof String?(String)raw:null));break;
            }
            case "roles_snapshot": {
                Protocol.requireCommand(id);Object role=command.opt("role"),generation=command.opt("generation"),page=command.get("offset");
                if(role!=null&&role!=JSONObject.NULL&&!(role instanceof String)||generation!=null&&generation!=JSONObject.NULL&&!(generation instanceof String))throw new IllegalArgumentException("Invalid role selector");
                if(!(page instanceof Integer||page instanceof Long))throw new IllegalArgumentException("Invalid role offset");
                long offset=((Number)page).longValue();if(offset<0||offset>=dev.makepad.octosense.roles.RolesSettingsContract.MAX_ROWS)throw new IllegalArgumentException("Invalid role offset");
                emit("launcher.roles_state",rolesSettings().snapshot(id,role instanceof String?dev.makepad.octosense.roles.RolesSettingsContract.RoleId.parse((String)role):null,(int)offset,generation instanceof String?(String)generation:null));break;
            }
            case "role_confirm": {
                Protocol.requireCommand(id);openRoleFlow(id,rolesSettings().confirmation(dev.makepad.octosense.roles.RolesSettingsContract.RoleId.parse(stringField(command,"role")),stringField(command,"key"),stringField(command,"target")));break;
            }
            case "permissions_snapshot": {
                Protocol.requireCommand(id);Object group=command.opt("group"),generation=command.opt("generation"),page=command.get("offset");
                if(group!=null&&group!=JSONObject.NULL&&!(group instanceof String)||generation!=null&&generation!=JSONObject.NULL&&!(generation instanceof String))throw new IllegalArgumentException("Invalid permission selector");
                if(!(page instanceof Integer||page instanceof Long))throw new IllegalArgumentException("Invalid permission offset");
                long offset=((Number)page).longValue();if(offset<0||offset>=dev.makepad.octosense.permissions.PermissionsSettingsContract.MAX_GROUPS)throw new IllegalArgumentException("Invalid permission offset");
                emit("launcher.permissions_state",permissionsSettings().snapshot(id,stringField(command,"package"),group instanceof String?dev.makepad.octosense.permissions.PermissionsSettingsContract.Group.parse((String)group):null,(int)offset,generation instanceof String?(String)generation:null));break;
            }
            case "permission_choice": {
                Protocol.requireCommand(id);openPermissionFlow(id,permissionsSettings().operation(stringField(command,"package"),dev.makepad.octosense.permissions.PermissionsSettingsContract.Group.parse(stringField(command,"group")),stringField(command,"key"),stringField(command,"target")));break;
            }
            case "app_language_snapshot": {
                Protocol.requireCommand(id);
                emit("launcher.app_language",appLanguageSettings().snapshot(id,stringField(command,"package"),stringField(command,"key"),stringField(command,"parent"),stringField(command,"query"),dndInt(command.get("offset"),0,1020)));break;
            }
            case "app_language_set": {
                Protocol.requireCommand(id);String reason=appLanguageSettings().select(stringField(command,"package"),stringField(command,"key"),stringField(command,"choice"));
                result(id,reason.equals("app_language_applied")||reason.equals("app_language_requested")?Protocol.COMPLETED:Protocol.UNCERTAIN,reason);break;
            }
            case "app_storage_snapshot": {
                Protocol.requireCommand(id);emit("launcher.app_storage_state",appStorageSettings().snapshot(id,stringField(command,"package")));break;
            }
            case "app_storage_action": {
                Protocol.requireCommand(id);android.os.Bundle response=appStorageSettings().action(stringField(command,"package"),stringField(command,"key"),stringField(command,"action"));
                String reason=response.getString("reason","app_storage_unavailable");
                if(reason.equals("app_storage_flow_opened")){openAppStorageFlow(id,response.getParcelable("flow"));break;}
                result(id,reason.equals("app_storage_requested")?Protocol.COMPLETED:Protocol.UNCERTAIN,reason);break;
            }
            case "app_battery_snapshot": {
                Protocol.requireCommand(id);emit("launcher.app_battery_state",appBatterySettings().snapshot(id,stringField(command,"package")));break;
            }
            case "app_battery_set": {
                Protocol.requireCommand(id);String reason=appBatterySettings().set(stringField(command,"package"),stringField(command,"key"),stringField(command,"mode"));
                result(id,reason.equals("app_battery_applied")||reason.equals("app_battery_requested")?Protocol.COMPLETED:Protocol.UNCERTAIN,reason);break;
            }
            case "app_network_snapshot": {
                Protocol.requireCommand(id);emit("launcher.app_network_state",appNetworkSettings().snapshot(id,stringField(command,"package")));break;
            }
            case "app_network_set": {
                Protocol.requireCommand(id);String reason=appNetworkSettings().set(stringField(command,"package"),stringField(command,"key"),dev.makepad.octosense.appnetwork.AppNetworkContract.Field.parse(stringField(command,"field")),dndBool(command.get("enabled")));
                result(id,reason.equals("app_network_applied")||reason.equals("app_network_unchanged")?Protocol.COMPLETED:Protocol.UNCERTAIN,reason);break;
            }
            case "dnd_snapshot": {
                Protocol.requireCommand(id);emit("launcher.dnd_state",dndSettings().snapshot(id,dndInt(command.get("offset"),0,255),dndOptionalString(command.opt("generation"))));break;
            }
            case "dnd_policy_set": {
                Protocol.requireCommand(id);dndResult(id,dndSettings().policy(stringField(command,"key"),dev.makepad.octosense.dnd.DndSettingsContract.Field.parse(stringField(command,"field")),stringField(command,"value")));break;
            }
            case "dnd_schedule_save": {
                Protocol.requireCommand(id);Object rawDays=command.get("days");if(!(rawDays instanceof JSONArray))throw new IllegalArgumentException("DND day array required");
                JSONArray array=(JSONArray)rawDays;if(array.length()>7)throw new IllegalArgumentException("DND day array too large");int[] days=new int[array.length()];for(int i=0;i<days.length;i++)days[i]=dndInt(array.get(i),1,7);
                dev.makepad.octosense.dnd.DndSettingsContract.Schedule schedule=new dev.makepad.octosense.dnd.DndSettingsContract.Schedule(stringField(command,"name"),days,dndInt(command.get("start_minute"),0,1439),dndInt(command.get("end_minute"),0,1439),dndBool(command.get("exit_at_alarm")),dndBool(command.get("enabled")));
                dndResult(id,dndSettings().schedule(stringField(command,"key"),dndOptionalString(command.opt("target")),schedule));break;
            }
            case "dnd_rule_enabled": {Protocol.requireCommand(id);dndResult(id,dndSettings().enabled(stringField(command,"key"),stringField(command,"target"),dndBool(command.get("enabled"))));break;}
            case "dnd_rule_delete": {Protocol.requireCommand(id);dndResult(id,dndSettings().delete(stringField(command,"key"),stringField(command,"target")));break;}
            case "app_notifications_set": {
                Protocol.requireCommand(id);
                String reason=appNotificationsSettings().set(stringField(command,"package"),stringField(command,"key"),stringField(command,"target"),
                    dev.makepad.octosense.notifications.AppNotificationsContract.Action.parse(stringField(command,"action")),stringField(command,"value"));
                result(id,"notifications_applied".equals(reason)?Protocol.COMPLETED:Protocol.UNCERTAIN,reason);break;
            }
            case "notification_history": {
                Protocol.requireCommand(id);
                Object raw=command.opt("key");
                if(raw!=null&&raw!=JSONObject.NULL&&!(raw instanceof String)) throw new IllegalArgumentException("Invalid history key");
                String key=raw instanceof String?(String)raw:null;
                Object page=command.get("offset");
                if(!(page instanceof Integer||page instanceof Long)) throw new IllegalArgumentException("Invalid history offset");
                long number=((Number)page).longValue();
                if(number<0||number>=dev.makepad.octosense.notifications.NotificationHistoryContract.MAX_ROWS) throw new IllegalArgumentException("Invalid history offset");
                JSONObject snapshot=agent.notificationHistory(id,key,(int)number);
                if(snapshot==null) snapshot=json("schema",1,"request_id",id,"status","unavailable","enabled",JSONObject.NULL,
                        "key",JSONObject.NULL,"offset",0,"total",0,"truncated",false,"rows",new JSONArray());
                emit("launcher.notification_history",snapshot);break;
            }
            case "control_set": {
                Protocol.requireCommand(id);Object page=command.get("page"),control=command.get("control"),value=command.get("value");
                if(!(page instanceof String)||!(control instanceof String)||!(value instanceof String)) throw new IllegalArgumentException("Invalid control");
                dev.makepad.octosense.controls.SettingsControlsContract.Page parsed=dev.makepad.octosense.controls.SettingsControlsContract.Page.parse((String)page);
                String reason=agent.setControl(parsed,dev.makepad.octosense.controls.SettingsControlsContract.Control.parse(parsed,(String)control),(String)value);
                result(id,reason.equals("control_applied")||reason.equals("control_requested")?Protocol.COMPLETED:Protocol.UNCERTAIN,reason);break;
            }
            case "wifi_snapshot": {
                Protocol.requireCommand(id);emit("launcher.wifi_state",wifiSettings().snapshot(id));break;
            }
            case "wifi_enabled": {
                Protocol.requireCommand(id);
                wifiResult(id,wifiSettings().enabled(dev.makepad.octosense.wifi.WifiSettingsContract.enabled(command.get("enabled"))));break;
            }
            case "wifi_scan": {Protocol.requireCommand(id);wifiResult(id,wifiSettings().scan());break;}
            case "wifi_network": {
                Protocol.requireCommand(id);Object key=command.get("key"),action=command.get("action");
                if(!(key instanceof String)||!(action instanceof String)) throw new IllegalArgumentException("Invalid Wi-Fi action");
                dev.makepad.octosense.wifi.WifiSettingsContract.Action parsed=dev.makepad.octosense.wifi.WifiSettingsContract.Action.parse((String)action);
                if(parsed==dev.makepad.octosense.wifi.WifiSettingsContract.Action.CONFIGURE)
                    openSettingsAppIntent(id,wifiSettings().configure((String)key));
                else wifiResult(id,wifiSettings().network((String)key,parsed));
                break;
            }
            case "wifi_access": {Protocol.requireCommand(id);openSettingsAppIntent(id,wifiSettings().access());break;}
            case "apps_catalog": {
                Protocol.requireCommand(id);
                Object query=command.get("query"),includeSystem=command.get("include_system"),generation=command.opt("generation");
                if(!(query instanceof String)||!(includeSystem instanceof Boolean)
                        ||(generation!=null&&generation!=JSONObject.NULL&&!(generation instanceof String))) throw new IllegalArgumentException("Invalid catalog request");
                emit("launcher.apps_catalog",new AppsSettingsBackend(activity).catalog(id,(String)query,(Boolean)includeSystem,
                        AppsSettingsContract.offset(command.get("offset")),generation instanceof String?(String)generation:null));break;
            }
            case "app_details": {
                Protocol.requireCommand(id);Object name=command.get("package");
                if(!(name instanceof String)) throw new IllegalArgumentException("Invalid package");
                emit("launcher.app_details",new AppsSettingsBackend(activity).details(id,(String)name,
                        AppsSettingsContract.offset(command.get("permission_offset"))));break;
            }
            case "app_action": {
                Protocol.requireCommand(id);Object name=command.get("package"),action=command.get("action");
                if(!(name instanceof String)||!(action instanceof String)) throw new IllegalArgumentException("Invalid app action");
                openSettingsAppIntent(id,new AppsSettingsBackend(activity).actionIntent((String)name,AppsSettingsContract.Action.parse((String)action)));break;
            }
            case "apps_usage_access": {
                Protocol.requireCommand(id);openSettingsAppIntent(id,new AppsSettingsBackend(activity).usageIntent());break;
            }
            case "device_settings_snapshot": {
                Protocol.requireCommand(id);
                emit("launcher.device_settings",new DeviceSettingsBackend(activity,agent).snapshot(id));break;
            }
            case "date_time_set": {
                Protocol.requireCommand(id);
                String key=stringField(command,"key"),action=stringField(command,"action"),value=stringField(command,"value");
                String occurrence=command.has("occurrence")?stringField(command,"occurrence"):null;
                dev.makepad.octosense.datetime.DateTimeContract.key(key);
                dev.makepad.octosense.datetime.DateTimeContract.validate(dev.makepad.octosense.datetime.DateTimeContract.Action.parse(action),value,occurrence);
                String reason=agent.setDateTime(key,action,value,occurrence);
                emit("launcher.device_settings",new DeviceSettingsBackend(activity,agent).snapshot(id));
                result(id,reason.equals("time_applied")||reason.equals("time_requested")?Protocol.COMPLETED:Protocol.UNCERTAIN,reason);break;
            }
            case "device_setting": {
                Protocol.requireCommand(id);
                DeviceSetting setting=DeviceSetting.parse(command.getString("setting"));
                Object value=command.get("value");
                DeviceSettingsBackend backend=new DeviceSettingsBackend(activity,agent);
                boolean confirmed=backend.apply(setting,value);
                emit("launcher.device_settings",backend.snapshot(id));
                result(id,confirmed?Protocol.COMPLETED:Protocol.UNCERTAIN,confirmed?"setting_applied":"setting_not_confirmed");
                if(setting==DeviceSetting.FONT_SCALE) emitUiMode();
                break;
            }
            case "device_settings_access": {
                Protocol.requireCommand(id);final long commandId=id;
                main.post(() -> {
                    boolean opened=dev.makepad.octosense.contracts.SystemSettings.open(activity,"home_write_settings");
                    result(commandId,opened?Protocol.COMPLETED:Protocol.UNSUPPORTED,opened?"settings_opened":"setting_unavailable");
                });break;
            }
            case "theme_apply": {
                Protocol.requireCommand(id);
                ThemeCatalog.Choice choice=ThemeApplier.parseChoice(activity,command.getJSONObject("theme"));
                ThemeApplier.Result applied=ThemeApplier.apply(activity,choice,agent,agentBinding);
                emitUiMode();
                emit("launcher.result",json("id",id,"status",applied.saved?Protocol.COMPLETED:Protocol.UNCERTAIN,
                        "reason",applied.reason(),"theme_saved",applied.saved,"system_palette",applied.palette,
                        "system_wallpaper",applied.wallpaper,"system_appearance",applied.appearance));
                break;
            }
            case "theme_appearance": {
                ThemeCatalog catalog=ThemeCatalog.get(activity);
                if(catalog.save(activity,catalog.read(activity).appearance(command.optBoolean("dark",false)?"dark":"light"))) emitUiMode();
                break;
            }
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
            case "pair_menu": pairMenu(command); break;
            case "pairs_set": { JSONArray next=command.getJSONArray("pairs"); placementEdit(() -> placements().setPairs(next)); break; }
            case "tile_menu": main.post(() -> {
                if(destroyed || activity.isFinishing()) return;
                String app=command.optString("app",""),label=command.optString("label",app);
                dialog(command.optBoolean("dark",false)).setTitle(label).setItems(new String[]{"Remove tile from Home"},(dialog,which) ->
                    placementEdit(() -> placements().hideTile(app,true))).setNegativeButton("Cancel",null).show();
            }); break;
            case "home_menu": main.post(() -> {
                if(destroyed || activity.isFinishing()) return;
                boolean hiddenTiles=command.optInt("hidden_tiles",0)>0;
                boolean dark=command.optBoolean("dark",false);
                String appearance=activity.getString(dev.makepad.android.R.string.octosense_wallpaper_style);
                int columns=command.optInt("columns",4);
                String grid=columns>=5?"Grid: 4 columns":"Grid: 5 columns";
                int nextColumns=columns>=5?4:5;
                boolean systemPanel=command.optBoolean("system_panel",false);
                String pulls=systemPanel?"Pull-downs: use the launcher's shade":"Pull-downs: use the system-wide panel";
                String[] items=hiddenTiles?new String[]{"Widgets",appearance,grid,pulls,"System setup","Show hidden tiles"}:new String[]{"Widgets",appearance,grid,pulls,"System setup"};
                dialog(dark).setTitle("Home").setItems(items,(dialog,which) -> {
                    if(which==0) widgets.show();
                    else if(which==1) activity.startActivity(new Intent(activity,ThemeSettingsActivity.class));
                    else if(which==2) placementEdit(() -> placements().setColumns(nextColumns));
                    else if(which==3) placementEdit(() -> placements().setLauncherShade(systemPanel));
                    else if(which==4) dev.makepad.octosense.contracts.SystemSettings.open(activity,"access");
                    else if(which==5) placementEdit(() -> placements().hideTile(null,false));
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
                    boolean opened="usage_access".equals(destination)?openUsageAccess():dev.makepad.octosense.contracts.SystemSettings.open(activity,destination);
                    result(id,opened ? Protocol.COMPLETED : Protocol.UNSUPPORTED,opened ? "settings_opened" : "setting_unavailable");
                }); break;
            }
            default: result(id,Protocol.UNSUPPORTED,"unknown_launcher_operation");
        }
    }
    private void openAccountFlow(long id,AccountsSettingsClient.Flow flow) {
        if(flow==null||(flow.intent==null&&flow.pending==null)) {result(id,Protocol.UNSUPPORTED,"account_target_changed");return;}
        if(flow.intent!=null) {openSettingsAppIntent(id,flow.intent);return;}
        main.post(() -> {
            if(destroyed||!resumed||activity.isFinishing()||!activity.hasWindowFocus()) {result(id,Protocol.UNCERTAIN,"operation_failed");return;}
            try {
                // Android 14+ requires the visible sender to explicitly lend its
                // launch privilege to this user-requested, broker-owned token.
                // The broker does not grant background launches when creating it.
                android.app.ActivityOptions options=android.app.ActivityOptions.makeBasic();
                // The pinned compile SDK exposes the boolean setter. Android
                // 15 ComponentOptions maps true to START_ALLOWED (mode 1).
                if(android.os.Build.VERSION.SDK_INT>=34) options.setPendingIntentBackgroundActivityLaunchAllowed(true);
                activity.startIntentSender(flow.pending.getIntentSender(),null,0,0,0,options.toBundle());
                result(id,Protocol.COMPLETED,"settings_opened");
            } catch(android.content.IntentSender.SendIntentException|SecurityException unavailable) {result(id,Protocol.UNSUPPORTED,"account_target_changed");}
        });
    }
    private void openKeyboardFlow(long id,android.app.PendingIntent flow){
        if(flow==null||!flow.isImmutable()||flow.getCreatorUid()!=android.os.Process.SYSTEM_UID||!"dev.makepad.octosense.settingsbroker".equals(flow.getCreatorPackage())){result(id,Protocol.UNSUPPORTED,"keyboard_target_changed");return;}
        main.post(()->{
            if(destroyed||!resumed||activity.isFinishing()||!activity.hasWindowFocus()){result(id,Protocol.UNCERTAIN,"keyboard_restricted");return;}
            try{android.app.ActivityOptions options=android.app.ActivityOptions.makeBasic();if(android.os.Build.VERSION.SDK_INT>=34)options.setPendingIntentBackgroundActivityLaunchAllowed(true);
                activity.startIntentSender(flow.getIntentSender(),null,0,0,0,options.toBundle());result(id,Protocol.COMPLETED,"keyboard_flow_opened");
            }catch(android.content.IntentSender.SendIntentException|SecurityException unavailable){result(id,Protocol.UNSUPPORTED,"keyboard_target_changed");}
        });
    }
    private void openRoleFlow(long id,android.app.PendingIntent flow){
        if(flow==null){result(id,Protocol.UNSUPPORTED,"role_target_changed");return;}
        main.post(() -> {
            if(destroyed||!resumed||activity.isFinishing()||!activity.hasWindowFocus()){result(id,Protocol.UNCERTAIN,"role_restricted");return;}
            try{
                android.app.ActivityOptions options=android.app.ActivityOptions.makeBasic();
                if(android.os.Build.VERSION.SDK_INT>=34)options.setPendingIntentBackgroundActivityLaunchAllowed(true);
                activity.startIntentSender(flow.getIntentSender(),null,0,0,0,options.toBundle());
                result(id,Protocol.COMPLETED,"role_confirmation_opened");
            }catch(android.content.IntentSender.SendIntentException|SecurityException unavailable){result(id,Protocol.UNSUPPORTED,"role_target_changed");}
        });
    }
    private void openAppStorageFlow(long id,android.app.PendingIntent flow){
        if(flow==null||!flow.isImmutable()||flow.getCreatorUid()!=android.os.Process.SYSTEM_UID||!"dev.makepad.octosense.settingsbroker".equals(flow.getCreatorPackage())){result(id,Protocol.UNSUPPORTED,"app_storage_target_changed");return;}
        main.post(() -> {
            if(destroyed||!resumed||activity.isFinishing()||!activity.hasWindowFocus()){result(id,Protocol.UNCERTAIN,"app_storage_restricted");return;}
            try{
                android.app.ActivityOptions options=android.app.ActivityOptions.makeBasic();
                if(android.os.Build.VERSION.SDK_INT>=34)options.setPendingIntentBackgroundActivityLaunchAllowed(true);
                activity.startIntentSender(flow.getIntentSender(),null,0,0,0,options.toBundle());
                result(id,Protocol.COMPLETED,"app_storage_flow_opened");
            }catch(android.content.IntentSender.SendIntentException|SecurityException unavailable){result(id,Protocol.UNSUPPORTED,"app_storage_target_changed");}
        });
    }
    private void openPermissionFlow(long id,android.app.PendingIntent flow){
        if(flow==null){result(id,Protocol.UNSUPPORTED,"permission_target_changed");return;}
        main.post(() -> {
            if(destroyed||!resumed||activity.isFinishing()||!activity.hasWindowFocus()){result(id,Protocol.UNCERTAIN,"permission_restricted");return;}
            try{
                android.app.ActivityOptions options=android.app.ActivityOptions.makeBasic();
                if(android.os.Build.VERSION.SDK_INT>=34)options.setPendingIntentBackgroundActivityLaunchAllowed(true);
                activity.startIntentSender(flow.getIntentSender(),null,0,0,0,options.toBundle());
                result(id,Protocol.COMPLETED,"permission_flow_opened");
            }catch(android.content.IntentSender.SendIntentException|SecurityException unavailable){result(id,Protocol.UNSUPPORTED,"permission_target_changed");}
        });
    }
    private void openSettingsAppIntent(long id,Intent intent) {
        if(intent==null) {result(id,Protocol.UNSUPPORTED,"setting_unavailable");return;}
        // Only finite intents constructed and checked on the worker reach this
        // point. Android owns the confirmation and final uninstall outcome.
        main.post(() -> {
            if(destroyed||!resumed||activity.isFinishing()) {result(id,Protocol.UNCERTAIN,"operation_failed");return;}
            try {
                // Keep native editors/consent in the calling task so Back
                // returns to the selected OctoSense Settings page.
                activity.startActivity(intent);
                result(id,Protocol.COMPLETED,"settings_opened");
            } catch(android.content.ActivityNotFoundException|SecurityException unavailable) {result(id,Protocol.UNSUPPORTED,"setting_unavailable");}
        });
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
    private interface PlacementEdit { void run() throws Exception; }
    /** A placement change from a dialog: on the worker, published, with the same failure copy as the icon menu. */
    private void placementEdit(PlacementEdit edit) {
        if(!offer(() -> {
            try {edit.run();publishPlacements();}
            catch(IllegalArgumentException e) {result(0,Protocol.INVALID_ARGUMENT,"home_placement_limit_or_identity");}
            catch(Exception e) {
                placements=null;
                result(0,Protocol.UNCERTAIN,"home_placement_storage_unavailable");
                try {publishPlacements();} catch(Exception ignored) {}
            }
        })) {queueCommandLoss.set(true);scheduleRecovery();}
    }
    /** A pair tile's menu: change either app (from the hosted catalog) or take the pair off the page. */
    private void pairMenu(JSONObject command) {
        main.post(() -> {
            if(destroyed || activity.isFinishing()) return;
            try {
                String name=command.getString("name");boolean dark=command.optBoolean("dark",false);
                JSONArray catalog=command.getJSONArray("catalog"),pairs=command.getJSONArray("pairs");
                JSONObject current=null;
                for(int index=0;index<pairs.length();index++) if(pairs.getJSONObject(index).getString("name").equals(name)) current=pairs.getJSONObject(index);
                if(current==null) return;
                JSONArray apps=current.getJSONArray("apps");
                String[] labels=new String[catalog.length()],ids=new String[catalog.length()];
                for(int index=0;index<catalog.length();index++) {ids[index]=catalog.getJSONObject(index).getString("id");labels[index]=catalog.getJSONObject(index).getString("label");}
                java.util.function.Function<String,String> labelOf=id -> {for(int i=0;i<ids.length;i++) if(ids[i].equals(id)) return labels[i];return id;};
                if(apps.length()>2) {
                    // A folder: take a member out, or the whole folder off the page.
                    String[] folderItems=new String[apps.length()+1];
                    for(int index=0;index<apps.length();index++) folderItems[index]="Remove "+labelOf.apply(apps.getString(index));
                    folderItems[apps.length()]="Remove folder from Home";
                    dialog(dark).setTitle(name+" · Folder").setItems(folderItems,(dialog,which) -> {
                        try {
                            JSONArray next=new JSONArray();
                            for(int index=0;index<pairs.length();index++) {
                                JSONObject pair=pairs.getJSONObject(index);
                                if(!pair.getString("name").equals(name)) {next.put(pair);continue;}
                                if(which==apps.length()) continue;
                                JSONArray kept=new JSONArray();
                                for(int a=0;a<apps.length();a++) if(a!=which) kept.put(apps.getString(a));
                                next.put(new JSONObject().put("name",name).put("apps",kept));
                            }
                            placementEdit(() -> placements().setPairs(next));
                        } catch(JSONException ignored) {}
                    }).setNegativeButton("Cancel",null).show();
                    return;
                }
                String[] items={"First app: "+labelOf.apply(apps.getString(0)),"Second app: "+labelOf.apply(apps.getString(1)),"Remove pair from Home"};
                dialog(dark).setTitle(name+" · App pair").setItems(items,(dialog,which) -> {
                    try {
                        if(which==2) {
                            JSONArray next=new JSONArray();
                            for(int index=0;index<pairs.length();index++) if(!pairs.getJSONObject(index).getString("name").equals(name)) next.put(pairs.getJSONObject(index));
                            placementEdit(() -> placements().setPairs(next));
                            return;
                        }
                        int slot=which;
                        dialog(dark).setTitle(slot==0?"First app":"Second app").setItems(labels,(picker,choice) -> {
                            try {
                                JSONArray chosen=new JSONArray().put(slot==0?ids[choice]:apps.getString(0)).put(slot==1?ids[choice]:apps.getString(1));
                                JSONArray next=new JSONArray();
                                for(int index=0;index<pairs.length();index++) {
                                    JSONObject pair=pairs.getJSONObject(index);
                                    next.put(pair.getString("name").equals(name)?new JSONObject().put("name",name).put("apps",chosen):pair);
                                }
                                placementEdit(() -> placements().setPairs(next));
                            } catch(JSONException ignored) {}
                        }).setNegativeButton("Cancel",null).show();
                    } catch(JSONException ignored) {}
                }).setNegativeButton("Cancel",null).show();
            } catch(JSONException ignored) {}
        });
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
        windowFocused=activity.hasWindowFocus();
        settingsAccessibility.setWindowFocused(windowFocused);
        resumed=true;settingsAccessibility.onResume(); homeGeometry.onResume(); widgets.onResume(); refreshCatalog(); bindBridge(); main.post(() -> { if(!destroyed && agent!=null) agent.bind(); }); requestResync();
        main.post(this::applyWindowChrome);
        offer(() -> {emitUiMode();emitHints();publishRecentApps();flushEvents();});
    }
    @Override public void onPause() {
        if(captionCustomSettings!=null)captionCustomSettings.retireInBackground();
        resumed=false;windowFocused=false;settingsAccessibility.onPause();if(soundsSettings!=null)agent.stopSoundInBackground();closePlacementMenu();replyComposer.close(); homeGeometry.onPause(); widgets.onPause();
        // Retain the latest lifecycle observation too: a queued pre-pause
        // ui_mode snapshot must not leave Settings polling in the background.
        offer(() -> {if(soundsSettings!=null)soundsSettings.invalidate();if(appNotificationsSettings!=null)appNotificationsSettings.invalidate();if(rolesSettings!=null)rolesSettings.invalidate();if(permissionsSettings!=null)permissionsSettings.invalidate();if(dndSettings!=null)dndSettings.invalidate();if(appNetworkSettings!=null)appNetworkSettings.invalidate();if(appBatterySettings!=null)appBatterySettings.invalidate();if(appStorageSettings!=null)appStorageSettings.invalidate();if(appLanguageSettings!=null)appLanguageSettings.invalidate();if(captionLanguageSettings!=null)captionLanguageSettings.invalidate();if(systemLanguageSettings!=null)systemLanguageSettings.invalidate();if(keyboardSettings!=null)keyboardSettings.invalidate();emitUiMode();});
    }
    @Override public boolean onActivityResult(int request,int result,Intent data) {return widgets.onActivityResult(request,result,data);}
    @Override public boolean onBackPressed() {return replyComposer.close()||widgets.hide();}
    private void sendSettingsEntry(SettingsEntryContract.Entry entry) {
        if(entry!=null) {
            JSONObject packet=json("schema",1,"id",entry.id,"route",entry.route);
            if(entry.packageName!=null)try{packet.put("package",entry.packageName);}catch(Exception impossible){return;}
            emit("settings.entry",packet);
        }
    }
    @Override public void onIntent(Intent intent) {
        if(intent!=null) {
            try {
                String action=intent.getAction();
                String requested=null;boolean valid=true;
                if(SettingsEntryContract.ACTION.equals(action)&&intent.hasExtra(SettingsEntryContract.EXTRA_ROUTE)) {
                    Object value=intent.getExtras().get(SettingsEntryContract.EXTRA_ROUTE);
                    valid=value instanceof String;if(valid)requested=(String)value;
                }
                final String packageName=SettingsEntryContract.APP_NOTIFICATIONS.equals(action)
                        ?SettingsEntryContract.packageSelector(intent.hasExtra(android.provider.Settings.EXTRA_APP_PACKAGE)
                            ?intent.getExtras().get(android.provider.Settings.EXTRA_APP_PACKAGE):null):null;
                String route=SettingsEntryContract.APP_NOTIFICATIONS.equals(action)
                        ?(packageName!=null?"app_notifications":null):valid?SettingsEntryContract.route(action,requested):null;
                if(route!=null&&intent.getData()==null&&intent.getType()==null&&intent.getSelector()==null) {
                    replyComposer.close();widgets.hide();closePlacementMenu();homeGeometry.invalidate();
                    long id=NEXT_SETTINGS_ENTRY.getAndUpdate(value -> value==Long.MAX_VALUE?value:value+1);
                    if(id>0&&id<Long.MAX_VALUE)offer(() -> sendSettingsEntry(settingsEntryDelivery.stage(id,route,packageName)));
                }
            }catch(RuntimeException malformed) { /* Untrusted Intent extras never become a host command. */ }
        }
        if(intent!=null&&Intent.ACTION_MAIN.equals(intent.getAction())&&intent.hasCategory(Intent.CATEGORY_HOME))
            offer(() -> {settingsEntryDelivery.cancel();outbound.remove("settings.entry");});
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
        offer(()->{if(captionLanguageSettings!=null)captionLanguageSettings.invalidate();if(systemLanguageSettings!=null)systemLanguageSettings.invalidate();if(keyboardSettings!=null)keyboardSettings.invalidate();});
        if(captionCustomSettings!=null)captionCustomSettings.retireInBackground();
        android.view.ViewTreeObserver focusObserver=activity.getWindow().getDecorView().getViewTreeObserver();
        if(focusObserver.isAlive())focusObserver.removeOnWindowFocusChangeListener(settingsFocusListener);
        settingsAccessibility.destroy();
        if(accessibilityPreferences!=null)accessibilityPreferences.close();
        synchronized(this) {
            destroyed=true;
            if(wifiSettings!=null) wifiSettings.close();
            if(bluetoothSettings!=null) bluetoothSettings.close();
            if(soundsSettings!=null) {SoundsSettingsClient closing=soundsSettings;Thread cleanup=new Thread(closing::close,"OctoSenseSoundStop");cleanup.setDaemon(true);cleanup.start();}
        }
        closePlacementMenu();
        replyComposer.close();
        homeGeometry.onDestroy();
        widgets.onDestroy();
        if(agent!=null) {if(soundsSettings!=null)agent.stopSoundInBackground();agent.unbind();}
        if(validationRemote!=null) try {validationRemote.close();} catch(java.io.IOException ignored) {}
        launcher.unregisterCallback(packageCallback);
        activity.unregisterReceiver(profileCallback);
        if(bound) { activity.unbindService(bridgeConnection); bound=false; }
        // The service removes subscriptions on final unbind, even if this
        // activity is recreated while the Home process and Binder remain alive.
        destroyed=true; main.removeCallbacksAndMessages(null); worker.shutdownNow();
    }
}
