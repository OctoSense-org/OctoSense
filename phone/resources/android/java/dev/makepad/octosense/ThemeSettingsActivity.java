package dev.makepad.octosense;

import android.app.Activity;
import android.app.UiModeManager;
import android.app.WallpaperManager;
import android.content.res.ColorStateList;
import android.content.res.Configuration;
import android.graphics.Bitmap;
import android.graphics.Canvas;
import android.graphics.LinearGradient;
import android.graphics.Paint;
import android.graphics.RectF;
import android.graphics.Shader;
import android.graphics.Typeface;
import android.graphics.drawable.GradientDrawable;
import android.os.Build;
import android.os.Bundle;
import android.view.Gravity;
import android.view.View;
import android.view.ViewGroup;
import android.view.WindowInsets;
import android.widget.Button;
import android.widget.LinearLayout;
import android.widget.RadioButton;
import android.widget.RadioGroup;
import android.widget.ScrollView;
import android.widget.TextView;
import android.widget.Toast;
import dev.makepad.octosense.agent.AgentPlatformClient;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;

/** Preview edits remain a draft until Apply. No renderer or app instance is replaced. */
public final class ThemeSettingsActivity extends Activity {
    private ThemeCatalog catalog;
    private ThemeCatalog.Choice saved,draft;
    private Preview preview;
    private RadioGroup modes,wallpapers;
    private Button apply,reset,cancel;
    private TextView status;
    private LinearLayout root;
    private final List<LinearLayout> cards=new ArrayList<>();
    private boolean updating,applying,systemDark;
    private AgentPlatformClient agent;
    private final CountDownLatch binding=new CountDownLatch(1);
    private final ExecutorService worker=Executors.newSingleThreadExecutor(r -> new Thread(r,"OctoSenseTheme"));

