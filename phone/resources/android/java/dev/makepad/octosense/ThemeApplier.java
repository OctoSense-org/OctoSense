package dev.makepad.octosense;

import android.app.UiModeManager;
import android.app.WallpaperManager;
import android.content.Context;
import android.graphics.Bitmap;
import android.graphics.Canvas;
import android.graphics.LinearGradient;
import android.graphics.Paint;
import android.graphics.Shader;
import dev.makepad.octosense.agent.AgentPlatformClient;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import org.json.JSONObject;

/** Shared worker-side theme transaction for the native and hosted Settings UIs. */
final class ThemeApplier {
    private ThemeApplier() {}

    static final class Result {
        final boolean saved, palette, wallpaper, appearance;
        Result(boolean saved, boolean palette, boolean wallpaper, boolean appearance) {
            this.saved=saved; this.palette=palette; this.wallpaper=wallpaper; this.appearance=appearance;
        }
        boolean complete() {return saved&&palette&&wallpaper&&appearance;}
        String reason() {return !saved?"theme_save_failed":complete()?"theme_applied":"theme_applied_partial";}
    }

    /** The command accepts only the versioned choice, never arbitrary settings or colors. */
    static ThemeCatalog.Choice parseChoice(Context context, JSONObject json) throws Exception {
        if(json.length()!=4 || !(json.get("version") instanceof Integer)
                || json.getInt("version")!=1 || !(json.get("preset") instanceof String)
                || !(json.get("appearance") instanceof String) || !(json.get("wallpaper") instanceof String))
            throw new IllegalArgumentException("Invalid theme choice");
        return ThemeCatalog.get(context).parse(json.toString());
    }

    // Both UIs have workers. Serialize the complete transaction so a slow wallpaper
    // write cannot finish after the other UI has selected a newer palette.
    static synchronized Result apply(Context context, ThemeCatalog.Choice next,
            AgentPlatformClient agent, CountDownLatch binding) {
        ThemeCatalog catalog=ThemeCatalog.get(context);
        if(!catalog.save(context,next)) return new Result(false,false,false,false);
        boolean dark=next.dark(ThemeCatalog.systemDark(context));
        boolean palette=false,wallpaper=false,appearance=next.appearance.equals("system");
        ThemeCatalog.Preset preset=catalog.preset(next.preset);
        try {
            WallpaperManager manager=WallpaperManager.getInstance(context);
            if(manager.isWallpaperSupported()&&manager.isSetWallpaperAllowed()) {
                Bitmap bitmap=Bitmap.createBitmap(720,1520,Bitmap.Config.ARGB_8888);
                try {
                    Canvas canvas=new Canvas(bitmap);Paint paint=new Paint();
                    paint.setShader(gradient(preset,next,dark,1520));canvas.drawRect(0,0,720,1520,paint);
                    manager.setBitmap(bitmap,null,true,WallpaperManager.FLAG_SYSTEM);
                    wallpaper=true;
                } finally {bitmap.recycle();}
            }
        } catch(Exception e) {android.util.Log.w("OctoSenseTheme","System wallpaper unavailable",e);}
        // Finish the wallpaper update before selecting the fixed Monet seed.
        if(agent!=null&&agent.available()) {
            try {binding.await(3,TimeUnit.SECONDS);}
            catch(InterruptedException e) {Thread.currentThread().interrupt();}
            palette=agent.applyThemePalette(preset.seed);
            try {
                if(!next.appearance.equals("system")) {
                    UiModeManager manager=context.getSystemService(UiModeManager.class);
                    int mode=dark?UiModeManager.MODE_NIGHT_YES:UiModeManager.MODE_NIGHT_NO;
                    manager.setNightMode(mode);appearance=manager.getNightMode()==mode;
                }
            } catch(RuntimeException e) {android.util.Log.w("OctoSenseTheme","System appearance unavailable",e);}
        }
        return new Result(true,palette,wallpaper,appearance);
    }

    static Shader gradient(ThemeCatalog.Preset preset,ThemeCatalog.Choice choice,boolean dark,float height) {
        int top=preset.color(choice.wallpaper.equals("solid")?"background":"wallpaper_top",dark);
        int bottom=preset.color(choice.wallpaper.equals("solid")?"background":"wallpaper_bottom",dark);
        return new LinearGradient(0,0,0,height,top,bottom,Shader.TileMode.CLAMP);
    }
}
