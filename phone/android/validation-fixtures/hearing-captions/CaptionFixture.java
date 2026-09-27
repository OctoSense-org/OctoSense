package dev.makepad.octosense.hearingfixture;

import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.view.accessibility.CaptioningManager;
import org.json.JSONObject;

/** DUMP-gated observation only. Reading does not force-stop the consumer or change settings. */
public final class CaptionFixture extends BroadcastReceiver {
    @Override public void onReceive(Context context,Intent intent){try{
        if(!"dev.makepad.octosense.hearingfixture.COMMAND".equals(intent.getAction())||!"state".equals(intent.getStringExtra("operation")))throw new IllegalArgumentException("Unknown fixture operation");
        CaptioningManager manager=context.getSystemService(CaptioningManager.class);if(manager==null)throw new IllegalStateException("CaptioningManager missing");CaptioningManager.CaptionStyle style=manager.getUserStyle();
        JSONObject observed=new JSONObject().put("enabled",manager.isEnabled()).put("font_scale",manager.getFontScale()).put("locale",manager.getLocale()==null?JSONObject.NULL:manager.getLocale().toString()).put("foreground",style.foregroundColor).put("background",style.backgroundColor).put("window",style.windowColor).put("edge_type",style.edgeType).put("edge_color",style.edgeColor).put("has_foreground",style.hasForegroundColor()).put("has_background",style.hasBackgroundColor()).put("has_window",style.hasWindowColor()).put("has_edge_type",style.hasEdgeType()).put("has_edge_color",style.hasEdgeColor());
        android.graphics.Typeface face=style.getTypeface();org.json.JSONArray matches=new org.json.JSONArray();
        for(String family:new String[]{"sans-serif","sans-serif-condensed","sans-serif-monospace","serif","serif-monospace","casual","cursive","sans-serif-smallcaps"})if(android.graphics.Typeface.create(family,android.graphics.Typeface.NORMAL).equals(face))matches.put(family);
        observed.put("typeface_default",face==null).put("typeface_matches",matches);
        android.content.SharedPreferences prefs=context.getSharedPreferences("caption_consumer",Context.MODE_PRIVATE);JSONObject state=new JSONObject().put("observed",observed);
        for(String key:new String[]{"created","callbacks","enabled_callbacks","style_callbacks","scale_callbacks","locale_callbacks","draws"})state.put(key,prefs.getInt(key,0));
        for(String key:new String[]{"applied","drawn"}){String json=prefs.getString(key,"");state.put(key,json.isEmpty()?JSONObject.NULL:new JSONObject(json));}state.put("error",prefs.getString("error",""));
        setResultCode(1);setResultData(android.util.Base64.encodeToString(state.toString().getBytes(java.nio.charset.StandardCharsets.UTF_8),android.util.Base64.NO_WRAP));
    }catch(Exception error){setResultCode(0);setResultData(error.getClass().getSimpleName());}}
}
