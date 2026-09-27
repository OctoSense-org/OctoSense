package dev.makepad.octosense.agent;

import android.app.ActivityManager;
import android.app.ActivityTaskManager;
import android.app.KeyguardManager;
import android.app.Service;
import android.app.StatusBarManager;
import android.content.Context;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.graphics.Bitmap;
import android.graphics.Rect;
import android.hardware.input.InputManager;
import android.os.Binder;
import android.os.Bundle;
import android.os.IBinder;
import android.os.Process;
import android.os.RemoteException;
import android.os.SystemClock;
import android.provider.Settings;
import android.util.Log;
import android.view.Display;
import android.view.InputDevice;
import android.view.KeyCharacterMap;
import android.view.KeyEvent;
import android.view.MotionEvent;
import android.view.WindowManagerGlobal;
import android.window.ScreenCapture;
import android.window.TaskSnapshot;

import java.io.ByteArrayOutputStream;
import java.io.FileDescriptor;
import java.io.PrintWriter;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;

/**
 * The agent platform (docs/agent-service.md): the framework's privileged APIs
 * behind one Binder surface. The process is persistent and platform-signed;
 * every call is gated by {@link #caller(String)} and written to the audit log.
 *
 * A test harness for the bench phone is the dump entry point:
 *   adb shell dumpsys activity service dev.makepad.octosense.agent/.AgentPlatformService <command>
 * (shell holds DUMP; the commands mirror the Binder methods).
 */
public class AgentPlatformService extends Service {
    static final String TAG = "OctoSenseAgent";
    static final int PROTOCOL_VERSION = 1;

    /** Packages that may call, besides signature-matched ones. */
    static final List<String> ALLOWED_PACKAGES = Arrays.asList(
            "dev.makepad.octosense",
            "dev.makepad.octosense.bridge",
            "dev.makepad.octosense.quickstep");

    private final ArrayDeque<String> audit = new ArrayDeque<>();
    private ActivityTaskManager atm;
    private InputManager input;
    private KeyguardManager keyguard;
    private StatusBarManager statusBar;
    private dev.makepad.octosense.wifi.WifiSettingsBackend wifiSettings;
    private dev.makepad.octosense.controls.SettingsControlsBackend settingsControls;
    private SensorSettingsClient sensorSettings;
    private ZenSettingsClient zenSettings;
    private AppNetworkClient appNetworkSettings;
    private AppBatteryClient appBatterySettings;
    private AppStorageClient appStorageSettings;
    private AppLanguageClient appLanguageSettings;
    private SystemLanguageClient systemLanguageSettings;
    private KeyboardClient keyboardSettings;
    private CaptionCustomSessions captionCustomSettings;
    private CaptionLocalePlatformSettings captionLanguageSettings;
    private AppNotificationsClient appNotifications;
    private RolesSettingsClient rolesSettings;
    private PermissionsSettingsClient permissionsSettings;
    private DateTimePlatformSettings dateTimeSettings;
    private dev.makepad.octosense.display.DisplaySettingsBackend displaySettings;
    private NotificationHistorySettings notificationHistory;
    private dev.makepad.octosense.sounds.SoundSettingsBackend soundSettings;
    private AccountSettingsClient accountSettings;
    private dev.makepad.octosense.bluetooth.BluetoothSettingsBackend bluetoothSettings;
    private dev.makepad.octosense.network.NetworkSettingsBackend networkSettings;

    @Override public void onCreate() {
        super.onCreate();
        atm = ActivityTaskManager.getInstance();
        input = getSystemService(InputManager.class);
        keyguard = getSystemService(KeyguardManager.class);
        statusBar = getSystemService(StatusBarManager.class);
        wifiSettings = new dev.makepad.octosense.wifi.WifiSettingsBackend(this,true);
        bluetoothSettings=new dev.makepad.octosense.bluetooth.BluetoothSettingsBackend(this,true,new BluetoothPlatformSettings());
        networkSettings=new dev.makepad.octosense.network.NetworkSettingsBackend(this,true,new NetworkPlatformSettings(this));
        sensorSettings=new SensorSettingsClient(this);
        zenSettings=new ZenSettingsClient(this);
        appNetworkSettings=new AppNetworkClient(this);
        appBatterySettings=new AppBatteryClient(this);
        appStorageSettings=new AppStorageClient(this);
        appLanguageSettings=new AppLanguageClient(this);
        systemLanguageSettings=new SystemLanguageClient(this);
        keyboardSettings=new KeyboardClient(this);
        captionCustomSettings=new CaptionCustomSessions(this);
        captionLanguageSettings=new CaptionLocalePlatformSettings(this);
        appNotifications=new AppNotificationsClient(this);
        rolesSettings=new RolesSettingsClient(this);
        permissionsSettings=new PermissionsSettingsClient(this);
        dateTimeSettings=new DateTimePlatformSettings(this);
        displaySettings=new dev.makepad.octosense.display.DisplaySettingsBackend(this,new DisplayPlatformSettings(this),() -> ActivityManager.getCurrentUser()==0&&getSystemService(android.os.UserManager.class).isAdminUser());
        notificationHistory=new NotificationHistorySettings(this);
        soundSettings=new dev.makepad.octosense.sounds.SoundSettingsBackend(this,() -> ActivityManager.getCurrentUser()==0);
        accountSettings=new AccountSettingsClient(this);
        settingsControls = new dev.makepad.octosense.controls.SettingsControlsBackend(this,true,new SettingsPlatformControls(this,sensorSettings,zenSettings));
        Log.i(TAG, "agent platform up, uid " + Process.myUid());
    }