    @Override public void onCreate(Bundle state) {
        super.onCreate(state);
        catalog=ThemeCatalog.get(this); saved=catalog.read(this); draft=saved;
        if(state!=null) try {draft=catalog.parse(state.getString("draft",""));} catch(Exception ignored) {}
        systemDark=ThemeCatalog.systemDark(this);
        agent=new AgentPlatformClient(this, ignored -> {binding.countDown();updateDraft();});
        build(); updateDraft();
    }
    @Override protected void onStart() {super.onStart();agent.bind();}
    @Override protected void onDestroy() {agent.unbind();worker.shutdown();super.onDestroy();}
    @Override protected void onSaveInstanceState(Bundle state) {super.onSaveInstanceState(state);state.putString("draft",draft.json().toString());}
    @Override public void onConfigurationChanged(Configuration configuration) {
        super.onConfigurationChanged(configuration);
        systemDark=ThemeCatalog.systemDark(this);
        if(!applying) build();
        updateDraft();
    }
    @Override public void onBackPressed() {if(!applying) super.onBackPressed();}
    private String copy(String en,String zh) {return getResources().getConfiguration().getLocales().get(0).getLanguage().equals("zh")?zh:en;}
    private int dp(float n) {return Math.round(n*getResources().getDisplayMetrics().density);}
    private LinearLayout column() {LinearLayout v=new LinearLayout(this);v.setOrientation(LinearLayout.VERTICAL);return v;}
    private TextView text(String value,float size) {
        TextView v=new TextView(this);v.setText(value);v.setTextSize(size);
        v.setTextColor(catalog.preset(saved.preset).color("text",saved.dark(systemDark)));return v;
    }
    private void heading(LinearLayout body,String label) {
        TextView v=text(label,16);v.setTypeface(null,Typeface.BOLD);v.setPadding(0,dp(20),0,dp(10));body.addView(v);
    }
    private Button button(String label,Runnable action) {
        Button b=new Button(this);b.setText(label);b.setAllCaps(false);b.setOnClickListener(v -> action.run());
        ThemeCatalog.Preset p=catalog.preset(saved.preset);boolean dark=saved.dark(systemDark);
        GradientDrawable shape=new GradientDrawable();shape.setColor(p.color("surface_variant",dark));shape.setCornerRadius(dp(12));
        b.setBackground(shape);b.setTextColor(p.color("text",dark));b.setStateListAnimator(null);return b;
    }
    private void build() {
        cards.clear();
        int ink=catalog.preset(saved.preset).color("text",saved.dark(systemDark));
        root=column();root.setBackgroundColor(catalog.preset(saved.preset).color("background",saved.dark(systemDark)));
        if(Build.VERSION.SDK_INT>=30) {
            getWindow().setDecorFitsSystemWindows(false);
            root.setOnApplyWindowInsetsListener((v,insets) -> {
                android.graphics.Insets safe=insets.getInsets(WindowInsets.Type.systemBars()|WindowInsets.Type.displayCutout());
                root.setPadding(safe.left+dp(20),safe.top,safe.right+dp(20),safe.bottom);return insets;
            });
        } else {root.setFitsSystemWindows(true);root.setPadding(dp(20),0,dp(20),0);}
        LinearLayout toolbar=new LinearLayout(this);toolbar.setGravity(Gravity.CENTER_VERTICAL);
        Button back=button("‹",() -> {if(!applying) finish();});back.setTextSize(28);back.setContentDescription(copy("Back","返回"));
        back.setBackgroundColor(android.graphics.Color.TRANSPARENT);
        toolbar.addView(back,new LinearLayout.LayoutParams(dp(48),dp(54)));
        TextView title=text(copy("Wallpaper & style","壁纸与风格"),22);title.setTypeface(null,Typeface.BOLD);
        toolbar.addView(title,new LinearLayout.LayoutParams(0,dp(54),1));title.setGravity(Gravity.CENTER_VERTICAL);root.addView(toolbar);
        ScrollView scroll=new ScrollView(this);scroll.setFillViewport(true);
        LinearLayout body=column();scroll.addView(body);root.addView(scroll,new LinearLayout.LayoutParams(-1,0,1));
        TextView intro=text(copy("Make OctoSense yours. Preview before applying.","选择你的 OctoSense 风格，预览后再应用。"),14);intro.setPadding(0,dp(8),0,dp(12));body.addView(intro);
        preview=new Preview();body.addView(preview,new LinearLayout.LayoutParams(-1,dp(236)));
        heading(body,copy("Themes","主题"));
        for(int row=0;row<2;row++) {
            LinearLayout line=new LinearLayout(this);
            for(int col=0;col<2;col++) {
                final ThemeCatalog.Preset preset=catalog.presets.get(row*2+col);
                LinearLayout card=column();card.setPadding(dp(12),dp(12),dp(12),dp(12));card.setMinimumHeight(dp(106));
                TextView name=text(preset.name,17);name.setTypeface(null,Typeface.BOLD);card.addView(name);
                TextView desc=text(copy(preset.description,preset.descriptionZh),12);desc.setPadding(0,dp(4),0,0);card.addView(desc);
                LinearLayout.LayoutParams params=new LinearLayout.LayoutParams(0,-1,1);
                params.setMargins(col==0?0:dp(5),0,col==0?dp(5):0,dp(10));line.addView(card,params);
                card.setClickable(true);card.setFocusable(true);
                card.setOnClickListener(v -> {if(!applying) {draft=draft.preset(preset.id);updateDraft();}});cards.add(card);
            }
            body.addView(line,new LinearLayout.LayoutParams(-1,-2));
        }
        heading(body,copy("Appearance","显示模式"));
        modes=choices(new String[]{copy("System","跟随系统"),copy("Light","浅色"),copy("Dark","深色")},index -> {
            draft=draft.appearance(new String[]{"system","light","dark"}[index]);updateDraft();
        },ink);body.addView(modes);
        heading(body,copy("Background","背景"));
        wallpapers=choices(new String[]{copy("Theme gradient","主题渐变"),copy("Solid color","纯色")},index -> {
            draft=draft.wallpaper(index==0?"gradient":"solid");updateDraft();
        },ink);body.addView(wallpapers);
        reset=button(copy("Restore default","恢复默认"),() -> {draft=ThemeCatalog.Choice.defaults();updateDraft();});body.addView(reset);
        status=text("",13);status.setAccessibilityLiveRegion(View.ACCESSIBILITY_LIVE_REGION_POLITE);status.setPadding(0,dp(8),0,dp(8));body.addView(status);
        LinearLayout footer=new LinearLayout(this);footer.setGravity(Gravity.CENTER_VERTICAL);footer.setPadding(0,dp(8),0,dp(8));
        cancel=button(copy("Cancel","取消"),this::finish);apply=button(copy("Apply","应用"),this::applyTheme);
        LinearLayout.LayoutParams cancelSize=new LinearLayout.LayoutParams(0,dp(52),1);cancelSize.setMarginEnd(dp(10));
        footer.addView(cancel,cancelSize);footer.addView(apply,new LinearLayout.LayoutParams(0,dp(52),1));root.addView(footer);
        setContentView(root);
        if(Build.VERSION.SDK_INT>=30&&getWindow().getInsetsController()!=null) {
            int mask=android.view.WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS|android.view.WindowInsetsController.APPEARANCE_LIGHT_NAVIGATION_BARS;
            getWindow().getInsetsController().setSystemBarsAppearance(saved.dark(systemDark)?0:mask,mask);
        }
    }
    private interface Selected {void accept(int index);}
    private RadioGroup choices(String[] labels,Selected callback,int ink) {
        RadioGroup group=new RadioGroup(this);group.setOrientation(RadioGroup.HORIZONTAL);
        for(String label:labels) {
            RadioButton radio=new RadioButton(this);radio.setId(View.generateViewId());radio.setText(label);radio.setTextColor(ink);radio.setTextSize(14);
            ThemeCatalog.Preset p=catalog.preset(saved.preset);boolean dark=saved.dark(systemDark);
            radio.setButtonTintList(new ColorStateList(new int[][]{new int[]{android.R.attr.state_checked},new int[]{}},
                    new int[]{p.color("accent",dark),p.color("muted",dark)}));
            radio.setMinimumHeight(dp(48));group.addView(radio,new RadioGroup.LayoutParams(0,-2,1));
        }
        group.setOnCheckedChangeListener((g,id) -> {if(!updating&&!applying) callback.accept(g.indexOfChild(g.findViewById(id)));});return group;
    }
    private void updateDraft() {
        if(root==null) return;
        updating=true;
        boolean dark=draft.dark(systemDark);
        for(int i=0;i<cards.size();i++) {
            ThemeCatalog.Preset preset=catalog.presets.get(i);LinearLayout card=cards.get(i);boolean selected=draft.preset.equals(preset.id);
            GradientDrawable background=new GradientDrawable();background.setColor(preset.color("surface",dark));background.setCornerRadius(dp(14));
            background.setStroke(dp(selected?3:1),preset.color(selected?"accent":"border",dark));card.setBackground(background);card.setSelected(selected);
            ((TextView)card.getChildAt(0)).setText(preset.name+(selected?"  ✓":""));
            ((TextView)card.getChildAt(0)).setTextColor(preset.color("text",dark));((TextView)card.getChildAt(1)).setTextColor(preset.color("muted",dark));
            card.setContentDescription(preset.name+(selected?copy(", selected","，已选择"):copy(", theme","，主题")));card.setEnabled(!applying);
        }
        modes.check(modes.getChildAt(draft.appearance.equals("system")?0:draft.appearance.equals("light")?1:2).getId());
        wallpapers.check(wallpapers.getChildAt(draft.wallpaper.equals("gradient")?0:1).getId());
        for(RadioGroup group:new RadioGroup[]{modes,wallpapers}) for(int i=0;i<group.getChildCount();i++) group.getChildAt(i).setEnabled(!applying);
        reset.setEnabled(!applying);cancel.setEnabled(!applying);apply.setEnabled(!applying&&!draft.equals(saved));
        apply.setAlpha(apply.isEnabled()?1f:0.45f);
        apply.setText(applying?copy("Applying…","正在应用…"):copy("Apply","应用"));
        ThemeCatalog.Preset current=catalog.preset(saved.preset);
        apply.setBackgroundTintList(ColorStateList.valueOf(current.color("accent",saved.dark(systemDark))));apply.setTextColor(current.color("on_accent",saved.dark(systemDark)));
        status.setText(applying?copy("Applying your theme…","正在应用主题…"):draft.equals(saved)?copy("Current theme","当前主题"):copy("Preview — tap Apply to save","预览中，点击“应用”保存"));
        preview.setContentDescription(copy("Preview of Home and Mail in ","桌面与邮件预览：")+catalog.preset(draft.preset).name);preview.invalidate();updating=false;
    }
    private void applyTheme() {
        if(applying||draft.equals(saved)) return;
        final ThemeCatalog.Choice next=draft;
        final boolean dark=next.dark(systemDark),rom=agent.available();
        applying=true;updateDraft();
        worker.execute(() -> {
            if(!catalog.save(this,next)) {
                runOnUiThread(() -> {if(isDestroyed()) return;applying=false;updateDraft();status.setText(copy("Theme could not be saved. Try again.","无法保存主题，请重试。"));});return;
            }
            boolean systemChanged=false,backgroundChanged=false,modeChanged=next.appearance.equals("system");
            ThemeCatalog.Preset preset=catalog.preset(next.preset);
            try {
                WallpaperManager manager=WallpaperManager.getInstance(this);
                if(manager.isWallpaperSupported()&&manager.isSetWallpaperAllowed()) {
                    Bitmap bitmap=Bitmap.createBitmap(720,1520,Bitmap.Config.ARGB_8888);
                    Canvas canvas=new Canvas(bitmap);Paint paint=new Paint();
                    paint.setShader(gradient(preset,next,dark,1520));canvas.drawRect(0,0,720,1520,paint);
                    try {manager.setBitmap(bitmap,null,true,WallpaperManager.FLAG_SYSTEM);backgroundChanged=true;} finally {bitmap.recycle();}
                }
            } catch(Exception e) {android.util.Log.w("OctoSenseTheme","System wallpaper unavailable",e);}
            // Finish the wallpaper update before selecting a fixed Monet seed.
            if(rom) {
                try {binding.await(3,TimeUnit.SECONDS);} catch(InterruptedException e) {Thread.currentThread().interrupt();}
                systemChanged=agent.applyThemePalette(preset.seed);
                try {
                    if(!next.appearance.equals("system")) {
                        UiModeManager manager=getSystemService(UiModeManager.class);
                        int mode=dark?UiModeManager.MODE_NIGHT_YES:UiModeManager.MODE_NIGHT_NO;
                        manager.setNightMode(mode);modeChanged=manager.getNightMode()==mode;
                    }
                } catch(RuntimeException e) {android.util.Log.w("OctoSenseTheme","System appearance unavailable",e);}
            }
            final boolean colors=systemChanged,wallpaper=backgroundChanged,mode=modeChanged;
            runOnUiThread(() -> {
                if(isDestroyed()) return;
                saved=next;applying=false;
                String message=colors&&wallpaper&&mode?copy("Theme applied","主题已应用"):
                    copy("OctoSense theme applied. Some Android system styling is unavailable on this device.","OctoSense 主题已应用，此设备不支持部分 Android 系统样式设置。");
                Toast.makeText(this,message,Toast.LENGTH_LONG).show();finish();
            });
        });
    }
    private static Shader gradient(ThemeCatalog.Preset p,ThemeCatalog.Choice choice,boolean dark,float height) {
        int top=p.color(choice.wallpaper.equals("solid")?"background":"wallpaper_top",dark);
        int bottom=p.color(choice.wallpaper.equals("solid")?"background":"wallpaper_bottom",dark);
        return new LinearGradient(0,0,0,height,top,bottom,Shader.TileMode.CLAMP);
    }
    /** Small synthetic previews use catalog colors and contain no user data. */
    private final class Preview extends View {
        private final Paint paint=new Paint(Paint.ANTI_ALIAS_FLAG);
        Preview() {super(ThemeSettingsActivity.this);}
        private void round(Canvas c,float x,float y,float w,float h,float radius,int color) {
            paint.setShader(null);paint.setColor(color);c.drawRoundRect(new RectF(x,y,x+w,y+h),radius,radius,paint);
        }
        private void label(Canvas c,String value,float x,float y,float size,int color,boolean bold) {
            paint.setShader(null);paint.setColor(color);paint.setTextSize(size);paint.setTypeface(bold?Typeface.DEFAULT_BOLD:Typeface.DEFAULT);c.drawText(value,x,y,paint);
        }
        @Override protected void onDraw(Canvas canvas) {
            super.onDraw(canvas);float scale=getWidth()/360f;canvas.save();canvas.scale(scale,scale);
            ThemeCatalog.Preset p=catalog.preset(draft.preset);boolean dark=draft.dark(systemDark);
            float width=171,height=Math.min(224,getHeight()/scale-8),radius=14;
            for(int screen=0;screen<2;screen++) {
                canvas.save();canvas.translate(screen*189,0);
                round(canvas,0,0,width,height,radius,p.color("surface",dark));
                if(screen==0) {
                    android.graphics.Path clip=new android.graphics.Path();clip.addRoundRect(new RectF(0,0,width,height),radius,radius,android.graphics.Path.Direction.CW);
                    canvas.save();canvas.clipPath(clip);paint.setShader(gradient(p,draft,dark,height));canvas.drawRect(0,0,width,height,paint);canvas.restore();
                    label(canvas,"9:41",48,48,29,p.color("text",dark),false);
                    for(int i=0;i<2;i++) {
                        round(canvas,12,67+i*49,147,41,p.radius/2,p.color("surface",dark));
                        round(canvas,21,77+i*49,22,22,6,p.color("accent",dark));
                        label(canvas,i==0?copy("Mail","邮件"):copy("Calendar","日历"),51,93+i*49,11,p.color("text",dark),true);
                    }
                    for(int i=0;i<4;i++) round(canvas,17+i*36,176,26,26,9,p.color(i%2==0?"accent":"surface_variant",dark));
                } else {
                    label(canvas,copy("Inbox","收件箱"),14,32,20,p.color("text",dark),true);
                    label(canvas,copy("3 unread","3 封未读"),14,51,10,p.color("accent",dark),false);
                    for(int i=0;i<3;i++) {
                        round(canvas,12,64+i*46,147,40,7,p.color("surface_variant",dark));
                        label(canvas,i==0?copy("Team update","团队动态"):i==1?copy("Your weekly digest","本周摘要"):copy("Meeting tomorrow","明日会议"),20,80+i*46,10,p.color("text",dark),true);
                        label(canvas,copy("A preview of your theme","主题效果预览"),20,94+i*46,8,p.color("muted",dark),false);
                    }
                }
                round(canvas,65,height-7,41,2,1,p.color("text",dark));canvas.restore();
            }
            canvas.restore();
        }
    }
}
