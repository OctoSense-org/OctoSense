package dev.makepad.octosense.settingsbroker;

import android.Manifest;
import android.app.ActivityManager;
import android.app.KeyguardManager;
import android.app.admin.DevicePolicyManager;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.pm.ActivityInfo;
import android.content.pm.ApplicationInfo;
import android.content.pm.PackageInfo;
import android.content.pm.PackageManager;
import android.content.res.Configuration;
import android.os.Looper;
import android.os.Process;
import android.os.UserHandle;
import android.os.UserManager;
import android.provider.Settings;
import android.view.inputmethod.InputMethodInfo;
import android.view.inputmethod.InputMethodManager;
import android.view.inputmethod.InputMethodSubtype;
import com.android.settingslib.inputmethod.InputMethodAndSubtypeUtil;
import com.android.settingslib.inputmethod.InputMethodSettingValuesWrapper;
import dev.makepad.octosense.keyboards.KeyboardPolicy;
import java.text.Collator;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;
import java.util.Objects;

/** Main-thread native SettingsLib model. There is no caller-supplied settings writer. */
final class KeyboardPlatform implements KeyboardPolicy.Platform {
    final Context context;
    private List<InputMethodInfo> observed=Collections.emptyList();
    KeyboardPlatform(Context context){this.context=context.getApplicationContext();}
    boolean owner(){
        UserManager users=context.getSystemService(UserManager.class);KeyguardManager lock=context.getSystemService(KeyguardManager.class);
        return Process.myUid()==Process.SYSTEM_UID&&UserHandle.myUserId()==0&&ActivityManager.getCurrentUser()==0
                &&users!=null&&users.isUserUnlocked()&&lock!=null&&!lock.isKeyguardLocked();
    }
    boolean writable(){return owner()&&context.checkSelfPermission(Manifest.permission.WRITE_SECURE_SETTINGS)==PackageManager.PERMISSION_GRANTED;}
    private static String text(CharSequence value,String fallback,int max){StringBuilder out=new StringBuilder();if(value!=null)value.toString().codePoints().filter(c->!Character.isISOControl(c)).limit(max).forEach(out::appendCodePoint);return out.toString().trim().isEmpty()?fallback:out.toString();}
    private String raw(String key){String value=Settings.Secure.getString(context.getContentResolver(),key);if(value!=null&&value.length()>131072)throw new IllegalStateException("Oversized keyboard state");return value==null?"absent":"value:"+value;}
    private String rawRevision(){return KeyboardPolicy.hash(raw(Settings.Secure.ENABLED_INPUT_METHODS),raw(Settings.Secure.DEFAULT_INPUT_METHOD),raw(Settings.Secure.SELECTED_INPUT_METHOD_SUBTYPE),raw(Settings.Secure.DISABLED_SYSTEM_INPUT_METHODS));}
    private String incarnation(InputMethodInfo imi)throws Exception{
        PackageInfo info=context.getPackageManager().getPackageInfo(imi.getPackageName(),PackageManager.GET_SIGNING_CERTIFICATES);
        ApplicationInfo app=info.applicationInfo;if(app==null||info.signingInfo==null)throw new IllegalStateException("Unknown keyboard package identity");
        List<String> signatures=new ArrayList<>();for(android.content.pm.Signature signature:info.signingInfo.getApkContentsSigners())signatures.add(KeyboardPolicy.hash(signature.toCharsString()));Collections.sort(signatures);
        android.system.StructStat source=android.system.Os.stat(app.sourceDir);
        List<String> values=new ArrayList<>(Arrays.asList(imi.getId(),String.valueOf(app.uid),String.valueOf(info.firstInstallTime),String.valueOf(info.lastUpdateTime),String.valueOf(info.getLongVersionCode()),app.sourceDir,String.valueOf(source.st_dev),String.valueOf(source.st_ino),signatures.toString(),String.valueOf(app.enabled),String.valueOf(imi.getServiceInfo().enabled),String.valueOf(imi.getServiceInfo().exported),String.valueOf(imi.getServiceInfo().permission),String.valueOf(imi.getSettingsActivity()),String.valueOf(imi.isAuxiliaryIme()),String.valueOf(imi.getSubtypeCount())));
        for(int i=0;i<imi.getSubtypeCount();i++){InputMethodSubtype subtype=imi.getSubtypeAt(i);values.add(KeyboardPolicy.hash(String.valueOf(subtype.hashCode()),subtype.getLocale(),subtype.getLanguageTag(),subtype.getMode(),subtype.getExtraValue(),String.valueOf(subtype.isAuxiliary()),String.valueOf(subtype.isAsciiCapable()),String.valueOf(subtype.overridesImplicitlyEnabledSubtype())));}
        return KeyboardPolicy.hash(values.toArray(new String[0]));
    }
    private boolean installedUsable(InputMethodInfo imi){ApplicationInfo app=imi.getServiceInfo().applicationInfo;return app!=null&&app.enabled&&imi.getServiceInfo().enabled&&imi.getServiceInfo().exported&&(app.flags&ApplicationInfo.FLAG_SUSPENDED)==0&&Manifest.permission.BIND_INPUT_METHOD.equals(imi.getServiceInfo().permission);}
    Intent provider(InputMethodInfo imi){
        try{
            String advertised=imi.getSettingsActivity();if(advertised==null||advertised.isEmpty())return null;
            ComponentName component=new ComponentName(imi.getPackageName(),advertised.startsWith(".")?imi.getPackageName()+advertised:advertised);
            ActivityInfo info=context.getPackageManager().getActivityInfo(component,0);
            if(!imi.getPackageName().equals(info.packageName)||!info.enabled||!info.exported||info.applicationInfo==null||!info.applicationInfo.enabled||(info.applicationInfo.flags&ApplicationInfo.FLAG_SUSPENDED)!=0
                    ||info.permission!=null&&context.checkSelfPermission(info.permission)!=PackageManager.PERMISSION_GRANTED)return null;
            return new Intent(Intent.ACTION_MAIN).setComponent(component);
        }catch(PackageManager.NameNotFoundException|SecurityException unavailable){return null;}
    }
    Intent subtypes(InputMethodInfo imi){
        Intent intent=new Intent(Settings.ACTION_INPUT_METHOD_SUBTYPE_SETTINGS).setPackage("com.android.settings").putExtra(Settings.EXTRA_INPUT_METHOD_ID,imi.getId());
        android.content.pm.ResolveInfo result=context.getPackageManager().resolveActivity(intent,PackageManager.MATCH_DEFAULT_ONLY);
        if(result==null||result.activityInfo==null)return null;ActivityInfo activity=result.activityInfo;
        if(!"com.android.settings".equals(activity.packageName)||!activity.enabled||!activity.exported||activity.applicationInfo==null||!activity.applicationInfo.enabled||(activity.applicationInfo.flags&ApplicationInfo.FLAG_SYSTEM)==0
                ||context.getPackageManager().checkSignatures("android",activity.packageName)!=PackageManager.SIGNATURE_MATCH
                ||activity.permission!=null&&context.checkSelfPermission(activity.permission)!=PackageManager.PERMISSION_GRANTED)return null;
        return intent.setComponent(new ComponentName(activity.packageName,activity.name));
    }
    InputMethodInfo method(String id){for(InputMethodInfo imi:observed)if(imi.getId().equals(id))return imi;return null;}
    List<InputMethodInfo> methods(){return new ArrayList<>(observed);}
    boolean hardwareKeyboard(){return context.getResources().getConfiguration().keyboard==Configuration.KEYBOARD_QWERTY;}
    @Override public KeyboardPolicy.State read()throws Exception{
        if(Looper.myLooper()!=Looper.getMainLooper())throw new IllegalStateException("Native keyboard model requires main thread");
        observed=Collections.emptyList();if(!owner())return new KeyboardPolicy.State(null,"restricted","locked",null,false,Collections.emptyList());
        if(!writable())return new KeyboardPolicy.State(null,"unavailable","service_unavailable",null,false,Collections.emptyList());
        String before=rawRevision();InputMethodSettingValuesWrapper wrapper=InputMethodSettingValuesWrapper.getInstance(context);wrapper.refreshAllInputMethodAndSubtypes();
        List<InputMethodInfo> methods=wrapper.getInputMethodList();if(methods==null||methods.size()>KeyboardPolicy.MAX_METHODS)throw new IllegalStateException("Oversized native keyboard inventory");
        DevicePolicyManager policy=context.getSystemService(DevicePolicyManager.class);if(policy==null)throw new IllegalStateException("Missing keyboard policy service");
        List<String> permitted=policy.getPermittedInputMethods();List<String> policyPackages=permitted==null?null:new ArrayList<>(permitted);if(policyPackages!=null)Collections.sort(policyPackages);
        String selected=Settings.Secure.getString(context.getContentResolver(),Settings.Secure.DEFAULT_INPUT_METHOD);
        boolean tv=(context.getResources().getConfiguration().uiMode&Configuration.UI_MODE_TYPE_MASK)==Configuration.UI_MODE_TYPE_TELEVISION;
        Collator order=Collator.getInstance();methods.sort((a,b)->{boolean ap=a.isSystem()&&InputMethodAndSubtypeUtil.isValidNonAuxAsciiCapableIme(a),bp=b.isSystem()&&InputMethodAndSubtypeUtil.isValidNonAuxAsciiCapableIme(b);if(ap!=bp)return ap?-1:1;int compared=order.compare(a.loadLabel(context.getPackageManager()).toString(),b.loadLabel(context.getPackageManager()).toString());return compared!=0?compared:a.getId().compareTo(b.getId());});
        List<KeyboardPolicy.Method> rows=new ArrayList<>();InputMethodManager manager=context.getSystemService(InputMethodManager.class);
        for(InputMethodInfo imi:methods){
            boolean enabled=wrapper.isEnabledImi(imi),always=wrapper.isAlwaysCheckedIme(imi),allowed=policyPackages==null||policyPackages.contains(imi.getPackageName())||enabled,usable=installedUsable(imi);
            List<InputMethodSubtype> subtypes=manager.getEnabledInputMethodSubtypeList(imi,true);
            String summary=text(InputMethodAndSubtypeUtil.getSubtypeLocaleNameListAsSentence(subtypes,context,imi),"",2048);
            rows.add(new KeyboardPolicy.Method(imi.getId(),incarnation(imi),text(imi.loadLabel(context.getPackageManager()),imi.getPackageName(),256),imi.getPackageName(),summary,!usable?"Unavailable provider":always?"Required keyboard":!allowed?"Restricted by administrator":"",enabled,enabled&&Objects.equals(selected,imi.getId()),imi.isSystem(),imi.getServiceInfo().directBootAware,tv,
                    usable&&!enabled&&!always&&allowed,usable&&enabled&&!always&&allowed,usable&&provider(imi)!=null,usable&&enabled&&subtypes(imi)!=null));
        }
        if(!writable()||!before.equals(rawRevision()))throw new IllegalStateException("Keyboard observation changed");
        List<String> afterPolicy=policy.getPermittedInputMethods();if(afterPolicy!=null){afterPolicy=new ArrayList<>(afterPolicy);Collections.sort(afterPolicy);}if(!Objects.equals(policyPackages,afterPolicy))throw new IllegalStateException("Keyboard policy changed");
        observed=new ArrayList<>(methods);String revision=KeyboardPolicy.hash(before,String.valueOf(policyPackages),String.valueOf(hardwareKeyboard()),String.valueOf(tv),context.getResources().getConfiguration().getLocales().toLanguageTags());
        return new KeyboardPolicy.State(revision,"available","none",selected,true,rows);
    }
}
