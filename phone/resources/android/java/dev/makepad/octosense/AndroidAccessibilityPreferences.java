package dev.makepad.octosense;

import android.content.Context;
import android.content.res.Configuration;
import android.database.ContentObserver;
import android.os.Build;
import android.os.Handler;
import android.provider.Settings;
import android.view.accessibility.AccessibilityManager;
import org.json.JSONObject;

/** Read-only native appearance/motion observations shared by all hosted renderers. */
final class AndroidAccessibilityPreferences implements AutoCloseable {
    private final Context context;
    private final ContentObserver observer;
    private final AccessibilityManager manager;
    private final Runnable removeServicesListener;
    AndroidAccessibilityPreferences(Context context,Handler main,Runnable changed){
        this.context=context;
        observer=new ContentObserver(main){@Override public void onChange(boolean selfChange){changed.run();}};
        for(String key:new String[]{"high_text_contrast_enabled","accessibility_interactive_ui_timeout_ms","accessibility_non_interactive_ui_timeout_ms"})try{
            context.getContentResolver().registerContentObserver(Settings.Secure.getUriFor(key),false,observer);
        }catch(SecurityException unavailable){/* Snapshot remains the read-only fallback on resume. */}
        try{context.getContentResolver().registerContentObserver(Settings.Global.getUriFor(Settings.Global.ANIMATOR_DURATION_SCALE),false,observer);}
        catch(SecurityException unavailable){/* Snapshot remains the read-only fallback on resume. */}
        manager=context.getSystemService(AccessibilityManager.class);
        removeServicesListener=Build.VERSION.SDK_INT>=33&&manager!=null?Api33.listen(context,manager,changed):()->{};
    }
    JSONObject snapshot(Configuration configuration){
        JSONObject out=new JSONObject();
        try{
            out.put("schema",1);
            Object contrast=JSONObject.NULL,scale=JSONObject.NULL;
            try{
                String raw=Settings.Secure.getString(context.getContentResolver(),"high_text_contrast_enabled");
                int value=raw==null?0:Integer.parseInt(raw);if(value==0||value==1)contrast=value==1;
            }catch(SecurityException|IllegalArgumentException unavailable){/* Keep unknown observations explicit. */}
            try{float value=Settings.Global.getFloat(context.getContentResolver(),Settings.Global.ANIMATOR_DURATION_SCALE,1f);if(Float.isFinite(value)&&value>=0)scale=value;}
            catch(SecurityException|IllegalArgumentException unavailable){/* Keep unknown observations explicit. */}
            int weight=Build.VERSION.SDK_INT>=31?configuration.fontWeightAdjustment:0;
            if(weight==Integer.MAX_VALUE)weight=0; // Configuration's native undefined value means no adjustment.
            out.put("high_contrast_text",contrast).put("font_weight_adjustment",weight>=-1000&&weight<=1000?weight:JSONObject.NULL).put("animator_scale",scale);
            if(Build.VERSION.SDK_INT>=29&&manager!=null){
                out.put("interactive_timeout_ms",manager.getRecommendedTimeoutMillis(0,AccessibilityManager.FLAG_CONTENT_CONTROLS));
                out.put("noninteractive_timeout_ms",manager.getRecommendedTimeoutMillis(0,AccessibilityManager.FLAG_CONTENT_TEXT|AccessibilityManager.FLAG_CONTENT_ICONS));
            }
        }catch(Exception unavailable){/* An absent field cannot fabricate a new renderer preference. */}
        return out;
    }
    @Override public void close(){
        context.getContentResolver().unregisterContentObserver(observer);
        removeServicesListener.run();
    }
    private static final class Api33 {
        static Runnable listen(Context context,AccessibilityManager manager,Runnable changed){
            AccessibilityManager.AccessibilityServicesStateChangeListener listener=value->changed.run();
            try{manager.addAccessibilityServicesStateChangeListener(context.getMainExecutor(),listener);}
            catch(SecurityException unavailable){return ()->{};}
            return ()->manager.removeAccessibilityServicesStateChangeListener(listener);
        }
    }
}
