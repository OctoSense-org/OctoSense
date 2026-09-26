package dev.makepad.octosense.languagefixture;
import android.app.Activity;import android.app.LocaleManager;import android.os.Bundle;import android.widget.TextView;
/** A real translated Activity: application-locale changes must recreate and render it. */
public final class LanguageActivity extends Activity {
 @Override public void onCreate(Bundle state){super.onCreate(state);
  android.content.SharedPreferences prefs=getSharedPreferences("probe",MODE_PRIVATE);int count=prefs.getInt("created",0)+1;
  if(!prefs.edit().putInt("created",count).putString("activity_tags",getResources().getConfiguration().getLocales().toLanguageTags()).putString("activity_greeting",getString(R.string.greeting)).commit())throw new IllegalStateException("Cannot save fixture observation");
  TextView text=new TextView(this);text.setTextSize(24);text.setPadding(24,48,24,24);text.setText(getString(R.string.greeting)+"\n"+getString(R.string.region)+"\n"+getSystemService(LocaleManager.class).getApplicationLocales().toLanguageTags()+"\nActivity creation: "+count);setContentView(text);
 }
}