    @Override public IBinder onBind(Intent intent) { return binder; }
    @Override public void onDestroy() {
        if(notificationHistory!=null) notificationHistory.close();
        if(soundSettings!=null) soundSettings.close();
        if(wifiSettings!=null) wifiSettings.close();
        if(sensorSettings!=null) sensorSettings.close();
        if(zenSettings!=null) zenSettings.close();
        if(accountSettings!=null) accountSettings.close();
        if(appNotifications!=null) appNotifications.close();
        if(rolesSettings!=null) rolesSettings.close();
        if(permissionsSettings!=null) permissionsSettings.close();
        if(appNetworkSettings!=null)appNetworkSettings.close();
        if(appBatterySettings!=null)appBatterySettings.close();
        if(appStorageSettings!=null)appStorageSettings.close();
        if(appLanguageSettings!=null)appLanguageSettings.close();
        if(systemLanguageSettings!=null)systemLanguageSettings.close();
        if(keyboardSettings!=null)keyboardSettings.close();
        if(captionCustomSettings!=null)captionCustomSettings.destroy();
        if(captionLanguageSettings!=null)captionLanguageSettings.invalidate();
        if(bluetoothSettings!=null) bluetoothSettings.close();
        super.onDestroy();
    }
    @Override public int onStartCommand(Intent intent, int flags, int startId) { return START_STICKY; }

    // ---- gate -------------------------------------------------------------

    static final class Denied extends SecurityException {
        Denied(String why) { super(why); }
    }

    /** Signature match with this (platform-signed) package and a known package name. */
    private String caller(String what) {
        int uid = Binder.getCallingUid();
        if (uid == Process.myUid() || uid == Process.SYSTEM_UID) return "system:" + what;
        PackageManager pm = getPackageManager();
        String[] pkgs = pm.getPackagesForUid(uid);
        String pkg = pkgs == null || pkgs.length == 0 ? "uid:" + uid : pkgs[0];
        boolean signed = pm.checkSignatures(uid, Process.myUid()) == PackageManager.SIGNATURE_MATCH;
        boolean listed = pkgs != null && Arrays.stream(pkgs).anyMatch(ALLOWED_PACKAGES::contains);
        if (!signed || !listed) {
            record(pkg, what, "denied");
            throw new Denied(pkg + " may not call " + what);
        }
        return pkg + ":" + what;
    }

    private synchronized void record(String who, String what, String outcome) {
        audit.addFirst(SystemClock.elapsedRealtime() + " " + who + " " + what + " " + outcome);
        while (audit.size() > 200) audit.removeLast();
    }

    private static Bundle ok() { Bundle b = new Bundle(); b.putBoolean("ok", true); return b; }
    private static Bundle fail(String reason) {
        Bundle b = new Bundle(); b.putBoolean("ok", false); b.putString("reason", reason); return b;
    }

    interface Op { Bundle run() throws Exception; }

    /** Runs an operation with the caller's identity cleared, records the outcome. */
    private Bundle guarded(String what, Op op) {
        String who = caller(what);
        long token = Binder.clearCallingIdentity();
        try {
            Bundle r = op.run();
            record(who, what, r.getBoolean("ok") ? "ok" : r.getString("reason", "failed"));
            return r;
        } catch (Exception e) {
            Log.w(TAG, what + " failed", e);
            record(who, what, "failed " + e.getClass().getSimpleName());
            return fail("failed");
        } finally {
            Binder.restoreCallingIdentity(token);
        }
    }

    // ---- capabilities -----------------------------------------------------

    Bundle capabilities() {
        Bundle b = ok();
        b.putInt("protocol", PROTOCOL_VERSION);
        ArrayList<String> caps = new ArrayList<>();
        if (has("android.permission.REAL_GET_TASKS")) caps.add("tasks");
        if (has("android.permission.READ_FRAME_BUFFER")) caps.add("screen");
        if (has("android.permission.INJECT_EVENTS")) caps.add("input");
        if (has("android.permission.WRITE_SECURE_SETTINGS")) caps.add("settings");
        if (has("android.permission.WRITE_SECURE_SETTINGS")) caps.add("settings_controls_v1");
        if (has("android.permission.WRITE_SECURE_SETTINGS")) caps.add("caption_custom_v1");
        if (has("android.permission.WRITE_SECURE_SETTINGS")) caps.add("caption_language_v1");
        caps.add("accounts_settings_v1");
        if(has("android.permission.MANAGE_TIME_AND_ZONE_DETECTION")) caps.add("date_time_settings_v1");
        if(has("android.permission.ACCESS_NOTIFICATIONS")) caps.add("notification_history_v1");
        caps.add("sounds_settings_v1");
        caps.add("display_settings_v1");
        caps.add("app_notifications_v1");
        caps.add("roles_settings_v1");
        caps.add("permissions_settings_v1");
        caps.add("dnd_settings_v1");
        caps.add("app_network_settings_v1");
        caps.add("app_battery_v1");
        caps.add("app_storage_v1");
        caps.add("app_language_v1");
        caps.add("system_languages_v1");
        caps.add("keyboards_v1");
        if(has("android.permission.NETWORK_SETTINGS")) caps.add("network_settings_v1");
        if (has("android.permission.BLUETOOTH_PRIVILEGED") && has("android.permission.BLUETOOTH_CONNECT")
                && has("android.permission.BLUETOOTH_SCAN") && has("android.permission.MODIFY_PHONE_STATE")) caps.add("bluetooth_settings_v1");
        if (has("android.permission.NETWORK_SETTINGS") && has("android.permission.ACCESS_WIFI_STATE")
                && has("android.permission.CHANGE_WIFI_STATE")) caps.add("wifi_settings_v1");
        if (has("android.permission.START_TASKS_FROM_RECENTS")) caps.add("apps");
        if (has("android.permission.STATUS_BAR")) caps.add("statusbar");
        if (has("android.permission.REBOOT") && has("android.permission.INSTALL_PACKAGES")) caps.add("update");
        if (has("android.permission.REBOOT") && has("android.permission.INSTALL_PACKAGES")) caps.add("updates_settings_v1");
        // "tree" (the system-wide accessibility node tree) needs an
        // AccessibilityService; it lands with the ShellAccessibility bridge.
        b.putStringArrayList("capabilities", caps);
        return b;
    }

    private boolean has(String permission) {
        return checkSelfPermission(permission) == PackageManager.PERMISSION_GRANTED;
    }

    // ---- tasks ------------------------------------------------------------

