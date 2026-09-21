package dev.makepad.octosense.quickstep;

import android.app.ActivityManager;
import android.app.ActivityTaskManager;
import android.app.KeyguardManager;
import android.app.StatusBarManager;
import android.app.TaskStackListener;
import android.content.BroadcastReceiver;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.content.ServiceConnection;
import android.content.SharedPreferences;
import android.content.pm.PackageManager;
import android.graphics.PixelFormat;
import android.graphics.Bitmap;
import android.graphics.Canvas;
import android.graphics.drawable.Drawable;
import android.os.Binder;
import android.os.Bundle;
import android.os.Handler;
import android.os.IBinder;
import android.os.Looper;
import android.os.PowerManager;
import android.os.ServiceManager;
import android.os.UserHandle;
import android.os.UserManager;
import android.util.DisplayMetrics;
import android.util.Log;
import android.view.Choreographer;
import android.view.ContextThemeWrapper;
import android.view.DisplayCutout;
import android.view.InputEvent;
import android.view.MotionEvent;
import android.view.WindowManager;
import com.android.internal.statusbar.IStatusBarService;
import com.android.systemui.shared.system.InputChannelCompat;
import com.android.systemui.shared.system.InputMonitorCompat;
import com.android.systemui.shared.system.QuickStepContract;
import dev.makepad.octosense.contracts.ISystemBridge;
import dev.makepad.octosense.contracts.ISystemBridgeCallback;
import dev.makepad.octosense.contracts.Protocol;
import dev.makepad.octosense.contracts.SystemSettings;
import java.util.UUID;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.CopyOnWriteArraySet;
import java.util.concurrent.ThreadPoolExecutor;
import java.util.concurrent.TimeUnit;

/** ROM-native edge entry and window owner. Does not replace or stop SystemUI. */
public final class GlobalShadeController implements AutoCloseable {
    private static final String TAG = "OctoSenseShade";
    private static GlobalShadeController active;
    static final CopyOnWriteArraySet<Runnable> observers = new CopyOnWriteArraySet<>();
    static SharedPreferences preferences(Context context) {
        return context.createDeviceProtectedStorageContext().getSharedPreferences("global-shade", Context.MODE_PRIVATE);
    }
    static String status() {
        GlobalShadeController c = active;
        if (c == null) return "Waiting for the OctoSense Recents service";
        if (!c.preferences.getBoolean("enabled", false)) return "Android's panel is active";
        return c.monitor != null ? "Active · Pull down from either top corner in any app"
                : "Waiting for System Bridge · Android's panel remains available";
    }
    private final Context context;
    private final Handler main = new Handler(Looper.getMainLooper());
    private final ThreadPoolExecutor worker = new ThreadPoolExecutor(1, 1, 0, TimeUnit.MILLISECONDS,
            new ArrayBlockingQueue<>(32), r -> new Thread(r, "OctoSenseShadeBridge"), new ThreadPoolExecutor.AbortPolicy());
    private final SharedPreferences preferences;
    private final IBinder disableToken = new Binder();
    private final IStatusBarService statusBar = IStatusBarService.Stub.asInterface(ServiceManager.getService("statusbar"));
    private final WindowManager windows;
    private final DisplayMetrics metrics = new DisplayMetrics();
    private int edgeHeight;
    private final java.util.concurrent.ConcurrentHashMap<String, String> appLabels = new java.util.concurrent.ConcurrentHashMap<>();
    private final java.util.concurrent.ConcurrentHashMap<String, Bitmap> appIcons = new java.util.concurrent.ConcurrentHashMap<>();
    private InputMonitorCompat monitor;
    private InputChannelCompat.InputEventReceiver receiver;
    private GlobalShadePanel panel;
    private Bundle state = Bundle.EMPTY;
    private volatile Connection connection;
    private int generation;
    private long revision = -1, commandId;
    private String epoch = "";
    private boolean bound, closed, tracking, suppressed, taskListenerRegistered;
    private float downX, downY;
    private final Runnable gestureTimeout = this::cancelGesture;
    private final SharedPreferences.OnSharedPreferenceChangeListener preferenceListener = (p, key) -> main.post(this::configure);

