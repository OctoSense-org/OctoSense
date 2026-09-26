package dev.makepad.octosense.controls;

import org.json.JSONArray;
import org.json.JSONObject;

/** Scope envelope around the same finite Controls row schema. */
public final class CaptionCustomContract {
    private CaptionCustomContract(){throw new AssertionError();}
    public static void read(long id,long visit){if(id<=0||visit<=0)throw new IllegalArgumentException("Positive caption identity required");}
    public static JSONObject envelope(long id,long visit,Boolean custom,boolean active,JSONArray controls)throws Exception{
        read(id,visit);return new JSONObject().put("schema",1).put("request_id",id).put("visit",visit)
            .put("page","caption_custom").put("custom_selected",custom==null?JSONObject.NULL:custom).put("scope_active",active).put("controls",controls);
    }
    public static JSONObject unavailable(long id,long visit)throws Exception{
        JSONArray rows=new JSONArray();for(CaptionCustomSettings.Field field:CaptionCustomSettings.Field.values())rows.put(new JSONObject()
            .put("id",field.id).put("value",JSONObject.NULL).put("options",new JSONArray()).put("availability","unavailable"));
        return envelope(id,visit,null,false,rows);
    }
}
