package dev.makepad.octosense.quickstep;

import android.app.Activity;
import android.content.res.Configuration;
import android.os.Bundle;
import android.view.ContextThemeWrapper;

public final class ShadeDesignActivity extends Activity {
    GlobalShadePanel panel;
    GlobalShadeController host;
    @Override public void onCreate(Bundle saved) {
        super.onCreate(saved);
        getWindow().setDecorFitsSystemWindows(false);
        getWindow().addFlags(android.view.WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
        Configuration config = new Configuration(getResources().getConfiguration());
        config.fontScale = getIntent().getFloatExtra("font_scale", 1f);
        config.uiMode = (config.uiMode & ~Configuration.UI_MODE_NIGHT_MASK)
                | (getIntent().getBooleanExtra("light", false) ? Configuration.UI_MODE_NIGHT_NO : Configuration.UI_MODE_NIGHT_YES);
        ContextThemeWrapper themed = new ContextThemeWrapper(createConfigurationContext(config), android.R.style.Theme_Material_NoActionBar);
        String scenario = getIntent().getStringExtra("scenario");
        host = new GlobalShadeController(scenario == null ? "controls" : scenario);
        panel = new GlobalShadePanel(themed, host, scenario == null || scenario.equals("controls") || scenario.equals("permissions") || scenario.equals("automatic"));
        if (getIntent().getBooleanExtra("rtl",false)) panel.setLayoutDirection(android.view.View.LAYOUT_DIRECTION_RTL);
        host.panel = panel; setContentView(panel); panel.update(host.state);
    }
}
