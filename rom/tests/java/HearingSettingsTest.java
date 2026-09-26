import dev.makepad.octosense.controls.HearingSettings;
import dev.makepad.octosense.controls.HearingSettings.Setting;
import dev.makepad.octosense.controls.HearingSettings.Access;
import java.util.EnumMap;

public final class HearingSettingsTest {
    static int checks;
    static void check(boolean value){checks++;if(!value)throw new AssertionError("Check "+checks);}
    static final class Store implements HearingSettings.Store{
        final EnumMap<Setting,String> raw=new EnumMap<>(Setting.class),actual=new EnumMap<>(Setting.class);boolean accept=true,propagate=true,throwAfterWrite,linkage;int writes;Runnable onRaw=()->{},afterWrite=()->{};Setting last,reject;
        Store(){for(Setting setting:Setting.values())actual.put(setting,setting.observed(null));}
        public String raw(Setting setting){onRaw.run();if(throwAfterWrite&&writes>0)throw new IllegalStateException();return raw.get(setting);}
        public String service(Setting setting){if(linkage)throw new NoSuchMethodError("Synthetic absent API");return actual.get(setting);}
        public boolean write(Setting setting,String value){writes++;last=setting;boolean accepted=accept&&setting!=reject;if(accepted){raw.put(setting,value);if(propagate)actual.put(setting,setting.observed(value));}afterWrite.run();return accepted;}
    }
    static void linkedCaptionChanges(){
        for(Setting setting:new Setting[]{Setting.CAPTIONS_FONT_SCALE,Setting.CAPTIONS_PRESET})for(String choice:setting.choices()){
            Store store=new Store();HearingSettings.Backend backend=new HearingSettings.Backend(store,()->Access.WRITABLE);
            check(backend.read(Setting.CAPTIONS_ENABLED).equals("off")&&backend.writable(setting));
            check(backend.apply(setting,choice).equals("control_applied"));
            check(store.writes==2&&store.raw.size()==2&&store.last==Setting.CAPTIONS_ENABLED);
            check(backend.read(setting).equals(setting.expected(choice))&&backend.read(Setting.CAPTIONS_ENABLED).equals("on"));
            int writes=store.writes;check(backend.apply(setting,choice).equals("control_applied"));check(store.writes==writes+1&&store.last==setting);
        }
        Store unknown=new Store();unknown.raw.put(Setting.CAPTIONS_ENABLED,"2");HearingSettings.Backend unknownBackend=new HearingSettings.Backend(unknown,()->Access.WRITABLE);
        check(unknownBackend.read(Setting.CAPTIONS_FONT_SCALE).equals("caption_scale:1.0"));
        check(!unknownBackend.writable(Setting.CAPTIONS_FONT_SCALE)&&!unknownBackend.writable(Setting.CAPTIONS_PRESET));
        check(unknownBackend.apply(Setting.CAPTIONS_PRESET,"caption_app").equals("control_unavailable")&&unknown.writes==0);
        Store revoked=new Store();Access[] authority={Access.WRITABLE};HearingSettings.Backend revokedBackend=new HearingSettings.Backend(revoked,()->authority[0]);
        revoked.afterWrite=()->authority[0]=Access.RESTRICTED;
        check(revokedBackend.apply(Setting.CAPTIONS_FONT_SCALE,"caption_scale:1.5").equals("control_partial"));
        check(revoked.writes==1&&revoked.raw.get(Setting.CAPTIONS_FONT_SCALE).equals("1.5")&&!revoked.raw.containsKey(Setting.CAPTIONS_ENABLED));
        Store failed=new Store();failed.reject=Setting.CAPTIONS_ENABLED;HearingSettings.Backend failedBackend=new HearingSettings.Backend(failed,()->Access.WRITABLE);
        check(failedBackend.apply(Setting.CAPTIONS_PRESET,"caption_custom").equals("control_partial"));
        check(failed.writes==2&&failedBackend.read(Setting.CAPTIONS_PRESET).equals("caption_custom")&&failedBackend.read(Setting.CAPTIONS_ENABLED).equals("off"));
        Store delayed=new Store();delayed.propagate=false;HearingSettings.Backend delayedBackend=new HearingSettings.Backend(delayed,()->Access.WRITABLE);
        check(delayedBackend.apply(Setting.CAPTIONS_FONT_SCALE,"caption_scale:2.0").equals("control_requested"));
        check(delayed.writes==2&&delayedBackend.read(Setting.CAPTIONS_ENABLED)==null&&delayedBackend.read(Setting.CAPTIONS_FONT_SCALE)==null);
        delayed.actual.put(Setting.CAPTIONS_ENABLED,"on");delayed.actual.put(Setting.CAPTIONS_FONT_SCALE,"caption_scale:2.0");
        check(delayedBackend.read(Setting.CAPTIONS_ENABLED).equals("on")&&delayedBackend.read(Setting.CAPTIONS_FONT_SCALE).equals("caption_scale:2.0"));
        Store changed=new Store();changed.afterWrite=()->changed.raw.put(Setting.CAPTIONS_ENABLED,"invalid");
        check(new HearingSettings.Backend(changed,()->Access.WRITABLE).apply(Setting.CAPTIONS_PRESET,"caption_yellow_blue").equals("control_partial"));check(changed.writes==1);
        Store retired=new Store();Access[] current={Access.WRITABLE};retired.onRaw=()->{if(retired.writes==2)current[0]=Access.RESTRICTED;};
        check(new HearingSettings.Backend(retired,()->current[0]).apply(Setting.CAPTIONS_FONT_SCALE,"caption_scale:0.5").equals("control_partial"));check(retired.writes==2);
    }
    public static void main(String[] args){
        linkedCaptionChanges();
        Store store=new Store();Access[] authority={Access.WRITABLE};HearingSettings.Backend backend=new HearingSettings.Backend(store,()->authority[0]);
        check(backend.read(Setting.MONO).equals("off"));check(backend.read(Setting.BALANCE).equals("balance:0.0"));check(backend.read(Setting.CAPTIONS_ENABLED).equals("off"));check(backend.read(Setting.CAPTIONS_FONT_SCALE).equals("caption_scale:1.0"));check(backend.read(Setting.CAPTIONS_PRESET).equals("caption_white_black"));check(store.writes==0&&store.raw.isEmpty());
        check(Setting.BALANCE.choices().length==201);
        for(int value=-100;value<=100;value++){check(backend.apply(Setting.BALANCE,"balance_percent:"+value).equals("control_applied"));check(Float.floatToIntBits(Float.parseFloat(store.raw.get(Setting.BALANCE)))==Float.floatToIntBits(value/100f));}
        store.raw.put(Setting.BALANCE,"0.123");store.actual.put(Setting.BALANCE,"balance:0.123");check(backend.read(Setting.BALANCE).equals("balance:0.123"));int count=store.writes;
        for(String bad:new String[]{"balance:0.123","balance_percent:+1","balance_percent:101","balance_percent:-101","balance_percent:01","0.123","NaN"})try{backend.apply(Setting.BALANCE,bad);throw new AssertionError();}catch(IllegalArgumentException expected){checks++;}check(store.writes==count);
        for(String invalid:new String[]{"NaN","Infinity","-Infinity","1.1","-1.1","","not a number"}){store.raw.put(Setting.BALANCE,invalid);check(backend.read(Setting.BALANCE)==null);check(backend.apply(Setting.BALANCE,"balance_percent:0").equals("control_unavailable"));}check(store.writes==count);
        store.raw.remove(Setting.BALANCE);store.actual.put(Setting.BALANCE,"balance:0.0");
        store.raw.put(Setting.CAPTIONS_FONT_SCALE,"1.25");store.actual.put(Setting.CAPTIONS_FONT_SCALE,"caption_scale:1.25");check(backend.read(Setting.CAPTIONS_FONT_SCALE).equals("caption_scale:1.25"));
        try{backend.apply(Setting.CAPTIONS_FONT_SCALE,"caption_scale:1.25");throw new AssertionError();}catch(IllegalArgumentException expected){checks++;}
        for(String size:Setting.CAPTIONS_FONT_SCALE.choices())check(backend.apply(Setting.CAPTIONS_FONT_SCALE,size).equals("control_applied"));
        for(String style:Setting.CAPTIONS_PRESET.choices()){String mono=store.raw.get(Setting.MONO),scale=store.raw.get(Setting.CAPTIONS_FONT_SCALE);check(backend.apply(Setting.CAPTIONS_PRESET,style).equals("control_applied"));check(store.last==Setting.CAPTIONS_PRESET&&java.util.Objects.equals(mono,store.raw.get(Setting.MONO))&&scale.equals(store.raw.get(Setting.CAPTIONS_FONT_SCALE)));}
        store.raw.put(Setting.CAPTIONS_PRESET,"99");check(backend.read(Setting.CAPTIONS_PRESET)==null);check(backend.apply(Setting.CAPTIONS_PRESET,"caption_app").equals("control_unavailable"));store.raw.remove(Setting.CAPTIONS_PRESET);store.actual.put(Setting.CAPTIONS_PRESET,"caption_white_black");
        authority[0]=Access.READ_ONLY;count=store.writes;check(backend.apply(Setting.MONO,"on").equals("control_unavailable"));authority[0]=Access.RESTRICTED;check(backend.apply(Setting.MONO,"on").equals("policy_restricted"));check(store.writes==count);
        authority[0]=Access.WRITABLE;store.onRaw=()->authority[0]=Access.RESTRICTED;check(backend.apply(Setting.MONO,"on").equals("policy_restricted"));check(store.writes==count);store.onRaw=()->{};authority[0]=Access.WRITABLE;
        store.accept=false;check(backend.apply(Setting.MONO,"on").equals("control_unavailable"));store.accept=true;store.propagate=false;check(backend.apply(Setting.MONO,"on").equals("control_requested"));check(backend.read(Setting.MONO)==null);store.actual.put(Setting.MONO,"on");check(backend.read(Setting.MONO).equals("on"));
        store.propagate=true;store.throwAfterWrite=true;Store pending=new Store();HearingSettings.Backend pendingBackend=new HearingSettings.Backend(pending,()->Access.WRITABLE);pending.throwAfterWrite=true;check(pendingBackend.apply(Setting.MONO,"on").equals("control_requested"));
        Store absent=new Store();absent.linkage=true;check(new HearingSettings.Backend(absent,()->Access.WRITABLE).apply(Setting.MONO,"on").equals("control_unavailable"));check(absent.writes==0);
        System.out.println("PASS hearing settings: "+checks);
    }
}
