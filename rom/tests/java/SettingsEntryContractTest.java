package dev.makepad.octosense;

public final class SettingsEntryContractTest {
    private static void check(boolean condition){if(!condition)throw new AssertionError();}
    public static void main(String[] args) {
        check("overview".equals(SettingsEntryContract.route(SettingsEntryContract.ACTION,null)));
        check("keyboards".equals(SettingsEntryContract.route(SettingsEntryContract.ACTION,"keyboards")));
        check(SettingsEntryContract.route("android.settings.INPUT_METHOD_SETTINGS","keyboards")==null);
        check("date_time".equals(SettingsEntryContract.route(SettingsEntryContract.ACTION,"date_time")));
        check("display".equals(SettingsEntryContract.route("android.settings.DISPLAY_SETTINGS","updates")));
        check("display_options".equals(SettingsEntryContract.route("android.settings.NIGHT_DISPLAY_SETTINGS","updates")));
        check("display_options".equals(SettingsEntryContract.route(SettingsEntryContract.ACTION,"display_options")));
        check("accessibility_text_interaction".equals(SettingsEntryContract.route(SettingsEntryContract.ACTION,"accessibility_text_interaction")));
        check("accessibility_vision".equals(SettingsEntryContract.route(SettingsEntryContract.ACTION,"accessibility_vision")));
        check("sound_feedback".equals(SettingsEntryContract.route(SettingsEntryContract.ACTION,"sound_feedback")));
        check("apps".equals(SettingsEntryContract.route("android.settings.APPLICATION_SETTINGS","erase")));
        check("default_apps".equals(SettingsEntryContract.route("android.settings.MANAGE_DEFAULT_APPS_SETTINGS","erase")));
        check("default_apps".equals(SettingsEntryContract.route(SettingsEntryContract.ACTION,"default_apps")));
        check("dnd".equals(SettingsEntryContract.route("android.settings.ZEN_MODE_SETTINGS","erase")));
        check("dnd".equals(SettingsEntryContract.route(SettingsEntryContract.ACTION,"dnd")));
        for(String invalid:new String[]{"","DISPLAY"," display","display\n","../../settings","android.settings.SETTINGS",
                "factory_reset","install_update","uninstall","set_volume","package:com.android.settings","wifi;reboot","wifi\0"})
            check(SettingsEntryContract.route(SettingsEntryContract.ACTION,invalid)==null);
        for(String unsupported:new String[]{"android.settings.APPLICATION_DETAILS_SETTINGS","android.settings.MANAGE_WRITE_SETTINGS",
                "android.settings.SECURITY_SETTINGS","android.settings.ACCESSIBILITY_SETTINGS","android.intent.action.VIEW",
                "android.intent.action.MAIN","android.intent.action.FACTORY_RESET","android.intent.action.REBOOT",null})
            check(SettingsEntryContract.route(unsupported,"overview")==null);
        check(SettingsEntryContract.route(SettingsEntryContract.ACTION,"x".repeat(65536))==null);
        SettingsEntryContract.Delivery delivery=new SettingsEntryContract.Delivery();
        check(!delivery.isReady());
        check(delivery.stage(1,"date_time")==null); // Never publish before renderer readiness.
        check(delivery.stage(2,"display")==null);
        SettingsEntryContract.Entry pending=delivery.ready();check(delivery.isReady()&&pending.id==2&&pending.route.equals("display"));
        delivery.received(1);check(delivery.ready()==pending); // Old receipts cannot discard newer navigation.
        delivery.received(2);check(delivery.ready()==null); // Resume cannot replay a delivered entry.
        check(delivery.stage(2,"wifi")==null);
        check(delivery.stage(3,"sound").id==3);delivery.cancel();check(delivery.ready()==null);
        check(delivery.stage(3,"sound")==null);check(delivery.stage(4,"overview").id==4);
        for(Object invalid:new Object[]{null,17,true,"", "no_dot", "com.example/intent", "com..bad", "com.example.1bad", "com.example\n", "中文.app", "a".repeat(256)})
            check(SettingsEntryContract.packageSelector(invalid)==null);
        check("android".equals(SettingsEntryContract.packageSelector("android")));
        check("com.example.app".equals(SettingsEntryContract.packageSelector("com.example.app")));
        // A custom route must not create a package-targeted entry without the standard action parser.
        check(SettingsEntryContract.route(SettingsEntryContract.ACTION,"app_notifications")==null);
        check(delivery.stage(5,"apps","com.example.app")==null);
        check(delivery.stage(5,"app_notifications",null)==null);
        pending=delivery.stage(5,"app_notifications","com.example.app");
        check(pending.id==5&&pending.route.equals("app_notifications")&&pending.packageName.equals("com.example.app"));
        check(delivery.stage(6,"app_notifications","com.example.next").packageName.equals("com.example.next"));
        delivery.received(5);check(delivery.ready().id==6);delivery.received(6);check(delivery.ready()==null);
        check(delivery.stage(6,"app_notifications","com.example.app")==null);
        check(delivery.stage(7,"default_apps","com.example.app")==null);
        check(delivery.stage(7,"default_apps",null).route.equals("default_apps"));
    }
}