    Bundle tasks(int max) {
        List<ActivityManager.RunningTaskInfo> infos = atm.getTasks(Math.max(1, Math.min(max, 64)));
        ArrayList<Bundle> out = new ArrayList<>();
        for (ActivityManager.RunningTaskInfo t : infos) {
            Bundle b = new Bundle();
            b.putInt("id", t.taskId);
            if (t.baseActivity != null) {
                b.putString("package", t.baseActivity.getPackageName());
                b.putString("activity", t.baseActivity.flattenToShortString());
            }
            if (t.topActivity != null) b.putString("top", t.topActivity.flattenToShortString());
            b.putBoolean("visible", t.isVisible());
            b.putBoolean("running", t.isRunning);
            b.putLong("lastActive", t.lastActiveTime);
            b.putString("label", label(t.baseActivity == null ? null : t.baseActivity.getPackageName()));
            out.add(b);
        }
        Bundle r = ok();
        r.putParcelableArrayList("tasks", out);
        return r;
    }

    private String label(String pkg) {
        if (pkg == null) return "";
        try {
            PackageManager pm = getPackageManager();
            return String.valueOf(pm.getApplicationLabel(pm.getApplicationInfo(pkg, 0)));
        } catch (PackageManager.NameNotFoundException e) {
            return pkg;
        }
    }

    Bundle taskSnapshot(int taskId, int maxWidth) throws RemoteException {
        TaskSnapshot snap = ActivityTaskManager.getService().getTaskSnapshot(taskId, false);
        if (snap == null || snap.getHardwareBuffer() == null) return fail("unavailable");
        Bitmap bmp = Bitmap.wrapHardwareBuffer(snap.getHardwareBuffer(), snap.getColorSpace());
        if (bmp == null) return fail("unavailable");
        return png(bmp.copy(Bitmap.Config.ARGB_8888, false), maxWidth);
    }

    // ---- screen -----------------------------------------------------------

    Bundle screen(int maxWidth) throws RemoteException {
        ScreenCapture.CaptureArgs args = new ScreenCapture.CaptureArgs.Builder<>().build();
        ScreenCapture.SynchronousScreenCaptureListener sync = ScreenCapture.createSyncCaptureListener();
        WindowManagerGlobal.getWindowManagerService().captureDisplay(Display.DEFAULT_DISPLAY, args, sync);
        ScreenCapture.ScreenshotHardwareBuffer buffer = sync.getBuffer();
        if (buffer == null) return fail("unavailable");
        Bitmap bmp = buffer.asBitmap();
        if (bmp == null) return fail("unavailable");
        return png(bmp.copy(Bitmap.Config.ARGB_8888, false), maxWidth);
    }

    private static Bundle png(Bitmap bmp, int maxWidth) {
        if (maxWidth > 0 && bmp.getWidth() > maxWidth) {
            int h = Math.max(1, bmp.getHeight() * maxWidth / bmp.getWidth());
            bmp = Bitmap.createScaledBitmap(bmp, maxWidth, h, true);
        }
        ByteArrayOutputStream bytes = new ByteArrayOutputStream();
        bmp.compress(Bitmap.CompressFormat.PNG, 100, bytes);
        Bundle r = ok();
        r.putByteArray("png", bytes.toByteArray());
        r.putInt("width", bmp.getWidth());
        r.putInt("height", bmp.getHeight());
        return r;
    }

    // ---- input ------------------------------------------------------------

    private Bundle inputAllowed() {
        // The agent never acts on the lock screen.
        if (keyguard.isKeyguardLocked()) return fail("keyguard");
        return null;
    }

    Bundle tap(float x, float y) {
        Bundle no = inputAllowed(); if (no != null) return no;
        long t = SystemClock.uptimeMillis();
        boolean a = inject(motion(t, t, MotionEvent.ACTION_DOWN, x, y));
        boolean b = inject(motion(t, t + 40, MotionEvent.ACTION_UP, x, y));
        return a && b ? ok() : fail("failed");
    }

    Bundle swipe(float x0, float y0, float x1, float y1, int durationMs) {
        Bundle no = inputAllowed(); if (no != null) return no;
        int duration = Math.max(16, Math.min(durationMs, 5000));
        long t0 = SystemClock.uptimeMillis();
        if (!inject(motion(t0, t0, MotionEvent.ACTION_DOWN, x0, y0))) return fail("failed");
        int steps = Math.max(2, duration / 8);
        for (int i = 1; i <= steps; i++) {
            float f = (float) i / steps;
            long t = t0 + (long) (duration * f);
            while (SystemClock.uptimeMillis() < t) SystemClock.sleep(1);
            inject(motion(t0, t, MotionEvent.ACTION_MOVE, x0 + (x1 - x0) * f, y0 + (y1 - y0) * f));
        }
        long tEnd = SystemClock.uptimeMillis();
        return inject(motion(t0, tEnd, MotionEvent.ACTION_UP, x1, y1)) ? ok() : fail("failed");
    }

    Bundle typeText(String text) {
        Bundle no = inputAllowed(); if (no != null) return no;
        if (text == null) return fail("failed");
        KeyCharacterMap map = KeyCharacterMap.load(KeyCharacterMap.VIRTUAL_KEYBOARD);
        KeyEvent[] events = map.getEvents(text.toCharArray());
        if (events == null) return fail("unavailable");
        for (KeyEvent e : events) {
            if (!inject(KeyEvent.changeTimeRepeat(e, SystemClock.uptimeMillis(), 0))) return fail("failed");
        }
        return ok();
    }

    Bundle pressKey(int keyCode) {
        Bundle no = inputAllowed(); if (no != null) return no;
        long t = SystemClock.uptimeMillis();
        boolean a = inject(key(t, KeyEvent.ACTION_DOWN, keyCode));
        boolean b = inject(key(t, KeyEvent.ACTION_UP, keyCode));
        return a && b ? ok() : fail("failed");
    }

    private static MotionEvent motion(long down, long now, int action, float x, float y) {
        MotionEvent e = MotionEvent.obtain(down, now, action, x, y, 0);
        e.setSource(InputDevice.SOURCE_TOUCHSCREEN);
        return e;
    }

    private static KeyEvent key(long t, int action, int code) {
        return new KeyEvent(t, t, action, code, 0, 0, KeyCharacterMap.VIRTUAL_KEYBOARD, 0,
                0, InputDevice.SOURCE_KEYBOARD);
    }