    private static final class Connection {
        final ISystemBridge api;
        final String session = UUID.randomUUID().toString();
        final int generation;
        Connection(ISystemBridge api, int generation) { this.api = api; this.generation = generation; }
    }
    public GlobalShadeController(Context service) {
        if (active != null) active.close();
        Context display = service.createDisplayContext(service.getSystemService(android.hardware.display.DisplayManager.class)
                .getDisplay(android.view.Display.DEFAULT_DISPLAY));
        context = new ContextThemeWrapper(display.createWindowContext(
                WindowManager.LayoutParams.TYPE_STATUS_BAR_SUB_PANEL, null), android.R.style.Theme_Material_NoActionBar);
        windows = context.getSystemService(WindowManager.class);
        preferences = preferences(context);
        preferences.registerOnSharedPreferenceChangeListener(preferenceListener);
        active = this;
        IntentFilter events = new IntentFilter(Intent.ACTION_SCREEN_OFF);
        events.addAction(Intent.ACTION_USER_UNLOCKED);
        events.addAction(Intent.ACTION_USER_SWITCHED);
        events.addAction(Intent.ACTION_CLOSE_SYSTEM_DIALOGS);
        events.addAction(Intent.ACTION_CONFIGURATION_CHANGED);
        context.registerReceiver(systemEvents, events, Context.RECEIVER_EXPORTED);
        try {
            ActivityTaskManager.getService().registerTaskStackListener(taskListener);
            taskListenerRegistered = true;
        } catch (android.os.RemoteException failure) { Log.w(TAG, "Task lifecycle unavailable"); }
        configure();
    }
    private void changed() { for (Runnable observer : observers) observer.run(); }
    private boolean wanted() {
        return !closed && preferences.getBoolean("enabled", false)
                && context.getSystemService(UserManager.class).isUserUnlocked()
                && ActivityManager.getCurrentUser() == UserHandle.myUserId();
    }
    private boolean unlocked() {
        return wanted() && context.getSystemService(PowerManager.class).isInteractive()
                && !context.getSystemService(KeyguardManager.class).isKeyguardLocked();
    }
    private void configure() {
        if (!wanted()) { stopMonitor(); disconnect(); changed(); return; }
        if (!bound) {
            Intent intent = new Intent().setComponent(new ComponentName(Protocol.BRIDGE_PACKAGE,
                    Protocol.BRIDGE_PACKAGE + ".SystemBridgeService"));
            try { bound = context.bindService(intent, bridgeConnection, Context.BIND_AUTO_CREATE); }
            catch (SecurityException denied) { Log.w(TAG, "Bridge unavailable: " + denied.getClass().getSimpleName()); }
        }
        if (connection != null && revision >= 0) startMonitor();
        changed();
    }
    private boolean enqueue(Runnable work) {
        if (closed) return false;
        try { worker.execute(work); return true; }
        catch (java.util.concurrent.RejectedExecutionException full) { message("Busy. Try again."); return false; }
    }
    private final ServiceConnection bridgeConnection = new ServiceConnection() {
        @Override public void onServiceConnected(ComponentName name, IBinder binder) {
            int attempt = ++generation;
            enqueue(() -> {
                try {
                    PackageManager pm = context.getPackageManager();
                    int uid = pm.getApplicationInfo(Protocol.BRIDGE_PACKAGE, 0).uid;
                    if (uid / 100000 != UserHandle.myUserId()
                            || pm.checkSignatures(context.getApplicationInfo().uid, uid) != PackageManager.SIGNATURE_MATCH)
                        throw new SecurityException("Bridge identity mismatch");
                    Connection next = new Connection(ISystemBridge.Stub.asInterface(binder), attempt);
                    if (next.api.getProtocolInfo().getInt("major") != Protocol.MAJOR)
                        throw new SecurityException("Bridge protocol mismatch");
                    main.post(() -> {
                        if (closed || attempt != generation || !wanted()) return;
                        connection = next; revision = -1; epoch = "";
                        enqueue(() -> {
                            try { next.api.subscribe(next.session, callback(next)); }
                            catch (Exception failure) { main.post(() -> lost(next)); }
                        });
                    });
                } catch (Exception failure) {
                    Log.w(TAG, "Bridge handshake failed: " + failure.getClass().getSimpleName());
                }
            });
        }
        @Override public void onServiceDisconnected(ComponentName name) { ++generation; connection = null; revision = -1; state = Bundle.EMPTY; stopMonitor(); changed(); }
        @Override public void onBindingDied(ComponentName name) { disconnect(); stopMonitor(); main.postDelayed(GlobalShadeController.this::configure, 500); }
        @Override public void onNullBinding(ComponentName name) { disconnect(); stopMonitor(); changed(); }
    };
    private ISystemBridgeCallback callback(Connection owner) {
        return new ISystemBridgeCallback.Stub() {
            @Override public void onSnapshot(String e, long r, Bundle value) {
                // Binder callback thread, never the input/UI thread. Do not retain or log content.
                java.util.ArrayList<Bundle> notices = value.getParcelableArrayList("notifications");
                if (notices != null) for (Bundle notice : notices) {
                    String pkg = notice.getString("package", "");
                    String label = appLabels.get(pkg);
                    if (label == null) {
                        try { label = context.getPackageManager().getApplicationLabel(context.getPackageManager().getApplicationInfo(pkg, 0)).toString(); }
                        catch (PackageManager.NameNotFoundException absent) { label = "Notification"; }
                        if (label.length() > 80) label = label.substring(0, 80);
                        if (appLabels.size() >= 128) appLabels.clear();
                        appLabels.put(pkg, label);
                    }
                    notice.putString("app_label", label);
                    Bitmap icon = appIcons.get(pkg);
                    if (icon == null) {
                        try {
                            Drawable drawable = context.getPackageManager().getApplicationIcon(pkg).mutate();
                            icon = Bitmap.createBitmap(96, 96, Bitmap.Config.ARGB_8888);
                            drawable.setBounds(0, 0, 96, 96); drawable.draw(new Canvas(icon));
                            if (appIcons.size() >= 64) appIcons.clear();
                            appIcons.put(pkg, icon);
                        } catch (PackageManager.NameNotFoundException | RuntimeException unavailable) { icon = null; }
                    }
                    if (icon != null) notice.putParcelable("app_icon", icon);
                }
                main.post(() -> {
                    if (closed || connection != owner || !wanted()) return;
                    if (e.equals(epoch) && r <= revision) return;
                    epoch = e; revision = r; state = value;
                    if (panel != null) panel.update(value);
                    startMonitor(); changed();
                });
            }
            @Override public void onDelta(String e, long r, Bundle patch) { snapshot(owner); }
            @Override public void onResyncRequired(String e) { snapshot(owner); }
            @Override public void onCommandResult(String session, long id, int result, String reason) {
                main.post(() -> {
                    if (connection == owner && owner.session.equals(session) && panel != null)
                        panel.result(id, result);
                });
            }
        };
    }
    private void snapshot(Connection owner) {
        enqueue(() -> { if (connection == owner) try { owner.api.requestSnapshot(owner.session); }
            catch (Exception failure) { main.post(() -> lost(owner)); } });
    }
    private void lost(Connection owner) {
        if (connection != owner) return;
        connection = null; revision = -1; state = Bundle.EMPTY; stopMonitor(); changed();
    }
    private void disconnect() {
        Connection previous = connection; connection = null; ++generation; revision = -1; state = Bundle.EMPTY;
        if (previous != null) enqueue(() -> { try { previous.api.unsubscribe(previous.session); } catch (Exception ignored) {} });
        if (bound) { context.unbindService(bridgeConnection); bound = false; }
    }
    private void startMonitor() {
        if (!wanted() || monitor != null || connection == null || revision < 0 || !taskListenerRegistered) return;
        try {
            geometry();
            monitor = new InputMonitorCompat("octosense-shade", context.getDisplayId());
            receiver = monitor.getInputReceiver(Looper.getMainLooper(), Choreographer.getInstance(), this::input);
            receiver.setBatchingEnabled(false);
            Log.i(TAG, "Edge panel ready");
        } catch (RuntimeException failure) { stopMonitor(); Log.w(TAG, "Edge panel unavailable: " + failure.getClass().getSimpleName()); }
    }
    private void input(InputEvent input) {
        if (!(input instanceof MotionEvent) || panel != null) return;
        MotionEvent event = (MotionEvent) input;
        if (event.getDisplayId() != context.getDisplayId()) return;
        try {
            int action = event.getActionMasked();
            if (action == MotionEvent.ACTION_DOWN) {
                cancelGesture();
                // Leave ordinary app touches without system queries or IPC.
                if (connection == null || event.getRawY() < 0 || event.getRawY() > edgeHeight) return;
                if (!unlocked()) return;
                tracking = true; downX = event.getRawX(); downY = event.getRawY();
                monitor.pilferPointers();
                suppress(true);
                main.postDelayed(gestureTimeout, 2000);
            } else if (tracking && action == MotionEvent.ACTION_UP) {
                float dx = event.getRawX() - downX, dy = event.getRawY() - downY;
                boolean controls = downX >= metrics.widthPixels * .5f;
                tracking = false; main.removeCallbacks(gestureTimeout);
                if (dy >= 24 * metrics.density && dy > Math.abs(dx) && unlocked()) show(controls);
                else suppress(false);
            } else if (tracking && (action == MotionEvent.ACTION_CANCEL || action == MotionEvent.ACTION_POINTER_DOWN)) cancelGesture();
        } catch (RuntimeException failure) { stopMonitor(); Log.w(TAG, "Gesture released after failure: " + failure.getClass().getSimpleName()); }
    }
    private void suppress(boolean disable) {
        if (suppressed == disable) return;
        try {
            statusBar.disableForUser(disable ? StatusBarManager.DISABLE_EXPAND : 0,
                    disableToken, context.getPackageName(), UserHandle.myUserId());
            suppressed = disable;
        } catch (Exception failure) { if (disable) throw new IllegalStateException("Cannot coordinate system panel", failure); }
    }
    private void cancelGesture() { tracking = false; main.removeCallbacks(gestureTimeout); if (panel == null) suppress(false); }
    private void geometry() {
        windows.getDefaultDisplay().getRealMetrics(metrics);
        int resource = context.getResources().getIdentifier("status_bar_height", "dimen", "android");
        edgeHeight = resource == 0 ? Math.round(28 * metrics.density) : context.getResources().getDimensionPixelSize(resource);
        // DisplayPolicy accepts swipes below the visible bar around a notch.
        // Match SystemGesturesPointerEventListener's top boundary; otherwise
        // those touches bypass this monitor and expand the stock shade.
        edgeHeight = Math.max(edgeHeight, context.getResources().getDimensionPixelSize(
                com.android.internal.R.dimen.system_gestures_start_threshold));
        DisplayCutout cutout = windows.getDefaultDisplay().getCutout();
        if (cutout != null) {
            edgeHeight = Math.max(edgeHeight, cutout.getBoundingRectTop().height()
                    + context.getResources().getDimensionPixelSize(
                            com.android.internal.R.dimen.display_cutout_touchable_region_size));
        }
    }
    public void onSystemUiStateChanged(long flags) {
        if ((flags & (QuickStepContract.SYSUI_STATE_STATUS_BAR_KEYGUARD_SHOWING
                | QuickStepContract.SYSUI_STATE_BOUNCER_SHOWING)) != 0) dismiss();
    }
    private final TaskStackListener taskListener = new TaskStackListener() {
        @Override public void onTaskMovedToFront(ActivityManager.RunningTaskInfo task) { main.post(GlobalShadeController.this::dismiss); }
    };
    private void show(boolean controls) {
        if (panel != null || !unlocked() || connection == null) { suppress(false); return; }
        try {
            panel = new GlobalShadePanel(context, this, controls);
            WindowManager.LayoutParams params = new WindowManager.LayoutParams(
                    WindowManager.LayoutParams.MATCH_PARENT, WindowManager.LayoutParams.MATCH_PARENT,
                    WindowManager.LayoutParams.TYPE_STATUS_BAR_SUB_PANEL,
                    WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN | WindowManager.LayoutParams.FLAG_DRAWS_SYSTEM_BAR_BACKGROUNDS,
                    PixelFormat.TRANSLUCENT);
            params.setTitle("OctoSense Global Shade");
            params.setFitInsetsTypes(0);
            params.layoutInDisplayCutoutMode = WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_ALWAYS;
            params.softInputMode = WindowManager.LayoutParams.SOFT_INPUT_ADJUST_RESIZE;
            windows.addView(panel, params);
            panel.update(state); panel.requestFocus();
            Log.i(TAG, controls ? "Opened controls" : "Opened notifications");
        } catch (RuntimeException failure) { dismiss(); Log.w(TAG, "Panel window unavailable: " + failure.getClass().getSimpleName()); }
    }
    void dismiss() {
        GlobalShadePanel previous = panel; panel = null;
        if (previous != null) {
            previous.clearPrivateState();
            if (previous.isAttachedToWindow()) windows.removeViewImmediate(previous);
        }
        cancelGesture(); suppress(false);
    }
    void settings(String destination) {
        dismiss();
        if (!SystemSettings.open(context, destination)) Log.w(TAG, "Requested settings destination unavailable");
    }
    boolean accessible(String operation) {
        Bundle capabilities = state.getBundle("capabilities");
        Bundle capability = capabilities == null ? null : capabilities.getBundle(operation);
        return connection != null && capability != null && capability.getBoolean("accessible");
    }
    long command(String operation, boolean toggle, float value, String handle, String reply) {
        Connection owner = connection;
        if (owner == null) { message("System Bridge disconnected"); return -1; }
        long id = ++commandId;
        boolean accepted = enqueue(() -> {
            if (connection != owner) return;
            try {
                switch (operation) {
                    case "wifi": owner.api.setWifiEnabled(owner.session, id, toggle); break;
                    case "bluetooth": owner.api.setBluetoothEnabled(owner.session, id, toggle); break;
                    case "torch": owner.api.setTorchEnabled(owner.session, id, toggle); break;
                    case "rotation": owner.api.setRotationLocked(owner.session, id, toggle); break;
                    case "brightness": owner.api.setBrightness(owner.session, id, value, false); break;
                    case "volume": owner.api.setVolume(owner.session, id, value); break;
                    case "dismiss": owner.api.dismissNotification(owner.session, id, handle); break;
                    case "dismiss_all": owner.api.dismissAllNotifications(owner.session, id); break;
                    case "action": owner.api.invokeNotificationAction(owner.session, id, handle, reply); break;
                    default: throw new IllegalArgumentException("Unknown panel operation");
                }
            } catch (Exception failure) { main.post(() -> { if (panel != null) panel.result(id, Protocol.UNCERTAIN); }); }
        });
        return accepted ? id : -1;
    }
    private void message(String value) { main.post(() -> { if (panel != null) panel.message(value); }); }
    private void stopMonitor() {
        dismiss();
        if (receiver != null) { receiver.dispose(); receiver = null; }
        if (monitor != null) { monitor.dispose(); monitor = null; }
    }
    private final BroadcastReceiver systemEvents = new BroadcastReceiver() {
        @Override public void onReceive(Context c, Intent intent) {
            dismiss();
            if (Intent.ACTION_CONFIGURATION_CHANGED.equals(intent.getAction())) geometry();
            if (Intent.ACTION_USER_UNLOCKED.equals(intent.getAction()) || Intent.ACTION_USER_SWITCHED.equals(intent.getAction())) configure();
        }
    };
    @Override public void close() {
        if (closed) return;
        stopMonitor(); disconnect(); closed = true;
        preferences.unregisterOnSharedPreferenceChangeListener(preferenceListener);
        context.unregisterReceiver(systemEvents); main.removeCallbacksAndMessages(null); worker.shutdown();
        if (taskListenerRegistered) try { ActivityTaskManager.getService().unregisterTaskStackListener(taskListener); }
            catch (android.os.RemoteException ignored) {}
        if (active == this) active = null;
        appIcons.clear(); appLabels.clear();
        changed();
    }
}
