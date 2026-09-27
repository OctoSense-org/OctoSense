package dev.makepad.octosense.systemlanguagesfixture;

import android.app.Instrumentation;
import android.app.LocaleManager;
import android.app.UiAutomation;
import android.os.Bundle;
import android.os.LocaleList;
import android.os.ParcelFileDescriptor;
import android.os.SystemClock;
import android.view.accessibility.AccessibilityNodeInfo;
import java.io.ByteArrayOutputStream;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.Locale;
import java.util.Objects;
import org.json.JSONArray;
import org.json.JSONObject;

/** Public SDK UI/LocaleManager consumer. Never calls the product Binder or a locale setter. */
public final class SystemLanguagesProbe extends Instrumentation {
    private static final String HOME="dev.makepad.octosense",SELF="dev.makepad.octosense.systemlanguagesfixture";
    private UiAutomation ui;private String scenario;private int checks;private List<String> original;private String preferences;private boolean mutating;private int lastConsumerCreated;private String phase="startup";private int preservedHomeActivities;
    interface Match{boolean test(AccessibilityNodeInfo node);}
    @Override public void onCreate(Bundle args){super.onCreate(args);scenario=args==null?"unavailable":args.getString("scenario","unavailable");start();}
    private void check(boolean value,String message){if(!value)throw new AssertionError(message);checks++;}
    private static String string(CharSequence value){return value==null?"":value.toString();}
    private static String label(AccessibilityNodeInfo node){String description=string(node.getContentDescription());return description.isEmpty()?string(node.getText()):description;}
    private static boolean role(AccessibilityNodeInfo node,String role){return ("android.widget."+role).equals(string(node.getClassName()));}
    private String shell(String command){
        ParcelFileDescriptor[] pipes=ui.executeShellCommandRw("/system/bin/sh");
        try(ParcelFileDescriptor.AutoCloseOutputStream input=new ParcelFileDescriptor.AutoCloseOutputStream(pipes[1])){input.write((command+"\n").getBytes(java.nio.charset.StandardCharsets.UTF_8));}
        catch(Exception e){throw new AssertionError("Shell input failed",e);}
        try(ParcelFileDescriptor.AutoCloseInputStream output=new ParcelFileDescriptor.AutoCloseInputStream(pipes[0]);ByteArrayOutputStream out=new ByteArrayOutputStream()){byte[] buffer=new byte[4096];int count;while((count=output.read(buffer))!=-1)out.write(buffer,0,count);return out.toString("UTF-8");}catch(Exception e){throw new AssertionError("Shell observation failed",e);}
    }
    private static String quote(String value){return "'"+value.replace("'","'\\''")+"'";}
    private void walk(AccessibilityNodeInfo node,List<AccessibilityNodeInfo> nodes,int depth){if(node==null||depth>12||nodes.size()>600)return;nodes.add(node);for(int i=0;i<node.getChildCount();i++)walk(node.getChild(i),nodes,depth+1);}
    private List<AccessibilityNodeInfo> nodes(){List<AccessibilityNodeInfo> result=new ArrayList<>();AccessibilityNodeInfo root=ui.getRootInActiveWindow();if(root!=null&&HOME.equals(string(root.getPackageName())))walk(root,result,0);return result;}
    private AccessibilityNodeInfo waitNode(Match match,String description){long end=SystemClock.uptimeMillis()+15000;do{for(AccessibilityNodeInfo node:nodes())if(match.test(node))return node;SystemClock.sleep(80);}while(SystemClock.uptimeMillis()<end);throw new AssertionError("Missing "+description);}
    private AccessibilityNodeInfo scroll(){return waitNode(n->role(n,"ScrollView"),"Settings scroll container");}
    private boolean scroll(int action){AccessibilityNodeInfo node=scroll();return node.performAction(action);}
    private void top(){
        long end=SystemClock.uptimeMillis()+10000;
        do{
            AccessibilityNodeInfo node=scroll();
            if(!node.getActionList().contains(AccessibilityNodeInfo.AccessibilityAction.ACTION_SCROLL_BACKWARD))return;
            // A just-applied list edit can reset scrolling between observation and action.
            // Re-fetch after a retired action; persistent rejection still hits this deadline.
            if(node.performAction(AccessibilityNodeInfo.ACTION_SCROLL_BACKWARD))checks++;
            SystemClock.sleep(80);
        }while(SystemClock.uptimeMillis()<end);
        throw new AssertionError("Could not reach the beginning of the Settings page");
    }
    private AccessibilityNodeInfo reveal(Match match,String description){top();long end=SystemClock.uptimeMillis()+20000;do{for(AccessibilityNodeInfo node:nodes())if(match.test(node))return node;AccessibilityNodeInfo scroll=scroll();if(scroll.getActionList().contains(AccessibilityNodeInfo.AccessibilityAction.ACTION_SCROLL_FORWARD))scroll.performAction(AccessibilityNodeInfo.ACTION_SCROLL_FORWARD);else top();SystemClock.sleep(100);}while(SystemClock.uptimeMillis()<end);throw new AssertionError("Missing visible "+description);}
    private AccessibilityNodeInfo button(String text){return reveal(n->role(n,"Button")&&n.isEnabled()&&label(n).equals(text),text);}
    private void click(AccessibilityNodeInfo node){check(node.performAction(AccessibilityNodeInfo.ACTION_CLICK),"Click not queued: "+label(node));}
    private void pane(String expected){waitNode(n->role(n,"TextView")&&expected.equals(label(n)),"pane "+expected);checks++;}
    private void open(){shell("am start -W -a "+HOME+".action.SETTINGS -n "+HOME+"/.SettingsEntry --es "+HOME+".extra.SETTINGS_ROUTE system_languages");pane("System languages");}
    private void ready(){button("Add a language");checks++;}
    private List<String> system(){LocaleList locales=getContext().getSystemService(LocaleManager.class).getSystemLocales();List<String> tags=new ArrayList<>();for(int i=0;i<locales.size();i++)tags.add(locales.get(i).toLanguageTag());return tags;}
    private void observed(List<String> expected){long end=SystemClock.uptimeMillis()+15000;do{if(system().equals(expected)){checks++;return;}SystemClock.sleep(80);}while(SystemClock.uptimeMillis()<end);throw new AssertionError("Native system locale order mismatch");}
    private String rawPreferences(){for(String line:shell("settings list system").split("\n"))if(line.startsWith("locale_preferences="))return line.substring("locale_preferences=".length());return null;}
    private void setPreferences(String value){if(value==null)shell("settings delete system locale_preferences");else shell("settings put system locale_preferences "+quote(value));check(Objects.equals(value,rawPreferences()),"Regional preference fixture did not apply");}
    private void ordinal(String tag,int index){reveal(n->role(n,"TextView")&&label(n).startsWith((index+1)+". ")&&Arrays.asList(label(n).split("\n")).stream().anyMatch(line->line.equals(tag)||line.startsWith(tag+" · ")),"draft language position");checks++;}
    private void moveUp(String tag,int next){click(reveal(n->role(n,"Button")&&n.isEnabled()&&label(n).startsWith("Move up: ")&&label(n).endsWith("("+tag+")"),"Move up exact native locale"));ordinal(tag,next);}
    private void remove(String tag){AccessibilityNodeInfo old=reveal(n->role(n,"Button")&&n.isEnabled()&&label(n).startsWith("Remove: ")&&label(n).endsWith("("+tag+")"),"Remove exact native locale");click(old);long end=SystemClock.uptimeMillis()+10000;do{if(!old.refresh()){checks++;return;}SystemClock.sleep(80);}while(SystemClock.uptimeMillis()<end);throw new AssertionError("Removed draft target did not retire");}
    private void review(){click(button("Review language order"));button("Apply reviewed language order");checks++;}
    private String homeIdentity(){
        String dump=shell("dumpsys activity top");
        java.util.regex.Matcher matcher=java.util.regex.Pattern.compile("ACTIVITY dev\\.makepad\\.octosense/[^\\n]+ pid=([0-9]+)[^\\n]*\\n[ \\t]*Local Activity ([0-9a-f]+) State:").matcher(dump);
        if(!matcher.find())throw new AssertionError("Home Activity instance was not observable");
        return matcher.group(1)+":"+matcher.group(2);
    }
    private void apply(List<String> expected){review();String before=homeIdentity();click(button("Apply reviewed language order"));observed(expected);reveal(n->role(n,"TextView")&&label(n).equals("Current language order"),"confirmed current order");ready();check(before.equals(homeIdentity()),"Changing system languages recreated Home's Activity or process");preservedHomeActivities++;}
    private void cancel(){click(button("Cancel language draft"));reveal(n->role(n,"TextView")&&label(n).equals("Current language order"),"cancelled draft closed");reveal(n->role(n,"Button")&&label(n).equals("Cancel language draft")&&!n.isEnabled(),"disabled cancelled draft");ready();check(nodes().stream().noneMatch(n->role(n,"Button")&&n.isEnabled()&&label(n).equals("Apply reviewed language order")),"Cancel retained confirmation");}
    private static String tag(AccessibilityNodeInfo node){for(String line:label(node).split("\n")){String value=line.split(" · ",2)[0];if(value.matches("[a-z]{2,3}(-[A-Za-z0-9]{1,8})*"))return value;}return "";}
    private void filter(String query){
        AccessibilityNodeInfo editor=reveal(n->role(n,"EditText")&&n.isEnabled(),"native catalog filter");Bundle args=new Bundle();args.putCharSequence(AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE,query);
        check(editor.performAction(AccessibilityNodeInfo.ACTION_SET_TEXT,args),"Filter edit not queued");waitNode(n->role(n,"EditText")&&query.equals(string(n.getText())),"exact filter edit");
        AccessibilityNodeInfo search=button("Search languages");try{ui.executeAndWaitForEvent(()->click(search),event->event.getEventType()==android.view.accessibility.AccessibilityEvent.TYPE_VIEW_CLICKED&&search.equals(event.getSource()),10000);}catch(Exception failure){throw new AssertionError("Filter action was not acknowledged",failure);}
    }
    private String choose(String wanted){click(button("Add a language"));waitNode(n->role(n,"EditText"),"language filter");
        for(int level=0;level<4;level++){
            filter(level==0?wanted.split("-")[0]:wanted);
            AccessibilityNodeInfo choice=reveal(n->role(n,"Button")&&n.isEnabled()&&!tag(n).isEmpty()&&(wanted.equals(tag(n))||wanted.startsWith(tag(n)+"-")||tag(n).startsWith(wanted+"-")),"filtered native language branch "+wanted);
            String selected=tag(choice);boolean branch=label(choice).contains("Choose region or numbering");click(choice);
            if(!branch){button("Review language order");return selected;}
            long end=SystemClock.uptimeMillis()+10000;do{if(!choice.refresh())break;SystemClock.sleep(80);}while(SystemClock.uptimeMillis()<end);
        }throw new AssertionError("Native language hierarchy exceeded depth");
    }
    private Locale systemLocale(){return getContext().getSystemService(LocaleManager.class).getSystemLocales().get(0);}
    private boolean rendered(JSONObject state,String expectedText,boolean rtl)throws Exception{return state!=null&&state.getString("text").equals(expectedText)&&state.getString("system").equals(state.getString("rendered_system"))&&state.getString("app").isEmpty()&&state.getBoolean("visible")&&state.getBoolean("focused")&&state.getInt("direction")== (rtl?1:0)&&Locale.forLanguageTag(state.getString("configuration").split(",")[0]).equals(systemLocale());}
    private JSONObject consumer(String expectedText,boolean rtl)throws Exception{
        int before=lastConsumerCreated;
        shell("am start -W -n "+SELF+"/.LanguageConsumerActivity");JSONObject state=null;long end=SystemClock.uptimeMillis()+10000;
        do{state=LanguageConsumerState.snapshot(getContext());if(rendered(state,expectedText,rtl)){checks++;break;}SystemClock.sleep(80);}while(SystemClock.uptimeMillis()<end);
        check(rendered(state,expectedText,rtl),"Real translated consumer configuration/visibility/RTL mismatch");check(state.getString("app").isEmpty(),"Consumer has a per-app locale override");check(state.getInt("created")>before,"Consumer Activity did not recreate for the changed system configuration");lastConsumerCreated=state.getInt("created");check(Locale.forLanguageTag(state.getString("configuration").split(",")[0]).equals(systemLocale()),"Rendered resources do not use the preferred native configuration locale");
        open();ready();return state;
    }
    private void restore()throws Exception{
        setPreferences(preferences);open();ready();List<String> current=new ArrayList<>(system());
        if(current.equals(original))return;
        for(String tag:new ArrayList<>(current))if(!original.contains(tag)){remove(tag);current.remove(tag);}
        for(int index=0;index<original.size();index++){int position=current.indexOf(original.get(index));check(position>=0,"Original language missing; do not invent a replacement locale");while(position>index){String tag=current.remove(position);current.add(position-1,tag);moveUp(tag,position-1);position--;}}
        apply(original);
    }
    private void unavailable()throws Exception{
        open();reveal(n->role(n,"TextView")&&label(n).contains("unavailable on this installation"),"ordinary unavailable state");
        for(String name:new String[]{"Add a language","Review language order"}){AccessibilityNodeInfo node=reveal(n->role(n,"Button")&&label(n).equals(name)&&!n.isEnabled(),"disabled "+name);check(!node.performAction(AccessibilityNodeInfo.ACTION_CLICK),"Unavailable language operation accepted");}
        shell("am start -W -a android.settings.SOUND_SETTINGS -p com.android.settings");shell("am start -W -n "+HOME+"/.MakepadApp");pane("System languages");click(button("Android language settings"));
        long end=SystemClock.uptimeMillis()+10000;do{AccessibilityNodeInfo root=ui.getRootInActiveWindow();if(root!=null&&"com.android.settings".equals(string(root.getPackageName()))){checks++;break;}SystemClock.sleep(80);}while(SystemClock.uptimeMillis()<end);
        check(ui.getRootInActiveWindow()!=null&&"com.android.settings".equals(string(ui.getRootInActiveWindow().getPackageName())),"Trusted native language recovery missing");ui.performGlobalAction(1);pane("System languages");click(button("Back"));pane("System");observed(original);
    }
    private void nativeControls(Bundle result)throws Exception{
        phase="open native language page";check(original.size()<=8,"This bounded fixture expects at most8 baseline languages");open();ready();
        if(original.size()==1){AccessibilityNodeInfo last=reveal(n->role(n,"Button")&&label(n).startsWith("Remove: ")&&!n.isEnabled(),"protected last language");check(!last.performAction(AccessibilityNodeInfo.ACTION_CLICK),"Last language removal accepted");}
        phase="add French then cancel";String french=choose("fr-CA");review();cancel();observed(original);
        phase="add French with regional preferences";setPreferences("und-u-hc-h23");open();ready();french=choose("fr-CA");check(french.contains("-u-")&&Locale.forLanguageTag(french).getUnicodeLocaleType("hc").equals("h23"),"Native regional Unicode preferences were not preserved");
        phase="apply appended French";List<String> current=new ArrayList<>(original);current.add(french);apply(current);int position=current.size()-1;while(position>0){moveUp(french,position-1);position--;}review();cancel();observed(current);
        phase="French primary and native consumer";position=current.size()-1;while(position>0){moveUp(french,position-1);position--;}current.remove(french);current.add(0,french);apply(current);result.putString("french_consumer",consumer("Langue du système : français",false).toString());
        shell("am force-stop "+HOME);open();ready();observed(current);
        phase="Chinese primary and native consumer";String chinese=choose("zh-Hans-CN");current.add(chinese);apply(current);position=current.size()-1;while(position>0){moveUp(chinese,position-1);position--;}current.remove(chinese);current.add(0,chinese);apply(current);result.putString("chinese_consumer",consumer("系统语言：简体中文",false).toString());
        phase="Arabic primary and native consumer";String arabic=choose("ar-EG");current.add(arabic);apply(current);position=current.size()-1;while(position>0){moveUp(arabic,position-1);position--;}current.remove(arabic);current.add(0,arabic);apply(current);result.putString("arabic_consumer",consumer("لغة النظام: العربية",true).toString());
        result.putBoolean("numbering_leaf_offered",Locale.forLanguageTag(arabic).getUnicodeLocaleType("nu")!=null);
        // A legitimate external regional-preference change invalidates the open review.
        phase="stale review denial";remove(french);review();AccessibilityNodeInfo stale=button("Apply reviewed language order");setPreferences("und-u-hc-h12");
        reveal(n->role(n,"TextView")&&label(n).contains("draft is preserved"),"retired language review");check(!stale.performAction(AccessibilityNodeInfo.ACTION_CLICK),"Retired language review remained actionable");observed(current);cancel();
        result.putBoolean("native_order_resources_rtl",true);result.putBoolean("unicode_preferences",true);result.putBoolean("review_cancel_restart_stale",true);
    }
    @Override public void onStart(){Bundle result=new Bundle();Throwable failure=null;try{ui=getUiAutomation();original=system();preferences=rawPreferences();result.putString("original_locales",String.join(",",original));check(!original.isEmpty(),"No authoritative system locales");check(getContext().getSystemService(LocaleManager.class).getApplicationLocales().isEmpty(),"Fixture must inherit system locales");mutating=scenario.equals("native");if(mutating)nativeControls(result);else if(scenario.equals("unavailable"))unavailable();else throw new IllegalArgumentException("Finite scenario required");}catch(Throwable error){failure=error;}finally{try{if(mutating&&original!=null)restore();if(original!=null){observed(original);check(Objects.equals(preferences,rawPreferences()),"Regional preference baseline changed");}result.putBoolean("ordered_locales_restored",true);result.putString("final_locales",String.join(",",system()));result.putBoolean("regional_preferences_restored",Objects.equals(preferences,rawPreferences()));}catch(Throwable cleanup){if(failure!=null)failure.addSuppressed(cleanup);else failure=cleanup;}result.putString("phase",phase);result.putInt("preserved_home_activities",preservedHomeActivities);result.putInt("checks",checks);result.putBoolean("passed",failure==null);if(failure!=null)result.putString("failure",android.util.Log.getStackTraceString(failure));finish(failure==null?0:1,result);}}
}
