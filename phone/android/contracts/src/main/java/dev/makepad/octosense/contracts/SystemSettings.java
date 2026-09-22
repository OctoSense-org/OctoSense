package dev.makepad.octosense.contracts;

import android.content.Context;
import android.content.ActivityNotFoundException;
import android.content.ComponentName;
import android.content.Intent;
import android.net.Uri;
import android.os.Build;
import android.provider.Settings;

/** A finite set of user-visible Android settings. Never accepts arbitrary intents or components. */
public final class SystemSettings {
    private SystemSettings() {}
    public static boolean open(Context activity, String destination) {
        Intent intent;
        String fallback = Settings.ACTION_SETTINGS;
        switch (destination) {
            case "internet":
                intent = new Intent(Build.VERSION.SDK_INT >= 29 ? Settings.Panel.ACTION_INTERNET_CONNECTIVITY : Settings.ACTION_WIFI_SETTINGS);
                fallback = Settings.ACTION_WIFI_SETTINGS; break;
            case "wifi": intent = new Intent(Settings.ACTION_WIFI_SETTINGS); break;
            case "bluetooth": intent = new Intent(Settings.ACTION_BLUETOOTH_SETTINGS); break;
            case "mobile": intent = new Intent(Settings.ACTION_NETWORK_OPERATOR_SETTINGS); fallback = Settings.ACTION_WIRELESS_SETTINGS; break;
            case "hotspot": intent = new Intent("android.settings.TETHER_SETTINGS"); fallback = Settings.ACTION_WIRELESS_SETTINGS; break;
            case "vpn": intent = new Intent(Settings.ACTION_VPN_SETTINGS); break;
            case "battery": intent = new Intent(Settings.ACTION_BATTERY_SAVER_SETTINGS); break;
            case "display": intent = new Intent(Settings.ACTION_DISPLAY_SETTINGS); break;
            case "sound": intent = new Intent(Settings.ACTION_SOUND_SETTINGS); break;
            case "dnd": intent = new Intent("android.settings.ZEN_MODE_SETTINGS"); fallback = Settings.ACTION_SOUND_SETTINGS; break;
            case "apps": intent = new Intent(Settings.ACTION_MANAGE_APPLICATIONS_SETTINGS); break;
            case "accessibility": intent = new Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS); break;
            case "home": intent = new Intent(Settings.ACTION_HOME_SETTINGS); break;
            case "shade":
                intent = new Intent().setComponent(new ComponentName(Protocol.QUICKSTEP_PACKAGE,
                        Protocol.QUICKSTEP_PACKAGE + ".GlobalShadeSettingsActivity")); break;
            case "notifications":
                intent = Build.VERSION.SDK_INT >= 30
                        ? new Intent(Settings.ACTION_NOTIFICATION_LISTENER_DETAIL_SETTINGS).putExtra(
                            Settings.EXTRA_NOTIFICATION_LISTENER_COMPONENT_NAME,
                            new ComponentName(Protocol.BRIDGE_PACKAGE, Protocol.BRIDGE_PACKAGE + ".NotificationAccessService").flattenToString())
                        : new Intent(Settings.ACTION_NOTIFICATION_LISTENER_SETTINGS);
                fallback = Settings.ACTION_NOTIFICATION_LISTENER_SETTINGS; break;
            case "brightness": case "rotation":
                intent = new Intent(Settings.ACTION_MANAGE_WRITE_SETTINGS, Uri.parse("package:" + Protocol.BRIDGE_PACKAGE));
                fallback = Settings.ACTION_DISPLAY_SETTINGS; break;
            case "policy": intent = new Intent(Settings.ACTION_NOTIFICATION_POLICY_ACCESS_SETTINGS); break;
            case "torch": case "access":
                intent = new Intent().setComponent(new ComponentName(Protocol.BRIDGE_PACKAGE,
                        Protocol.BRIDGE_PACKAGE + ".BridgeSettingsActivity"));
                intent.putExtra("section", destination); break;
            default: return false;
        }
        if (start(activity, intent)) return true;
        return start(activity, new Intent(fallback));
    }
    private static boolean start(Context activity, Intent intent) {
        // Home is singleInstance. Without CLEAR_TOP Android may merely bring
        // an existing Settings task forward with an unrelated subpage on top.
        intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_CLEAR_TOP);
        try { activity.startActivity(intent); return true; }
        catch (ActivityNotFoundException | SecurityException unavailable) { return false; }
    }
}
