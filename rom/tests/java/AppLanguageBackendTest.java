import dev.makepad.octosense.applanguage.AppLanguageBackend;
import dev.makepad.octosense.applanguage.AppLanguageContract;
import org.json.JSONObject;

public final class AppLanguageBackendTest {
    static int checks;
    static void check(boolean value){checks++;if(!value)throw new AssertionError("Check "+checks);}
    static final String PKG="fixture.language";
    static final class Platform implements AppLanguageBackend.Platform {
        String incarnation="install-A",revision="current-A-config-A",availability="available",reason="none";boolean writable=true,throwing,cycle;int writes;
        AppLanguageBackend.Write result=AppLanguageBackend.Write.APPLIED;
        public AppLanguageBackend.State read(String pkg){
            AppLanguageBackend.State s=new AppLanguageBackend.State();s.packageName=pkg;s.label="Fixture";s.incarnation=incarnation;s.revision=revision;s.availability=availability;s.reason=reason;s.writable=writable;
            s.current.add(new AppLanguageBackend.Current("zz-Latn-ZZ","Custom primary"));s.current.add(new AppLanguageBackend.Current("fr-CA","French secondary"));
            s.nodes.add(new AppLanguageBackend.Node("default","","System default",null,"select","region","system",false,true,false));
            s.nodes.add(new AppLanguageBackend.Node("french",cycle?"french":"","French",null,"open","region","fr",false,false,false));
            for(int i=0;i<25;i++)s.nodes.add(new AppLanguageBackend.Node("region"+i,"french","Region "+i,null,"select","region","fr-X"+i,false,false,false));return s;
        }
        public AppLanguageBackend.Write select(AppLanguageBackend.State state,AppLanguageBackend.Node node){writes++;if(throwing)throw new IllegalStateException();return result;}
    }
    static JSONObject root(AppLanguageBackend b,long id)throws Exception{return b.snapshot(id,PKG,"","","",0);}
    static String row(JSONObject j,int index){return j.getJSONArray("rows").getJSONObject(index).getString("key");}
    public static void main(String[] args)throws Exception{
        long[] now={100};Platform p=new Platform();AppLanguageBackend b=new AppLanguageBackend(p,()->now[0]);JSONObject r=root(b,1);String key=r.getString("key"),choice=row(r,0),folder=row(r,1);
        check(r.getJSONArray("current").length()==2&&!r.getBoolean("system_default"));check(r.getJSONArray("current").getJSONObject(0).getString("tag").equals("zz-Latn-ZZ"));
        JSONObject page=b.snapshot(2,PKG,key,folder,"",0);check(page.getInt("total")==25&&page.getJSONArray("rows").length()==20);check(page.getString("key").equals(key));
        JSONObject last=b.snapshot(3,PKG,key,folder,"",20);check(last.getJSONArray("rows").length()==5&&last.getInt("offset")==20);
        check(b.snapshot(4,PKG,key,folder,"Region 24",20).getInt("offset")==0);check(b.snapshot(5,PKG,key,folder,"null",0).getInt("total")==0);
        check(b.select(PKG,key,folder).equals("app_language_target_changed")&&p.writes==0);
        check(b.select(PKG,key,choice).equals("app_language_applied")&&p.writes==1);check(b.select(PKG,key,choice).equals("app_language_target_changed")&&p.writes==1);
        for(String change:new String[]{"incarnation","configuration","current","writable","locked"}){
            r=root(b,6);key=r.getString("key");choice=row(r,0);
            if(change.equals("incarnation"))p.incarnation+="next";else if(change.equals("writable"))p.writable=false;else if(change.equals("locked")){p.availability="restricted";p.reason="locked";}else p.revision+=change;
            String result=b.select(PKG,key,choice);check(result.equals(change.equals("locked")?"app_language_restricted":"app_language_target_changed"));check(p.writes==1);p.writable=true;p.availability="available";p.reason="none";
        }
        r=root(b,7);key=r.getString("key");choice=row(r,0);now[0]+=20001;check(b.select(PKG,key,choice).equals("app_language_target_changed"));
        b.snapshot(8,PKG,key,"","",0);check(b.select(PKG,key,choice).equals("app_language_applied"));
        r=root(b,9);key=r.getString("key");now[0]+=600001;check(b.snapshot(10,PKG,key,"","",0).getString("reason").equals("catalog_expired"));
        r=root(b,11);key=r.getString("key");p.writable=false;check(b.snapshot(12,PKG,key,"","",0).getString("availability").equals("stale"));p.writable=true;
        for(AppLanguageBackend.Write result:AppLanguageBackend.Write.values()){
            r=root(b,13);p.result=result;String expected="app_language_"+result.name().toLowerCase(java.util.Locale.ROOT);check(b.select(PKG,r.getString("key"),row(r,0)).equals(expected));
            int count=p.writes;check(b.select(PKG,r.getString("key"),row(r,0)).equals("app_language_target_changed")&&p.writes==count);
        }
        p.throwing=true;r=root(b,14);check(b.select(PKG,r.getString("key"),row(r,0)).equals("app_language_unconfirmed"));p.throwing=false;
        p.cycle=true;check(root(b,15).getString("availability").equals("unavailable"));p.cycle=false;
        p.availability="unavailable";p.reason="invalid_config";r=root(b,16);check(r.getString("reason").equals("invalid_config")&&r.opt("current")==null&&r.getJSONArray("rows").length()==0);
        for(String tag:new String[]{"en-US","../fr","","F".repeat(64)})try{AppLanguageContract.key(tag);throw new AssertionError();}catch(IllegalArgumentException expected){checks++;}
        try{AppLanguageContract.read(1,PKG,"","","",20);throw new AssertionError();}catch(IllegalArgumentException expected){checks++;}
        System.out.println("PASS app language: "+checks);
    }
}
