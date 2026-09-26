package dev.makepad.octosense.systemlanguagesfixture;

import android.app.Activity;
import android.app.LocaleManager;
import android.os.Bundle;
import android.widget.TextView;

/** Reads actual rendered properties after layout; no locale or permission setter. */
public final class LanguageConsumerActivity extends Activity {
    private TextView sample;
    @Override public void onCreate(Bundle state){super.onCreate(state);int count=getSharedPreferences("consumer",MODE_PRIVATE).getInt("created",0)+1;
        getSharedPreferences("consumer",MODE_PRIVATE).edit().putInt("created",count).commit();
        sample=new TextView(this);sample.setTextSize(24);sample.setPadding(24,48,24,24);sample.setText(getString(R.string.language_sample));setContentView(sample);
    }
    private void observe(){if(sample==null)return;sample.post(()->getSharedPreferences("consumer",MODE_PRIVATE).edit()
        .putString("text",sample.getText().toString()).putString("configuration",getResources().getConfiguration().getLocales().toLanguageTags())
        .putString("system",getSystemService(LocaleManager.class).getSystemLocales().toLanguageTags())
        .putString("app",getSystemService(LocaleManager.class).getApplicationLocales().toLanguageTags())
        .putInt("direction",sample.getLayoutDirection()).putBoolean("visible",sample.isShown())
        .putBoolean("focused",hasWindowFocus()).commit());}
    @Override public void onResume(){super.onResume();observe();}
    @Override public void onWindowFocusChanged(boolean focused){super.onWindowFocusChanged(focused);observe();}
}
