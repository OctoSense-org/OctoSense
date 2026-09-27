package dev.makepad.octosense.languagefixture;
import android.app.LocaleConfig;import android.app.LocaleManager;import android.content.BroadcastReceiver;import android.content.Context;import android.content.Intent;import android.os.LocaleList;import org.json.JSONObject;
/** Shell-gated finite fixture operations; no input accepts a package, locale tag or path. */
public final class LanguageFixture extends BroadcastReceiver {
 @Override public void onReceive(Context c,Intent intent){try{
  if(!"dev.makepad.octosense.languagefixture.COMMAND".equals(intent.getAction())||!"dev.makepad.octosense.languagefixture".equals(c.getPackageName()))throw new SecurityException("Wrong fixture target");
  LocaleManager manager=c.getSystemService(LocaleManager.class);if(manager==null)throw new IllegalStateException("LocaleManager missing");
  String operation=intent.getStringExtra("operation");
  if("override_subset".equals(operation))manager.setOverrideLocaleConfig(new LocaleConfig(LocaleList.forLanguageTags("en-US,fr-FR")));
  else if("override_clear".equals(operation))manager.setOverrideLocaleConfig(null);
  else if(!"state".equals(operation))throw new IllegalArgumentException("Unknown fixture command");
  LocaleConfig config=new LocaleConfig(c);android.content.SharedPreferences prefs=c.getSharedPreferences("probe",Context.MODE_PRIVATE);
  JSONObject state=new JSONObject().put("application_tags",manager.getApplicationLocales().toLanguageTags()).put("resource_tags",c.getResources().getConfiguration().getLocales().toLanguageTags()).put("greeting",c.getString(R.string.greeting)).put("region",c.getString(R.string.region)).put("config_status",config.getStatus()).put("config_tags",config.getSupportedLocales()==null?JSONObject.NULL:config.getSupportedLocales().toLanguageTags()).put("override",manager.getOverrideLocaleConfig()!=null).put("created",prefs.getInt("created",0)).put("activity_tags",prefs.getString("activity_tags","")).put("activity_greeting",prefs.getString("activity_greeting",""));
  setResultCode(1);setResultData(android.util.Base64.encodeToString(state.toString().getBytes(java.nio.charset.StandardCharsets.UTF_8),android.util.Base64.NO_WRAP));
 }catch(Exception error){setResultCode(0);setResultData(error.getClass().getSimpleName());}}
}
