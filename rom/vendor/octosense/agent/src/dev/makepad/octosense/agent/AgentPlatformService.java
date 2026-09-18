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

    @Override public void onCreate() {
        super.onCreate();
        atm = ActivityTaskManager.getInstance();
        input = getSystemService(InputManager.class);
        keyguard = getSystemService(KeyguardManager.class);
        statusBar = getSystemService(StatusBarManager.class);
        Log.i(TAG, "agent platform up, uid " + Process.myUid());
    }

    @Override public IBinder onBind(Intent intent) { return binder; }
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
        if (has("android.permission.START_TASKS_FROM_RECENTS")) caps.add("apps");
        if (has("android.permission.STATUS_BAR")) caps.add("statusbar");
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

    synchronized Bundle auditLog(int max) {
        Bundle r = ok();
        ArrayList<String> lines = new ArrayList<>();
        for (String s : audit) { if (lines.size() >= Math.max(1, max)) break; lines.add(s); }
        r.putStringArrayList("log", lines);
        return r;
    }

    // ---- Binder -----------------------------------------------------------

    private final IAgentPlatform.Stub binder = new IAgentPlatform.Stub() {
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
    };

    // ---- dumpsys harness --------------------------------------------------

    @Override protected void dump(FileDescriptor fd, PrintWriter pw, String[] args) {
        if (args == null || args.length == 0) {
            pw.println("OctoSense agent platform, protocol " + PROTOCOL_VERSION);
            pw.println("capabilities " + capabilities().getStringArrayList("capabilities"));
            pw.println("commands: tasks | snapshot <taskId> <path> | screen <path> | tap <x> <y> | swipe <x0> <y0> <x1> <y1> <ms>");
            pw.println("          type <text> | key <code> | get <table> <name> | put <table> <name> <value>");
            pw.println("          start-task <id> | remove-task <id> | force-stop <pkg> | notifications | qs | collapse | audit");
            return;
        }
        long token = Binder.clearCallingIdentity();
        try {
            Bundle r = dumpCommand(args);
            if (r.containsKey("png")) {
                byte[] png = r.getByteArray("png");
                // screen <path> [maxWidth] | snapshot <taskId> <path>
                String path = args[0].equals("snapshot") ? args[2] : args[1];
                try (java.io.FileOutputStream out = new java.io.FileOutputStream(path)) { out.write(png); }
                r.remove("png");
                r.putString("path", path);
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
            case "snapshot": return taskSnapshot(Integer.parseInt(a[1]), 540);
            case "screen": return screen(a.length > 2 ? Integer.parseInt(a[2]) : 0);
            // (the path is consumed by dump(); a[1])
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
            default: return fail("unknown command " + a[0]);
        }
    }
}
