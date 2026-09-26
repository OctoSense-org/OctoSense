package dev.makepad.octosense;

/** Navigation only. External callers cannot supply settings values or commands. */
final class SettingsEntryContract {
    static final String ACTION="dev.makepad.octosense.action.SETTINGS";
    static final String EXTRA_ROUTE="dev.makepad.octosense.extra.SETTINGS_ROUTE";
    static final String APP_NOTIFICATIONS="android.settings.APP_NOTIFICATION_SETTINGS";
    private SettingsEntryContract() {}
    static final class Entry {
        final long id;final String route,packageName;
        Entry(long id,String route,String packageName){this.id=id;this.route=route;this.packageName=packageName;}
    }
    /** Worker-owned: JNI enqueue acceptance is not renderer receipt at startup. */
    static final class Delivery {
        private boolean ready;
        private Entry pending;
        private long latest;
        Entry stage(long id,String route) {
            return stage(id,route,null);
        }
        Entry stage(long id,String route,String packageName) {
            if(id<=latest||route==null||("app_notifications".equals(route)
                    ?packageSelector(packageName)==null:known(route)==null||packageName!=null))return null;
            latest=id;pending=new Entry(id,route,packageName);return ready?pending:null;
        }
        Entry ready(){ready=true;return pending;}
        boolean isReady(){return ready;}
        void received(long id){if(pending!=null&&pending.id==id)pending=null;}
        void cancel(){pending=null;}
    }
    /** Untrusted navigation selector only; actual package authority requires a fresh read. */
    static String packageSelector(Object value) {
        if(!(value instanceof String))return null;String name=(String)value;
        if(name.length()>255||(!name.equals("android")&&!name.contains(".")))return null;
        return name.matches("[A-Za-z_][A-Za-z0-9_]*(\\.[A-Za-z_][A-Za-z0-9_]*)*")?name:null;
    }
    static String route(String action,String requested) {
        if(action==null)return null;
        if(ACTION.equals(action))return requested==null?"overview":known(requested);
        // Extras never override the meaning of a standard Android action.
        switch(action) {
            case "android.settings.SETTINGS":return "overview";
            case "android.settings.NIGHT_DISPLAY_SETTINGS":return "display_options";
            case "android.settings.DISPLAY_SETTINGS":return "display";
            case "android.settings.SOUND_SETTINGS":return "sound";
            case "android.settings.WIFI_SETTINGS":return "wifi";
            case "android.settings.BLUETOOTH_SETTINGS":return "bluetooth";
            case "android.settings.MANAGE_APPLICATIONS_SETTINGS":
            case "android.settings.APPLICATION_SETTINGS":return "apps";
            case "android.settings.MANAGE_DEFAULT_APPS_SETTINGS":return "default_apps";
            case "android.settings.SYNC_SETTINGS":return "accounts";
            case "android.settings.LOCATION_SOURCE_SETTINGS":return "location";
            case "android.settings.BATTERY_SAVER_SETTINGS":return "battery_policy";
            case "android.settings.INTERNAL_STORAGE_SETTINGS":return "storage";
            case "android.settings.DATE_SETTINGS":return "date_time";
            case "android.settings.DEVICE_INFO_SETTINGS":return "about";
            case "android.settings.SYSTEM_UPDATE_SETTINGS":return "updates";
            case "android.settings.AIRPLANE_MODE_SETTINGS":
            case "android.settings.DATA_SAVER_SETTINGS":return "advanced_network";
            case "android.settings.PRIVACY_SETTINGS":return "privacy";
            case "android.settings.NOTIFICATION_SETTINGS":return "notifications";
            case "android.settings.ZEN_MODE_SETTINGS":return "dnd";
            default:return null;
        }
    }
    private static String known(String route) {
        switch(route) {
            case "overview":case "search":case "appearance":case "display":case "display_options":case "sound":case "sound_feedback":
            case "wifi":case "bluetooth":case "apps":case "default_apps":case "accounts":case "notifications":
            case "dnd":case "privacy":case "location":case "battery":case "battery_policy":case "storage":
            case "keyboards":case "system_languages":case "accessibility_text_interaction":case "caption_language":case "caption_custom":case "accessibility_hearing":case "accessibility_vision":case "date_time":case "about":case "system":case "updates":case "advanced_network":return route;
            default:return null;
        }
    }
}
