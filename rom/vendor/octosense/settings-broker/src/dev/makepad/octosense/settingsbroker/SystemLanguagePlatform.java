package dev.makepad.octosense.settingsbroker;

import android.Manifest;
import android.app.ActivityManager;
import android.app.KeyguardManager;
import android.content.Context;
import android.content.pm.PackageManager;
import android.content.res.Configuration;
import android.os.LocaleList;
import android.os.Process;
import android.os.UserHandle;
import android.os.UserManager;
import android.provider.Settings;
import com.android.internal.app.LocaleHelper;
import com.android.internal.app.LocalePicker;
import com.android.internal.app.LocaleStore;
import dev.makepad.octosense.systemlanguage.SystemLanguageContract;
import dev.makepad.octosense.systemlanguage.SystemLanguageSettings;
import java.lang.reflect.Constructor;
import java.lang.reflect.Method;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Objects;
import java.util.Set;

/** UID1000 adapter for the native SystemLocaleCollector and LocalePicker configuration path. */
final class SystemLanguagePlatform implements SystemLanguageSettings.Platform {
    private final Context context;
    SystemLanguagePlatform(Context context){this.context=context;}
    private static final class State extends SystemLanguageSettings.State {
        LocaleList actual;String preferences;final Map<String,Locale> targets=new HashMap<>();
    }
    private boolean owner(){UserManager users=context.getSystemService(UserManager.class);KeyguardManager lock=context.getSystemService(KeyguardManager.class);return Process.myUid()==Process.SYSTEM_UID&&UserHandle.myUserId()==0&&ActivityManager.getCurrentUser()==0&&users!=null&&users.isUserUnlocked()&&lock!=null&&!lock.isKeyguardLocked();}
    private boolean restricted(){UserManager users=context.getSystemService(UserManager.class);return users==null||users.hasUserRestriction(UserManager.DISALLOW_CONFIG_LOCALE)||users.isDemoUser()||Settings.Global.getInt(context.getContentResolver(),"device_demo_mode",0)!=0;}
    private boolean writable(){return owner()&&!restricted()&&context.checkSelfPermission(Manifest.permission.CHANGE_CONFIGURATION)==PackageManager.PERMISSION_GRANTED&&Settings.System.canWrite(context);}
    private State denied(String availability,String reason){State state=new State();state.availability=availability;state.reason=reason;return state;}
    private LocaleList authoritative()throws Exception{
        // LocalePicker.getLocales silently falls back to process defaults on RemoteException.
        // Such a fallback must never authorize replacing the system's actual ordered list.
        Configuration configuration=ActivityManager.getService().getConfiguration();
        LocaleList locales=configuration==null?null:configuration.getLocales();
        if(locales==null||locales.isEmpty()||locales.size()>SystemLanguageContract.MAX_CURRENT)throw new IllegalStateException("Invalid native system locales");
        return locales;
    }
    private String preferences(){String value=Settings.System.getString(context.getContentResolver(),"locale_preferences");if(value!=null&&(value.length()>256||value.codePoints().anyMatch(Character::isISOControl)))throw new IllegalStateException("Invalid regional preferences");return value;}
    private static String text(String value,String fallback){StringBuilder out=new StringBuilder();if(value!=null)value.codePoints().filter(c->!Character.isISOControl(c)).limit(256).forEach(out::appendCodePoint);return out.toString().trim().isEmpty()?fallback:out.toString();}
    private static String semantic(LocaleStore.LocaleInfo row){return row.getLocale().toLanguageTag();}
    private static Locale withPreferences(Locale locale,String raw){
        if(raw==null||raw.isEmpty()||raw.equals("und"))return locale;
        Locale record=Locale.forLanguageTag(raw);Locale.Builder builder=new Locale.Builder().setLocale(locale);
        for(String key:record.getUnicodeLocaleKeys())builder.setUnicodeLocaleKeyword(key,record.getUnicodeLocaleType(key));return builder.build();
    }
    private static final class NativeCollector {
        final Object instance;final Method supported;
        NativeCollector(Object instance,Method supported){this.instance=instance;this.supported=supported;}
    }
    private NativeCollector collector()throws Exception{
        // The native class AND its constructor are package-private in AOSP35, but public in
        // the pinned ROM. Direct class literals or invokes therefore fail before constructor
        // access can help. Resolve only this exact native implementation and exact API.
        Class<?> nativeClass=Class.forName("com.android.internal.app.SystemLocaleCollector");
        Constructor<?> constructor=nativeClass.getDeclaredConstructor(Context.class,LocaleList.class);
        Method supported=nativeClass.getDeclaredMethod("getSupportedLocaleList",LocaleStore.LocaleInfo.class,boolean.class,boolean.class);
        if(supported.getReturnType()!=Set.class)throw new IllegalStateException("Unsupported native collector API");
        constructor.setAccessible(true);supported.setAccessible(true);
        return new NativeCollector(constructor.newInstance(context,null),supported);
    }
    private List<LocaleStore.LocaleInfo> children(NativeCollector collector,LocaleStore.LocaleInfo parent)throws Exception{
        Object observed=collector.supported.invoke(collector.instance,parent,false,parent!=null);
        if(!(observed instanceof Set<?>))throw new IllegalStateException("No native language catalog");
        List<LocaleStore.LocaleInfo> rows=new ArrayList<>();
        for(Object row:(Set<?>)observed){if(!(row instanceof LocaleStore.LocaleInfo))throw new IllegalStateException("Invalid native locale row");rows.add((LocaleStore.LocaleInfo)row);}
        LocaleHelper.LocaleInfoComparator order=new LocaleHelper.LocaleInfoComparator(parent==null?Locale.getDefault():parent.getLocale(),parent!=null);
        rows.sort((a,b)->{int compared=order.compare(a,b);return compared!=0?compared:semantic(a).compareTo(semantic(b));});return rows;
    }
    private void build(State state,NativeCollector collector,List<LocaleStore.LocaleInfo> rows,String parent,String level,int depth)throws Exception{
        if(depth>SystemLanguageContract.MAX_DEPTH)throw new IllegalStateException("Native language hierarchy too deep");
        Set<String> siblings=new HashSet<>();for(LocaleStore.LocaleInfo row:rows){
            if(row.isSystemLocale()||!siblings.add(semantic(row)))throw new IllegalStateException("Ambiguous system language");
            if(state.nodes.size()>=SystemLanguageContract.MAX_NODES)throw new IllegalStateException("Native language hierarchy too large");
            boolean direct=row.isSuggested()||(row.getParent()!=null&&!row.hasNumberingSystems())||level.equals("numbering");
            String nextLevel=row.hasNumberingSystems()?"numbering":"region";List<LocaleStore.LocaleInfo> next=null;LocaleStore.LocaleInfo selected=row;
            if(!direct){next=children(collector,row);if(next.size()==1){selected=next.get(0);direct=true;}else if(next.isEmpty())continue;}
            Locale target=direct?withPreferences(selected.getLocale(),state.preferences):row.getLocale();
            String id=SystemLanguageContract.hash(parent,semantic(row));String label=level.equals("numbering")?LocaleHelper.getDisplayNumberingSystemKeyValue(row.getLocale(),row.getLocale()):level.equals("region")?row.getFullCountryNameNative():row.getFullNameNative();
            state.nodes.add(new SystemLanguageSettings.Node(id,parent,text(label,semantic(row)),text(row.getFullNameInUiLanguage(),semantic(row)),direct?"select":"open",nextLevel,target.toLanguageTag(),selected.isTranslated(),row.isSuggested()));
            if(direct)state.targets.put(id,target);else build(state,collector,next,id,nextLevel,depth+1);
        }
    }
    @Override public SystemLanguageSettings.State read()throws Exception{
        if(!owner())return denied("restricted","locked");if(restricted())return denied("restricted","policy");
        State state=new State();state.actual=authoritative();state.preferences=preferences();state.writable=writable();
        NativeCollector collector=collector();build(state,collector,children(collector,null),"","language",1);
        for(int index=0;index<state.actual.size();index++){Locale locale=state.actual.get(index);LocaleStore.LocaleInfo info=LocaleStore.getLocaleInfo(locale);String tag=locale.toLanguageTag();state.current.add(new SystemLanguageSettings.Current(tag,text(info.getFullNameInUiLanguage(),tag),text(info.getFullNameNative(),tag),info.isTranslated()));}
        if(!owner()||restricted())return denied("restricted","locked");
        if(!state.actual.equals(authoritative())||!Objects.equals(state.preferences,preferences())||state.writable!=writable())return denied("stale","target_changed");
        List<String> revision=new ArrayList<>(Arrays.asList("owner0",state.actual.toLanguageTags(),state.preferences==null?"absent":"value:"+state.preferences,LocaleList.getDefault().toLanguageTags(),String.valueOf(state.writable)));
        for(SystemLanguageSettings.Current row:state.current)revision.add(SystemLanguageContract.hash(row.tag,row.label,row.nativeLabel,String.valueOf(row.translated)));
        for(SystemLanguageSettings.Node row:state.nodes)revision.add(SystemLanguageContract.hash(row.id,row.parent,row.label,row.secondary,row.kind,row.level,row.tag,String.valueOf(row.translated),String.valueOf(row.suggested)));
        state.revision=SystemLanguageContract.hash(revision.toArray(new String[0]));state.availability="available";state.reason="none";return state;
    }
    @Override public SystemLanguageSettings.Write apply(SystemLanguageSettings.State observed,List<SystemLanguageSettings.Selection> order)throws Exception{
        State before=(State)observed,current=(State)read();
        if(!owner()||restricted()||"restricted".equals(current.availability))return SystemLanguageSettings.Write.RESTRICTED;
        if(!"available".equals(current.availability)||!current.writable||!Objects.equals(before.revision,current.revision))return SystemLanguageSettings.Write.TARGET_CHANGED;
        List<Locale> locales=new ArrayList<>();Set<Locale> unique=new HashSet<>();
        for(SystemLanguageSettings.Selection selection:order){Locale locale=null;if(selection.existing){for(int i=0;i<current.actual.size();i++)if(current.actual.get(i).toLanguageTag().equals(selection.tag))locale=current.actual.get(i);}else locale=current.targets.get(selection.id);
            if(locale==null||!locale.toLanguageTag().equals(selection.tag)||!unique.add(locale))return SystemLanguageSettings.Write.TARGET_CHANGED;locales.add(locale);
        }
        LocaleList requested=new LocaleList(locales.toArray(new Locale[0]));if(requested.isEmpty())return SystemLanguageSettings.Write.TARGET_CHANGED;
        if(!writable())return SystemLanguageSettings.Write.RESTRICTED;
        if(!current.actual.equals(authoritative())||!Objects.equals(current.preferences,preferences()))return SystemLanguageSettings.Write.TARGET_CHANGED;
        // Preserve native filtering, userSetLocale, attributed persistent configuration, and backup.
        // Native updateLocales swallows RemoteException: accepted transport is not proof of apply.
        LocalePicker.updateLocales(requested);
        if(!owner()||restricted())return SystemLanguageSettings.Write.UNCONFIRMED;
        return requested.equals(authoritative())?SystemLanguageSettings.Write.APPLIED:SystemLanguageSettings.Write.UNCONFIRMED;
    }
}
