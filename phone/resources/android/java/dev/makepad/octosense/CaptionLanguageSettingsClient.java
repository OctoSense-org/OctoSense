package dev.makepad.octosense;

import android.os.SystemClock;
import java.util.HashSet;
import java.util.Set;
import java.util.function.BooleanSupplier;
import org.json.JSONArray;
import org.json.JSONObject;

/** The foreground page offers one reviewed native caption-language mutation at a time. */
final class CaptionLanguageSettingsClient {
    interface Bridge {
        JSONObject snapshot(long id,String key,String query,int offset)throws Exception;
        String select(String key,String choice)throws Exception;
    }
    private final Bridge bridge;private final BooleanSupplier foreground;
    private String key;private long observed=-1;private final Set<String> choices=new HashSet<>();
    CaptionLanguageSettingsClient(Bridge bridge,BooleanSupplier foreground){this.bridge=bridge;this.foreground=foreground;}
    synchronized void invalidate(){key=null;observed=-1;choices.clear();}
    static String key(String value){if(value==null||!value.matches("[0-9a-f]{64}"))throw new IllegalArgumentException("Invalid caption language key");return value;}
    static void request(long id,String key,String query,int offset){
        if(id<=0||key==null||!key.isEmpty()&&!key.matches("[0-9a-f]{64}")||query==null||query.length()>80||query.codePoints().anyMatch(Character::isISOControl)||offset<0||offset>1024||offset%24!=0||key.isEmpty()&&offset!=0)throw new IllegalArgumentException("Invalid caption language page");
    }
    static JSONObject unavailable(long id,String query,int offset,String reason)throws Exception{return new JSONObject().put("schema",1).put("request_id",id).put("page_size",24).put("status",reason).put("key","").put("current","").put("system_default",false).put("custom",false).put("query",query).put("offset",offset).put("total",0).put("rows",new JSONArray());}
    private static String text(JSONObject value,String name,int max,boolean empty)throws Exception{String text=value.getString(name);if((!empty&&text.isEmpty())||text.length()>max||text.codePoints().anyMatch(Character::isISOControl))throw new IllegalStateException("Invalid caption language text");return text;}
    synchronized JSONObject snapshot(long id,String catalog,String query,int offset)throws Exception{
        request(id,catalog,query,offset);invalidate();if(!foreground.getAsBoolean())return unavailable(id,query,offset,"policy_restricted");
        JSONObject state=bridge.snapshot(id,catalog,query,offset);if(state==null)return unavailable(id,query,offset,"control_unavailable");
        if(state.getInt("schema")!=1||state.getInt("page_size")!=24||state.getLong("request_id")!=id||!query.equals(state.getString("query"))||state.getInt("offset")!=offset)throw new IllegalStateException("Uncorrelated caption language state");
        String reason=state.getString("status"),current=text(state,"current",128,true);boolean systemDefault=state.getBoolean("system_default"),custom=state.getBoolean("custom");int total=state.getInt("total");JSONArray rows=state.getJSONArray("rows");
        if(!reason.equals("ready")){
            if(!Set.of("policy_restricted","control_unavailable","caption_language_changed","caption_language_page_changed").contains(reason)||!state.getString("key").isEmpty()||!current.isEmpty()||systemDefault||custom||total!=0||rows.length()!=0)throw new IllegalStateException("Unavailable caption catalog offered state");
            return state;
        }
        String reviewed=key(state.getString("key"));if(!catalog.isEmpty()&&!catalog.equals(reviewed)||total<0||total>1025||offset>total||rows.length()!=Math.min(24,total-offset)||systemDefault!=current.isEmpty()||systemDefault&&custom)throw new IllegalStateException("Invalid caption catalog bounds");
        Set<String> offered=new HashSet<>(),locales=new HashSet<>();
        for(int i=0;i<rows.length();i++){JSONObject row=rows.getJSONObject(i);String choice=key(row.getString("choice")),locale=text(row,"locale",128,true);text(row,"label",256,false);boolean selected=row.getBoolean("selected");if(!offered.add(choice)||!locales.add(locale)||selected!=locale.equals(current)||selected&&custom)throw new IllegalStateException("Invalid caption language choice");}
        if(!foreground.getAsBoolean())return unavailable(id,query,offset,"policy_restricted");key=reviewed;choices.addAll(offered);observed=SystemClock.elapsedRealtime();return state;
    }
    synchronized String select(String catalog,String choice)throws Exception{
        key(catalog);key(choice);long now=SystemClock.elapsedRealtime();
        if(!foreground.getAsBoolean()||!catalog.equals(key)||observed<0||now<observed||now-observed>20000||!choices.contains(choice)){invalidate();return "caption_language_changed";}
        invalidate();return bridge.select(catalog,choice);
    }
}
