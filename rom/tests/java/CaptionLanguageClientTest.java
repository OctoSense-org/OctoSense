package dev.makepad.octosense;

import org.json.JSONArray;
import org.json.JSONObject;

public final class CaptionLanguageClientTest {
    private static final String KEY="a".repeat(64),DEFAULT="b".repeat(64),FRENCH="c".repeat(64),FOREIGN="d".repeat(64);
    private static void check(boolean value,String why){if(!value)throw new AssertionError(why);}
    interface Throwing{void run()throws Exception;}
    private static void invalid(Throwing action)throws Exception{try{action.run();throw new AssertionError("Malformed caption state accepted");}catch(IllegalArgumentException|IllegalStateException expected){}}
    private static JSONObject state(long id)throws Exception{return new JSONObject().put("schema",1).put("page_size",24).put("request_id",id).put("status","ready").put("key",KEY).put("current","").put("system_default",true).put("custom",false).put("query","").put("offset",0).put("total",2).put("rows",new JSONArray().put(new JSONObject().put("choice",DEFAULT).put("label","System default").put("locale","").put("selected",true)).put(new JSONObject().put("choice",FRENCH).put("label","Français").put("locale","fr_FR").put("selected",false)));}
    private static final class Fake implements CaptionLanguageSettingsClient.Bridge {
        boolean foreground=true,loseFocus;int reads,writes;JSONObject next;String selected;
        final CaptionLanguageSettingsClient client=new CaptionLanguageSettingsClient(this,()->foreground);
        public JSONObject snapshot(long id,String key,String query,int offset)throws Exception{reads++;if(loseFocus)foreground=false;return next==null?state(id):next;}
        public String select(String key,String choice){writes++;selected=choice;return "control_applied";}
        JSONObject read()throws Exception{return client.snapshot(1,"","",0);}
    }
    public static void main(String[] args)throws Exception{
        android.os.SystemClock.now=1000;Fake f=new Fake();check(f.read().getString("status").equals("ready"),"Native catalog not observed");check(f.client.select(KEY,FRENCH).equals("control_applied")&&f.writes==1,"Visible native choice rejected");check(f.client.select(KEY,FRENCH).equals("caption_language_changed")&&f.writes==1,"Selection replayed");
        f.read();check(f.client.select(KEY,FOREIGN).equals("caption_language_changed")&&f.writes==1,"Unoffered key crossed bridge");
        f.read();f.foreground=false;check(f.client.select(KEY,FRENCH).equals("caption_language_changed")&&f.writes==1,"Background selection crossed bridge");f.foreground=true;check(f.client.select(KEY,FRENCH).equals("caption_language_changed"),"Focus restore revived old selection");
        f.read();android.os.SystemClock.now+=20001;check(f.client.select(KEY,FRENCH).equals("caption_language_changed"),"Expired page crossed bridge");
        f.read();android.os.SystemClock.now--;check(f.client.select(KEY,FRENCH).equals("caption_language_changed"),"Clock rollback retained authority");
        f=new Fake();f.foreground=false;check(f.read().getString("status").equals("policy_restricted")&&f.reads==0,"Background page called service");
        f=new Fake();f.loseFocus=true;check(f.read().getString("status").equals("policy_restricted")&&f.client.select(KEY,FRENCH).equals("caption_language_changed"),"Focus loss during read invented choices");
        for(String field:new String[]{"schema","page_size","request_id","offset","total"}){final Fake bad=new Fake();bad.next=state(1).put(field,9999);invalid(bad::read);check(bad.client.select(KEY,FRENCH).equals("caption_language_changed")&&bad.writes==0,"Malformed state offered mutation");}
        final Fake custom=new Fake();custom.next=state(1).put("custom",true);invalid(custom::read);
        final Fake duplicate=new Fake();duplicate.next=state(1);duplicate.next.getJSONArray("rows").getJSONObject(1).put("choice",DEFAULT);invalid(duplicate::read);
        final Fake mismatched=new Fake();mismatched.next=state(1);mismatched.next.getJSONArray("rows").getJSONObject(1).put("selected",true);invalid(mismatched::read);
        final Fake unavailable=new Fake();unavailable.next=state(1).put("status","control_unavailable");invalid(unavailable::read);
        final Fake request=new Fake();invalid(()->request.client.snapshot(0,"","",0));invalid(()->request.client.snapshot(1,"fr_FR","",0));invalid(()->request.client.snapshot(1,"","",24));invalid(()->request.client.snapshot(1,KEY,"",25));invalid(()->request.client.snapshot(1,KEY,"\n",0));invalid(()->request.client.select(KEY,"fr_FR"));
        f=new Fake();f.read();f.client.invalidate();check(f.client.select(KEY,FRENCH).equals("caption_language_changed"),"Explicit lifecycle retirement ignored");
        System.out.println("Caption language foreground, bounded snapshot and one-use bridge tests passed");
    }
}
