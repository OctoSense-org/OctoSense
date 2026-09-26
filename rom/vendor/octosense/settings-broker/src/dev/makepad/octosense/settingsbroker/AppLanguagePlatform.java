package dev.makepad.octosense.settingsbroker;

import android.app.ActivityManager;
import android.app.KeyguardManager;
import android.app.LocaleConfig;
import android.app.LocaleManager;
import android.content.Context;
import android.content.Intent;
import android.content.pm.ApplicationInfo;
import android.content.pm.PackageInfo;
import android.content.pm.PackageManager;
import android.os.LocaleList;
import android.os.Process;
import android.os.UserHandle;
import android.os.UserManager;
import android.util.FeatureFlagUtils;
import com.android.internal.app.AppLocaleCollector;
import com.android.internal.app.LocaleHelper;
import com.android.internal.app.LocaleStore;
import dev.makepad.octosense.applanguage.AppLanguageBackend;
import dev.makepad.octosense.applanguage.AppLanguageContract;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Objects;
import java.util.Set;

/** Same native collector and selection semantics as LocalePickerWithRegion; no tag API. */
final class AppLanguagePlatform implements AppLanguageBackend.Platform {
    private final Context context;
    AppLanguagePlatform(Context context){this.context=context;}
    private static final class State extends AppLanguageBackend.State {
        final Map<String,LocaleStore.LocaleInfo> targets=new HashMap<>();
        LocaleList actual;
    }
    private boolean owner(){UserManager users=context.getSystemService(UserManager.class);KeyguardManager lock=context.getSystemService(KeyguardManager.class);return Process.myUid()==Process.SYSTEM_UID&&UserHandle.myUserId()==0&&ActivityManager.getCurrentUser()==0&&users!=null&&users.isUserUnlocked()&&lock!=null&&!lock.isKeyguardLocked();}
    private static String text(CharSequence raw,String fallback){StringBuilder out=new StringBuilder();if(raw!=null)raw.toString().codePoints().filter(c->!Character.isISOControl(c)).limit(256).forEach(out::appendCodePoint);return out.toString().trim().isEmpty()?fallback:out.toString();}
    private State denied(String pkg,String availability,String reason){State out=new State();out.packageName=pkg;out.availability=availability;out.reason=reason;return out;}
    private Context nativeSettings()throws Exception{
        PackageManager pm=context.getPackageManager();ApplicationInfo info=pm.getApplicationInfo("com.android.settings",0);
        if((info.flags&ApplicationInfo.FLAG_SYSTEM)==0||pm.checkSignatures("android","com.android.settings")!=PackageManager.SIGNATURE_MATCH)throw new SecurityException("Untrusted native Settings resources");
        return context.createPackageContext("com.android.settings",0);
    }
    @Override public AppLanguageBackend.State read(String pkg)throws Exception{
        AppLanguageContract.packageName(pkg);
        if(!owner())return denied(pkg,"restricted","locked");
        PackageManager pm=context.getPackageManager();PackageInfo info;
        try{info=pm.getPackageInfo(pkg,PackageManager.GET_SIGNING_CERTIFICATES);}catch(PackageManager.NameNotFoundException missing){return denied(pkg,"missing","app_missing");}
        ApplicationInfo app=info.applicationInfo;
        if(app==null||UserHandle.getUserId(app.uid)!=0)throw new SecurityException("Wrong language target user");
        String[] peers=pm.getPackagesForUid(app.uid);if(peers==null||!Arrays.asList(peers).contains(pkg))throw new SecurityException("Wrong language target UID");
        Context settings=nativeSettings();int exclusions=settings.getResources().getIdentifier("config_disallowed_app_localeChange_packages","array","com.android.settings");
        if(exclusions==0)throw new IllegalStateException("Unknown native locale eligibility");
        boolean excluded=Arrays.asList(settings.getResources().getStringArray(exclusions)).contains(pkg);
        boolean launcher=pm.queryIntentActivities(new Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LAUNCHER).setPackage(pkg),0).stream().anyMatch(r->r.activityInfo!=null&&r.activityInfo.applicationInfo.uid==app.uid&&pkg.equals(r.activityInfo.packageName));
        if(excluded||app.isSignedWithPlatformKey()||!launcher)return denied(pkg,"unsupported","not_supported");
        Context target=context.createPackageContext(pkg,0);LocaleConfig config=new LocaleConfig(target);int status=config.getStatus();
        LocaleList supported=config.getSupportedLocales();String[] assets=target.getResources().getAssets().getNonSystemLocales();if(assets==null)assets=new String[0];Arrays.sort(assets);
        boolean optIn=FeatureFlagUtils.isEnabled(context,FeatureFlagUtils.SETTINGS_APP_LOCALE_OPT_IN_ENABLED);
        if(status==LocaleConfig.STATUS_SUCCESS){if(supported==null||supported.isEmpty())return denied(pkg,"unsupported","not_supported");}
        else if(status!=LocaleConfig.STATUS_NOT_SPECIFIED)return denied(pkg,"unavailable","invalid_config");
        else if(optIn||assets.length==0)return denied(pkg,"unsupported","not_supported");
        LocaleManager manager=context.getSystemService(LocaleManager.class);if(manager==null)return denied(pkg,"unavailable","service_unavailable");
        State out=new State();out.packageName=pkg;out.label=text(app.loadLabel(pm),pkg);out.actual=manager.getApplicationLocales(pkg);
        if(out.actual==null||out.actual.size()>AppLanguageContract.MAX_CURRENT)return denied(pkg,"unavailable","catalog_limit");
        for(int i=0;i<out.actual.size();i++){Locale locale=out.actual.get(i);out.current.add(new AppLanguageBackend.Current(locale.toLanguageTag(),text(locale.getDisplayName(Locale.getDefault()),locale.toLanguageTag())));}
        if(info.signingInfo==null)throw new IllegalStateException("Unknown app signer");List<String> signatures=new ArrayList<>();for(android.content.pm.Signature signature:info.signingInfo.getApkContentsSigners())signatures.add(AppLanguageContract.hash(signature.toCharsString()));java.util.Collections.sort(signatures);
        android.system.StructStat source=android.system.Os.stat(app.sourceDir);
        out.incarnation=AppLanguageContract.hash(pkg,String.valueOf(app.uid),String.valueOf(info.firstInstallTime),String.valueOf(info.lastUpdateTime),String.valueOf(info.getLongVersionCode()),app.sourceDir,String.valueOf(source.st_dev),String.valueOf(source.st_ino),signatures.toString());
        AppLocaleCollector collector=new AppLocaleCollector(context,pkg);
        build(out,collector,null,"","language",1);
        // Include the full native tree, current custom list and eligibility, not STOPPED state.
        List<String> revision=new ArrayList<>(Arrays.asList(out.incarnation,String.valueOf(status),supported==null?"":supported.toLanguageTags(),Arrays.toString(assets),String.valueOf(optIn),String.valueOf(excluded),String.valueOf(launcher),String.valueOf(app.isSignedWithPlatformKey()),out.actual.toLanguageTags(),LocaleList.getDefault().toLanguageTags(),out.label,"writable=true"));
        for(AppLanguageBackend.Node node:out.nodes)revision.add(AppLanguageContract.hash(node.id,node.parent,node.kind,node.level,node.target,node.label,node.secondary,String.valueOf(node.selected),String.valueOf(node.systemDefault),String.valueOf(node.suggested)));
        out.revision=AppLanguageContract.hash(revision.toArray(new String[0]));out.availability="available";out.reason="none";out.writable=true;
        if(!owner())return denied(pkg,"restricted","locked");return out;
    }
    private String systemTitle(){int id=context.getResources().getIdentifier("system_locale_title","string","android");if(id==0)throw new IllegalStateException("Missing native system language label");return context.getString(id);}
    private static String semantic(LocaleStore.LocaleInfo locale){return locale.isSystemLocale()?"system":locale.getLocale().toLanguageTag();}
    private List<LocaleStore.LocaleInfo> children(AppLocaleCollector collector,LocaleStore.LocaleInfo parent){
        Set<LocaleStore.LocaleInfo> actual=collector.getSupportedLocaleList(parent,false,parent!=null);if(actual==null)throw new IllegalStateException("Missing native locale collection");
        List<LocaleStore.LocaleInfo> rows=new ArrayList<>(actual);LocaleHelper.LocaleInfoComparator order=new LocaleHelper.LocaleInfoComparator(parent==null?Locale.getDefault():parent.getLocale(),parent!=null);
        rows.sort((a,b)->{int compared=order.compare(a,b);if(compared!=0)return compared;int stable=semantic(a).compareTo(semantic(b));if(stable!=0)return stable;return Boolean.compare(b.isSuggested(),a.isSuggested());});return rows;
    }
    private void build(State out,AppLocaleCollector collector,LocaleStore.LocaleInfo parent,String parentId,String level,int depth){buildRows(out,collector,children(collector,parent),parentId,level,depth);}
    private void buildRows(State out,AppLocaleCollector collector,List<LocaleStore.LocaleInfo> rows,String parentId,String level,int depth){
        if(depth>AppLanguageContract.MAX_DEPTH)throw new IllegalStateException("Native locale hierarchy too deep");
        Set<String> siblingIds=new java.util.HashSet<>();
        for(LocaleStore.LocaleInfo row:rows){
            if(!siblingIds.add(semantic(row)))throw new IllegalStateException("Ambiguous native locale identity");
            if(out.nodes.size()>=AppLanguageContract.MAX_NODES)throw new IllegalStateException("Native locale hierarchy too large");
            boolean direct=row.isSystemLocale()||row.isSuggested()||(row.getParent()!=null&&!row.hasNumberingSystems())||level.equals("numbering");
            String nextLevel=row.hasNumberingSystems()?"numbering":"region";List<LocaleStore.LocaleInfo> next=null;LocaleStore.LocaleInfo selected=row;
            if(!direct){next=children(collector,row);if(next.size()==1){selected=next.get(0);direct=true;}else if(next.isEmpty())continue;}
            String id=AppLanguageContract.hash(parentId,semantic(row));
            String label=row.isSystemLocale()?systemTitle():level.equals("numbering")?LocaleHelper.getDisplayNumberingSystemKeyValue(row.getLocale(),row.getLocale()):level.equals("region")?row.getFullCountryNameNative():row.getFullNameNative();
            String target=semantic(selected);boolean current=selected.isSystemLocale()?out.actual.isEmpty():out.actual.size()==1&&out.actual.get(0).equals(selected.getLocale());
            AppLanguageBackend.Node node=new AppLanguageBackend.Node(id,parentId,text(label,semantic(row)),row.isSystemLocale()?"":text(row.getFullNameInUiLanguage(),semantic(row)),direct?"select":"open",nextLevel,target,current,selected.isSystemLocale(),row.isSuggested());
            out.nodes.add(node);if(direct)out.targets.put(id,selected);else buildRows(out,collector,next,id,nextLevel,depth+1);
        }
    }
    @Override public AppLanguageBackend.Write select(AppLanguageBackend.State raw,AppLanguageBackend.Node node)throws Exception{
        State before=(State)raw;State current=(State)read(before.packageName);
        if(!owner()||"restricted".equals(current.availability))return AppLanguageBackend.Write.RESTRICTED;
        if(!"available".equals(current.availability)||!current.writable||!Objects.equals(before.incarnation,current.incarnation)||!Objects.equals(before.revision,current.revision))return AppLanguageBackend.Write.TARGET_CHANGED;
        LocaleStore.LocaleInfo target=current.targets.get(node.id);if(target==null||!semantic(target).equals(node.target))return AppLanguageBackend.Write.TARGET_CHANGED;
        LocaleList requested=target.isSystemLocale()?LocaleList.getEmptyLocaleList():new LocaleList(target.getLocale());
        LocaleManager manager=context.getSystemService(LocaleManager.class);if(manager==null)return AppLanguageBackend.Write.UNAVAILABLE;
        if(!owner())return AppLanguageBackend.Write.RESTRICTED;
        // Native LocaleManager owns configuration changes and target Activity recreation.
        manager.setApplicationLocales(current.packageName,requested);
        if(!owner())return AppLanguageBackend.Write.UNCONFIRMED;
        LocaleList observed=manager.getApplicationLocales(current.packageName);
        State after=(State)read(current.packageName);
        if(!"available".equals(after.availability)||!Objects.equals(current.incarnation,after.incarnation))return AppLanguageBackend.Write.UNCONFIRMED;
        return requested.equals(observed)&&requested.equals(after.actual)?AppLanguageBackend.Write.APPLIED:AppLanguageBackend.Write.REQUESTED;
    }
}
