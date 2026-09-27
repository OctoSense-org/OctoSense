package dev.makepad.octosense.settingsbroker;

import android.app.ActivityManager;
import android.content.Context;
import android.os.LocaleList;
import android.provider.Settings;
import com.android.internal.app.LocalePicker;
import com.android.internal.app.CollectorFixture;
import dev.makepad.octosense.systemlanguage.SystemLanguageSettings;
import org.json.JSONObject;

public final class SystemLanguagePlatformTest {
    static int checks;static void check(boolean condition){if(!condition)throw new AssertionError("Native language check "+checks);checks++;}
    static String find(JSONObject state,String tag){for(int i=0;i<state.getJSONArray("rows").length();i++){JSONObject row=state.getJSONArray("rows").getJSONObject(i);if(tag.equals(row.getString("tag")))return row.getString("target");}throw new AssertionError("Missing native tag "+tag);}
    public static void main(String[] args)throws Exception{
        Context context=new Context();SystemLanguagePlatform platform=new SystemLanguagePlatform(context);SystemLanguageSettings.State state=platform.read();
        check(state.writable&&state.current.size()==2&&state.current.get(0).tag.equals("en-US-u-hc-h23"));check(state.nodes.size()==10);
        CollectorFixture.reverse=true;check(platform.read().revision.equals(state.revision));CollectorFixture.reverse=false;
        check(state.nodes.stream().anyMatch(row->row.kind.equals("select")&&row.tag.equals("ja-JP-u-hc-h23")));
        check(state.nodes.stream().anyMatch(row->row.kind.equals("open")&&row.tag.equals("ar-EG")&&row.level.equals("numbering")));
        check(state.nodes.stream().anyMatch(row->row.kind.equals("select")&&row.tag.equals("ar-EG-u-hc-h23-nu-latn")));
        SystemLanguageSettings backend=new SystemLanguageSettings(platform,()->100);JSONObject root=backend.snapshot(1,"","","",0);String key=root.getString("key");String oldCustom=root.getJSONArray("current").getJSONObject(1).getString("target");
        JSONObject arabic=backend.snapshot(2,key,find(root,"ar"),"",0),numbering=backend.snapshot(3,key,find(arabic,"ar-EG"),"",0);
        check(backend.apply(key,new String[]{find(numbering,"ar-EG-u-hc-h23-nu-latn"),oldCustom}).equals("languages_applied"));
        check(ActivityManager.SERVICE.config.locales.toLanguageTags().equals("ar-EG-u-hc-h23-nu-latn,zz-Latn-ZZ-u-nu-latn"));check(LocalePicker.writes==1);
        for(boolean filter:new boolean[]{false,true}){root=backend.snapshot(4,"","","",0);key=root.getString("key");LocalePicker.fail=!filter;LocalePicker.filter=filter;check(backend.apply(key,new String[]{find(root,"ja-JP-u-hc-h23")}).equals("languages_unconfirmed"));}LocalePicker.fail=false;LocalePicker.filter=false;
        root=backend.snapshot(5,"","","",0);key=root.getString("key");String target=root.getJSONArray("current").getJSONObject(0).getString("target");int before=LocalePicker.writes;Settings.System.preferences="und-u-hc-h12";
        check(backend.apply(key,new String[]{target}).equals("languages_target_changed")&&LocalePicker.writes==before);
        context.users.restricted=true;check(platform.read().availability.equals("restricted"));context.users.restricted=false;context.keyguard.locked=true;check(platform.read().availability.equals("restricted"));context.keyguard.locked=false;ActivityManager.user=10;check(platform.read().availability.equals("restricted"));ActivityManager.user=0;
        Settings.System.writable=false;check(!platform.read().writable);Settings.System.writable=true;context.permission=-1;check(!platform.read().writable);context.permission=0;
        Settings.Global.demo=1;check(platform.read().availability.equals("restricted"));Settings.Global.demo=0;
        ActivityManager.fail=true;check(backend.snapshot(6,"","","",0).getString("availability").equals("unavailable"));ActivityManager.fail=false;
        Settings.System.preferences="und-u-hc-h23";CollectorFixture.changePreferences=true;check(platform.read().availability.equals("stale"));CollectorFixture.changePreferences=false;
        ActivityManager.SERVICE.config.locales=new LocaleList();check(backend.snapshot(7,"","","",0).getString("availability").equals("unavailable"));
        System.out.println("PASS native system language adapter "+checks);
    }
}