    private boolean inject(android.view.InputEvent e) {
        return input.injectInputEvent(e, InputManager.INJECT_INPUT_EVENT_MODE_WAIT_FOR_FINISH);
    }

    // ---- settings ---------------------------------------------------------

    Bundle getSetting(String table, String name) {
        String value;
        switch (String.valueOf(table)) {
            case "secure": value = Settings.Secure.getString(getContentResolver(), name); break;
            case "system": value = Settings.System.getString(getContentResolver(), name); break;
            case "global": value = Settings.Global.getString(getContentResolver(), name); break;
            default: return fail("failed");
        }
        Bundle r = ok();
        r.putString("value", value);
        return r;
    }

    Bundle putSetting(String table, String name, String value) {
        boolean done;
        switch (String.valueOf(table)) {
            case "secure": done = Settings.Secure.putString(getContentResolver(), name, value); break;
            case "system": done = Settings.System.putString(getContentResolver(), name, value); break;
            case "global": done = Settings.Global.putString(getContentResolver(), name, value); break;
            default: return fail("failed");
        }
        return done ? ok() : fail("failed");
    }

    // ---- apps -------------------------------------------------------------

    Bundle launch(Intent intent) {
        if (intent == null) return fail("failed");
        intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
        startActivity(intent);
        return ok();
    }

    Bundle startTask(int taskId) throws RemoteException {
        int result = ActivityTaskManager.getService().startActivityFromRecents(taskId, null);
        return result >= 0 ? ok() : fail("unavailable");
    }

    Bundle removeTask(int taskId) throws RemoteException {
        return ActivityTaskManager.getService().removeTask(taskId) ? ok() : fail("unavailable");
    }

    Bundle forceStop(String pkg) {
        if (pkg == null || pkg.startsWith("dev.makepad.octosense")) return fail("denied");
        getSystemService(ActivityManager.class).forceStopPackage(pkg);
        return ok();
    }

    // ---- statusbar --------------------------------------------------------

    Bundle expandNotifications() { statusBar.expandNotificationsPanel(); return ok(); }
    Bundle expandQuickSettings() { statusBar.expandSettingsPanel(); return ok(); }
    Bundle collapsePanels() { statusBar.collapsePanels(); return ok(); }

    // ---- update -----------------------------------------------------------

    private static Updater updater() { return AgentApplication.get().updater(); }

    /** "rom", "home" or "all": starts in the background and returns at once. */
    Bundle applyUpdateAsync(String part) {
        String what = part == null ? "all" : part;
        if(!Arrays.asList("rom","home","all").contains(what)) return fail("invalid_update_part");
        AgentApplication.get().work().execute(() -> {
            try {
                Bundle check = updater().check();
                if (!what.equals("rom") && check.getBoolean("home_newer")) updater().applyHome();
                if (!what.equals("home") && check.getBoolean("rom_newer")) updater().applyRom();
            } catch (Exception e) {
                Log.w(TAG, "update apply failed", e);
            }
        });
        return ok();
    }

    synchronized Bundle auditLog(int max) {
        Bundle r = ok();
        ArrayList<String> lines = new ArrayList<>();
        for (String s : audit) { if (lines.size() >= Math.max(1, max)) break; lines.add(s); }
        r.putStringArrayList("log", lines);
        return r;
    }

    // ---- Binder -----------------------------------------------------------

    private Bundle wifiGuarded(String operation,Op op) {
        // This persistent helper is created for its Android user. Until a
        // per-user service binding exists, refuse another foreground user;
        // never leak owner profiles into a secondary user's Settings page.
        int callerUser=android.os.UserHandle.getUserId(Binder.getCallingUid());
        int ownUser=android.os.UserHandle.myUserId();
        if(callerUser!=ownUser) {caller(operation);return fail("wrong_user");}
        return guarded(operation,() -> {
            if(ActivityManager.getCurrentUser()!=ownUser||keyguard.isKeyguardLocked()) return fail("keyguard");
            return op.run();
        });
    }
    private Bundle dndResult(Bundle result) {
        if(result!=null)return result;
        Bundle unavailable=new Bundle();unavailable.putBoolean("ok",false);unavailable.putString("reason","dnd_unavailable");return unavailable;
    }
    private Bundle soundResult(String reason) {
        Bundle result=ok();result.putBoolean("ok",Arrays.asList("sound_applied","sound_preview_started","sound_preview_requested","sound_silent","sound_stopped").contains(reason));
        result.putString("reason",reason);return result;
    }
    private Bundle wifiResult(String reason) {
        Bundle result=ok();
        boolean accepted=reason.equals("wifi_requested")||reason.equals("wifi_scan_requested");
        result.putBoolean("ok",accepted);result.putString("reason",reason);return result;
    }
    private Bundle updateResult(String reason) {
        Bundle result=ok();result.putBoolean("ok",Arrays.asList("update_check_requested","update_install_requested","update_reboot_requested").contains(reason));
        result.putString("reason",reason);return result;
    }
    private Bundle networkResult(String reason) {
        Bundle result=ok();result.putBoolean("ok",reason.equals("network_applied")||reason.equals("network_requested"));result.putString("reason",reason);return result;
    }

