import dev.makepad.octosense.controls.CaptionLocaleSettings;
import dev.makepad.octosense.controls.CaptionLocaleSettings.*;
import java.util.*;

public final class CaptionLocaleSettingsTest {
    private static void check(boolean value,String message){if(!value)throw new AssertionError(message);}
    private static final class Fake implements Store {
        String raw,service;List<LocaleEntry> locales=new ArrayList<>(Arrays.asList(new LocaleEntry("en_US","English (United States)"),new LocaleEntry("fr_FR","Français (France)"),new LocaleEntry("zh_CN","简体中文")));
        Access access=Access.WRITABLE;long now=1000;int nonce,writes;boolean pending,fail,loseAfterRead,loseAfterWrite,failAfterWrite;Runnable onRead;
        final Backend backend=new Backend(this,()->access,()->now,()->String.format(Locale.ROOT,"%064x",++nonce));
        public State read(){if(failAfterWrite&&writes>0)throw new IllegalStateException("Readback failed");State state=new State(raw,service,locales);if(loseAfterRead)access=Access.RESTRICTED;if(onRead!=null){Runnable next=onRead;onRead=null;next.run();}return state;}
        public boolean write(String value){writes++;if(fail)return false;raw=value;if(!pending)service=value;if(loseAfterWrite)access=Access.RESTRICTED;return true;}
        Snapshot fresh(){return backend.snapshot("","",0);}
    }
    private static Row row(Snapshot state,String locale){return state.rows.stream().filter(r->r.locale.equals(locale)).findFirst().orElseThrow(AssertionError::new);}
    private static void bad(Runnable action){try{action.run();throw new AssertionError("Accepted malformed request");}catch(IllegalArgumentException expected){}}
    public static void main(String[] args){
        Fake f=new Fake();Snapshot s=f.fresh();check(s.reason.equals("ready")&&s.systemDefault&&s.current.isEmpty()&&s.rows.size()==4&&row(s,"").selected,"Missing value must observe default");check(f.raw==null&&f.writes==0,"Read wrote a default");
        check(f.backend.select(s.key,row(s,"fr_FR").choice).equals("control_applied")&&f.raw.equals("fr_FR"),"Native underscore locale not written");check(f.backend.select(s.key,row(s,"fr_FR").choice).equals("caption_language_changed")&&f.writes==1,"Choice replayed");
        s=f.fresh();check(row(s,"fr_FR").selected,"Native current missing");check(f.backend.select(s.key,row(s,"").choice).equals("control_applied")&&f.raw.equals(""),"Default must write empty native value");
        f.raw=f.service="custom_NATIVE_Value";s=f.fresh();check(s.custom&&!s.systemDefault&&s.current.equals(f.raw)&&s.rows.stream().noneMatch(r->r.selected)&&f.writes==2,"Custom language silently normalized");
        f.raw=f.service="fr_FR";check(f.backend.select(s.key,row(s,"zh_CN").choice).equals("caption_language_changed")&&f.writes==2,"External raw change not retired");
        s=f.fresh();f.locales.remove(2);check(f.backend.select(s.key,row(s,"zh_CN").choice).equals("caption_language_changed"),"Removed native entry written");
        s=f.fresh();f.locales.set(1,new LocaleEntry("fr_FR","French"));check(f.backend.snapshot(s.key,"",0).reason.equals("caption_language_changed"),"Changed catalog labels retained");
        s=f.fresh();f.now+=CaptionLocaleSettings.CATALOG_TTL_MS-1;check(f.backend.snapshot(s.key,"",0).reason.equals("ready"),"Catalog expired early");f.now++;check(f.backend.select(s.key,row(s,"").choice).equals("caption_language_changed"),"Polling extended expiry");
        s=f.fresh();f.now--;check(f.backend.snapshot(s.key,"",0).reason.equals("caption_language_changed"),"Clock rollback retained catalog");
        for(Access access:new Access[]{Access.RESTRICTED,Access.READ_ONLY}){f=new Fake();s=f.fresh();f.access=access;check(f.backend.select(s.key,row(s,"fr_FR").choice).equals(access==Access.RESTRICTED?"policy_restricted":"control_unavailable")&&f.writes==0,"Lost authority wrote language");check(f.fresh().rows.isEmpty(),"Unavailable authority offered choices");}
        f=new Fake();s=f.fresh();f.loseAfterRead=true;check(f.backend.select(s.key,row(s,"fr_FR").choice).equals("policy_restricted")&&f.writes==0,"Authority loss during observation ignored");
        f=new Fake();s=f.fresh();f.loseAfterWrite=true;check(f.backend.select(s.key,row(s,"fr_FR").choice).equals("control_requested")&&f.writes==1,"Lost final authority reported applied");
        f=new Fake();s=f.fresh();f.pending=true;check(f.backend.select(s.key,row(s,"fr_FR").choice).equals("control_requested"),"Delayed native readback reported applied");
        f=new Fake();s=f.fresh();f.failAfterWrite=true;check(f.backend.select(s.key,row(s,"fr_FR").choice).equals("control_requested")&&f.writes==1,"Failed readback hid a completed write");
        f=new Fake();s=f.fresh();f.fail=true;check(f.backend.select(s.key,row(s,"fr_FR").choice).equals("control_unavailable"),"Provider failure reported applied");
        f=new Fake();f.raw="fr_FR";check(f.fresh().rows.isEmpty()&&f.writes==0,"Provider/service mismatch invented current language");
        f=new Fake();f.locales.add(new LocaleEntry("en_US","duplicate"));check(f.fresh().rows.isEmpty(),"Duplicate catalog accepted");
        f=new Fake();f.locales.clear();for(int i=0;i<100;i++)f.locales.add(new LocaleEntry("x_"+i,"Locale "+i));s=f.fresh();check(s.total==101&&s.rows.size()==24,"Catalog was silently truncated");Snapshot next=f.backend.snapshot(s.key,"",24);check(next.rows.size()==24&&next.offset==24&&next.key.equals(s.key),"Pagination changed catalog identity");
        Snapshot filtered=f.backend.snapshot(s.key,"x_99",0);check(filtered.total==1&&filtered.rows.size()==1,"Native locale code search failed");check(f.backend.select(s.key,filtered.rows.get(0).choice).equals("control_applied")&&f.raw.equals("x_99"),"Offscreen native locale unreachable");
        f=new Fake();s=f.backend.snapshot("","Français",0);check(s.rows.size()==1&&s.total==1,"Native display-label filter failed");String notOffered=String.format(Locale.ROOT,"%064x",3);check(f.backend.select(s.key,notOffered).equals("caption_language_changed")&&f.writes==0,"Unoffered row accepted");
        final Fake malformed=new Fake();bad(()->malformed.backend.snapshot("foreign","",0));bad(()->malformed.backend.snapshot("","\n",0));bad(()->malformed.backend.snapshot("","",-1));bad(()->malformed.backend.select("fr_FR","fr_FR"));
        System.out.println("Caption locale native catalog, exact-state lease, access and mutation checks passed");
    }
}
