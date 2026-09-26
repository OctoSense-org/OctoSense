package dev.makepad.octosense.interactionfixture;

import android.app.Activity;
import android.content.Context;
import android.content.SharedPreferences;
import android.content.res.Configuration;
import android.graphics.Canvas;
import android.graphics.Color;
import android.os.Bundle;
import android.os.SystemClock;
import android.view.MotionEvent;
import android.view.View;
import android.view.ViewConfiguration;
import android.view.accessibility.AccessibilityManager;
import android.widget.Button;
import android.widget.LinearLayout;
import android.widget.TextView;
import java.lang.ref.WeakReference;
import org.json.JSONArray;
import org.json.JSONObject;

/** Independent ordinary Android Views. No system settings setters or privileges. */
public final class InteractionActivity extends Activity {
    static WeakReference<InteractionActivity> current=new WeakReference<>(null);
    private SharedPreferences prefs;private SampleLayout layout;private TextView sample;private Button target;
    private int created,configurations,draws,clicks,longClicks,mouseClicks,hovers;
    private long downAt,hoverAt,lastLongDelay=-1,lastMouseDelay=-1;private boolean resumed;
    private String previousDraw="";
    private float mouseX=-1,mouseY=-1;private int mouseDevice=-1;
    @Override public boolean dispatchGenericMotionEvent(MotionEvent event){
        if(event.isFromSource(android.view.InputDevice.SOURCE_MOUSE)){
            mouseX=event.getRawX();mouseY=event.getRawY();mouseDevice=event.getDeviceId();save();
        }
        return super.dispatchGenericMotionEvent(event);
    }
    @Override public void onCreate(Bundle state){super.onCreate(state);current=new WeakReference<>(this);prefs=getSharedPreferences("consumer",MODE_PRIVATE);created=prefs.getInt("created",0)+1;configurations=prefs.getInt("configurations",0);draws=prefs.getInt("draws",0);
        layout=new SampleLayout(this);layout.setOrientation(LinearLayout.VERTICAL);layout.setPadding(24,24,24,24);layout.setBackgroundColor(Color.WHITE);
        TextView title=new TextView(this);title.setText("Independent Android text and interaction consumer");title.setTextColor(Color.BLACK);title.setTextSize(18);layout.addView(title);
        sample=new TextView(this);sample.setText("Readable Aa 123\n可读文字");sample.setTextColor(Color.rgb(150,150,150));sample.setTextSize(22);sample.setPadding(12,24,12,24);layout.addView(sample,new LinearLayout.LayoutParams(-1,-2));
        target=new Button(this);target.setText("Touch, hold or hover here");target.setMinHeight(180);layout.addView(target,new LinearLayout.LayoutParams(-1,220));
        target.setOnTouchListener((view,event)->{if(event.getActionMasked()==MotionEvent.ACTION_DOWN){downAt=SystemClock.uptimeMillis();if(event.isFromSource(android.view.InputDevice.SOURCE_MOUSE)){mouseClicks++;lastMouseDelay=hoverAt==0?-1:downAt-hoverAt;}}save();return false;});
        target.setOnHoverListener((view,event)->{if(event.getActionMasked()==MotionEvent.ACTION_HOVER_ENTER||event.getActionMasked()==MotionEvent.ACTION_HOVER_MOVE){hoverAt=SystemClock.uptimeMillis();hovers++;save();}return false;});
        target.setOnClickListener(view->{clicks++;save();});target.setOnLongClickListener(view->{longClicks++;lastLongDelay=downAt==0?-1:SystemClock.uptimeMillis()-downAt;save();return true;});setContentView(layout);save();
    }
    @Override public void onConfigurationChanged(Configuration configuration){super.onConfigurationChanged(configuration);configurations++;save();layout.invalidate();}
    @Override public void onResume(){super.onResume();resumed=true;save();}
    @Override public void onPause(){resumed=false;save();super.onPause();}
    @Override public void onWindowFocusChanged(boolean focus){super.onWindowFocusChanged(focus);save();}
    @Override public void onDestroy(){if(current.get()==this)current=new WeakReference<>(null);super.onDestroy();}
    JSONObject observation(){save();return viewState();}
    void resetCounters(){clicks=longClicks=mouseClicks=hovers=0;downAt=hoverAt=0;lastLongDelay=lastMouseDelay=-1;save();}
    private JSONObject viewState(){try{int[] xy=new int[2];target.getLocationOnScreen(xy);Configuration config=getResources().getConfiguration();AccessibilityManager manager=getSystemService(AccessibilityManager.class);return new JSONObject().put("resumed",resumed).put("focused",hasWindowFocus()).put("created",created).put("configurations",configurations).put("configuration_weight",config.fontWeightAdjustment).put("font_scale",config.fontScale).put("text_size_px",sample.getTextSize()).put("text_color",sample.getCurrentTextColor()).put("original_typeface_weight",sample.getTypeface().getWeight()).put("typeface_weight",sample.getPaint().getTypeface().getWeight()).put("long_press_ms",ViewConfiguration.getLongPressTimeout()).put("recommended_controls_ms",manager==null?JSONObject.NULL:manager.getRecommendedTimeoutMillis(0,AccessibilityManager.FLAG_CONTENT_CONTROLS)).put("recommended_other_ms",manager==null?JSONObject.NULL:manager.getRecommendedTimeoutMillis(0,AccessibilityManager.FLAG_CONTENT_TEXT|AccessibilityManager.FLAG_CONTENT_ICONS)).put("clicks",clicks).put("long_clicks",longClicks).put("mouse_downs",mouseClicks).put("mouse_x",mouseX).put("mouse_y",mouseY).put("mouse_device",mouseDevice).put("hover_events",hovers).put("last_long_delay_ms",lastLongDelay).put("last_mouse_delay_ms",lastMouseDelay).put("target",new JSONArray(new int[]{xy[0],xy[1],target.getWidth(),target.getHeight()}));}catch(Exception impossible){throw new IllegalStateException(impossible);}}
    private void save(){if(prefs==null||sample==null||target==null)return;prefs.edit().putInt("created",created).putInt("configurations",configurations).putString("state",viewState().toString()).apply();}
    private final class SampleLayout extends LinearLayout {
        SampleLayout(Context context){super(context);}
        @Override protected void dispatchDraw(Canvas canvas){super.dispatchDraw(canvas);if(sample==null||target==null)return;String state=viewState().toString();if(!state.equals(previousDraw)){previousDraw=state;draws++;prefs.edit().putString("drawn",state).putInt("draws",draws).apply();}save();}
    }
}
