package dev.makepad.octosense;

import dev.makepad.octosense.systemlanguage.SystemLanguageContract;
import dev.makepad.octosense.systemlanguage.SystemLanguageSettings;
import java.util.List;
import org.json.JSONObject;

public final class SystemLanguageSettingsTest {
    static int checks;
    static void check(boolean value){if(!value)throw new AssertionError("System language check "+checks);checks++;}
    interface Bad {void run()throws Exception;}
    static void invalid(Bad code)throws Exception{try{code.run();throw new AssertionError("Accepted malformed language operation");}catch(IllegalArgumentException expected){checks++;}}
    static final class Platform implements SystemLanguageSettings.Platform {
        String revision="native-A",availability="available",reason="none";boolean writable=true,cycle,empty,throwing,duplicate;int writes;
        SystemLanguageSettings.Write result=SystemLanguageSettings.Write.APPLIED;List<SystemLanguageSettings.Selection> written;
        public SystemLanguageSettings.State read(){
            SystemLanguageSettings.State state=new SystemLanguageSettings.State();state.revision=revision;state.availability=availability;state.reason=reason;state.writable=writable;
            if(!empty){state.current.add(new SystemLanguageSettings.Current("en-US-u-hc-h23","English (United States)","English",true));state.current.add(new SystemLanguageSettings.Current("zz-Latn-ZZ-u-nu-latn","Custom language","Custom language",false));}
            state.nodes.add(new SystemLanguageSettings.Node("fr",cycle?"fr":"","Français","French","open","region","fr",true,false));
            for(int i=0;i<25;i++)state.nodes.add(new SystemLanguageSettings.Node("fr"+i,"fr","Region "+i,"French region","select","region",duplicate?"en-US-u-hc-h23":"fr-X"+i+"-u-hc-h23",true,false));
            state.nodes.add(new SystemLanguageSettings.Node("ar","","العربية","Arabic","open","region","ar",true,false));
            state.nodes.add(new SystemLanguageSettings.Node("ar-EG","ar","مصر","Egypt","open","numbering","ar-EG",true,false));
            state.nodes.add(new SystemLanguageSettings.Node("ar-EG-nu","ar-EG","Latin digits","latn","select","numbering","ar-EG-u-hc-h23-nu-latn",true,false));return state;
        }
        public SystemLanguageSettings.Write apply(SystemLanguageSettings.State state,List<SystemLanguageSettings.Selection> order){writes++;written=order;if(throwing)throw new IllegalStateException("native unknown");return result;}
    }
    static JSONObject root(SystemLanguageSettings backend)throws Exception{return backend.snapshot(1,"","","",0);}
    static String row(JSONObject value,int index){return value.getJSONArray("rows").getJSONObject(index).getString("target");}
    static String current(JSONObject value,int index){return value.getJSONArray("current").getJSONObject(index).getString("target");}
    public static void main(String[] args)throws Exception{
        long[] now={100};Platform platform=new Platform();SystemLanguageSettings backend=new SystemLanguageSettings(platform,()->now[0]);JSONObject state=root(backend);String key=state.getString("key"),first=current(state,0),custom=current(state,1),french=row(state,0);
        check(state.getJSONArray("current").getJSONObject(1).getString("tag").equals("zz-Latn-ZZ-u-nu-latn"));
        check(backend.apply(key,new String[]{french}).equals("languages_target_changed")&&platform.writes==0);
        JSONObject regions=backend.snapshot(2,key,french,"",0);String leaf=row(regions,0);check(regions.getInt("total")==25&&regions.getJSONArray("rows").length()==20);
        check(backend.snapshot(3,key,french,"",20).getJSONArray("rows").length()==5);
        check(backend.snapshot(4,key,french,"Region 24",20).getInt("offset")==0);
        now[0]+=30000;check(backend.apply(key,new String[]{custom,leaf,first}).equals("languages_applied"));
        check(platform.written.size()==3&&platform.written.get(0).tag.equals("zz-Latn-ZZ-u-nu-latn")&&platform.written.get(1).tag.equals("fr-X0-u-hc-h23"));
        check(backend.apply(key,new String[]{first}).equals("languages_target_changed")&&platform.writes==1);
        state=root(backend);key=state.getString("key");JSONObject arabic=backend.snapshot(5,key,row(state,1),"",0);JSONObject numbering=backend.snapshot(6,key,row(arabic,0),"",0);
        check(numbering.getString("level").equals("numbering"));check(backend.apply(key,new String[]{row(numbering,0)}).equals("languages_applied"));
        check(platform.written.get(0).tag.equals("ar-EG-u-hc-h23-nu-latn"));
        for(String change:new String[]{"current","regional_preferences","catalog","writable","locked","policy"}){
            state=root(backend);key=state.getString("key");String target=current(state,0);int count=platform.writes;
            if(change.equals("writable"))platform.writable=false;else if(change.equals("locked")||change.equals("policy")){platform.availability="restricted";platform.reason=change;}else platform.revision+=change;
            check(backend.apply(key,new String[]{target}).equals(change.equals("locked")||change.equals("policy")?"languages_restricted":"languages_target_changed"));check(platform.writes==count);
            platform.writable=true;platform.availability="available";platform.reason="none";
        }
        state=root(backend);key=state.getString("key");now[0]+=590000;backend.snapshot(7,key,"","",0);now[0]+=10001;
        check(backend.snapshot(8,key,"","",0).getString("reason").equals("catalog_expired"));
        for(SystemLanguageSettings.Write result:SystemLanguageSettings.Write.values()){
            platform.result=result;state=root(backend);key=state.getString("key");String target=current(state,0);int before=platform.writes;
            check(backend.apply(key,new String[]{target}).equals("languages_"+result.name().toLowerCase(java.util.Locale.ROOT))&&platform.writes==before+1);
            check(backend.apply(key,new String[]{target}).equals("languages_target_changed")&&platform.writes==before+1);
        }
        platform.throwing=true;state=root(backend);check(backend.apply(state.getString("key"),new String[]{current(state,0)}).equals("languages_unconfirmed"));platform.throwing=false;
        platform.duplicate=true;state=root(backend);key=state.getString("key");regions=backend.snapshot(9,key,row(state,0),"",0);int before=platform.writes;
        check(backend.apply(key,new String[]{current(state,0),row(regions,0)}).equals("languages_target_changed")&&platform.writes==before);platform.duplicate=false;
        platform.empty=true;check(root(backend).getString("reason").equals("invalid_catalog"));platform.empty=false;platform.cycle=true;check(root(backend).getString("reason").equals("invalid_catalog"));platform.cycle=false;
        String observedKey="a".repeat(64),token="b".repeat(64);invalid(()->SystemLanguageContract.order(observedKey,new String[0]));invalid(()->SystemLanguageContract.order(observedKey,new String[]{token,token}));invalid(()->SystemLanguageContract.order(observedKey,new String[]{"en-US"}));invalid(()->SystemLanguageContract.read(0,"","","",0));invalid(()->SystemLanguageContract.read(1,"","","",20));invalid(()->SystemLanguageContract.query("x\n"));invalid(()->SystemLanguageContract.query("字".repeat(81)));check(SystemLanguageContract.query("😀".repeat(80)).codePointCount(0,160)==80);invalid(()->SystemLanguageContract.query("😀".repeat(81)));
        // Home authorization accumulates only displayed leaves across the same fixed catalog.
        boolean[] foreground={true};platform.result=SystemLanguageSettings.Write.APPLIED;
        SystemLanguageSettingsClient client=new SystemLanguageSettingsClient(new SystemLanguageSettingsClient.Bridge(){public JSONObject snapshot(long id,String key,String parent,String query,int offset)throws Exception{return backend.snapshot(id,key,parent,query,offset);}public String apply(String key,String[] targets){return backend.apply(key,targets);}},()->foreground[0]);
        state=client.snapshot(10,"","","",0);key=state.getString("key");first=current(state,0);regions=client.snapshot(11,key,row(state,0),"",0);leaf=row(regions,0);client.snapshot(12,key,"","",0);
        check(client.apply(key,new String[]{leaf,first}).equals("languages_applied"));check(client.apply(key,new String[]{first}).equals("languages_target_changed"));
        state=client.snapshot(13,"","","",0);key=state.getString("key");foreground[0]=false;check(client.apply(key,new String[]{current(state,0)}).equals("languages_target_changed"));client.invalidate();foreground[0]=true;
        check(client.snapshot(14,key,"","",0).getString("availability").equals("stale"));
        System.out.println("PASS system languages "+checks);
    }
}