    private Bundle accountJson(String json) {
        if(json==null) return fail("accounts_unavailable");Bundle result=ok();result.putString("json",json);return result;
    }
    private Bundle syncResult(String reason) {
        Bundle result=ok();result.putString("reason",reason);result.putBoolean("ok",reason.equals("sync_applied")||reason.equals("sync_requested"));return result;
    }
    private Bundle accountFlow(android.app.PendingIntent pending) {
        if(pending==null) return fail("account_target_changed");Bundle result=ok();result.putParcelable("pending",pending);return result;
    }
    private Bundle bluetoothResult(String reason) {
        Bundle result=ok();result.putString("reason",reason);
        result.putBoolean("ok",reason.equals("bluetooth_requested")||reason.equals("bluetooth_name_applied")||reason.equals("bluetooth_sharing_applied"));return result;
    }
    private final IAgentPlatform.Stub binder = new IAgentPlatform.Stub() {
        @Override public Bundle getNetworkSnapshot(long id) {return wifiGuarded("networkSnapshot",() -> {
            Bundle result=ok();result.putString("json",networkSettings.snapshot(id).toString());return result;
        });}
        @Override public Bundle networkAirplane(String key,boolean enabled) {return wifiGuarded("networkAirplane",() -> networkResult(networkSettings.airplane(key,enabled)));}
        @Override public Bundle networkDataSaver(String key,boolean enabled) {return wifiGuarded("networkDataSaver",() -> networkResult(networkSettings.dataSaver(key,enabled)));}
        @Override public Bundle networkPrivateDns(String key,String mode,String hostname) {return wifiGuarded("networkPrivateDns",() -> networkResult(networkSettings.privateDns(key,mode,hostname)));}
        @Override public Bundle getUpdatesSnapshot(long id) {return wifiGuarded("updatesSnapshot",() -> {
            Bundle result=ok();result.putString("json",AgentApplication.get().updateSettings().snapshot(id).toString());return result;
        });}
        @Override public Bundle checkReviewedUpdates() {return wifiGuarded("updatesCheck",() -> updateResult(AgentApplication.get().updateSettings().check()));}
        @Override public Bundle installReviewedUpdate(String key,String part) {return wifiGuarded("updatesInstall",() -> updateResult(AgentApplication.get().updateSettings().install(key,part)));}
        @Override public Bundle rebootReviewedUpdate(String key) {return wifiGuarded("updatesReboot",() -> updateResult(AgentApplication.get().updateSettings().reboot(key)));}
        @Override public Bundle getAccountsSnapshot(long id) {return wifiGuarded("accountsSnapshot",() -> accountJson(accountSettings.snapshot(id)));}
        @Override public Bundle getAccountDetails(long id,String key) {return wifiGuarded("accountDetails",() -> accountJson(accountSettings.details(id,key)));}
        @Override public Bundle setAccountsMasterSync(boolean value) {return wifiGuarded("accountsMasterSync",() -> syncResult(accountSettings.master(value)));}
        @Override public Bundle accountSync(String key,String authority,String action,boolean value) {return wifiGuarded("accountSync",() -> {
            dev.makepad.octosense.accounts.AccountsSettingsContract.SyncAction parsed=dev.makepad.octosense.accounts.AccountsSettingsContract.SyncAction.parse(action);
            return syncResult(accountSettings.sync(key,authority,parsed,parsed==dev.makepad.octosense.accounts.AccountsSettingsContract.SyncAction.AUTO?value:null));
        });}
        @Override public Bundle accountAddition(String key) {return wifiGuarded("accountAddition",() -> accountFlow(accountSettings.addition(key)));}
        @Override public Bundle accountRemoval(String key) {return wifiGuarded("accountRemoval",() -> accountFlow(accountSettings.removal(key)));}
        @Override public Bundle getBluetoothSnapshot(long id) {return wifiGuarded("bluetoothSnapshot",() -> {
            Bundle result=ok();result.putString("json",bluetoothSettings.snapshot(id).toString());return result;
        });}
        @Override public Bundle setBluetoothEnabled(boolean value) {return wifiGuarded("bluetoothEnabled",() -> bluetoothResult(bluetoothSettings.enabled(value)));}
        @Override public Bundle scanBluetooth(boolean value) {return wifiGuarded("bluetoothScan",() -> bluetoothResult(bluetoothSettings.scan(value)));}
        @Override public Bundle setBluetoothName(String name) {return wifiGuarded("bluetoothName",() -> bluetoothResult(bluetoothSettings.name(name)));}
        @Override public Bundle bluetoothDevice(String key,String action) {return wifiGuarded("bluetoothDevice",() ->
            bluetoothResult(bluetoothSettings.device(key,dev.makepad.octosense.bluetooth.BluetoothSettingsContract.Action.parse(action))));}
        @Override public Bundle bluetoothSharing(String key,String kind,String value) {return wifiGuarded("bluetoothSharing",() ->
            bluetoothResult(bluetoothSettings.sharing(key,kind,value)));}
        @Override public Bundle getCapabilities() { return guarded("capabilities", AgentPlatformService.this::capabilities); }
        @Override public Bundle getTasks(int max) { return guarded("tasks", () -> tasks(max)); }
        @Override public Bundle getTaskSnapshot(int id, int w) { return guarded("taskSnapshot", () -> taskSnapshot(id, w)); }
        @Override public Bundle captureScreen(int w) { return guarded("screen", () -> screen(w)); }
        @Override public Bundle tap(float x, float y) { return guarded("tap", () -> AgentPlatformService.this.tap(x, y)); }
        @Override public Bundle swipe(float x0, float y0, float x1, float y1, int d) { return guarded("swipe", () -> AgentPlatformService.this.swipe(x0, y0, x1, y1, d)); }
        @Override public Bundle typeText(String t) { return guarded("type", () -> AgentPlatformService.this.typeText(t)); }
        @Override public Bundle pressKey(int k) { return guarded("key", () -> AgentPlatformService.this.pressKey(k)); }
        @Override public Bundle getSetting(String t, String n) { return guarded("getSetting", () -> AgentPlatformService.this.getSetting(t, n)); }
        @Override public Bundle putSetting(String t, String n, String v) { return guarded("putSetting", () -> AgentPlatformService.this.putSetting(t, n, v)); }
        @Override public Bundle startActivity(Intent i) { return guarded("startActivity", () -> launch(i)); }
        @Override public Bundle startTask(int id) { return guarded("startTask", () -> AgentPlatformService.this.startTask(id)); }
        @Override public Bundle removeTask(int id) { return guarded("removeTask", () -> AgentPlatformService.this.removeTask(id)); }
        @Override public Bundle forceStop(String p) { return guarded("forceStop", () -> AgentPlatformService.this.forceStop(p)); }
        @Override public Bundle expandNotifications() { return guarded("expandNotifications", AgentPlatformService.this::expandNotifications); }
        @Override public Bundle expandQuickSettings() { return guarded("expandQuickSettings", AgentPlatformService.this::expandQuickSettings); }
        @Override public Bundle collapsePanels() { return guarded("collapsePanels", AgentPlatformService.this::collapsePanels); }
        @Override public Bundle getAuditLog(int max) { return guarded("audit", () -> auditLog(max)); }
        @Override public Bundle checkUpdate() { return guarded("updateCheck", () -> updater().check()); }
        @Override public Bundle applyUpdate(String part) { return guarded("updateApply", () -> applyUpdateAsync(part)); }
        @Override public Bundle getUpdateStatus() { return guarded("updateStatus", () -> updater().status()); }
        @Override public Bundle rebootToUpdate() { return guarded("updateReboot", () -> updater().reboot()); }
        @Override public Bundle getWifiSnapshot(long id) {return wifiGuarded("wifiSnapshot",() -> {
            Bundle result=ok();result.putString("json",wifiSettings.snapshot(id).toString());return result;
        });}
        @Override public Bundle setWifiEnabled(boolean enabled) {
            return wifiGuarded("wifiEnabled",() -> wifiResult(wifiSettings.setEnabled(enabled)));
        }
        @Override public Bundle scanWifi() {return wifiGuarded("wifiScan",() -> wifiResult(wifiSettings.scan()));}
        @Override public Bundle wifiNetwork(String key,String action) {return wifiGuarded("wifiNetwork",() ->
                wifiResult(wifiSettings.network(key,dev.makepad.octosense.wifi.WifiSettingsContract.Action.parse(action))));}
        @Override public Bundle wifiConfiguration(String key) {return wifiGuarded("wifiConfiguration",() -> {
            Intent intent=wifiSettings.configureObserved(key);
            if(intent==null) return fail("wifi_target_changed");
            Bundle result=ok();result.putParcelable("intent",intent);return result;
        });}
        @Override public Bundle getControlsSnapshot(long id,String page) {return wifiGuarded("controlsSnapshot",() -> {
            Bundle result=ok();result.putString("json",settingsControls.snapshot(id,
                    dev.makepad.octosense.controls.SettingsControlsContract.Page.parse(page)).toString());return result;
        });}
        @Override public Bundle setControl(String page,String control,String value) {return wifiGuarded("setControl",() -> {
            dev.makepad.octosense.controls.SettingsControlsContract.Page parsed=dev.makepad.octosense.controls.SettingsControlsContract.Page.parse(page);
            dev.makepad.octosense.controls.SettingsControlsContract.Control selected=dev.makepad.octosense.controls.SettingsControlsContract.Control.parse(parsed,control);
            selected.validate(value);
            if(selected==dev.makepad.octosense.controls.SettingsControlsContract.Control.NOTIFICATION_HISTORY) notificationHistory.invalidate();
            String reason=settingsControls.apply(parsed,selected,value);
            Bundle result=ok();result.putBoolean("ok",reason.equals("control_applied")||reason.equals("control_requested"));result.putString("reason",reason);return result;
        });}
        @Override public Bundle getCaptionCustomSnapshot(long id,IBinder owner,String session,long visit) {
            final int pid=Binder.getCallingPid();return wifiGuarded("captionCustomSnapshot",()->{Bundle result=ok();result.putString("json",captionCustomSettings.snapshot(id,owner,session,visit,pid).toString());return result;});
        }
        @Override public Bundle setCaptionCustom(IBinder owner,String session,long visit,String field,String value) {
            final int pid=Binder.getCallingPid();return wifiGuarded("captionCustomSet",()->{String reason=captionCustomSettings.apply(owner,session,visit,field,value,pid);Bundle result=ok();result.putBoolean("ok",reason.equals("control_applied")||reason.equals("control_requested"));result.putString("reason",reason);return result;});
        }
        @Override public Bundle closeCaptionCustom(IBinder owner,String session,long visit) {
            final int pid=Binder.getCallingPid();return guarded("captionCustomClose",()->{captionCustomSettings.close(owner,session,visit,pid);return ok();});
        }
        @Override public Bundle getCaptionLanguageSnapshot(long id,String key,String query,int offset){return wifiGuarded("captionLanguageSnapshot",()->captionLanguageSettings.snapshot(id,key,query,offset));}
        @Override public Bundle setCaptionLanguage(String key,String choice){return wifiGuarded("captionLanguageSet",()->{String reason=captionLanguageSettings.select(key,choice);Bundle result=ok();result.putBoolean("ok",reason.equals("control_applied")||reason.equals("control_requested"));result.putString("reason",reason);return result;});}
        @Override public Bundle getDateTimeSnapshot(long id) {return wifiGuarded("dateTimeSnapshot",() -> {
            Bundle result=ok();result.putString("json",dateTimeSettings.snapshot(id).toString());return result;
        });}
        @Override public Bundle getSoundsSnapshot(long id,String type,String key,int offset) {return wifiGuarded("soundsSnapshot",() -> {
            Bundle result=ok();result.putString("json",soundSettings.snapshot(id,type,key,offset).toString());return result;
        });}
        @Override public Bundle previewSound(String type,String key,String target) {return wifiGuarded("soundPreview",() -> soundResult(soundSettings.preview(type,key,target)));}
        @Override public Bundle saveSound(String type,String key,String target) {return wifiGuarded("soundSave",() -> soundResult(soundSettings.save(type,key,target)));}
        @Override public Bundle getAppNotifications(long id,String pkg,int offset,String generation) {return wifiGuarded("appNotificationsSnapshot",() -> {
            Bundle snapshot=appNotifications.snapshot(id,pkg,offset,generation);if(snapshot!=null)return snapshot;
            Bundle unavailable=ok();unavailable.putString("json",dev.makepad.octosense.notifications.AppNotificationsContract.unavailable(id,pkg,"unavailable").toString());return unavailable;
        });}
        @Override public Bundle getRolesSnapshot(long id,String role,int offset,String generation) {return wifiGuarded("rolesSnapshot",() -> {
            Bundle snapshot=rolesSettings.snapshot(id,role,offset,generation);if(snapshot!=null)return snapshot;
            Bundle unavailable=ok();unavailable.putString("json",dev.makepad.octosense.roles.RolesSettingsContract.unavailable(id,role==null?null:dev.makepad.octosense.roles.RolesSettingsContract.RoleId.parse(role),"unavailable").toString());return unavailable;
        });}
        @Override public Bundle confirmRole(String role,String key,String target) {return wifiGuarded("roleConfirmation",() -> {
            Bundle flow=rolesSettings.confirmation(role,key,target);return flow==null?fail("role_target_changed"):flow;
        });}
        @Override public Bundle getPermissionsSnapshot(long id,String pkg,String group,int offset,String generation) {return wifiGuarded("permissionsSnapshot",() -> {
            Bundle snapshot=permissionsSettings.snapshot(id,pkg,group,offset,generation);if(snapshot!=null)return snapshot;
            Bundle unavailable=ok();unavailable.putString("json",dev.makepad.octosense.permissions.PermissionsSettingsContract.unavailable(id,pkg,group==null?null:dev.makepad.octosense.permissions.PermissionsSettingsContract.Group.parse(group),"unavailable").toString());return unavailable;
        });}
        @Override public Bundle requestPermissionChoice(String pkg,String group,String key,String target) {return wifiGuarded("permissionChoice",() -> {
            Bundle flow=permissionsSettings.operation(pkg,group,key,target);return flow==null?fail("permission_target_changed"):flow;
        });}
        @Override public Bundle getKeyboardsSnapshot(long id,String query,int offset){return wifiGuarded("keyboardsSnapshot",()->{
            dev.makepad.octosense.keyboards.KeyboardContract.read(id,query,offset);Bundle state=keyboardSettings.snapshot(id,query,offset);if(state!=null)return state;
            Bundle out=ok();out.putString("json",dev.makepad.octosense.keyboards.KeyboardJson.unavailable(id,query,"unavailable").toString());return out;
        });}
        @Override public Bundle prepareKeyboardFlow(long id,String key,String target,String operation){return wifiGuarded("keyboardFlow",()->{
            dev.makepad.octosense.keyboards.KeyboardContract.flow(id,key,target,operation);Bundle flow=keyboardSettings.prepare(id,key,target,operation);return flow==null?fail("keyboard_target_changed"):flow;
        });}
        @Override public Bundle getSystemLanguagesSnapshot(long id,String key,String parent,String query,int offset){return wifiGuarded("systemLanguagesSnapshot",()->{
            Bundle state=systemLanguageSettings.snapshot(id,key,parent,query,offset);if(state!=null)return state;
            Bundle unavailable=ok();unavailable.putString("json",dev.makepad.octosense.systemlanguage.SystemLanguageContract.unavailable(id,query,"unavailable","service_unavailable").toString());return unavailable;
        });}
        @Override public Bundle applySystemLanguages(String key,String[] targets){return wifiGuarded("systemLanguagesApply",()->{
            String reason=systemLanguageSettings.apply(key,targets);Bundle out=new Bundle();out.putString("reason",reason);out.putBoolean("ok",reason.equals("languages_applied")||reason.equals("languages_requested"));return out;
        });}
        @Override public Bundle getAppLanguageSnapshot(long id,String pkg,String key,String parent,String query,int offset){return wifiGuarded("appLanguageSnapshot",()->{
            Bundle state=appLanguageSettings.snapshot(id,pkg,key,parent,query,offset);if(state!=null)return state;
            Bundle unavailable=ok();unavailable.putString("json",dev.makepad.octosense.applanguage.AppLanguageContract.unavailable(id,pkg,query,"unavailable","service_unavailable").toString());return unavailable;
        });}
        @Override public Bundle setAppLanguage(String pkg,String key,String choice){return wifiGuarded("appLanguageSet",()->{
            String reason=appLanguageSettings.select(pkg,key,choice);Bundle out=new Bundle();out.putString("reason",reason);out.putBoolean("ok",reason.equals("app_language_applied")||reason.equals("app_language_requested"));return out;
        });}
        @Override public Bundle getAppStorageSnapshot(long id,String pkg){return wifiGuarded("appStorageSnapshot",()->{
            Bundle state=appStorageSettings.snapshot(id,pkg);if(state!=null)return state;
            Bundle unavailable=ok();unavailable.putString("json",dev.makepad.octosense.appstorage.AppStorageContract.unavailable(id,pkg).toString());return unavailable;
        });}
        @Override public Bundle applyAppStorage(String pkg,String key,String action){return wifiGuarded("appStorageAction",()->{
            Bundle state=appStorageSettings.action(pkg,key,action);if(state!=null)return state;
            Bundle unavailable=new Bundle();unavailable.putBoolean("ok",false);unavailable.putString("reason","app_storage_unavailable");return unavailable;
        });}
        @Override public Bundle getAppBatterySnapshot(long id,String pkg){return wifiGuarded("appBatterySnapshot",()->{
            Bundle state=appBatterySettings.snapshot(id,pkg);if(state!=null)return state;
            Bundle unavailable=ok();unavailable.putString("json",dev.makepad.octosense.battery.AppBatteryContract.unavailable(id,pkg).toString());return unavailable;
        });}
        @Override public Bundle setAppBattery(String pkg,String key,String mode){return wifiGuarded("appBatterySet",()->{
            String reason=appBatterySettings.set(pkg,key,mode);Bundle result=new Bundle();result.putString("reason",reason);result.putBoolean("ok",reason.equals("app_battery_applied")||reason.equals("app_battery_requested"));return result;
        });}
        @Override public Bundle getAppNetworkSnapshot(long id,String pkg){return wifiGuarded("appNetworkSnapshot",()->{
            Bundle state=appNetworkSettings.snapshot(id,pkg);if(state!=null)return state;
            Bundle unavailable=ok();unavailable.putString("json",new dev.makepad.octosense.appnetwork.AppNetworkBackend(null,SystemClock::elapsedRealtime).snapshot(id,pkg).toString());return unavailable;
        });}
        @Override public Bundle setAppNetwork(String pkg,String key,String field,boolean enabled){return wifiGuarded("appNetworkSet",()->{
            Bundle state=appNetworkSettings.set(pkg,key,field,enabled);return state==null?fail("app_network_unavailable"):state;
        });}
        @Override public Bundle getDndSnapshot(long id,int offset,String generation) {return wifiGuarded("dndSnapshot",()->dndResult(zenSettings.settingsSnapshot(id,offset,generation)));}
        @Override public Bundle setDndPolicy(String key,String field,String value) {return wifiGuarded("dndPolicy",()->dndResult(zenSettings.policy(key,field,value)));}
        @Override public Bundle saveDndSchedule(String key,String target,String name,int[] days,int start,int end,boolean exitAtAlarm,boolean enabled) {return wifiGuarded("dndSchedule",()->dndResult(zenSettings.schedule(key,target,name,days,start,end,exitAtAlarm,enabled)));}
        @Override public Bundle setDndRuleEnabled(String key,String target,boolean enabled) {return wifiGuarded("dndEnabled",()->dndResult(zenSettings.enabled(key,target,enabled)));}
        @Override public Bundle deleteDndRule(String key,String target) {return wifiGuarded("dndDelete",()->dndResult(zenSettings.deleteRule(key,target)));}
        @Override public Bundle setAppNotification(String pkg,String key,String target,String action,String value) {return wifiGuarded("appNotificationsSet",() -> {
            String reason=appNotifications.set(pkg,key,target,action,value);Bundle result=ok();result.putBoolean("ok",reason.equals("notifications_applied"));result.putString("reason",reason);return result;
        });}
        @Override public Bundle getDisplaySnapshot(long id) {return wifiGuarded("displaySnapshot",() -> {
            Bundle result=ok();result.putString("json",displaySettings.snapshot(id).toString());return result;
        });}
        @Override public Bundle setDisplaySetting(String key,String setting,String value) {return wifiGuarded("displaySet",() -> {
            String reason=displaySettings.set(key,dev.makepad.octosense.display.DisplaySettingsContract.Setting.parse(setting),value);
            Bundle result=ok();result.putBoolean("ok",reason.equals("display_applied")||reason.equals("display_requested"));result.putString("reason",reason);return result;
        });}
        @Override public Bundle stopSound() {return guarded("soundStop",() -> {soundSettings.stop();return soundResult("sound_stopped");});}
        @Override public Bundle getNotificationHistory(long id,String key,int offset) {return wifiGuarded("notificationHistory",() -> {
            Bundle result=ok();result.putString("json",notificationHistory.snapshot(id,key,offset).toString());return result;
        });}
        @Override public Bundle setDateTime(String key,String action,String value,String occurrence) {return wifiGuarded("setDateTime",() -> {
            String reason=dateTimeSettings.apply(key,action,value,occurrence);
            Bundle result=ok();result.putBoolean("ok",reason.equals("time_applied")||reason.equals("time_requested"));result.putString("reason",reason);return result;
        });}
    };

