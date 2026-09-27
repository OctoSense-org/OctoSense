import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.pm.ActivityInfo;
import android.content.pm.ApplicationInfo;
import android.content.pm.PackageManager;
import dev.makepad.octosense.contracts.SystemSettings;
import java.util.ArrayList;
import java.util.List;

/** Executes the real routing class against a replaceable package/launch boundary. */
public final class SystemSettingsRoutingTest {
    private static final String HOME = "dev.makepad.octosense";
    private static final String EXTRA = HOME + ".extra.SETTINGS_ROUTE";
    private static void check(boolean value, String why) { if (!value) throw new AssertionError(why); }
    private static final class Packages extends PackageManager {
        ActivityInfo info = new ActivityInfo();
        ActivityInfo controller = new ActivityInfo();
        boolean googleController;
        int signer = SIGNATURE_MATCH, applicationState, componentState, lookups;
        boolean missing, removedAfterLookup;
        Packages() {
            info.name = HOME + ".SettingsEntry"; info.packageName = HOME;
            info.targetActivity = HOME + ".MakepadApp"; info.enabled = true; info.exported = true;
            info.applicationInfo = new ApplicationInfo(); info.applicationInfo.enabled = true;
            info.applicationInfo.flags = ApplicationInfo.FLAG_SYSTEM;
            info.metaData = new android.os.Bundle();
            info.metaData.putBoolean(HOME + ".SETTINGS_DEFAULT_APPS", true);
            info.metaData.putBoolean(HOME + ".SETTINGS_DND", true);
            controller.name="com.android.permissioncontroller.role.ui.DefaultAppListActivity";
            controller.packageName="com.android.permissioncontroller";controller.enabled=true;controller.exported=true;
            controller.applicationInfo=new ApplicationInfo();controller.applicationInfo.enabled=true;controller.applicationInfo.flags=ApplicationInfo.FLAG_SYSTEM;
        }
        @Override public ActivityInfo getActivityInfo(ComponentName name, int flags) throws NameNotFoundException {
            if(name.getPackageName().equals("com.android.permissioncontroller")||name.getPackageName().equals("com.google.android.permissioncontroller")){
                if(name.getPackageName().startsWith("com.google.")!=googleController)throw new NameNotFoundException();return controller;
            }
            lookups++; check(HOME.equals(name.getPackageName()), "only exact Home may be inspected");
            check((flags & GET_META_DATA) != 0, "preferred routing observes declared capabilities");
            if (missing) throw new NameNotFoundException(); return info;
        }
        @Override public int checkSignatures(String a, String b) {
            check("android".equals(a) && (HOME.equals(b)||"com.android.permissioncontroller".equals(b)), "compare finite target with platform, never caller"); return signer;
        }
        @Override public int getApplicationEnabledSetting(String name) {
            if (removedAfterLookup) throw new IllegalArgumentException("removed"); return applicationState;
        }
        @Override public int getComponentEnabledSetting(ComponentName name) { return componentState; }
    }
    private static final class Caller extends Context {
        final Packages packages = new Packages(); final List<Intent> launched = new ArrayList<>();
        boolean entryUnavailable;
        @Override public PackageManager getPackageManager() { return packages; }
        @Override public String getPackageName() { return "independently.signed.caller"; }
        @Override public void startActivity(Intent intent) {
            if (entryUnavailable && intent.getComponent() != null && HOME.equals(intent.getComponent().getPackageName()))
                throw new android.content.ActivityNotFoundException();
            launched.add(intent);
        }
        Intent last() { return launched.get(launched.size() - 1); }
    }
    private static void nativeFallback(Caller caller, String why) {
        check(SystemSettings.openPreferred(caller, "wifi"), why + " fallback launches");
        check(caller.last().getComponent() == null && "com.android.settings".equals(caller.last().getPackage()), why);
    }
    public static void main(String[] args) {
        String[][] routes = {{"all","overview"},{"wifi","wifi"},{"bluetooth","bluetooth"},
                {"display","display"},{"sound","sound"},{"apps","apps"},{"system","system"},
                {"location","location"},{"privacy","privacy"},{"accounts","accounts"},
                {"storage","storage"},{"about","about"},{"battery","battery_policy"},
                {"dnd","dnd"},{"notifications_settings","notifications"},{"default_apps","default_apps"}};
        for (String[] route : routes) {
            Caller caller = new Caller(); check(SystemSettings.openPreferred(caller, route[0]), route[0]);
            Intent sent = caller.last(); check(sent.getComponent().getClassName().equals(HOME + ".SettingsEntry"), "exact alias");
            check(route[1].equals(sent.getStringExtra(EXTRA)), "finite navigation route " + route[0]);
            check(sent.extras.size() == 1 && sent.getAction().equals(HOME + ".action.SETTINGS"), "no mutation parameters");
        }
        Caller updated = new Caller(); updated.packages.info.applicationInfo.flags = ApplicationInfo.FLAG_UPDATED_SYSTEM_APP;
        check(SystemSettings.openPreferred(updated, "all") && updated.last().getComponent() != null, "signed system update");
        Caller ordinary = new Caller(); ordinary.packages.info.applicationInfo.flags = 0; nativeFallback(ordinary, "ordinary Home");
        Caller signer = new Caller(); signer.packages.signer = -3; nativeFallback(signer, "non-platform signer");
        Caller missing = new Caller(); missing.packages.missing = true; nativeFallback(missing, "older Home without alias");
        Caller activityDisabled = new Caller(); activityDisabled.packages.info.enabled = false; nativeFallback(activityDisabled, "disabled alias");
        Caller appDisabled = new Caller(); appDisabled.packages.info.applicationInfo.enabled = false; nativeFallback(appDisabled, "disabled app");
        Caller unexported = new Caller(); unexported.packages.info.exported = false; nativeFallback(unexported, "private alias");
        Caller wrongTarget = new Caller(); wrongTarget.packages.info.targetActivity = HOME + ".Other"; nativeFallback(wrongTarget, "wrong renderer");
        for (int disabled : new int[]{2,3,4}) {
            Caller component = new Caller(); component.packages.componentState = disabled; nativeFallback(component, "component override " + disabled);
            Caller app = new Caller(); app.packages.applicationState = disabled; nativeFallback(app, "app override " + disabled);
        }
        Caller removed = new Caller(); removed.packages.removedAfterLookup = true; nativeFallback(removed, "removal during observation");
        Caller launchDenied = new Caller(); launchDenied.entryUnavailable = true; nativeFallback(launchDenied, "launch race");
        for (String destination : new String[]{"internet","mobile","hotspot","vpn","accessibility","home","security", "notifications", "brightness", "rotation", "home_write_settings", "policy", "torch", "access", "shade"}) {
            Caller caller = new Caller(); check(SystemSettings.openPreferred(caller, destination), "preserved flow " + destination);
            check(caller.packages.lookups == 0, "consent/unmigrated flow must not inspect preferred alias");
            check(!caller.last().extras.containsKey(EXTRA), "consent is not generic navigation");
        }
        Caller nativeOnly = new Caller(); check(SystemSettings.open(nativeOnly, "wifi"), "native open");
        check(nativeOnly.packages.lookups == 0 && "com.android.settings".equals(nativeOnly.last().getPackage()), "recovery never prefers Home");
        Caller defaults = new Caller();check(SystemSettings.open(defaults,"default_apps"),"native defaults recovery");
        check(defaults.last().getComponent().getClassName().equals("com.android.permissioncontroller.role.ui.DefaultAppListActivity"),"platform controller owns defaults UI");
        int fresh=Intent.FLAG_ACTIVITY_NEW_TASK|Intent.FLAG_ACTIVITY_MULTIPLE_TASK|Intent.FLAG_ACTIVITY_EXCLUDE_FROM_RECENTS;
        check((defaults.last().flags&fresh)==fresh,"defaults recovery cannot reuse an unrelated controller task");
        for (int oldCapability : new int[]{0,1}) {
            Caller old = new Caller();
            if(oldCapability==0)old.packages.info.metaData=null;
            else old.packages.info.metaData.putBoolean(HOME + ".SETTINGS_DEFAULT_APPS", false);
            check(SystemSettings.openPreferred(old,"default_apps"),"older Home uses native Defaults");
            check(old.last().getComponent().getPackageName().equals("com.android.permissioncontroller"),"unsupported alias cannot swallow Defaults");
            check((old.last().flags&fresh)==fresh,"older Home preserves native fresh-task recovery");
            check(SystemSettings.openPreferred(old,"sound")&&old.last().getComponent().getPackageName().equals(HOME),"existing preferred routes need no new capability");
        }
        Caller dndNative = new Caller();check(SystemSettings.open(dndNative,"dnd"),"native DND recovery");
        check("com.android.settings".equals(dndNative.last().getPackage()) && (dndNative.last().flags&fresh)==fresh,"DND recovery is native and cannot reuse an old Settings task");
        Caller visionNative = new Caller();check(SystemSettings.open(visionNative,"accessibility"),"native accessibility recovery");
        check("com.android.settings".equals(visionNative.last().getPackage()) && (visionNative.last().flags&fresh)==fresh,"Accessibility recovery returns to Home instead of a previous native Settings stack");
        for (int oldCapability : new int[]{0,1}) {
            Caller old = new Caller();
            if(oldCapability==0)old.packages.info.metaData=null;
            else old.packages.info.metaData.putBoolean(HOME + ".SETTINGS_DND",false);
            check(SystemSettings.openPreferred(old,"dnd"),"old Home retains native DND");
            check("com.android.settings".equals(old.last().getPackage()) && (old.last().flags&fresh)==fresh,"old Home cannot swallow the new DND route");
        }
        Caller ordinaryDefaults=new Caller();ordinaryDefaults.packages.info.applicationInfo.flags=0;
        check(SystemSettings.openPreferred(ordinaryDefaults,"default_apps")&&ordinaryDefaults.last().getComponent().getPackageName().equals("com.android.permissioncontroller"),"ordinary Home retains native Defaults");
        Caller google=new Caller();google.packages.googleController=true;google.packages.controller.packageName="com.google.android.permissioncontroller";google.packages.signer=-3;
        check(SystemSettings.open(google,"default_apps")&&google.last().getComponent().getPackageName().equals("com.google.android.permissioncontroller"),"Google system controller may use its mainline signer");
        Caller impostorController=new Caller();impostorController.packages.controller.applicationInfo.flags=0;
        check(!SystemSettings.open(impostorController,"default_apps")&&impostorController.launched.isEmpty(),"untrusted controller cannot receive defaults navigation");
        Caller hiddenController=new Caller();hiddenController.packages.controller.exported=false;
        check(!SystemSettings.open(hiddenController,"default_apps")&&hiddenController.launched.isEmpty(),"private controller activity cannot receive recovery");
        Caller invalid = new Caller(); check(!SystemSettings.openPreferred(invalid, null), "null rejected");
        check(!SystemSettings.openPreferred(invalid, "arbitrary.intent"), "unknown rejected");
        check(invalid.launched.isEmpty() && invalid.packages.lookups == 0, "invalid causes no launch");
    }
}
