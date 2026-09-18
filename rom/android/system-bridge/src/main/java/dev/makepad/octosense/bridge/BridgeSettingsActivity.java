package dev.makepad.octosense.bridge;

import android.Manifest;
import android.app.Activity;
import android.app.NotificationManager;
import android.bluetooth.BluetoothAdapter;
import android.bluetooth.BluetoothManager;
import android.content.ComponentName;
import android.content.pm.PackageManager;
import android.os.Build;
import android.os.Bundle;
import android.provider.Settings;
import android.view.View;
import android.widget.Button;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;
import android.widget.Toast;
import dev.makepad.octosense.contracts.NetworkStatus;
import dev.makepad.octosense.contracts.SystemSettings;
import java.util.LinkedHashMap;
import java.util.Map;

/** Native consent and setup, reached from Home's shade or the app drawer. */
public final class BridgeSettingsActivity extends Activity {
    private final Map<String, TextView> summaries = new LinkedHashMap<>();
    private final Runnable update = this::updateStatus;
    private LinearLayout content;
    private BridgeState state() { return ((BridgeApplication)getApplication()).state(); }
    private int dp(int value) { return Math.round(value * getResources().getDisplayMetrics().density); }

    @Override public void onCreate(Bundle saved) {
        super.onCreate(saved);
        ScrollView scroll = new ScrollView(this);
        scroll.setFillViewport(true);
        content = new LinearLayout(this);
        content.setOrientation(LinearLayout.VERTICAL);
        content.setPadding(dp(20), dp(20), dp(20), dp(24));
        scroll.addView(content);
        scroll.setOnApplyWindowInsetsListener((view, insets) -> {
            content.setPadding(dp(20) + insets.getSystemWindowInsetLeft(), dp(16) + insets.getSystemWindowInsetTop(),
                    dp(20) + insets.getSystemWindowInsetRight(), dp(24) + insets.getSystemWindowInsetBottom());
            return insets;
        });
        text("OctoSense system setup", 26);
        text("Connect your launcher to Android. Each access request opens Android's own consent screen. Return to Home to use the shade controls.", 15);
        section("Notifications");
        row("notifications", "Notification access", "notifications");
        text("After enabling access: pull down from the top left to read notifications. Swipe left for app actions or Reply; swipe right to dismiss. Android may hide sensitive content.", 14);
        section("Network and connections");
        row("internet", "Internet · Wi-Fi and mobile data", "internet");
        row("bluetooth", "Bluetooth · Pair devices", "bluetooth");
        row("mobile", "SIM and mobile network", "mobile");
        row("hotspot", "Hotspot and tethering", "hotspot");
        row("vpn", "VPN", "vpn");
        section("Device controls");
        row("write", "Brightness and rotation access", "brightness");
        row("torch", "Flashlight access", () -> {
            if (granted(Manifest.permission.CAMERA)) manageAppPermissions();
            else requestPermissions(new String[]{Manifest.permission.CAMERA}, 1);
        });
        row("nearby", "Bluetooth status access", () -> {
            if (Build.VERSION.SDK_INT >= 31 && !granted(Manifest.permission.BLUETOOTH_CONNECT))
                requestPermissions(new String[]{Manifest.permission.BLUETOOTH_CONNECT}, 2);
            else manageAppPermissions();
        });
        row("dnd", "Do Not Disturb", "dnd");
        if (Build.VERSION.SDK_INT < 35) row("policy", "Do Not Disturb access", "policy");
        row("battery", "Battery saver", "battery");
        row("display", "Display and sleep", "display");
        row("sound", "Sound and vibration", "sound");
        section("Launcher and Android");
        row("shade", "System-wide OctoSense panel", "shade");
        row("home", "Default home app", "home");
        row("apps", "Apps and permissions", "apps");
        row("accessibility", "Accessibility", "accessibility");
        section("Optional direct controls");
        text("Wi-Fi, Bluetooth and battery saver work through Android settings. On supported rooted phones, you can additionally allow direct switches in the launcher. Magisk asks for this access separately.", 14);
        row("root", "Connect direct system controls", () -> state().connectRoot());
        Button done = new Button(this);
        done.setText(R.string.return_to_launcher);
        done.setOnClickListener(view -> {
            android.content.Intent home = new android.content.Intent(android.content.Intent.ACTION_MAIN)
                    .addCategory(android.content.Intent.CATEGORY_HOME);
            startActivity(home); finish();
        });
        content.addView(done);
        setContentView(scroll);
        if ("torch".equals(getIntent().getStringExtra("section"))) {
            scroll.post(() -> scroll.smoothScrollTo(0, ((View)summaries.get("torch").getParent()).getTop()));
        }
    }
    private void text(String value, int size) {
        TextView label = new TextView(this);
        label.setText(value); label.setTextSize(size);
        label.setPadding(0, dp(6), 0, dp(8)); content.addView(label);
    }
    private void section(String value) { text(value, 20); }
    private void row(String key, String title, String destination) {
        row(key, title, () -> {
            if (!SystemSettings.open(this, destination)) Toast.makeText(this, "This setting is unavailable on this device.", Toast.LENGTH_LONG).show();
        });
    }
    private void row(String key, String title, Runnable action) {
        LinearLayout card = new LinearLayout(this);
        card.setOrientation(LinearLayout.VERTICAL);
        card.setPadding(dp(12), dp(10), dp(12), dp(10));
        android.util.TypedValue selectable = new android.util.TypedValue();
        getTheme().resolveAttribute(android.R.attr.selectableItemBackground, selectable, true);
        card.setBackgroundResource(selectable.resourceId);
        card.setMinimumHeight(dp(64));
        TextView label = new TextView(this); label.setText(title); label.setTextSize(17); card.addView(label);
        TextView status = new TextView(this); status.setTextSize(14); status.setPadding(0, dp(4), 0, 0);
        card.addView(status); summaries.put(key, status);
        card.setOnClickListener(view -> action.run());
        card.setFocusable(true); content.addView(card);
    }
    private void manageAppPermissions() {
        try {
            startActivity(new android.content.Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS,
                    android.net.Uri.parse("package:" + getPackageName()))
                    .addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK | android.content.Intent.FLAG_ACTIVITY_CLEAR_TOP));
        } catch (android.content.ActivityNotFoundException | SecurityException unavailable) {
            SystemSettings.open(this, "apps");
        }
    }
    private boolean granted(String permission) { return checkSelfPermission(permission) == PackageManager.PERMISSION_GRANTED; }
    private void status(String key, String value) { TextView label = summaries.get(key); if (label != null) label.setText(value); }
    private void updateStatus() {
        NotificationManager notifications = getSystemService(NotificationManager.class);
        boolean allowed = Build.VERSION.SDK_INT >= 27 && notifications.isNotificationListenerAccessGranted(new ComponentName(this, NotificationAccessService.class));
        if (Build.VERSION.SDK_INT < 27) {
            String listeners = Settings.Secure.getString(getContentResolver(), "enabled_notification_listeners");
            allowed = listeners != null && java.util.Arrays.asList(listeners.split(":")).contains(new ComponentName(this, NotificationAccessService.class).flattenToString());
        }
        Bundle caps = state().capabilities();
        Bundle listener = caps.getBundle("notifications");
        status("notifications", !allowed ? "Not enabled · Tap to allow reading, replies and dismissal"
                : listener != null && listener.getBoolean("accessible") ? "Connected · Notifications appear in your launcher" : "Access enabled · Connecting to Android");
        status("internet", NetworkStatus.read(this).getString("network_summary"));
        boolean nearby = Build.VERSION.SDK_INT < 31 || granted(Manifest.permission.BLUETOOTH_CONNECT);
        BluetoothManager manager = getSystemService(BluetoothManager.class);
        BluetoothAdapter adapter = manager == null ? null : manager.getAdapter();
        String bluetooth = "Tap to pair and manage devices";
        try { if (nearby && adapter != null) bluetooth = (adapter.isEnabled() ? "On" : "Off") + " · Tap to pair and manage devices"; }
        catch (SecurityException denied) { nearby = false; }
        status("bluetooth", bluetooth);
        status("mobile", "Choose a SIM, carrier and mobile data settings");
        status("hotspot", "Share this phone's connection");
        status("vpn", "Manage VPN connections");
        status("write", Settings.System.canWrite(this) ? "Enabled · Sliders and rotation work in the shade" : "Not enabled · Allow changing system settings");
        status("torch", !getPackageManager().hasSystemFeature(PackageManager.FEATURE_CAMERA_FLASH) ? "No flashlight hardware" : granted(Manifest.permission.CAMERA) ? "Enabled · Use Torch in the shade" : "Not enabled · Camera access is required for the flashlight");
        status("nearby", nearby ? "Enabled · Bluetooth state is visible" : "Not enabled · Allow nearby-device access to show Bluetooth state");
        status("dnd", notifications.getCurrentInterruptionFilter() == NotificationManager.INTERRUPTION_FILTER_ALL ? "Off · Manage modes and schedules" : "On · Manage modes and schedules");
        status("policy", notifications.isNotificationPolicyAccessGranted() ? "Enabled" : "Not enabled · Tap to allow");
        status("battery", getSystemService(android.os.PowerManager.class).isPowerSaveMode() ? "Battery saver is on" : "Battery saver is off");
        status("display", "Adaptive brightness, screen timeout and appearance");
        status("sound", "Ringtone, notification and media volume");
        status("home", "Choose OctoSense as your launcher");
        status("shade", "Use OctoSense notifications and controls above other apps");
        status("apps", "Review app access, defaults and storage");
        status("accessibility", "Text size, screen readers and interaction settings");
        String root = caps.getString("root_state", "not_requested");
        status("root", root.equals("connected_unvalidated") ? "Connected · Direct switches available"
                : root.equals("connecting") ? "Waiting for Magisk…" : root.equals("not_requested") ? "Optional · Not connected"
                : root.equals("incompatible") ? "This ROM has no supported direct-control adapter" : "Not connected · Tap to retry");
    }
    @Override protected void onResume() {
        super.onResume(); state().addSettingsObserver(update); state().permissionsChanged(); updateStatus();
    }
    @Override protected void onPause() { state().removeSettingsObserver(update); super.onPause(); }
    @Override public void onRequestPermissionsResult(int requestCode, String[] permissions, int[] grants) {
        super.onRequestPermissionsResult(requestCode, permissions, grants); state().permissionsChanged(); updateStatus();
    }
}
