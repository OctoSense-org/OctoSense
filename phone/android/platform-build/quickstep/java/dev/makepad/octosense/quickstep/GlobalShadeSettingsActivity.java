package dev.makepad.octosense.quickstep;

import android.app.Activity;
import android.content.res.ColorStateList;
import android.os.Bundle;
import android.view.Gravity;
import android.view.WindowInsets;
import android.view.WindowInsetsController;
import android.widget.Button;
import android.widget.ImageView;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.Switch;
import android.widget.TextView;

/** Visible opt-in and an immediate, persistent return to Android's panel. */
public final class GlobalShadeSettingsActivity extends Activity {
    private TextView status;
    private GlobalShadeStyle ui;
    private final Runnable update = () -> status.setText(GlobalShadeController.status());
    private int dp(int n) { return ui.dp(n); }
    @Override public void onCreate(Bundle saved) {
        super.onCreate(saved); ui = new GlobalShadeStyle(this);
        getWindow().setDecorFitsSystemWindows(false);
        LinearLayout root=column(); root.setBackgroundColor(ui.background);
        root.setOnApplyWindowInsetsListener((v,insets) -> {
            android.graphics.Insets safe=insets.getInsets(WindowInsets.Type.systemBars()|WindowInsets.Type.displayCutout());
            root.setPadding(dp(24)+safe.left,dp(20)+safe.top,dp(24)+safe.right,dp(12)+safe.bottom); return insets;
        });
        ScrollView scroll=new ScrollView(this); scroll.setFillViewport(true); scroll.setVerticalScrollBarEnabled(false);
        LinearLayout content=column(); scroll.addView(content); root.addView(scroll,new LinearLayout.LayoutParams(-1,0,1));
        TextView brand=ui.label("OCTOSENSE",11,ui.muted); brand.setLetterSpacing(.14f); content.addView(brand);
        TextView heading=ui.label("Your panel,\neverywhere.",34,ui.text); ui.medium(heading); heading.setPadding(0,dp(18),0,dp(12)); content.addView(heading);
        TextView help=ui.label("Notifications and device controls, one swipe away in any app.",16,ui.muted); help.setLineSpacing(dp(3),1); help.setPadding(0,0,0,dp(28)); content.addView(help);
        LinearLayout gestures=column(); gestures.setPadding(dp(20),dp(8),dp(20),dp(8)); gestures.setBackground(ui.shape(ui.surface,24));
        gesture(gestures,"bell","Top left","Your notifications"); gesture(gestures,"grid","Top right","Your device controls"); content.addView(gestures);
        LinearLayout setting=column(); setting.setPadding(dp(20),dp(20),dp(20),dp(20)); setting.setBackground(ui.shape(ui.surface,24));
        LinearLayout.LayoutParams gap=new LinearLayout.LayoutParams(-1,-2); gap.topMargin=dp(16); content.addView(setting,gap);
        Switch enabled=new Switch(this); enabled.setText("Use OctoSense panel across apps"); enabled.setTextSize(16); enabled.setTextColor(ui.text); enabled.setSwitchPadding(dp(16)); enabled.setMinHeight(dp(64));
        enabled.setThumbTintList(new ColorStateList(new int[][]{new int[]{android.R.attr.state_checked},new int[]{}},new int[]{ui.accent,ui.muted}));
        enabled.setTrackTintList(ColorStateList.valueOf(ui.elevated));
        enabled.setChecked(GlobalShadeController.preferences(this).getBoolean("enabled",false));
        enabled.setOnCheckedChangeListener((button,checked) -> GlobalShadeController.preferences(this).edit().putBoolean("enabled",checked).apply()); setting.addView(enabled);
        status=ui.label("",13,ui.muted); status.setPadding(0,dp(12),0,0); status.setAccessibilityLiveRegion(android.view.View.ACCESSIBILITY_LIVE_REGION_POLITE); setting.addView(status);
        TextView note=ui.label("Close or swipe up on the panel heading to return to your app. Turn this off anytime to use Android’s panel.",14,ui.muted); note.setLineSpacing(dp(3),1); note.setPadding(dp(4),dp(24),dp(4),dp(24)); content.addView(note);
        Button done=ui.button("Done",this::finish,true); root.addView(done,new LinearLayout.LayoutParams(-1,-2)); setContentView(root);
        root.post(() -> {
            int light=WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS | WindowInsetsController.APPEARANCE_LIGHT_NAVIGATION_BARS;
            WindowInsetsController bars=root.getWindowInsetsController();
            if(bars!=null) bars.setSystemBarsAppearance(ui.dark?0:light,light);
        });
    }
    private LinearLayout column() { LinearLayout view=new LinearLayout(this); view.setOrientation(LinearLayout.VERTICAL); return view; }
    private void gesture(LinearLayout parent,String symbol,String side,String purpose) {
        LinearLayout row=new LinearLayout(this); row.setGravity(Gravity.CENTER_VERTICAL); row.setPadding(0,dp(14),0,dp(14));
        ImageView icon=new ImageView(this); icon.setImageDrawable(ui.icon(symbol,ui.accent)); icon.setImportantForAccessibility(android.view.View.IMPORTANT_FOR_ACCESSIBILITY_NO); row.addView(icon,new LinearLayout.LayoutParams(dp(26),dp(26)));
        LinearLayout words=column(); words.setPadding(dp(16),0,0,0); TextView title=ui.label(side,16,ui.text); ui.medium(title); words.addView(title);
        TextView detail=ui.label(purpose,13,ui.muted); detail.setPadding(0,dp(5),0,0); words.addView(detail); row.addView(words,new LinearLayout.LayoutParams(0,-2,1)); parent.addView(row);
    }
    @Override protected void onResume() { super.onResume(); GlobalShadeController.observers.add(update); update.run(); }
    @Override protected void onPause() { GlobalShadeController.observers.remove(update); super.onPause(); }
}
