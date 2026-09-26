package dev.makepad.octosense.keyboards;

import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

/** Bounded observation wire; component names never become caller-supplied actions. */
public final class KeyboardJson {
    private KeyboardJson(){}
    public static JSONObject unavailable(long id,String query,String availability){
        try{return new JSONObject().put("schema",1).put("request_id",id).put("availability",availability)
                .put("reason","restricted".equals(availability)?"locked":"service_unavailable")
                .put("key",JSONObject.NULL).put("default_id",JSONObject.NULL).put("can_choose_default",false)
                .put("query",query).put("offset",0).put("total",0).put("page_size",KeyboardPolicy.PAGE_SIZE).put("rows",new JSONArray());}
        catch(JSONException impossible){throw new IllegalStateException(impossible);}
    }
    public static JSONObject page(long id,KeyboardPolicy.Page page){
        try{
            JSONObject value=unavailable(id,page.query,page.availability).put("reason",page.reason)
                    .put("key",page.key==null?JSONObject.NULL:page.key)
                    .put("default_id",page.defaultId==null?JSONObject.NULL:page.defaultId)
                    .put("can_choose_default",page.canPickDefault).put("offset",page.offset).put("total",page.total);
            JSONArray rows=new JSONArray();for(KeyboardPolicy.Row item:page.rows){KeyboardPolicy.Method row=item.method;
                rows.put(new JSONObject().put("target",item.target).put("id",row.id).put("package",row.packageName)
                        .put("label",row.label).put("summary",row.summary).put("restriction",row.restriction)
                        .put("enabled",row.enabled).put("selected",row.selected).put("system",row.system)
                        .put("direct_boot",row.directBoot).put("can_enable",row.canEnable).put("can_disable",row.canDisable)
                        .put("can_settings",row.settings).put("can_subtypes",row.subtypes));
            }return value.put("rows",rows);
        }catch(JSONException impossible){throw new IllegalStateException(impossible);}
    }
}
