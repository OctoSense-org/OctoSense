package dev.makepad.octosense.contracts;

import android.content.Context;
import android.content.ActivityNotFoundException;
import android.content.ComponentName;
import android.content.Intent;
import android.content.pm.ActivityInfo;
import android.content.pm.ApplicationInfo;
import android.content.pm.PackageManager;
import android.net.Uri;
import android.os.Build;
import android.provider.Settings;

/** A finite set of user-visible Android settings. Never accepts arbitrary intents or components. */
public final class SystemSettings {
    private static final String ENTRY_ACTION = Protocol.HOME_PACKAGE + ".action.SETTINGS";
    private static final String ENTRY_ROUTE = Protocol.HOME_PACKAGE + ".extra.SETTINGS_ROUTE";
    private static final String DEFAULT_APPS_CAPABILITY = Protocol.HOME_PACKAGE + ".SETTINGS_DEFAULT_APPS";
    private static final String DND_CAPABILITY = Protocol.HOME_PACKAGE + ".SETTINGS_DND";
    private SystemSettings() {}

    /** System navigation only; consent and recovery callers must keep using open(). */
    public static boolean openPreferred(Context context, String destination) {
        String route = preferredRoute(destination);
        if (route != null) {
            ComponentName entry = new ComponentName(Protocol.HOME_PACKAGE,
                    Protocol.HOME_PACKAGE + ".SettingsEntry");
            try {
                PackageManager packages = context.getPackageManager();
                ActivityInfo info = packages.getActivityInfo(entry, PackageManager.GET_META_DATA);
                ApplicationInfo app = info.applicationInfo;
                if (info.exported && info.enabled && app != null && app.enabled
                        && Protocol.HOME_PACKAGE.equals(info.packageName)
                        && entry.getClassName().equals(info.name)
                        && (Protocol.HOME_PACKAGE + ".MakepadApp").equals(info.targetActivity)
                        && (app.flags & (ApplicationInfo.FLAG_SYSTEM | ApplicationInfo.FLAG_UPDATED_SYSTEM_APP)) != 0
                        && packages.checkSignatures("android", Protocol.HOME_PACKAGE) == PackageManager.SIGNATURE_MATCH
                        && (!"default_apps".equals(route) || (info.metaData != null
                            && info.metaData.getBoolean(DEFAULT_APPS_CAPABILITY, false)))
                        && (!"dnd".equals(route) || (info.metaData != null
                            && info.metaData.getBoolean(DND_CAPABILITY, false)))
                        && enabled(packages.getApplicationEnabledSetting(Protocol.HOME_PACKAGE))
                        && enabled(packages.getComponentEnabledSetting(entry))) {
                    Intent intent = new Intent(ENTRY_ACTION).setComponent(entry)
                            .putExtra(ENTRY_ROUTE, route);
                    if (start(context, intent)) return true;
                }
            } catch (PackageManager.NameNotFoundException | SecurityException | IllegalArgumentException unavailable) {
                // An older or restricted ROM retains its native Settings route.
            }
        }
        return open(context, destination);
    }

    private static boolean enabled(int state) {
        return state == PackageManager.COMPONENT_ENABLED_STATE_DEFAULT
                || state == PackageManager.COMPONENT_ENABLED_STATE_ENABLED;
    }

    private static String preferredRoute(String destination) {
        if (destination == null) return null;
        switch (destination) {
            case "all": return "overview";
            case "battery": return "battery_policy";
            case "notifications_settings": return "notifications";
            case "dnd": return "dnd";
            case "wifi": case "bluetooth": case "display": case "sound":
            case "apps": case "default_apps": case "system": case "location": case "privacy":
            case "accounts": case "storage": case "about": return destination;
            // Access/credential requests and unmigrated destinations retain
            // their exact native flow; Internet still includes mobile data.
            default: return null;
        }
    }

