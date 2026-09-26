package dev.makepad.octosense;

import android.Manifest;
import android.app.AppOpsManager;
import android.app.admin.DevicePolicyManager;
import android.app.usage.StorageStats;
import android.app.usage.StorageStatsManager;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.pm.ApplicationInfo;
import android.content.pm.PackageInfo;
import android.content.pm.PackageManager;
import android.content.pm.PermissionInfo;
import android.content.pm.ResolveInfo;
import android.net.Uri;
import android.os.Process;
import android.os.UserHandle;
import android.os.UserManager;
import android.os.storage.StorageManager;
import android.provider.Settings;
import android.text.format.DateFormat;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.text.Collator;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.Date;
import java.util.List;
import java.util.Locale;
import java.util.UUID;
import org.json.JSONArray;
import org.json.JSONObject;

/** Current-user package observations and finite user-facing activities. Queries run on the worker. */
final class AppsSettingsBackend {
    private final Context context;
    private final PackageManager packages;
    AppsSettingsBackend(Context context) {this.context=context;packages=context.getPackageManager();}
    private static JSONObject packet(long id) throws Exception {return new JSONObject().put("schema",1).put("request_id",id);}
    private static String displayText(CharSequence value,int max) {
        if(value==null) return "";
        StringBuilder out=new StringBuilder();
        value.codePoints().filter(c -> !Character.isISOControl(c)&&!(c>=0x202a&&c<=0x202e)&&!(c>=0x2066&&c<=0x2069))
                .limit(max).forEach(out::appendCodePoint);
        return out.toString();
    }
    private String label(ApplicationInfo info) {
        try {String text=displayText(info.loadLabel(packages),160);return text.isEmpty()?info.packageName:text;}
        catch(RuntimeException unavailable) {return info.packageName;}
    }
    private static boolean system(ApplicationInfo app) {return (app.flags&(ApplicationInfo.FLAG_SYSTEM|ApplicationInfo.FLAG_UPDATED_SYSTEM_APP))!=0;}
    private static boolean installed(ApplicationInfo app) {
        return (app.flags&ApplicationInfo.FLAG_INSTALLED)!=0&&Process.myUserHandle().equals(UserHandle.getUserHandleForUid(app.uid));
    }
    private static boolean suspended(ApplicationInfo app) {return (app.flags&ApplicationInfo.FLAG_SUSPENDED)!=0;}
    private ApplicationInfo application(String name) throws PackageManager.NameNotFoundException {
        AppsSettingsContract.packageName(name);
        ApplicationInfo app=packages.getApplicationInfo(name,0);
        if(!installed(app)) throw new PackageManager.NameNotFoundException(name);
        return app;
    }
    private boolean launchable(ApplicationInfo app) {
        return app.enabled&&!suspended(app)&&!app.packageName.equals(context.getPackageName())&&packages.getLaunchIntentForPackage(app.packageName)!=null;
    }
    private static final class Row {
        final ApplicationInfo app;final String label;final boolean launchable;
        Row(ApplicationInfo app,String label,boolean launchable) {this.app=app;this.label=label;this.launchable=launchable;}
        JSONObject json() throws Exception {return new JSONObject().put("package",app.packageName).put("label",label)
                .put("system",system(app)).put("enabled",app.enabled).put("launchable",launchable);}
    }
    JSONObject catalog(long id,String query,boolean includeSystem,int offset,String expectedGeneration) throws Exception {
        AppsSettingsContract.query(query);AppsSettingsContract.offset(offset);AppsSettingsContract.generation(expectedGeneration);
        if(offset>0&&expectedGeneration==null) throw new IllegalArgumentException("Pagination needs an observed generation");
        String needle=query.trim().toLowerCase(Locale.ROOT);
        ArrayList<Row> rows=new ArrayList<>();
        for(ApplicationInfo app:packages.getInstalledApplications(0)) {
            if(!installed(app)||(!includeSystem&&system(app))) continue;
            try {AppsSettingsContract.packageName(app.packageName);} catch(IllegalArgumentException invalid) {continue;}
            String label=label(app);
            if(!label.toLowerCase(Locale.ROOT).contains(needle)&&!app.packageName.toLowerCase(Locale.ROOT).contains(needle)) continue;
            rows.add(new Row(app,label,launchable(app)));
        }
        if(rows.size()>AppsSettingsContract.MAX_OFFSET) throw new IllegalStateException("Catalog too large");
        Collator collator=Collator.getInstance();
        rows.sort((a,b) -> {int result=collator.compare(a.label,b.label);return result!=0?result:a.app.packageName.compareTo(b.app.packageName);});
        MessageDigest digest=MessageDigest.getInstance("SHA-256");
        digest.update((query+"\0"+includeSystem+"\0"+Locale.getDefault().toLanguageTag()).getBytes(StandardCharsets.UTF_8));
        for(Row row:rows) {digest.update((byte)0);digest.update(row.json().toString().getBytes(StandardCharsets.UTF_8));}
        StringBuilder generation=new StringBuilder();for(byte value:digest.digest()) generation.append(String.format(Locale.ROOT,"%02x",value&255));
        boolean stale=expectedGeneration!=null&&(!expectedGeneration.equals(generation.toString())||(offset>0&&offset>=rows.size()));
        if(stale||offset>=rows.size()) offset=0;
        JSONArray page=new JSONArray();
        for(int i=offset;i<Math.min(offset+AppsSettingsContract.PAGE_SIZE,rows.size());i++) page.put(rows.get(i).json());
        return packet(id).put("query",query).put("include_system",includeSystem).put("offset",offset)
                .put("page_size",AppsSettingsContract.PAGE_SIZE).put("total",rows.size()).put("generation",generation.toString())
                .put("stale",stale).put("apps",page);
    }
    private boolean restricted(String restriction) {
        UserManager users=context.getSystemService(UserManager.class);
        return users==null||!users.isUserUnlocked()||users.hasUserRestriction(restriction);
    }
    private boolean uninstallable(ApplicationInfo app) {
        if(system(app)||AppsSettingsContract.protectedPackage(app.packageName)||app.packageName.equals(context.getPackageName())
                ||restricted(UserManager.DISALLOW_UNINSTALL_APPS)||restricted(UserManager.DISALLOW_APPS_CONTROL)
                ||context.checkSelfPermission(Manifest.permission.REQUEST_DELETE_PACKAGES)!=PackageManager.PERMISSION_GRANTED) return false;
        DevicePolicyManager policy=context.getSystemService(DevicePolicyManager.class);
        if(policy==null) return false;
        try {
            List<ComponentName> admins=policy.getActiveAdmins();
            if(admins!=null&&admins.stream().anyMatch(admin -> admin.getPackageName().equals(app.packageName))) return false;
            if(policy.isUninstallBlocked(null,app.packageName)) return false;
        } catch(SecurityException unavailable) {return false;}
        return true;
    }
    private Intent systemIntent(Intent intent) {
        ResolveInfo resolved=packages.resolveActivity(intent,PackageManager.MATCH_DEFAULT_ONLY|PackageManager.MATCH_SYSTEM_ONLY);
        if(resolved==null||resolved.activityInfo==null||!system(resolved.activityInfo.applicationInfo)) return null;
        return intent.setComponent(new ComponentName(resolved.activityInfo.packageName,resolved.activityInfo.name));
    }
    private boolean declaredLanguagePicker(ApplicationInfo app) {
        try {
        // Recovery is offered for positively identified native-picker targets.
        // The privileged built-in collector separately supports legacy asset-only apps.
        if(android.os.Build.VERSION.SDK_INT<33||!launchable(app)
                ||packages.checkSignatures("android",app.packageName)==PackageManager.SIGNATURE_MATCH)return false;
        android.content.res.Resources nativeResources=packages.getResourcesForApplication("com.android.settings");
        int exclusions=nativeResources.getIdentifier("config_disallowed_app_localeChange_packages","array","com.android.settings");
        if(exclusions==0||java.util.Arrays.asList(nativeResources.getStringArray(exclusions)).contains(app.packageName))return false;
        android.app.LocaleConfig config=new android.app.LocaleConfig(context.createPackageContext(app.packageName,0));
        return config.getStatus()==android.app.LocaleConfig.STATUS_SUCCESS&&config.getSupportedLocales()!=null&&!config.getSupportedLocales().isEmpty();
        }catch(PackageManager.NameNotFoundException|android.content.res.Resources.NotFoundException|SecurityException|LinkageError unavailable){return false;}
    }
    Intent actionIntent(String name,AppsSettingsContract.Action action) throws Exception {
        ApplicationInfo app=application(name);
        switch(action) {
            case LAUNCH: return launchable(app)?packages.getLaunchIntentForPackage(name):null;
            // Use the Android confirmation activity; never silently remove a
            // package through a privileged PackageInstaller/Binder operation.
            case UNINSTALL: return uninstallable(app)?systemIntent(new Intent(Intent.ACTION_UNINSTALL_PACKAGE,Uri.fromParts("package",name,null))):null;
            case APP_INFO: return systemIntent(new Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS,Uri.fromParts("package",name,null))
                    .setPackage("com.android.settings")
                    .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK|Intent.FLAG_ACTIVITY_MULTIPLE_TASK|Intent.FLAG_ACTIVITY_EXCLUDE_FROM_RECENTS));
            // Home is singleInstance: a native activity cannot join its task.
            // A fresh recovery task makes Back return to Home instead of an
            // unrelated Settings screen already open in Android's old task.
            // Keep that existing task intact, and never resolve this recovery
            // action back to a privileged OctoSense public Settings entry.
            case LANGUAGE: return declaredLanguagePicker(app)?systemIntent(new Intent(Settings.ACTION_APP_LOCALE_SETTINGS,Uri.fromParts("package",name,null))
                    .setPackage("com.android.settings")
                    .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK|Intent.FLAG_ACTIVITY_MULTIPLE_TASK|Intent.FLAG_ACTIVITY_EXCLUDE_FROM_RECENTS)):null;
            case NOTIFICATIONS: return systemIntent(new Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS)
                    .setPackage("com.android.settings")
                    .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK|Intent.FLAG_ACTIVITY_MULTIPLE_TASK|Intent.FLAG_ACTIVITY_EXCLUDE_FROM_RECENTS)
                    .putExtra(Settings.EXTRA_APP_PACKAGE,name));
            default: throw new IllegalArgumentException("Unsupported app action");
        }
    }
    Intent usageIntent() {
        return systemIntent(new Intent(Settings.ACTION_USAGE_ACCESS_SETTINGS,Uri.fromParts("package",context.getPackageName(),null)));
    }
    private boolean usageAccess() {
        AppOpsManager ops=context.getSystemService(AppOpsManager.class);
        if(ops==null) return false;
        int mode=ops.checkOpNoThrow(AppOpsManager.OPSTR_GET_USAGE_STATS,Process.myUid(),context.getPackageName());
        return mode==AppOpsManager.MODE_ALLOWED||(mode==AppOpsManager.MODE_DEFAULT
                &&context.checkSelfPermission(Manifest.permission.PACKAGE_USAGE_STATS)==PackageManager.PERMISSION_GRANTED);
    }
    private JSONObject storage(ApplicationInfo app) throws Exception {
        JSONObject result=new JSONObject().put("available",false).put("can_request_usage_access",!usageAccess()&&usageIntent()!=null);
        String[] peers=packages.getPackagesForUid(app.uid);
        result.put("shared_uid",peers!=null&&peers.length>1);
        try {
            StorageStatsManager manager=context.getSystemService(StorageStatsManager.class);
            if(manager!=null) {
                UUID volume=app.storageUuid==null?StorageManager.UUID_DEFAULT:app.storageUuid;
                StorageStats stats=manager.queryStatsForPackage(volume,app.packageName,Process.myUserHandle());
                result.put("app_bytes",stats.getAppBytes()).put("data_bytes",stats.getDataBytes()).put("cache_bytes",stats.getCacheBytes())
                        .put("available",true).put("can_request_usage_access",false);
            }
        } catch(SecurityException|java.io.IOException|PackageManager.NameNotFoundException unavailable) { }
        return result;
    }
    private String date(long milliseconds) {
        Date value=new Date(milliseconds);
        return DateFormat.getDateFormat(context).format(value)+" "+DateFormat.getTimeFormat(context).format(value);
    }
    JSONObject details(long id,String name,int permissionOffset) throws Exception {
        AppsSettingsContract.packageName(name);AppsSettingsContract.offset(permissionOffset);
        JSONObject result=packet(id).put("package",name).put("exists",false).put("actions",new JSONArray());
        ApplicationInfo app;PackageInfo info;
        try {app=application(name);info=packages.getPackageInfo(name,PackageManager.GET_PERMISSIONS);}
        catch(PackageManager.NameNotFoundException removed) {return result;}
        result.put("exists",true).put("label",label(app)).put("system",system(app)).put("enabled",app.enabled).put("suspended",suspended(app))
                .put("version_name",info.versionName==null?JSONObject.NULL:displayText(info.versionName,160))
                .put("version_code",android.os.Build.VERSION.SDK_INT>=28?info.getLongVersionCode():info.versionCode)
                .put("target_sdk",app.targetSdkVersion).put("first_install_ms",info.firstInstallTime).put("last_update_ms",info.lastUpdateTime)
                .put("first_install_text",date(info.firstInstallTime)).put("last_update_text",date(info.lastUpdateTime));
        JSONArray actions=new JSONArray();
        for(AppsSettingsContract.Action action:AppsSettingsContract.Action.values()) if(actionIntent(name,action)!=null) actions.put(action.wire);
        result.put("actions",actions).put("storage",storage(app));
        ArrayList<Integer> order=new ArrayList<>();
        String[] names=info.requestedPermissions==null?new String[0]:info.requestedPermissions;
        for(int i=0;i<names.length;i++) order.add(i);
        order.sort(Comparator.comparing(index -> names[index]));
        if(permissionOffset>=order.size()) permissionOffset=0;
        JSONArray permissions=new JSONArray();
        for(int page=permissionOffset;page<Math.min(permissionOffset+AppsSettingsContract.PAGE_SIZE,order.size());page++) {
            int index=order.get(page);String permission=names[index];
            JSONObject item=new JSONObject().put("name",displayText(permission,255));
            item.put("granted",info.requestedPermissionsFlags!=null&&index<info.requestedPermissionsFlags.length
                    ? (Object)((info.requestedPermissionsFlags[index]&PackageInfo.REQUESTED_PERMISSION_GRANTED)!=0):JSONObject.NULL);
            try {
                PermissionInfo definition=packages.getPermissionInfo(permission,0);
                int protection=definition.protectionLevel&PermissionInfo.PROTECTION_MASK_BASE;
                item.put("label",displayText(definition.loadLabel(packages),160)).put("runtime",protection==PermissionInfo.PROTECTION_DANGEROUS)
                        .put("protection",protection==PermissionInfo.PROTECTION_DANGEROUS?"runtime":protection==PermissionInfo.PROTECTION_NORMAL?"normal":
                                protection==PermissionInfo.PROTECTION_SIGNATURE||protection==PermissionInfo.PROTECTION_SIGNATURE_OR_SYSTEM?"signature":"other");
            } catch(PackageManager.NameNotFoundException missing) {item.put("runtime",JSONObject.NULL).put("protection",JSONObject.NULL);}
            permissions.put(item);
        }
        return result.put("permissions",permissions).put("permissions_total",order.size()).put("permissions_offset",permissionOffset);
    }
}