    // ---- dumpsys harness --------------------------------------------------

    @Override protected void dump(FileDescriptor fd, PrintWriter pw, String[] args) {
        if (args == null || args.length == 0) {
            pw.println("OctoSense agent platform, protocol " + PROTOCOL_VERSION);
            pw.println("capabilities " + capabilities().getStringArrayList("capabilities"));
            pw.println("commands: tasks | snapshot <taskId> [maxWidth] | screen [maxWidth] | tap <x> <y> | swipe <x0> <y0> <x1> <y1> <ms>");
            pw.println("          type <text> | key <code> | get <table> <name> | put <table> <name> <value>");
            pw.println("          start-task <id> | remove-task <id> | force-stop <pkg> | notifications | qs | collapse | audit");
            pw.println("          update-check | update-apply [rom|home|all|force-rom] | update-status | update-cancel | update-reboot");
            return;
        }
        long token = Binder.clearCallingIdentity();
        try {
            Bundle r = dumpCommand(args);
            if (r.containsKey("png")) {
                // A system app cannot write where the shell reads, so the image
                // travels in the dump output; scripts/agent-test.sh decodes it.
                byte[] png = r.getByteArray("png");
                r.remove("png");
                r.putString("png_base64", android.util.Base64.encodeToString(png, android.util.Base64.NO_WRAP));
            }
            for (String k : r.keySet()) pw.println(k + "=" + r.get(k));
        } catch (Exception e) {
            pw.println("error " + e);
        } finally {
            Binder.restoreCallingIdentity(token);
        }
        record("shell", args[0], "dump");
    }