    public static boolean open(Context activity, String destination) {
        if (destination == null) return false;
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
            case "dnd":
                // Home is singleInstance. A dedicated native recovery task keeps
                // Back from exposing an unrelated pre-existing Settings stack.
                return startFresh(activity,new Intent("android.settings.ZEN_MODE_SETTINGS"))
                        || startFresh(activity,new Intent(Settings.ACTION_SOUND_SETTINGS));
            case "apps": intent = new Intent(Settings.ACTION_MANAGE_APPLICATIONS_SETTINGS); break;
            case "default_apps":
                intent=defaultAppsIntent(activity);if(intent==null)return false;
                fallback=Settings.ACTION_MANAGE_APPLICATIONS_SETTINGS;break;
            case "keyboards":
                return startFresh(activity,new Intent(Settings.ACTION_INPUT_METHOD_SETTINGS))
                        || startFresh(activity,new Intent(Settings.ACTION_SETTINGS));
            case "languages":
                return startFresh(activity,new Intent(Settings.ACTION_LOCALE_SETTINGS))
                        || startFresh(activity,new Intent(Settings.ACTION_SETTINGS));
            case "captions":
                return startFresh(activity,new Intent("android.settings.CAPTIONING_SETTINGS"))
                        || startFresh(activity,new Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS));
            case "accessibility":
                return startFresh(activity,new Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS))
                        || startFresh(activity,new Intent(Settings.ACTION_SETTINGS));
            case "home": intent = new Intent(Settings.ACTION_HOME_SETTINGS); break;
            case "system":
                // AOSP/Lineage expose this page as a component, without a public
                // system-dashboard action. Other devices use the Settings root.
                intent = new Intent().setComponent(new ComponentName("com.android.settings",
                        "com.android.settings.Settings$SystemDashboardActivity")); break;
            case "security": intent = new Intent(Settings.ACTION_SECURITY_SETTINGS); break;
            case "location": intent = new Intent(Settings.ACTION_LOCATION_SOURCE_SETTINGS); break;
            case "privacy": intent = new Intent(Settings.ACTION_PRIVACY_SETTINGS); break;
            case "accounts": intent = new Intent(Settings.ACTION_SYNC_SETTINGS); break;
            case "storage": intent = new Intent(Settings.ACTION_INTERNAL_STORAGE_SETTINGS); break;
            case "about": intent = new Intent(Settings.ACTION_DEVICE_INFO_SETTINGS); break;
            case "all": intent = new Intent(Settings.ACTION_SETTINGS); break;
            case "notifications_settings":
                intent = new Intent(Build.VERSION.SDK_INT >= 33
                        ? Settings.ACTION_ALL_APPS_NOTIFICATION_SETTINGS : "android.settings.NOTIFICATION_SETTINGS");
                fallback = Settings.ACTION_MANAGE_APPLICATIONS_SETTINGS; break;
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
            case "home_write_settings":
                intent = new Intent(Settings.ACTION_MANAGE_WRITE_SETTINGS, Uri.parse("package:" + activity.getPackageName()));
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
    private static Intent defaultAppsIntent(Context context){
        // Google images retain the native class in a renamed system package.
        // SYSTEM provenance also covers Google-signed mainline updates: they
        // need not share the framework's platform signer. Ordinary packages
        // cannot acquire this flag or update either package without its signer.
        PackageManager packages=context.getPackageManager();
        String activity="com.android.permissioncontroller.role.ui.DefaultAppListActivity";
        for(String pkg:new String[]{"com.android.permissioncontroller","com.google.android.permissioncontroller"}){
            ComponentName target=new ComponentName(pkg,activity);
            try{
                ActivityInfo info=packages.getActivityInfo(target,0);ApplicationInfo app=info.applicationInfo;
                if(info.exported&&info.enabled&&pkg.equals(info.packageName)&&activity.equals(info.name)&&app!=null&&app.enabled
                        &&(app.flags&(ApplicationInfo.FLAG_SYSTEM|ApplicationInfo.FLAG_UPDATED_SYSTEM_APP))!=0
                        &&enabled(packages.getApplicationEnabledSetting(pkg))&&enabled(packages.getComponentEnabledSetting(target)))
                    return new Intent().setComponent(target).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK|Intent.FLAG_ACTIVITY_MULTIPLE_TASK|Intent.FLAG_ACTIVITY_EXCLUDE_FROM_RECENTS);
            }catch(PackageManager.NameNotFoundException|SecurityException|IllegalArgumentException unavailable){/* next finite system variant */}
        }
        return null;
    }
    private static boolean startFresh(Context activity,Intent intent) {
        intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK|Intent.FLAG_ACTIVITY_MULTIPLE_TASK|Intent.FLAG_ACTIVITY_EXCLUDE_FROM_RECENTS);
        return start(activity,intent);
    }
    private static boolean start(Context activity, Intent intent) {
        // These are the explicit native fallback controls inside OctoSense.
        // Never resolve them back to Home's public Settings entry alias.
        if(intent.getComponent()==null)intent.setPackage("com.android.settings");
        // Home is singleInstance. Without CLEAR_TOP Android may merely bring
        // an existing Settings task forward with an unrelated subpage on top.
        intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_CLEAR_TOP);
        try { activity.startActivity(intent); return true; }
        catch (ActivityNotFoundException | SecurityException unavailable) { return false; }
    }
}
