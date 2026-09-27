"""Exercise the real Broker adapter against finite native API doubles, including AOSP class/constructor ABI."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
STUBS = {
    'com/android/internal/app/CollectorFixture.java': 'package com.android.internal.app;public final class CollectorFixture{public static boolean reverse,changePreferences;}',
    'android/Manifest.java': 'package android;public class Manifest{public static class permission{public static final String CHANGE_CONFIGURATION="change";}}',
    'android/content/ContentResolver.java': 'package android.content;public class ContentResolver{}',
    'android/content/Context.java': '''package android.content;public class Context{public int permission=0;public android.os.UserManager users=new android.os.UserManager();public android.app.KeyguardManager keyguard=new android.app.KeyguardManager();public <T>T getSystemService(Class<T> c){return c.cast(c==android.os.UserManager.class?users:keyguard);}public int checkSelfPermission(String p){return permission;}public ContentResolver getContentResolver(){return new ContentResolver();}}''',
    'android/content/pm/PackageManager.java': 'package android.content.pm;public class PackageManager{public static final int PERMISSION_GRANTED=0;}',
    'android/content/res/Configuration.java': 'package android.content.res;public class Configuration{public android.os.LocaleList locales=android.os.LocaleList.forLanguageTags("en-US-u-hc-h23,zz-Latn-ZZ-u-nu-latn");public android.os.LocaleList getLocales(){return locales;}}',
    'android/app/ActivityManager.java': '''package android.app;public class ActivityManager{public static int user=0;public static boolean fail;public static final Service SERVICE=new Service();public static int getCurrentUser(){return user;}public static Service getService(){return SERVICE;}public static class Service{public final android.content.res.Configuration config=new android.content.res.Configuration();public android.content.res.Configuration getConfiguration(){if(fail)throw new IllegalStateException("native read failed");return config;}}}''',
    'android/app/KeyguardManager.java': 'package android.app;public class KeyguardManager{public boolean locked;public boolean isKeyguardLocked(){return locked;}}',
    'android/os/Process.java': 'package android.os;public class Process{public static final int SYSTEM_UID=1000;public static int myUid(){return 1000;}}',
    'android/os/UserHandle.java': 'package android.os;public class UserHandle{public static int myUserId(){return 0;}}',
    'android/os/UserManager.java': 'package android.os;public class UserManager{public static final String DISALLOW_CONFIG_LOCALE="no_config_locale";public boolean unlocked=true,restricted,demo;public boolean isUserUnlocked(){return unlocked;}public boolean hasUserRestriction(String s){return restricted;}public boolean isDemoUser(){return demo;}}',
    'android/os/LocaleList.java': '''package android.os;import java.util.*;public final class LocaleList{private final List<Locale> values;public LocaleList(Locale...values){this.values=Arrays.asList(values);}public static LocaleList forLanguageTags(String s){return new LocaleList(Arrays.stream(s.split(",")).filter(v->!v.isEmpty()).map(Locale::forLanguageTag).toArray(Locale[]::new));}public static LocaleList getDefault(){return forLanguageTags("en-US");}public int size(){return values.size();}public boolean isEmpty(){return values.isEmpty();}public Locale get(int i){return values.get(i);}public String toLanguageTags(){return values.stream().map(Locale::toLanguageTag).collect(java.util.stream.Collectors.joining(","));}public boolean equals(Object o){return o instanceof LocaleList&&values.equals(((LocaleList)o).values);}public int hashCode(){return values.hashCode();}}''',
    'android/provider/Settings.java': '''package android.provider;public class Settings{public static class System{public static boolean writable=true;public static String preferences="und-u-hc-h23";public static boolean canWrite(android.content.Context c){return writable;}public static String getString(android.content.ContentResolver c,String name){return preferences;}}public static class Global{public static int demo;public static int getInt(android.content.ContentResolver c,String name,int fallback){return demo;}}}''',
    'com/android/internal/app/LocalePicker.java': '''package com.android.internal.app;public class LocalePicker{public static int writes;public static boolean fail,filter;public static void updateLocales(android.os.LocaleList locales){writes++;if(!fail)android.app.ActivityManager.SERVICE.config.locales=filter?android.os.LocaleList.forLanguageTags("en-US"):locales;}}''',
    'com/android/internal/app/LocaleHelper.java': '''package com.android.internal.app;public class LocaleHelper{public static class LocaleInfoComparator implements java.util.Comparator<LocaleStore.LocaleInfo>{public LocaleInfoComparator(java.util.Locale l,boolean c){}public int compare(LocaleStore.LocaleInfo a,LocaleStore.LocaleInfo b){return 0;}}public static String getDisplayNumberingSystemKeyValue(java.util.Locale l,java.util.Locale ui){return l.getUnicodeLocaleType("nu");}}''',
    'com/android/internal/app/LocaleStore.java': '''package com.android.internal.app;import java.util.*;public class LocaleStore{public static LocaleInfo getLocaleInfo(Locale l){return new LocaleInfo(l.toLanguageTag(),false,false,true);}public static class LocaleInfo{private final Locale locale;private final boolean suggested,numbering,translated;public LocaleInfo(String tag,boolean suggested,boolean numbering,boolean translated){locale=Locale.forLanguageTag(tag);this.suggested=suggested;this.numbering=numbering;this.translated=translated;}public Locale getLocale(){return locale;}public boolean isSystemLocale(){return false;}public boolean isSuggested(){return suggested;}public Locale getParent(){return locale.getCountry().isEmpty()?null:Locale.forLanguageTag(locale.getLanguage());}public boolean hasNumberingSystems(){return numbering;}public boolean isTranslated(){return translated;}public String getFullCountryNameNative(){return locale.getDisplayCountry(locale);}public String getFullNameNative(){return locale.getDisplayName(locale);}public String getFullNameInUiLanguage(){return locale.getDisplayName(Locale.US);}}}''',
    'com/android/internal/app/SystemLocaleCollector.java': '''package com.android.internal.app;import java.util.*;public class SystemLocaleCollector{SystemLocaleCollector(android.content.Context c,android.os.LocaleList explicit){}public Set<LocaleStore.LocaleInfo> getSupportedLocaleList(LocaleStore.LocaleInfo parent,boolean translated,boolean country){if(translated||country!=(parent!=null))throw new AssertionError("Wrong native collector arguments");if(CollectorFixture.changePreferences)android.provider.Settings.System.preferences="und-u-hc-h12";List<LocaleStore.LocaleInfo> rows=new ArrayList<>();String tag=parent==null?"":parent.getLocale().toLanguageTag();switch(tag){case "":rows.add(new LocaleStore.LocaleInfo("fr",false,false,true));rows.add(new LocaleStore.LocaleInfo("ar",false,false,true));rows.add(new LocaleStore.LocaleInfo("ja",false,false,true));rows.add(new LocaleStore.LocaleInfo("es-ES",true,false,true));break;case "fr":rows.add(new LocaleStore.LocaleInfo("fr-CA",false,false,true));rows.add(new LocaleStore.LocaleInfo("fr-FR",false,false,true));break;case "ar":rows.add(new LocaleStore.LocaleInfo("ar-EG",false,true,true));rows.add(new LocaleStore.LocaleInfo("ar-SA",false,false,true));break;case "ar-EG":rows.add(new LocaleStore.LocaleInfo("ar-EG-u-nu-arab",false,false,true));rows.add(new LocaleStore.LocaleInfo("ar-EG-u-nu-latn",false,false,true));break;case "ja":rows.add(new LocaleStore.LocaleInfo("ja-JP",false,false,true));break;default:throw new AssertionError("Unexpected native branch "+tag);}if(CollectorFixture.reverse)Collections.reverse(rows);return new LinkedHashSet<>(rows);}}''',
}


class SystemLanguagePlatformTest(unittest.TestCase):
    def test_native_hierarchy_readback_and_authority(self):
        from test_display_settings_backend import STUBS as JSON
        jdk = os.environ.get('JAVA_HOME')
        javac = str(Path(jdk)/'bin/javac') if jdk else shutil.which('javac')
        java = str(Path(jdk)/'bin/java') if jdk else shutil.which('java')
        if not javac or not java:
            self.skipTest('JDK required')
        with tempfile.TemporaryDirectory() as tmp:
            sources=[]
            stubs=dict(STUBS)
            for name, source in JSON.items():
                if name.startswith('org/json/'):
                    stubs[name]=source.replace('private final java.util.Map','public static final Object NULL=new Object();private final java.util.Map')
            for name, source in stubs.items():
                path=Path(tmp)/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_text(source);sources.append(path)
            sources+=list((ROOT/'vendor/octosense/settings/src/dev/makepad/octosense/systemlanguage').glob('*.java'))
            sources+=[ROOT/'vendor/octosense/settings-broker/src/dev/makepad/octosense/settingsbroker/SystemLanguagePlatform.java',ROOT/'tests/java/SystemLanguagePlatformTest.java']
            for command in ([javac,'-d',tmp,*map(str,sources)],[java,'-cp',tmp,'dev.makepad.octosense.settingsbroker.SystemLanguagePlatformTest']):
                result=subprocess.run(command,capture_output=True,text=True)
                self.assertEqual(result.returncode,0,result.stdout+result.stderr)
            # The pinned ROM exports this class, but AOSP35 makes the same native
            # implementation package-private. Replace only the runtime collector
            # after compiling against the public ROM declaration, reproducing the
            # real binary visibility mismatch instead of changing the algorithm.
            collector=Path(tmp)/'com/android/internal/app/SystemLocaleCollector.java'
            collector.write_text(collector.read_text().replace('public class SystemLocaleCollector','class SystemLocaleCollector'))
            for command in ([javac,'-cp',tmp,'-d',tmp,str(collector)],[java,'-cp',tmp,'dev.makepad.octosense.settingsbroker.SystemLanguagePlatformTest']):
                result=subprocess.run(command,capture_output=True,text=True)
                self.assertEqual(result.returncode,0,result.stdout+result.stderr)