    private Bundle dumpCommand(String[] a) throws Exception {
        switch (a[0]) {
            case "tasks": return tasks(a.length > 1 ? Integer.parseInt(a[1]) : 16);
            case "snapshot": return taskSnapshot(Integer.parseInt(a[1]), a.length > 2 ? Integer.parseInt(a[2]) : 540);
            case "screen": return screen(a.length > 1 ? Integer.parseInt(a[1]) : 0);
            case "tap": return tap(Float.parseFloat(a[1]), Float.parseFloat(a[2]));
            case "swipe": return swipe(Float.parseFloat(a[1]), Float.parseFloat(a[2]), Float.parseFloat(a[3]), Float.parseFloat(a[4]), Integer.parseInt(a[5]));
            case "type": return typeText(String.join(" ", Arrays.copyOfRange(a, 1, a.length)));
            case "key": return pressKey(Integer.parseInt(a[1]));
            case "get": return getSetting(a[1], a[2]);
            case "put": return putSetting(a[1], a[2], a[3]);
            case "start-task": return startTask(Integer.parseInt(a[1]));
            case "remove-task": return removeTask(Integer.parseInt(a[1]));
            case "force-stop": return forceStop(a[1]);
            case "notifications": return expandNotifications();
            case "qs": return expandQuickSettings();
            case "collapse": return collapsePanels();
            case "audit": return auditLog(50);
            // dump() runs on the main thread, where network calls are refused: hop to the worker.
            case "update-check": return AgentApplication.get().work()
                    .submit(() -> updater().check()).get(60, java.util.concurrent.TimeUnit.SECONDS);
            // force-rom skips the newer-than check: reinstalls the offered build into the other slot.
            case "update-apply":
                if (a.length > 1 && a[1].equals("force-rom")) return AgentApplication.get().work()
                        .submit(() -> updater().applyRom()).get(60, java.util.concurrent.TimeUnit.SECONDS);
                return applyUpdateAsync(a.length > 1 ? a[1] : "all");
            case "update-status": return updater().status();
            case "update-cancel": return updater().cancel();
            case "update-reboot": return updater().reboot();
            default: return fail("unknown command " + a[0]);
        }
    }
}
