package dev.makepad.octosense.interactionfixture;

import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.SharedPreferences;
import android.provider.Settings;
import android.view.accessibility.AccessibilityManager;
import org.json.JSONObject;

/** DUMP-gated finite read/reset of this disposable fixture's own counters only. */
public final class InteractionFixture extends BroadcastReceiver {
    @Override public void onReceive(Context context,Intent intent){try{
        String operation=intent.getStringExtra("operation");if(!"state".equals(operation)&&!"reset_counters".equals(operation)){setResultCode(0);return;}
        InteractionActivity activity=InteractionActivity.current.get();if("reset_counters".equals(operation)&&activity!=null)activity.resetCounters();
        SharedPreferences prefs=context.getSharedPreferences("consumer",Context.MODE_PRIVATE);JSONObject result=new JSONObject().put("schema",1).put("running",activity!=null).put("draws",prefs.getInt("draws",0)).put("state",activity==null?new JSONObject(prefs.getString("state","{}")):activity.observation()).put("drawn",new JSONObject(prefs.getString("drawn","{}")));
        JSONObject raw=new JSONObject();for(String key:new String[]{"high_text_contrast_enabled","font_weight_adjustment","long_press_timeout","accessibility_interactive_ui_timeout_ms","accessibility_non_interactive_ui_timeout_ms","accessibility_autoclick_enabled","accessibility_autoclick_delay","accessibility_large_pointer_icon"}){String value=Settings.Secure.getString(context.getContentResolver(),key);raw.put(key,value==null?JSONObject.NULL:value);}for(String key:new String[]{"window_animation_scale","transition_animation_scale","animator_duration_scale"}){String value=Settings.Global.getString(context.getContentResolver(),key);raw.put(key,value==null?JSONObject.NULL:value);}result.put("raw",raw);
        AccessibilityManager manager=context.getSystemService(AccessibilityManager.class);Object contrast=JSONObject.NULL;String spelling="";if(manager!=null)for(String name:new String[]{"isHighContrastTextEnabled","isHighTextContrastEnabled"})try{contrast=AccessibilityManager.class.getMethod(name).invoke(manager);spelling=name;break;}catch(ReflectiveOperationException|SecurityException unavailable){/* Public-SDK target may legitimately lack access. */}
        result.put("native_high_contrast",contrast).put("native_contrast_getter",spelling);setResultCode(1);setResultData(android.util.Base64.encodeToString(result.toString().getBytes(java.nio.charset.StandardCharsets.UTF_8),android.util.Base64.NO_WRAP));
    }catch(Exception|LinkageError failure){setResultCode(0);setResultData("{\"error\":\""+failure.getClass().getSimpleName()+"\"}");}}
}
