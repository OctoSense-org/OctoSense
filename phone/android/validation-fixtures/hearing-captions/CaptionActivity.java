package dev.makepad.octosense.hearingfixture;

import android.app.Activity;
import android.content.Context;
import android.content.SharedPreferences;
import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.Typeface;
import android.graphics.drawable.ColorDrawable;
import android.os.Bundle;
import android.view.View;
import android.view.accessibility.CaptioningManager;
import android.widget.LinearLayout;
import android.widget.TextView;
import java.util.Locale;
import org.json.JSONObject;

/** Ordinary independent caption consumer. It never writes a device preference. */
public final class CaptionActivity extends Activity {
    private android.media.AudioTrack silentTrack;
    private CaptioningManager manager;private SharedPreferences prefs;private TextView sample;private RenderedLayout layout;
    private int created,callbacks,enabledCallbacks,styleCallbacks,scaleCallbacks,localeCallbacks,draws;
    private String lastDraw="";
    private final CaptioningManager.CaptioningChangeListener listener=new CaptioningManager.CaptioningChangeListener(){
        @Override public void onEnabledChanged(boolean enabled){callbacks++;enabledCallbacks++;apply();}
        @Override public void onUserStyleChanged(CaptioningManager.CaptionStyle style){callbacks++;styleCallbacks++;apply();}
        @Override public void onFontScaleChanged(float scale){callbacks++;scaleCallbacks++;apply();}
        @Override public void onLocaleChanged(Locale locale){callbacks++;localeCallbacks++;apply();}
    };
    @Override public void onCreate(Bundle saved){super.onCreate(saved);prefs=getSharedPreferences("caption_consumer",Context.MODE_PRIVATE);created=prefs.getInt("created",0)+1;callbacks=prefs.getInt("callbacks",0);enabledCallbacks=prefs.getInt("enabled_callbacks",0);styleCallbacks=prefs.getInt("style_callbacks",0);scaleCallbacks=prefs.getInt("scale_callbacks",0);localeCallbacks=prefs.getInt("locale_callbacks",0);draws=prefs.getInt("draws",0);
        manager=getSystemService(CaptioningManager.class);layout=new RenderedLayout(this);layout.setOrientation(LinearLayout.VERTICAL);layout.setPadding(24,24,24,24);layout.setBackgroundColor(Color.rgb(225,225,225));
        TextView heading=new TextView(this);heading.setText("Independent Android caption consumer");heading.setTextColor(Color.BLACK);heading.setTextSize(18);layout.addView(heading);
        sample=new TextView(this);sample.setText("Caption sample\n字幕示例");sample.setPadding(16,16,16,16);layout.addView(sample,new LinearLayout.LayoutParams(-1,-2));setContentView(layout);
        if(manager!=null)manager.addCaptioningChangeListener(listener);apply();
        // Exercise the real mixer without producing audible sound or changing volume.
        // Idle AudioFlinger outputs retain the previous balance processor gains.
        silentTrack=new android.media.AudioTrack.Builder().setAudioAttributes(new android.media.AudioAttributes.Builder().setUsage(android.media.AudioAttributes.USAGE_MEDIA).setContentType(android.media.AudioAttributes.CONTENT_TYPE_MUSIC).build())
                .setAudioFormat(new android.media.AudioFormat.Builder().setEncoding(android.media.AudioFormat.ENCODING_PCM_16BIT).setSampleRate(48000).setChannelMask(android.media.AudioFormat.CHANNEL_OUT_STEREO).build())
                .setTransferMode(android.media.AudioTrack.MODE_STATIC).setBufferSizeInBytes(48000*4).build();
        short[] silence=new short[48000*2];
        if(silentTrack.write(silence,0,silence.length)!=silence.length||silentTrack.setLoopPoints(0,48000,-1)!=android.media.AudioTrack.SUCCESS)throw new IllegalStateException("Silent mixer fixture failed");
        silentTrack.play();
    }
    @Override public void onResume(){super.onResume();if(sample!=null)apply();}
    @Override public void onDestroy(){if(silentTrack!=null){silentTrack.release();silentTrack=null;}if(manager!=null)manager.removeCaptioningChangeListener(listener);super.onDestroy();}
    private int foreground=Color.WHITE,background=Color.BLACK,window=Color.TRANSPARENT;private float appliedScale=1;private String error="";
    private void apply(){
        try{if(manager==null)throw new IllegalStateException("CaptioningManager missing");CaptioningManager.CaptionStyle style=manager.getUserStyle();appliedScale=manager.getFontScale();
            foreground=style.hasForegroundColor()?style.foregroundColor:Color.WHITE;background=style.hasBackgroundColor()?style.backgroundColor:Color.BLACK;window=style.hasWindowColor()?style.windowColor:Color.TRANSPARENT;
            sample.setVisibility(manager.isEnabled()?View.VISIBLE:View.GONE);sample.setTextSize(android.util.TypedValue.COMPLEX_UNIT_SP,20f*appliedScale);sample.setTextColor(foreground);sample.setBackgroundColor(background);layout.setBackgroundColor(window==Color.TRANSPARENT?Color.rgb(225,225,225):window);
            Typeface face=style.getTypeface();sample.setTypeface(face==null?Typeface.DEFAULT:face);int edge=style.hasEdgeType()?style.edgeType:0;int edgeColor=style.hasEdgeColor()?style.edgeColor:Color.BLACK;
            if(edge==2)sample.setShadowLayer(3,2,2,edgeColor);else sample.setShadowLayer(0,0,0,Color.TRANSPARENT);
            error="";prefs.edit().putString("applied",viewState().toString()).putInt("created",created).putInt("callbacks",callbacks).putInt("enabled_callbacks",enabledCallbacks).putInt("style_callbacks",styleCallbacks).putInt("scale_callbacks",scaleCallbacks).putInt("locale_callbacks",localeCallbacks).putString("error",error).apply();layout.invalidate();
        }catch(RuntimeException error){this.error=error.getClass().getSimpleName();prefs.edit().putString("error",this.error).apply();}
    }
    private static Object actualBackground(View view){return view.getBackground() instanceof ColorDrawable?((ColorDrawable)view.getBackground()).getColor():JSONObject.NULL;}
    private JSONObject viewState(){try{return new JSONObject().put("visible",sample.getVisibility()==View.VISIBLE).put("text_size_px",sample.getTextSize()).put("font_scale",appliedScale).put("scaled_density",getResources().getDisplayMetrics().scaledDensity).put("foreground",sample.getCurrentTextColor()).put("background",actualBackground(sample)).put("window",window).put("container_background",actualBackground(layout)).put("typeface_style",sample.getTypeface().getStyle()).put("text",sample.getText().toString()).put("width",sample.getWidth()).put("height",sample.getHeight());}catch(Exception impossible){throw new IllegalStateException(impossible);}}
    private final class RenderedLayout extends LinearLayout {
        RenderedLayout(Context context){super(context);}
        @Override protected void dispatchDraw(Canvas canvas){super.dispatchDraw(canvas);if(sample==null||prefs==null)return;String state=viewState().toString();if(!state.equals(lastDraw)){lastDraw=state;draws++;prefs.edit().putString("drawn",state).putInt("draws",draws).apply();}}
    }
}
