package dev.makepad.octosense.settingsa11yfixture;

import android.app.Instrumentation;
import android.app.UiAutomation;
import android.graphics.Rect;
import android.os.Bundle;
import android.os.ParcelFileDescriptor;
import android.os.SystemClock;
import android.view.accessibility.AccessibilityNodeInfo;
import android.view.accessibility.AccessibilityEvent;
import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import java.util.regex.Matcher;
import java.util.regex.Pattern;
import org.json.JSONObject;

/** Public Android APIs only. No private Settings service binding or content logging. */
public final class SettingsAccessibilityProbe extends Instrumentation {
    private static final String HOME="dev.makepad.octosense";
    private String scenario="inspect";
    private UiAutomation automation;
    private int checks;
    private int navigationRetries;
    interface Match {boolean accepts(AccessibilityNodeInfo node);}
    @Override public void onCreate(Bundle args) {
        super.onCreate(args);if(args!=null)scenario=args.getString("scenario","inspect");start();
    }
    private static String value(CharSequence value) {return value==null?"":value.toString();}
    private static boolean role(AccessibilityNodeInfo node,String name) {return ("android.widget."+name).equals(value(node.getClassName()));}
    private static String label(AccessibilityNodeInfo node) {
        String label=value(node.getContentDescription());if(!label.isEmpty())return label;
        label=value(node.getHintText());return label.isEmpty()?value(node.getText()):label;
    }
    private void check(boolean condition,String failure) {if(!condition)throw new AssertionError(failure);checks++;}
    private void collect(AccessibilityNodeInfo node,List<AccessibilityNodeInfo> result,int depth) {
        if(node==null||depth>10||result.size()>512)return;
        result.add(node);for(int i=0;i<node.getChildCount();i++)collect(node.getChild(i),result,depth+1);
    }
    private List<AccessibilityNodeInfo> nodes() {
        ArrayList<AccessibilityNodeInfo> nodes=new ArrayList<>();AccessibilityNodeInfo root=automation.getRootInActiveWindow();
        if(root!=null&&HOME.equals(value(root.getPackageName())))collect(root,nodes,0);
        return nodes;
    }
    private AccessibilityNodeInfo waitNode(Match match,String description) {
        long deadline=SystemClock.uptimeMillis()+10_000;
        do {
            for(AccessibilityNodeInfo node:nodes())if(match.accepts(node))return node;
            SystemClock.sleep(100);
        }while(SystemClock.uptimeMillis()<deadline);
        throw new AssertionError("Missing accessible "+description);
    }
    private AccessibilityNodeInfo button(String name) {
        return waitNode(node->role(node,"Button")&&label(node).equals(name),name+" button");
    }
    private void click(AccessibilityNodeInfo node) {check(node.performAction(AccessibilityNodeInfo.ACTION_CLICK),"Click was not queued: "+label(node));}
    private void clickAndWaitForAcknowledgment(AccessibilityNodeInfo node) {
        try {
            automation.executeAndWaitForEvent(()->click(node),event->event.getEventType()==AccessibilityEvent.TYPE_VIEW_CLICKED
                    &&HOME.equals(value(event.getPackageName()))&&node.equals(event.getSource()),10_000);
            checks++;
        }catch(java.util.concurrent.TimeoutException failure){throw new AssertionError("Click was not acknowledged: "+label(node),failure);}
    }
    private void setText(AccessibilityNodeInfo node,String text) {
        Bundle args=new Bundle();args.putCharSequence(AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE,text);
        check(node.performAction(AccessibilityNodeInfo.ACTION_SET_TEXT,args),"Editor replacement was not queued");
    }
    private String shell(String command) {
        try(ParcelFileDescriptor descriptor=automation.executeShellCommand(command);
                ParcelFileDescriptor.AutoCloseInputStream input=new ParcelFileDescriptor.AutoCloseInputStream(descriptor);
                ByteArrayOutputStream output=new ByteArrayOutputStream()) {
            byte[] buffer=new byte[4096];int count;
            while((count=input.read(buffer))!=-1)output.write(buffer,0,count);
            return new String(output.toByteArray(),StandardCharsets.UTF_8);
        }catch(IOException failure){throw new AssertionError("Cannot inspect keyboard visibility",failure);}
    }
    private void waitKeyboard(boolean visible) {
        long deadline=SystemClock.uptimeMillis()+10_000;
        do {
            if(shell("dumpsys input_method").contains("mInputShown="+visible)){checks++;return;}
            SystemClock.sleep(100);
        }while(SystemClock.uptimeMillis()<deadline);
        throw new AssertionError("Keyboard visibility did not become "+visible);
    }
    private static boolean action(AccessibilityNodeInfo node,int id) {
        for(AccessibilityNodeInfo.AccessibilityAction action:node.getActionList())if(action.getId()==id)return true;
        return false;
    }
    private void inspect(Bundle result) {
        waitNode(node->role(node,"ScrollView"),"Settings scroll container");
        int buttons=0,text=0,edit=0,scroll=0;
        for(AccessibilityNodeInfo node:nodes()) {
            if(role(node,"Button"))buttons++;if(role(node,"TextView"))text++;if(role(node,"EditText"))edit++;if(role(node,"ScrollView"))scroll++;
            if(role(node,"Button")||role(node,"TextView")||role(node,"EditText")) {
                Rect bounds=new Rect();node.getBoundsInScreen(bounds);check(!bounds.isEmpty()&&node.isVisibleToUser(),"Visible node has invalid bounds");
            }
        }
        result.putInt("buttons",buttons);result.putInt("text_nodes",text);result.putInt("editors",edit);result.putInt("scroll_containers",scroll);
        check(text>0&&scroll>0,"Settings tree lacks text or scroll hierarchy");
    }
    private void smoke(Bundle result) {
        AccessibilityNodeInfo overviewSearch=button("Search settings");click(overviewSearch);
        AccessibilityNodeInfo editor=waitNode(node->role(node,"EditText")&&node.isEnabled(),"Search editor");
        check(editor.isEditable()&&!label(editor).isEmpty(),"Search editor lacks editable semantics or label");
        check(!overviewSearch.performAction(AccessibilityNodeInfo.ACTION_CLICK),"Retired overview node remained actionable");
        AccessibilityNodeInfo previous=button("Previous");
        check(!previous.isEnabled(),"First-page Previous should be disabled");
        check(!previous.performAction(AccessibilityNodeInfo.ACTION_CLICK),"Disabled button remained actionable");
        check(editor.performAction(AccessibilityNodeInfo.ACTION_FOCUS),"Input focus was not queued");
        editor=waitNode(node->role(node,"EditText")&&node.isFocused(),"focused Search editor");
        waitKeyboard(true);
        setText(editor,"volume");
        editor=waitNode(node->role(node,"EditText")&&value(node.getText()).equals("volume"),"edited Search value");
        check(editor.performAction(AccessibilityNodeInfo.ACTION_ACCESSIBILITY_FOCUS),"Accessibility focus failed");
        SystemClock.sleep(6200);
        AccessibilityNodeInfo stable=waitNode(node->role(node,"EditText")&&value(node.getText()).equals("volume"),"stable Search value");
        check(stable.equals(editor)&&stable.isAccessibilityFocused(),"Polling replaced the editor identity or lost focus");
        shell("input keyevent 4");waitKeyboard(false);
        SystemClock.sleep(1200);waitKeyboard(false);
        click(stable);waitKeyboard(true);
        check(waitNode(node->role(node,"EditText")&&value(node.getText()).equals("volume"),"editor after keyboard reopen").equals(editor),
                "Accessible editor activation lost its draft or identity");
        shell("input keyevent 4");waitKeyboard(false);
        AccessibilityNodeInfo retiredEditor=editor;
        shell("am start -W -a android.settings.SETTINGS -p com.android.settings");
        Bundle staleEdit=new Bundle();staleEdit.putCharSequence(AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE,"stale");
        check(!retiredEditor.performAction(AccessibilityNodeInfo.ACTION_SET_TEXT,staleEdit),"Background Settings editor remained actionable");
        shell("am start -W -n dev.makepad.octosense/.MakepadApp");
        editor=waitNode(node->role(node,"EditText")&&value(node.getText()).equals("volume"),"Search draft after Activity return");
        check(!editor.equals(retiredEditor),"Activity return reused a retired accessibility identity");
        check(!retiredEditor.performAction(AccessibilityNodeInfo.ACTION_SET_TEXT,staleEdit),"Retired editor rebound after Activity return");
        AccessibilityNodeInfo resultNode=waitNode(node->role(node,"Button")&&label(node).contains("Media volume"),"Media volume search result");
        click(resultNode);
        AccessibilityNodeInfo media=waitNode(node->role(node,"TextView")&&label(node).startsWith("Media volume:"),"observed media volume");
        check(!resultNode.performAction(AccessibilityNodeInfo.ACTION_CLICK),"Retired search result remained actionable");
        Bundle forged=new Bundle();forged.putCharSequence(AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE,"invalid");
        check(!media.performAction(AccessibilityNodeInfo.ACTION_SET_TEXT,forged),"Read-only observation accepted editor action");
        AccessibilityNodeInfo ancestor=media.getParent();
        check(ancestor!=null&&role(ancestor,"ScrollView"),"Media label has no scroll ancestor");
        click(button("Back"));
        waitNode(node->role(node,"EditText")&&value(node.getText()).equals("volume"),"Search draft after nested Back");
        check(!editor.performAction(AccessibilityNodeInfo.ACTION_SET_TEXT,forged),"Old page visit editor remained editable");
        click(button("Back"));
        waitNode(node->role(node,"Button")&&label(node).equals("Search settings"),"restored Settings overview");
        check(!editor.performAction(AccessibilityNodeInfo.ACTION_SET_TEXT,forged),"Retired editor remained editable");
        // Sound fits without scrolling on tall displays. The overview is long
        // on both validation devices, so it exercises actual scroll movement.
        AccessibilityNodeInfo appearance=button("Appearance");ancestor=appearance.getParent();
        check(ancestor!=null&&role(ancestor,"ScrollView")&&ancestor.isScrollable(),"Overview has no scrollable ancestor");
        check(action(ancestor,AccessibilityNodeInfo.ACTION_SCROLL_FORWARD),"Overview cannot scroll forward");
        check(ancestor.performAction(AccessibilityNodeInfo.ACTION_SCROLL_FORWARD),"Scroll was not queued");
        AccessibilityNodeInfo scrolled=waitNode(node->role(node,"ScrollView")&&action(node,AccessibilityNodeInfo.ACTION_SCROLL_BACKWARD),"scrolled overview container");
        check(!appearance.refresh()||!appearance.isVisibleToUser(),"Forward scroll did not move the first row offscreen");
        check(scrolled.performAction(AccessibilityNodeInfo.ACTION_SCROLL_BACKWARD),"Reverse scroll was not queued");
        check(button("Appearance").equals(appearance),"Reverse scroll did not restore the same row identity");
        result.putBoolean("navigation",true);result.putBoolean("editing",true);result.putBoolean("stable_focus",true);
        result.putBoolean("keyboard_reopen",true);
        result.putBoolean("activity_retirement",true);
        result.putBoolean("scroll_ancestor",true);result.putBoolean("stale_and_disabled_denial",true);
    }
    private void pane(String title) {
        waitNode(node->role(node,"TextView")&&label(node).equals(title),"pane "+title);checks++;
    }
    private void openRoute(String route,String title) {
        shell("am start -W -a dev.makepad.octosense.action.SETTINGS -p "+HOME
                +" --es dev.makepad.octosense.extra.SETTINGS_ROUTE "+route);
        pane(title);
    }
    private AccessibilityNodeInfo revealButton(String name) {
        return reveal(node->role(node,"Button")&&label(node).equals(name),name);
    }
    private AccessibilityNodeInfo reveal(Match match,String name) {
        for(int attempt=0;attempt<8;attempt++) {
            // A page heading can arrive before its final child layout. Wait
            // for either the target or a usable scroll action; a tall viewport
            // may never need scrolling once the target has been published.
            AccessibilityNodeInfo candidate=null;long deadline=SystemClock.uptimeMillis()+10_000;
            do {
                List<AccessibilityNodeInfo> visible=nodes();
                // Prefer a visible target over its earlier tree-order scroll
                // ancestor; otherwise a long page skips the requested row.
                for(AccessibilityNodeInfo node:visible)if(match.accepts(node))return node;
                for(AccessibilityNodeInfo node:visible)if(role(node,"ScrollView")&&action(node,AccessibilityNodeInfo.ACTION_SCROLL_FORWARD)){candidate=node;break;}
                if(candidate!=null)break;SystemClock.sleep(100);
            }while(SystemClock.uptimeMillis()<deadline);
            if(candidate==null)throw new AssertionError("Cannot find "+name+" or a forward scroll action");
            check(candidate.performAction(AccessibilityNodeInfo.ACTION_SCROLL_FORWARD),"Cannot reveal native fallback");
            SystemClock.sleep(500);
        }
        throw new AssertionError("Native fallback button not visible");
    }
    private static final String[][] ANDROID_ACTIONS={{"SETTINGS","OctoSense Settings"},{"DISPLAY_SETTINGS","Display"},{"SOUND_SETTINGS","Sound"},
            {"WIFI_SETTINGS","Wi-Fi"},{"BLUETOOTH_SETTINGS","Bluetooth"},{"MANAGE_APPLICATIONS_SETTINGS","Apps"},
            {"APPLICATION_SETTINGS","Apps"},{"SYNC_SETTINGS","Accounts"},{"LOCATION_SOURCE_SETTINGS","Location"},
            {"BATTERY_SAVER_SETTINGS","Battery policy"},{"INTERNAL_STORAGE_SETTINGS","Storage"},{"DATE_SETTINGS","Date and time"},
            {"DEVICE_INFO_SETTINGS","About phone"},{"SYSTEM_UPDATE_SETTINGS","System updates"},
            {"AIRPLANE_MODE_SETTINGS","Advanced connections"},{"DATA_SAVER_SETTINGS","Advanced connections"},
            {"PRIVACY_SETTINGS","Privacy"},{"NOTIFICATION_SETTINGS","Notifications"},{"ZEN_MODE_SETTINGS","Do Not Disturb"},
            {"NIGHT_DISPLAY_SETTINGS","Display size and Night Light"},{"MANAGE_DEFAULT_APPS_SETTINGS","Default apps"}};
    private void entries(Bundle result) {
        shell("am force-stop "+HOME);
        openRoute("date_time","Date and time");
        String[][] routes={{"overview","OctoSense Settings"},{"search","Search settings"},{"appearance","Appearance"},
            {"display","Display"},{"sound","Sound"},{"wifi","Wi-Fi"},{"bluetooth","Bluetooth"},{"apps","Apps"},{"default_apps","Default apps"},
            {"accounts","Accounts"},{"notifications","Notifications"},{"dnd","Do Not Disturb"},{"privacy","Privacy"},{"location","Location"},
            {"battery","Battery"},{"battery_policy","Battery policy"},{"storage","Storage"},{"about","About phone"},
            {"system","System"},{"updates","System updates"},{"advanced_network","Advanced connections"},
            {"display_options","Display size and Night Light"},{"sound_feedback","Sound feedback and haptics"},{"accessibility_vision","Accessibility: colors"},{"accessibility_hearing","Accessibility: hearing"},{"caption_custom","Caption appearance"},{"caption_language","Caption language"},{"accessibility_text_interaction",INTERACTION_PANE},{"system_languages","System languages"}};
        for(String[] route:routes)openRoute(route[0],route[1]);

        for(String[] action:ANDROID_ACTIONS) {
            shell("am start -W -a android.settings."+action[0]+" -p "+HOME);pane(action[1]);
        }
        shell("am start -W -a android.settings.DISPLAY_SETTINGS -p "+HOME
                +" --es dev.makepad.octosense.extra.SETTINGS_ROUTE updates");pane("Display");
        for(String invalid:new String[]{"factory_reset","install_update","set_volume","DISPLAY"}) {
            shell("am start -W -a dev.makepad.octosense.action.SETTINGS -p "+HOME
                    +" --es dev.makepad.octosense.extra.SETTINGS_ROUTE "+invalid);
            SystemClock.sleep(300);pane("Display");
        }
        openRoute("sound","Sound");click(revealButton("More sound settings in Android"));
        long deadline=SystemClock.uptimeMillis()+10_000;boolean nativeSettings=false;
        do {
            AccessibilityNodeInfo root=automation.getRootInActiveWindow();
            nativeSettings=root!=null&&"com.android.settings".equals(value(root.getPackageName()));
            if(nativeSettings)break;SystemClock.sleep(100);
        }while(SystemClock.uptimeMillis()<deadline);
        check(nativeSettings,"Native fallback resolved back into OctoSense or a chooser");
        openRoute("overview","OctoSense Settings");
        result.putBoolean("cold_entry",true);result.putInt("custom_routes",routes.length+1);result.putInt("android_actions",ANDROID_ACTIONS.length);
        result.putBoolean("invalid_entry_denial",true);result.putBoolean("native_fallback",true);
    }
    private void coldEntries(Bundle result) {
        String[][] routes={{"date_time","Date and time"},{"wifi","Wi-Fi"},{"display","Display"},
                {"search","Search settings"},{"notifications","Notifications"},{"about","About phone"},
                {"sound_feedback","Sound feedback and haptics"},{"dnd","Do Not Disturb"},{"caption_custom","Caption appearance"},{"caption_language","Caption language"},{"accessibility_text_interaction",INTERACTION_PANE},{"system_languages","System languages"}};
        for(String[] route:routes) {
            shell("am force-stop "+HOME);
            openRoute(route[0],route[1]);
            // A later Resume must not replay an acknowledged entry over
            // in-app navigation. Return to the parent and take an Activity hop.
            click(button("Back"));
            String parent=route[0].startsWith("caption_")?"Accessibility: hearing":(route[0].equals("date_time")||route[0].equals("accessibility_text_interaction")||route[0].equals("system_languages"))?"System":route[0].equals("wifi")?"Connections":route[0].equals("sound_feedback")?"Sound":route[0].equals("dnd")?"Notifications":"OctoSense Settings";
            pane(parent);
            shell("am start -W -a android.settings.SETTINGS -p com.android.settings");
            shell("am start -W -n dev.makepad.octosense/.MakepadApp");pane(parent);
        }
        openRoute("overview","OctoSense Settings");
        result.putInt("cold_entries",routes.length);result.putBoolean("acknowledged_entries_not_replayed",true);
    }
    private void waitPackage(String expected) {
        long deadline=SystemClock.uptimeMillis()+10_000;
        do {
            AccessibilityNodeInfo root=automation.getRootInActiveWindow();
            if(root!=null&&expected.equals(value(root.getPackageName()))){checks++;return;}
            SystemClock.sleep(100);
        }while(SystemClock.uptimeMillis()<deadline);
        throw new AssertionError("Expected foreground package "+expected);
    }
    private void preferred(String destination) {
        shell("am start -W -n dev.makepad.octosense.settingsa11yfixture/.SettingsRouteProbeActivity --es destination "+destination);
    }
    private void preferredFallback(Bundle result) {
        openRoute("overview","OctoSense Settings");
        preferred("sound");waitPackage("com.android.settings");
        check(shell("cmd package resolve-activity --brief -a android.settings.SETTINGS").contains("com.android.settings/"),
                "Ordinary Home install replaced Android's default");
        check(shell("cmd package resolve-activity --brief -a android.settings.APP_NOTIFICATION_SETTINGS").contains("com.android.settings/"),
                "Ordinary Home install replaced Android's per-app notification default");
        String nativeDefaults=nativeDefaultsPackage();preferred("default_apps");waitPackage(nativeDefaults);
        openRoute("overview","OctoSense Settings");preferred("dnd");waitPackage("com.android.settings");shell("input keyevent 4");pane("OctoSense Settings");
        openRoute("overview","OctoSense Settings");result.putBoolean("ordinary_install_native_fallback",true);
    }
    private void romDefaults(Bundle result) {
        shell("cmd statusbar collapse");
        for(String[] route:ANDROID_ACTIONS) {
            check(shell("cmd package resolve-activity --brief -a android.settings."+route[0]).contains(HOME+"/.MakepadApp"),
                    "Privileged ROM entry is not the default for "+route[0]);
            shell("am start -W -a android.settings."+route[0]);pane(route[1]);
        }
        check(shell("cmd package resolve-activity --brief -a android.settings.APP_NOTIFICATION_SETTINGS").contains(HOME+"/.MakepadApp"),
                "Privileged per-app notification entry is not the default");
        shell("am start -W -a android.settings.APP_NOTIFICATION_SETTINGS --es android.provider.extra.APP_PACKAGE "+HOME);
        pane("App notifications");
        click(enabledNotificationButton("Android notification settings"));waitPackage("com.android.settings");
        shell("input keyevent 4");pane("App notifications");
        preferred("sound");pane("Sound");
        click(revealButton("More sound settings in Android"));waitPackage("com.android.settings");
        preferred("default_apps");pane("Default apps");
        click(revealButton("Android default-app settings"));waitPackage(CONTROLLER);shell("input keyevent 4");pane("Default apps");
        preferred("dnd");pane("Do Not Disturb");click(revealButton("Android Do Not Disturb settings"));waitPackage("com.android.settings");shell("input keyevent 4");pane("Do Not Disturb");
        preferred("accessibility");waitPackage("com.android.settings");
        shell("cmd statusbar expand-settings");waitPackage("com.android.systemui");
        long deadline=SystemClock.uptimeMillis()+10_000;AccessibilityNodeInfo gear=null;
        do {
            List<AccessibilityNodeInfo> tree=new ArrayList<>();collect(automation.getRootInActiveWindow(),tree,0);
            // AOSP's Compose footer labels the icon "Open settings." and
            // exposes its containing View as the actionable node.
            for(AccessibilityNodeInfo node:tree)if(label(node).equals("Open settings.")) {
                AccessibilityNodeInfo target=node;
                for(int depth=0;depth<2&&target!=null&&!target.isClickable();depth++)target=target.getParent();
                if(target!=null&&target.isClickable()&&target.isEnabled()&&target.isVisibleToUser()){gear=target;break;}
            }
            if(gear!=null)break;SystemClock.sleep(100);
        }while(SystemClock.uptimeMillis()<deadline);
        check(gear!=null,"SystemUI Settings gear not accessible");click(gear);pane("OctoSense Settings");
        result.putBoolean("implicit_rom_routes",true);result.putBoolean("systemui_gear",true);
        result.putInt("android_actions",ANDROID_ACTIONS.length+1);
        result.putBoolean("trusted_preferred_route",true);result.putBoolean("native_fallback",true);
    }
    private static int matchedInt(String text,String pattern,int fallback) {
        Matcher match=Pattern.compile(pattern).matcher(text);return match.find()?Integer.parseInt(match.group(1)):fallback;
    }
    private Rect bounds(AccessibilityNodeInfo node) {
        Rect rect=new Rect();node.getBoundsInScreen(rect);return rect;
    }
    private String secure(String key) {return shell("settings get secure "+key).trim();}
    private void waitSecure(String key,String expected) {
        long end=SystemClock.uptimeMillis()+10_000;
        do {if(secure(key).equals(expected)){checks++;return;}SystemClock.sleep(100);}while(SystemClock.uptimeMillis()<end);
        throw new AssertionError("Expected "+key+"="+expected+", observed "+secure(key));
    }
    private void displayPage() {
        // A same-page external entry is delivered asynchronously after am
        // returns. Observe a different pane first so the subsequent scroll
        // cannot race with that entry resetting Display's scroll position.
        openRoute("overview","OctoSense Settings");
        openRoute("display_options","Display size and Night Light");
        waitNode(node->role(node,"TextView")&&label(node).startsWith("Display size: "),"observed display size");
        waitNode(node->role(node,"Button")&&label(node).equals("Refresh display information")&&node.isEnabled(),"fresh display snapshot");
    }
    private void displayChoice(String choice) {
        displayPage();AccessibilityNodeInfo target=revealButton(choice);
        if(!target.isEnabled())target=waitNode(node->role(node,"Button")&&label(node).equals(choice)&&node.isEnabled(),"authorized display choice "+choice);
        click(target);
        waitNode(node->role(node,"TextView")&&label(node).startsWith("Review "),"display review");
    }
    private void saveDisplay() {
        AccessibilityNodeInfo save=waitNode(node->role(node,"Button")&&label(node).equals("Save reviewed display setting")&&node.isEnabled(),"enabled display Save");
        click(save);
        waitNode(node->role(node,"Button")&&label(node).equals("Refresh display information")&&node.isEnabled(),"completed display write");
    }
    private static final String[] DISPLAY_ROWS={"display_density_forced","night_display_activated","night_display_color_temperature",
        "night_display_auto_mode","night_display_custom_start_time","night_display_custom_end_time","night_display_last_activated_time"};
    private void displayControls(Bundle result) {
        check(shell("getprop ro.kernel.qemu").trim().equals("1"),"Display controls probe requires an emulator");
        check(shell("dumpsys color_display").contains("Night display:"),"Enable the documented emulator Night Light capability fixture first");
        String initialDensity=shell("wm density");int physical=matchedInt(initialDensity,"Physical density: (\\d+)",-1);
        int override=matchedInt(initialDensity,"Override density: (\\d+)",-1);
        check(physical>0&&override<0,"Display controls probe requires the emulator's default density");
        String[] original=new String[DISPLAY_ROWS.length];for(int i=0;i<DISPLAY_ROWS.length;i++)original[i]=secure(DISPLAY_ROWS[i]);
        String location=shell("cmd location is-location-enabled --user 0").trim();
        check(location.equals("true")||location.equals("false"),"Cannot observe original Location state");
        try {
            shell("cmd statusbar collapse");displayPage();
            AccessibilityNodeInfo smaller=waitNode(node->role(node,"Button")&&label(node).startsWith("Smaller (")&&node.isEnabled(),"smaller density choice");
            String densityLabel=label(smaller),process=shell("pidof "+HOME).trim();
            click(smaller);
            waitNode(node->role(node,"TextView")&&label(node).startsWith("Review display size:"),"density review");
            check(nodes().stream().noneMatch(node->role(node,"EditText")),"Time editor leaked into density review");
            click(button("Cancel display change"));
            check(shell("wm density").equals(initialDensity),"Cancel changed display density");
            displayChoice(densityLabel);saveDisplay();
            long end=SystemClock.uptimeMillis()+10_000;
            while(shell("wm density").equals(initialDensity)&&SystemClock.uptimeMillis()<end)SystemClock.sleep(100);
            int applied=matchedInt(shell("wm density"),"Override density: (\\d+)",-1);
            check(applied>0&&applied<physical,"Saved smaller display size was not applied");
            check(shell("pidof "+HOME).trim().equals(process),"Saving display size restarted Home");
            displayPage();waitNode(node->role(node,"TextView")&&label(node).startsWith("Display size: Smaller"),"actual smaller size readback");
            displayChoice("Default");saveDisplay();
            end=SystemClock.uptimeMillis()+10_000;
            while(!shell("wm density").equals(initialDensity)&&SystemClock.uptimeMillis()<end)SystemClock.sleep(100);
            check(shell("wm density").equals(initialDensity),"Default did not clear density override");
            check(shell("pidof "+HOME).trim().equals(process),"Default display size restarted Home");
            displayChoice("Turn Night Light on");click(button("Cancel display change"));
            check(secure("night_display_activated").equals(original[1]),"Cancel changed Night Light activation");
            displayChoice("Turn Night Light on");saveDisplay();waitSecure("night_display_activated","1");
            SystemClock.sleep(1500);
            String color=shell("dumpsys color_display");
            check(color.contains("Activated: true"),"ColorDisplay service did not activate Night Light");
            String compositor=shell("dumpsys SurfaceFlinger");
            String transform=compositor.lines().filter(line->line.contains("colorTransformMatrix")).findFirst().orElse("");
            check(!transform.isEmpty()&&!transform.contains("[[1.000,0.000,0.000,0.000][0.000,1.000,0.000,0.000][0.000,0.000,1.000,0.000][0.000,0.000,0.000,1.000]]"),"Night Light did not change compositor color transform");
            result.putString("compositor_color_transform",transform);
            displayChoice("Make Night Light warmer");
            AccessibilityNodeInfo temperature=waitNode(node->role(node,"TextView")&&label(node).startsWith("Review color temperature:"),"temperature review");
            check(nodes().stream().noneMatch(node->role(node,"EditText")),"Time editor leaked into temperature review");
            int kelvin=matchedInt(label(temperature),"temperature: (\\d+)",-1);check(kelvin>0,"Invalid temperature review");
            saveDisplay();waitSecure("night_display_color_temperature",Integer.toString(kelvin));
            // Restore warmth through ColorDisplayManager as well as its
            // persisted row. The service caches its tint temperature; deleting
            // the Settings row alone does not reset that in-memory value.
            displayChoice("Make Night Light cooler");saveDisplay();waitSecure("night_display_color_temperature",Integer.toString(kelvin+100));
            displayChoice("Turn Night Light off");saveDisplay();waitSecure("night_display_activated","0");
            SystemClock.sleep(1500);
            check(shell("dumpsys color_display").contains("Activated: false"),"Night Light Off did not reach ColorDisplay service");
            displayChoice("Custom times");saveDisplay();waitSecure("night_display_auto_mode","1");
            String previousStart=secure("night_display_custom_start_time");
            displayChoice("Edit start time");
            AccessibilityNodeInfo editor=waitNode(node->role(node,"EditText")&&node.isEnabled(),"Night Light time editor");
            setText(editor,"24:99");
            waitNode(node->role(node,"EditText")&&value(node.getText()).equals("24:99"),"invalid Night Light time");
            waitNode(node->role(node,"Button")&&label(node).equals("Save reviewed display setting")&&!node.isEnabled(),"disabled invalid-time Save");
            check(secure("night_display_custom_start_time").equals(previousStart),"Invalid time changed the schedule");
            setText(editor,"23:01:02");
            waitNode(node->role(node,"EditText")&&value(node.getText()).equals("23:01:02"),"valid seconds-preserving time");
            saveDisplay();waitSecure("night_display_custom_start_time","82862000");
            displayChoice("Edit end time");
            editor=waitNode(node->role(node,"EditText")&&node.isEnabled(),"Night Light end editor");setText(editor,"07:05:06");
            waitNode(node->role(node,"EditText")&&value(node.getText()).equals("07:05:06"),"edited Night Light end");
            saveDisplay();waitSecure("night_display_custom_end_time","25506000");
            shell("cmd location set-location-enabled false --user 0");displayPage();
            AccessibilityNodeInfo sunset=revealButton("Sunset to sunrise");
            check(!sunset.isEnabled(),"Sunset schedule remained enabled with Location off");
            check(!sunset.performAction(AccessibilityNodeInfo.ACTION_CLICK),"Disabled sunset schedule accepted a click");
            check(shell("cmd location is-location-enabled --user 0").trim().equals("false"),"Display controls silently enabled Location");
            check(secure("night_display_auto_mode").equals("1"),"Denied sunset action changed schedule");
            shell("cmd location set-location-enabled true --user 0");
            displayChoice("Sunset to sunrise");saveDisplay();waitSecure("night_display_auto_mode","2");
            displayChoice("No schedule");saveDisplay();waitSecure("night_display_auto_mode","0");
            result.putBoolean("save_cancel",true);result.putBoolean("density_applied_without_restart",true);
            result.putBoolean("night_service_activated",true);result.putBoolean("temperature",true);
            result.putBoolean("schedule_seconds_and_validation",true);result.putBoolean("location_gate",true);
        } finally {
            // Only cleanup adopts shell permission. Every tested change above
            // goes through the visible built-in Settings controls and helper.
            shell(override>0?"wm density "+override:"wm density reset");
            shell("cmd location set-location-enabled "+location+" --user 0");
            automation.adoptShellPermissionIdentity("android.permission.WRITE_SECURE_SETTINGS");
            try {for(int i=0;i<DISPLAY_ROWS.length;i++)check(android.provider.Settings.Secure.putString(
                    getContext().getContentResolver(),DISPLAY_ROWS[i],original[i].equals("null")?null:original[i]),"Cannot restore "+DISPLAY_ROWS[i]);}
            finally {automation.dropShellPermissionIdentity();}
        }
        for(int i=0;i<DISPLAY_ROWS.length;i++)check(secure(DISPLAY_ROWS[i]).equals(original[i]),"Display row not restored: "+DISPLAY_ROWS[i]);
        check(shell("wm density").equals(initialDensity),"Display test density not restored");
        check(shell("cmd location is-location-enabled --user 0").trim().equals(location),"Display test Location not restored");
        openRoute("overview","OctoSense Settings");result.putBoolean("restored",true);
    }
    private void density(Bundle result) {
        check(shell("getprop ro.kernel.qemu").trim().equals("1"),"Density probe requires an emulator");
        shell("cmd statusbar collapse");openRoute("search","Search settings");
        AccessibilityNodeInfo editor=waitNode(node->role(node,"EditText")&&node.isEnabled(),"Search editor");
        check(editor.performAction(AccessibilityNodeInfo.ACTION_FOCUS),"Cannot focus density test editor");waitKeyboard(true);
        setText(editor,"density draft");
        editor=waitNode(node->role(node,"EditText")&&value(node.getText()).equals("density draft"),"density test draft");
        final AccessibilityNodeInfo identity=editor;
        String process=shell("pidof "+HOME).trim();check(!process.isEmpty(),"Home process absent");
        String initial=shell("wm density");
        String rawDensity=shell("settings get secure display_density_forced").trim();
        check(rawDensity.equals("null")||rawDensity.matches("[0-9]*"),"Unexpected raw density setting");
        int physical=matchedInt(initial,"Physical density: (\\d+)",-1);
        int override=matchedInt(initial,"Override density: (\\d+)",-1);
        check(physical>=160&&physical<=640,"Unexpected fixture physical density");
        int current=override>0?override:physical;
        String expected="density draft";
        try {
            for(int dpi:new int[]{physical*6/5,physical*9/10}) {
                if(dpi==current)dpi+=20;
                Rect before=bounds(editor);shell("wm density "+dpi);
                final String preserved=expected;
                editor=waitNode(node->role(node,"EditText")&&value(node.getText()).equals(preserved)
                        &&!bounds(node).equals(before),"rescaled editor with preserved draft");
                check(editor.equals(identity),"Density change replaced the editor identity");
                check(shell("pidof "+HOME).trim().equals(process),"Density change restarted Home");
                check(editor.isFocused(),"Density change lost editor focus");waitKeyboard(true);
                // Use a real physical-coordinate touch, not an accessibility
                // click, to verify Makepad's coordinate conversion after DPI.
                AccessibilityNodeInfo clear=button("Clear search");Rect target=bounds(clear);
                check(!target.isEmpty()&&clear.isVisibleToUser(),"Clear has invalid physical bounds");
                shell("input tap "+target.centerX()+" "+target.centerY());
                waitNode(node->role(node,"EditText")&&value(node.getText()).isEmpty(),"Clear after density touch");
                expected="bookkeeper"+dpi;shell("input text "+expected);
                final String typed=expected;
                editor=waitNode(node->role(node,"EditText")&&value(node.getText()).equals(typed),"exact typing after density touch");
                check(editor.equals(identity),"Typing rebound the editor after density change");
                current=dpi;
            }
        } finally {
            shell(override>0?"wm density "+override:"wm density reset");
            // WMS reset stores an empty row. Preserve absence as well as the
            // effective value when the fixture started without an override.
            automation.adoptShellPermissionIdentity("android.permission.WRITE_SECURE_SETTINGS");
            try {
                check(android.provider.Settings.Secure.putString(getContext().getContentResolver(),
                        "display_density_forced",rawDensity.equals("null")?null:rawDensity),"Raw density restoration failed");
            } finally { automation.dropShellPermissionIdentity(); }
        }
        check(shell("settings get secure display_density_forced").trim().equals(rawDensity),"Raw density row was not restored");
        check(shell("wm density").equals(initial),"Original density override was not restored");
        final String finalText=expected;
        editor=waitNode(node->role(node,"EditText")&&value(node.getText()).equals(finalText),"draft after density restore");
        check(editor.equals(identity)&&shell("pidof "+HOME).trim().equals(process),"Density restore recreated Home/editor");
        openRoute("overview","OctoSense Settings");
        result.putBoolean("density_restore",true);result.putBoolean("editor_identity",true);
        result.putBoolean("physical_touch",true);result.putBoolean("exact_typing",true);
    }
    private void nightUnavailable(Bundle result) {
        check(shell("getprop ro.kernel.qemu").trim().equals("1"),"Unsupported Night Light probe requires an emulator");
        check(!shell("dumpsys color_display").contains("Color temp:"),"Remove the Night Light capability fixture before this scenario");
        displayPage();
        waitNode(node->role(node,"Button")&&label(node).startsWith("Smaller (")&&node.isEnabled(),"working density helper on an unsupported Night Light display");
        for(String name:new String[]{"Turn Night Light on","Make Night Light warmer","Make Night Light cooler","No schedule","Custom times","Sunset to sunrise","Edit start time","Edit end time"}) {
            AccessibilityNodeInfo target=revealButton(name);
            check(!target.isEnabled(),"Unsupported Night Light control enabled: "+name);
            check(!target.performAction(AccessibilityNodeInfo.ACTION_CLICK),"Unsupported Night Light accepted an action: "+name);
        }
        check(nodes().stream().noneMatch(node->role(node,"EditText")),"Unsupported Night Light exposed a time editor");
        openRoute("overview","OctoSense Settings");result.putBoolean("unsupported_denial",true);
    }
    private void soundFeedbackPage() {
        openRoute("overview","OctoSense Settings");openRoute("sound_feedback","Sound feedback and haptics");
        // Refresh is enabled while a read is still in flight. Page entry
        // retires the old choices, so an enabled observed choice proves the
        // new snapshot has arrived before we scroll its dependent row layout.
        // Charging sounds remains writable when master vibration is off.
        waitNode(node->role(node,"Button")&&node.isEnabled()
                &&(label(node).equals("Charging sounds: Off")||label(node).equals("Charging sounds: On")),
                "fresh writable sound feedback snapshot");
    }
    private void feedbackChoice(String title,String choice) {
        soundFeedbackPage();String name=title+": "+choice;
        AccessibilityNodeInfo target=revealButton(name);
        if(!target.isEnabled())target=waitNode(node->role(node,"Button")&&label(node).equals(name)&&node.isEnabled(),"writable "+name);
        click(target);
    }
    private String setting(String table,String key) {return shell("settings get "+table+" "+key).trim();}
    private void waitSetting(String table,String key,String value) {
        long end=SystemClock.uptimeMillis()+10_000;
        do {if(setting(table,key).equals(value)){checks++;return;}SystemClock.sleep(100);}while(SystemClock.uptimeMillis()<end);
        throw new AssertionError("Expected "+table+"/"+key+"="+value+", observed "+setting(table,key));
    }
    private void waitVibrator(String observed) {
        long end=SystemClock.uptimeMillis()+10_000;
        do {if(shell("dumpsys vibrator_manager").contains(observed)){checks++;return;}SystemClock.sleep(100);}while(SystemClock.uptimeMillis()<end);
        throw new AssertionError("Vibrator service did not report "+observed);
    }
    private static final String[][] FEEDBACK_ROWS={
        {"secure","charging_sounds_enabled","Charging sounds"},{"secure","charging_vibration_enabled","Charging vibration"},
        {"system","lockscreen_sounds_enabled","Screen lock sounds"},{"system","dtmf_tone","Dial pad tones"},
        {"system","vibrate_on","Vibration and haptics"},{"system","keyboard_vibration_enabled","Keyboard vibration"},
        {"system","ring_vibration_intensity","Ring vibration intensity","RINGTONE"},
        {"system","notification_vibration_intensity","Notification vibration intensity","NOTIFICATION"},
        {"system","alarm_vibration_intensity","Alarm vibration intensity","ALARM"},
        {"system","media_vibration_intensity","Media vibration intensity","MEDIA"},
        {"system","haptic_feedback_intensity","Touch vibration intensity","TOUCH"},
        {"system","vibrate_when_ringing"},{"system","haptic_feedback_enabled"},{"system","hardware_haptic_feedback_intensity"}};
    private void soundFeedback(Bundle result) {
        check(shell("getprop ro.kernel.qemu").trim().equals("1"),"Sound feedback probe requires an emulator");
        check(shell("cmd overlay lookup com.android.settings com.android.settings:integer/config_vibration_supported_intensity_levels").trim().equals("1"),"This scenario requires native Off/Device default granularity");
        String[] original=new String[FEEDBACK_ROWS.length];
        for(int i=0;i<FEEDBACK_ROWS.length;i++) {
            original[i]=setting(FEEDBACK_ROWS[i][0],FEEDBACK_ROWS[i][1]);
            check(original[i].matches("null|[0-9]+"),"Unexpected fixture sound value: "+FEEDBACK_ROWS[i][1]);
        }
        waitVibrator("vibrateOn = true");
        for(int i=6;i<=10;i++)waitVibrator(FEEDBACK_ROWS[i][3]+" = MEDIUM, default: MEDIUM");
        try {
            for(int i=0;i<4;i++) {
                String[] row=FEEDBACK_ROWS[i];
                feedbackChoice(row[2],"Off");waitSetting(row[0],row[1],"0");
                feedbackChoice(row[2],"On");waitSetting(row[0],row[1],"1");
            }
            feedbackChoice("Vibration and haptics","Off");waitSetting("system","vibrate_on","0");waitVibrator("vibrateOn = false");
            soundFeedbackPage();
            for(int i=6;i<=10;i++) {
                String title=FEEDBACK_ROWS[i][2];
                reveal(node->role(node,"TextView")&&label(node).equals(title),title);
                check(nodes().stream().noneMatch(node->role(node,"Button")&&label(node).startsWith(title+": ")&&node.isEnabled()),"Master-Off still offers "+title);
            }
            feedbackChoice("Vibration and haptics","On");waitSetting("system","vibrate_on","1");waitVibrator("vibrateOn = true");
            for(int i=6;i<=10;i++) {
                String[] row=FEEDBACK_ROWS[i];
                feedbackChoice(row[2],"Off");waitSetting("system",row[1],"0");waitVibrator(row[3]+" = OFF, default: MEDIUM");
                if(i==6)waitSetting("system","vibrate_when_ringing","0");
                if(i==10){waitSetting("system","haptic_feedback_enabled","0");waitSetting("system","hardware_haptic_feedback_intensity","2");}
                feedbackChoice(row[2],"Device default");waitSetting("system",row[1],"2");waitVibrator(row[3]+" = MEDIUM, default: MEDIUM");
                if(i==6)waitSetting("system","vibrate_when_ringing","1");
                if(i==10){waitSetting("system","haptic_feedback_enabled","1");waitSetting("system","hardware_haptic_feedback_intensity","2");}
                // The one-level native configuration must not acquire guessed
                // Low/Medium/High choices after a write and fresh snapshot.
                soundFeedbackPage();reveal(node->role(node,"TextView")&&label(node).equals(row[2]),row[2]);
                check(nodes().stream().noneMatch(node->role(node,"Button")&&label(node).startsWith(row[2]+": ")
                        &&(label(node).contains(": Low")||label(node).contains(": Medium")||label(node).contains(": High"))),"Unsupported intensity levels were advertised");
            }
            soundFeedbackPage();reveal(node->role(node,"TextView")&&label(node).equals("Keyboard vibration"),"keyboard vibration row");
            check(nodes().stream().noneMatch(node->role(node,"Button")&&label(node).startsWith("Keyboard vibration: ")&&node.isEnabled()),"Missing keyboard capability still offers writes");
            result.putBoolean("sound_preferences",true);result.putBoolean("vibrator_service",true);
            result.putBoolean("master_dependency",true);result.putBoolean("legacy_couplings",true);
            result.putBoolean("native_intensity_choices",true);result.putBoolean("unsupported_keyboard",true);
        } finally {
            // Settings.System's public client refuses some private keys even
            // with adopted shell permission. Use the shell Settings command
            // for restoration only. Keys are constants and values were
            // validated before any mutation; no quoting or arbitrary input.
            for(int i=0;i<FEEDBACK_ROWS.length;i++) {
                String[] row=FEEDBACK_ROWS[i];String value=original[i];
                shell(value.equals("null")?"settings delete "+row[0]+" "+row[1]
                        :"settings put "+row[0]+" "+row[1]+" "+value);
            }
        }
        for(int i=0;i<FEEDBACK_ROWS.length;i++)check(setting(FEEDBACK_ROWS[i][0],FEEDBACK_ROWS[i][1]).equals(original[i]),"Sound setting not restored: "+FEEDBACK_ROWS[i][1]);
        waitVibrator("vibrateOn = true");for(int i=6;i<=10;i++)waitVibrator(FEEDBACK_ROWS[i][3]+" = MEDIUM, default: MEDIUM");
        openRoute("overview","OctoSense Settings");result.putBoolean("restored",true);
    }
    private static final String NOTIFICATION_FIXTURE="dev.makepad.octosense.notificationfixture";
    private static final String LEGACY_NOTIFICATION_FIXTURE="dev.makepad.octosense.notificationlegacyfixture";
    private static final String PERMISSION_FIXTURE="dev.makepad.octosense.permissionfixture";
    private static final String LEGACY_PERMISSION_FIXTURE="dev.makepad.octosense.permissionlegacyfixture";
    private JSONObject permissionFixture(String pkg)throws Exception {
        if(!pkg.equals(PERMISSION_FIXTURE)&&!pkg.equals(LEGACY_PERMISSION_FIXTURE))throw new AssertionError("Not a disposable permission fixture");
        String report=shell("am instrument -w "+pkg+"/dev.makepad.octosense.permissionfixture.PermissionFixture");
        check(report.contains("INSTRUMENTATION_RESULT: passed=true"),"Public permission observation failed");
        for(String line:report.split("\\r?\\n"))if(line.startsWith("INSTRUMENTATION_RESULT: state=")){
            JSONObject state=new JSONObject(line.substring("INSTRUMENTATION_RESULT: state=".length()));
            check(pkg.equals(state.getString("package")),"Wrong permission observer package");return state;
        }
        throw new AssertionError("Permission fixture did not return observed state");
    }
    private JSONObject permissionRow(JSONObject state,String shortName)throws Exception {
        org.json.JSONArray rows=state.getJSONArray("permissions");String name="android.permission."+shortName;
        for(int i=0;i<rows.length();i++)if(name.equals(rows.getJSONObject(i).getString("name")))return rows.getJSONObject(i);
        throw new AssertionError("Fixture did not request "+name);
    }
    private void permissionGrant(JSONObject state,String shortName,boolean granted)throws Exception {
        JSONObject row=permissionRow(state,shortName);
        for(String field:new String[]{"granted","package_granted","requested_granted"})
            check(row.getBoolean(field)==granted,"Unexpected "+shortName+" "+field+" state");
    }
    private void permissionMode(JSONObject state,String shortName,int expected)throws Exception {
        check(permissionRow(state,shortName).getInt("mode")==expected,"Unexpected raw "+shortName+" AppOp mode");
    }
    private String permissionFlags(String pkg,String shortName) {
        if(!pkg.equals(PERMISSION_FIXTURE)&&!pkg.equals(LEGACY_PERMISSION_FIXTURE))throw new AssertionError("Not a permission fixture");
        Matcher row=Pattern.compile("android\\.permission\\."+Pattern.quote(shortName)+": granted=(?:true|false), flags=\\[([^]]*)\\]")
                .matcher(shell("dumpsys package "+pkg));
        check(row.find(),"Missing actual permission flags for "+shortName);return row.group(1);
    }
    private static String permissionLabel(AccessibilityNodeInfo node){return label(node).replace('’','\'');}
    private void restorePermissionRotation(String key,String value){
        if(!(key.equals("user_rotation")||key.equals("accelerometer_rotation"))||!value.matches("null|[0-3]"))throw new AssertionError("Invalid rotation restoration");
        shell(value.equals("null")?"settings delete system "+key:"settings put system "+key+" "+value);
    }
    private android.view.Display permissionDisplay(){
        android.view.Display display=getContext().getSystemService(android.hardware.display.DisplayManager.class)
                .getDisplay(android.view.Display.DEFAULT_DISPLAY);
        if(display==null)throw new AssertionError("Missing default emulator display");return display;
    }
    private void waitPermissionRotation(String pkg,int rotation,int retiredWindow){
        long deadline=SystemClock.uptimeMillis()+10_000;
        do{
            android.view.Display display=permissionDisplay();
            if(display.getRotation()==rotation){
                android.graphics.Point size=new android.graphics.Point();display.getRealSize(size);
                for(android.view.accessibility.AccessibilityWindowInfo window:automation.getWindows()){
                    AccessibilityNodeInfo root=window.getRoot();
                    if(!window.isFocused()||window.getId()==retiredWindow||root==null||!pkg.equals(value(root.getPackageName())))continue;
                    Rect bounds=new Rect();root.getBoundsInScreen(bounds);
                    // Home occupies the display. Its laid-out orientation must
                    // match the restored display, not merely the stored setting.
                    if(HOME.equals(pkg)&&(bounds.width()>bounds.height())!=(size.x>size.y))continue;
                    checks++;return;
                }
            }
            SystemClock.sleep(100);
        }while(SystemClock.uptimeMillis()<deadline);
        throw new AssertionError("Display rotation/focused window did not settle for "+pkg);
    }
    private void permissionPage(String pkg){
        notificationAppDetails(pkg);click(enabledNotificationButton("Permissions"));pane("App permissions");
    }
    private void permissionGroup(String group){
        rolesTop();click(reveal(node->role(node,"Button")&&label(node).startsWith(group+"\n")&&node.isEnabled(),group+" group"));
        pane(group+" permissions");
    }
    private AccessibilityNodeInfo permissionChoice(String group,String name,boolean selected){
        rolesTop();String expected=group+": "+name+(selected?" · Selected":"");
        return reveal(node->role(node,"Button")&&permissionLabel(node).equals(expected)&&(selected||node.isEnabled()),expected);
    }
    private void selectedPermission(String group,String name){
        String expected=group+": "+name+" · Selected";
        AccessibilityNodeInfo node=waitNode(current->role(current,"Button")&&permissionLabel(current).equals(expected),expected);
        check(!node.isEnabled()&&!node.performAction(AccessibilityNodeInfo.ACTION_CLICK),"Selected permission choice remained actionable");
    }
    private void choosePermission(String group,String name){
        AccessibilityNodeInfo choice=permissionChoice(group,name,false);
        check(choice.isEnabled(),"Native permission choice is disabled: "+name);click(choice);
        selectedPermission(group,name);
        check(!choice.performAction(AccessibilityNodeInfo.ACTION_CLICK),"Retired permission choice remained actionable");
    }
    private void simplePermissionGroup(String group,String... permissions)throws Exception {
        permissionGroup(group);
        AccessibilityNodeInfo allow=waitNode(node->role(node,"Button")&&node.isEnabled()
                &&(permissionLabel(node).equals(group+": Allow")||permissionLabel(node).equals(group+": Allow only while using the app")),"native allow choice for "+group);
        String selected=permissionLabel(allow).substring((group+": ").length());choosePermission(group,selected);
        JSONObject state=permissionFixture(PERMISSION_FIXTURE);
        for(String permission:permissions){permissionGrant(state,permission,true);int mode=permissionRow(state,permission).getInt("mode");check(mode==0||mode==4,"Granted "+permission+" AppOp still blocks access");}
        choosePermission(group,"Don't allow");state=permissionFixture(PERMISSION_FIXTURE);
        for(String permission:permissions){permissionGrant(state,permission,false);permissionMode(state,permission,1);}
        click(button("Back"));pane("App permissions");
    }
    private void permissionStage(Bundle result,String stage){
        result.putString("permission_stage",stage);
        android.util.Log.i("OctoSensePermissionAcceptance","stage="+stage);
    }
    private JSONObject dndNativeState()throws Exception{
        automation.adoptShellPermissionIdentity("android.permission.MANAGE_NOTIFICATIONS");
        try{return dev.makepad.octosense.dndfixture.DndNativeSnapshot.read(
                getTargetContext().getSystemService(android.app.NotificationManager.class));}
        finally{automation.dropShellPermissionIdentity();}
    }
    private org.json.JSONArray dndRanking()throws Exception{
        String report=shell("am instrument -w -e operation post_ranking dev.makepad.octosense.dndfixture/.DndFixture");
        check(report.contains("INSTRUMENTATION_RESULT: passed=true"),"Synthetic DND ranking probe failed: "+report);
        for(String line:report.split("\\r?\\n"))if(line.startsWith("INSTRUMENTATION_RESULT: ranking="))
            return new org.json.JSONArray(line.substring("INSTRUMENTATION_RESULT: ranking=".length()));
        throw new AssertionError("Synthetic DND ranking probe returned no observations");
    }
    private void dndObserver(Bundle result)throws Exception{
        openRoute("overview","OctoSense Settings");
        JSONObject before=dndNativeState();org.json.JSONArray ranking=dndRanking();
        pane("OctoSense Settings");check(button("Search settings").isEnabled(),"Ranking probe displaced the Settings automation connection");
        check(before.toString().equals(dndNativeState().toString()),"Read-only DND observer changed native state");
        check(ranking.length()==6,"Missing synthetic DND categories");
        result.putBoolean("nested_ranking_preserves_ui_automation",true);
    }
    private void dndUnavailable(Bundle result){
        dndOpen();waitNode(node->role(node,"TextView")&&value(node.getText()).contains("Do Not Disturb controls are unavailable"),"unavailable DND explanation");
        dndPolicyPage();
        for(String[] field:DND_FIELDS){
            rolesTop();AccessibilityNodeInfo choice=revealButton(dndChoiceLabel(field[0],field[2]));
            check(!choice.isEnabled()&&!choice.performAction(AccessibilityNodeInfo.ACTION_CLICK),"Unavailable DND policy remains actionable: "+field[0]);
        }
        click(button("Back"));pane("Do Not Disturb");dndRulesPage();
        for(String name:new String[]{"Add time schedule","Previous rules","Next rules"}){
            rolesTop();AccessibilityNodeInfo choice=revealButton(name);check(!choice.isEnabled()&&!choice.performAction(AccessibilityNodeInfo.ACTION_CLICK),"Unavailable rule action remains enabled: "+name);
        }
        for(AccessibilityNodeInfo node:nodes())check(!role(node,"Button")||!label(node).matches("(?s).*\\n(?:On|Off) · .*"),"Unavailable service invented a rule row");
        click(button("Back"));pane("Do Not Disturb");
        click(revealButton("Android Do Not Disturb settings"));waitPackage("com.android.settings");shell("input keyevent 4");pane("Do Not Disturb");
        click(button("Back"));pane("Notifications");openRoute("overview","OctoSense Settings");
        result.putBoolean("unavailable_dnd_controls_disabled",true);result.putBoolean("native_dnd_recovery_and_nested_back",true);
    }
    private void dndEntries(Bundle result){
        shell("am force-stop "+HOME);openRoute("dnd","Do Not Disturb");
        click(button("Back"));pane("Notifications");
        shell("am start -W -a android.settings.SOUND_SETTINGS -p com.android.settings");
        shell("am start -W -n "+HOME+"/.MakepadApp");pane("Notifications");
        shell("am start -W -a android.settings.ZEN_MODE_SETTINGS -p "+HOME+" --es dev.makepad.octosense.extra.SETTINGS_ROUTE updates --es value off");pane("Do Not Disturb");
        click(revealButton("Schedules and rules"));pane("Schedules and rules");click(button("Back"));pane("Do Not Disturb");
        click(revealButton("Android Do Not Disturb settings"));waitPackage("com.android.settings");shell("input keyevent 4");pane("Do Not Disturb");
        click(button("Back"));pane("Notifications");openRoute("overview","OctoSense Settings");
        result.putBoolean("cold_dnd_entry_and_no_replay",true);result.putBoolean("standard_dnd_entry_ignores_untrusted_extras",true);
        result.putBoolean("native_dnd_recovery_with_existing_settings_task",true);
    }
    private static final String[][] DND_FIELDS={
        {"calls","Calls","anyone","contacts","starred","none"},
        {"messages","Messages","anyone","contacts","starred","none"},
        {"conversations","Conversations","all","important","none"},
        {"repeat_callers","Repeated callers","on","off"},
        {"alarms","Alarms","on","off"},{"media","Media","on","off"},
        {"system","System sounds","on","off"},{"reminders","Reminders","on","off"},
        {"events","Calendar events","on","off"}};
    private static int dndBit(String field){
        switch(field){case "calls":return 8;case "messages":return 4;case "conversations":return 256;
            case "repeat_callers":return 16;case "alarms":return 32;case "media":return 64;
            case "system":return 128;case "reminders":return 1;case "events":return 2;
            default:throw new AssertionError("Unknown DND field");}
    }
    private static String dndValue(JSONObject policy,String field)throws Exception{
        if((policy.getInt("categories")&dndBit(field))==0)
            return field.equals("calls")||field.equals("messages")||field.equals("conversations")?"none":"off";
        if(field.equals("calls")||field.equals("messages"))return new String[]{"anyone","contacts","starred"}[policy.getInt(field)];
        if(field.equals("conversations"))return new String[]{"unknown","all","important","none"}[policy.getInt(field)];
        return "on";
    }
    private static String dndChoiceLabel(String field,String value){
        String title=null;for(String[] row:DND_FIELDS)if(row[0].equals(field))title=row[1];
        String choice;
        switch(value){case "anyone":choice="Anyone";break;case "contacts":choice="Contacts";break;
            case "starred":choice="Starred contacts";break;case "none":choice=field.equals("conversations")?"None":"No one";break;
            case "all":choice="All";break;case "important":choice="Important";break;case "on":choice="On";break;case "off":choice="Off";break;
            default:throw new AssertionError("Unknown DND choice");}
        if(title==null)throw new AssertionError("Unknown DND field");return title+": "+choice;
    }
    private void dndOpen(){
        openRoute("notifications","Notifications");click(revealButton("Do Not Disturb"));pane("Do Not Disturb");
    }
    private void dndPolicyPage(){click(revealButton("Allowed interruptions"));pane("Allowed interruptions");}
    private void dndMode(String choice,int filter)throws Exception{
        String label="Do Not Disturb: "+choice;rolesTop();
        AccessibilityNodeInfo node=reveal(current->role(current,"Button")
                &&(label(current).equals(label)||label(current).equals(label+" · Selected")),label);
        // Page visibility precedes the independently loaded manual-mode capability.
        // Wait for a real observation rather than treating its loading state as denial.
        node=waitNode(current->role(current,"Button")
                &&(label(current).equals(label)&&current.isEnabled()||label(current).equals(label+" · Selected")),label+" observed capability");
        if(!label(node).endsWith(" · Selected")){check(node.isEnabled(),"DND mode is unavailable");click(node);}
        long deadline=SystemClock.uptimeMillis()+10_000;
        do{if(dndNativeState().getInt("filter")==filter){
            waitNode(current->role(current,"Button")&&label(current).equals(label+" · Selected")&&!current.isEnabled(),label+" selected");checks++;return;
        }SystemClock.sleep(100);}while(SystemClock.uptimeMillis()<deadline);
        throw new AssertionError("Native interruption filter did not become "+filter);
    }
    private void dndPolicySet(String field,String value)throws Exception{
        android.util.Log.i("OctoSenseDndAcceptance","policy field="+field+" value="+value);
        JSONObject before=dndNativeState();String name=dndChoiceLabel(field,value);rolesTop();
        AccessibilityNodeInfo node=reveal(current->role(current,"Button")
                &&(label(current).equals(name)||label(current).equals(name+" · Selected")),name);
        if(!label(node).endsWith(" · Selected")){check(node.isEnabled(),"DND policy choice is unavailable: "+name);click(node);}
        // The clicked row remains visible. Wait for its asynchronous readback;
        // searching by scrolling here can move past it before the label changes.
        AccessibilityNodeInfo selected=waitNode(current->role(current,"Button")&&label(current).equals(name+" · Selected"),name+" selected");
        check(!selected.isEnabled()&&!selected.performAction(AccessibilityNodeInfo.ACTION_CLICK),"Selected DND policy remained actionable");
        JSONObject after=dndNativeState(),original=before.getJSONObject("policy"),actual=after.getJSONObject("policy");
        check(dndValue(actual,field).equals(value),"Native DND policy differs from "+name);
        int bit=dndBit(field),categories=original.getInt("categories");
        int expected=value.equals("off")||value.equals("none")?categories&~bit:categories|bit;
        check(actual.getInt("categories")==expected,"DND mutation changed unrelated policy categories");
        for(String sender:new String[]{"calls","messages","conversations"}){
            int senderValue=original.getInt(sender);
            if(sender.equals(field)){
                if(value.equals("anyone"))senderValue=0;else if(value.equals("contacts"))senderValue=1;else if(value.equals("starred"))senderValue=2;
                if(field.equals("conversations"))senderValue=value.equals("all")?1:value.equals("important")?2:3;
            }
            check(actual.getInt(sender)==senderValue,"DND mutation lost native "+sender+" sender policy");
        }
        check(actual.getInt("visual_effects")==original.getInt("visual_effects"),"DND mutation changed visual suppression");
        check(before.getJSONObject("rules").toString().equals(after.getJSONObject("rules").toString()),"Default policy edit changed an automatic rule");
    }
    private void dndExpectRanking(String allowed)throws Exception{
        org.json.JSONArray rows=dndRanking();check(rows.length()==6,"Missing DND ranking categories");
        for(int i=0;i<rows.length();i++){
            JSONObject row=rows.getJSONObject(i);String category=row.getString("category");
            check(row.getBoolean("matches_filter")==category.equals(allowed),"Unexpected DND ranking for "+category+" while allowing "+allowed);
            check(!row.getBoolean("bypass_dnd")&&!row.getBoolean("suspended")&&row.getInt("importance")==3,"Synthetic ranking bypasses the policy under test");
        }
    }
    private void dndPolicy(Bundle result)throws Exception{
        JSONObject baseline=dndNativeState();check(baseline.getInt("filter")==1,"DND acceptance requires the disposable Off baseline");
        JSONObject original=baseline.getJSONObject("policy");
        try{
            dndOpen();dndPolicyPage();result.putString("dnd_stage","policy_fields");
            for(String[] field:DND_FIELDS){
                for(int i=2;i<field.length;i++)dndPolicySet(field[0],field[i]);
                dndPolicySet(field[0],dndValue(original,field[0]));
            }
            check(dndNativeState().getJSONObject("policy").toString().equals(original.toString()),"All policy choices did not restore the complete native policy");
            result.putString("dnd_stage","notification_ranking");
            for(String field:new String[]{"calls","messages","conversations"})dndPolicySet(field,"none");
            for(String field:new String[]{"repeat_callers","alarms","reminders","events"})dndPolicySet(field,"off");
            click(button("Back"));pane("Do Not Disturb");dndMode("Priority only",2);dndPolicyPage();
            dndExpectRanking("");
            for(String[] row:new String[][]{{"alarms","on","alarm"},{"reminders","on","reminder"},
                    {"events","on","event"},{"messages","anyone","msg"},{"calls","anyone","call"}}){
                dndPolicySet(row[0],row[1]);dndExpectRanking(row[2]);
                dndPolicySet(row[0],row[1].equals("on")?"off":"none");
            }
            result.putBoolean("all_policy_fields_native_readback",true);
            result.putBoolean("independent_synthetic_dnd_ranking",true);
        }catch(Exception|AssertionError failure){
            result.putString("dnd_primary_failure",failure.getClass().getSimpleName()+": "+failure.getMessage());
            throw failure;
        }finally{
            if(!dndNativeState().toString().equals(baseline.toString())){
                dndOpen();if(dndNativeState().getInt("filter")!=1)dndMode("Off",1);dndPolicyPage();
                for(String[] field:DND_FIELDS)dndPolicySet(field[0],dndValue(original,field[0]));
            }
            check(dndNativeState().toString().equals(baseline.toString()),"DND policy acceptance did not restore exact native state");
            openRoute("overview","OctoSense Settings");
        }
        result.putString("dnd_stage","complete");
    }
    private static final String DND_SYNTHETIC="OctoSense validation ";
    private static final String[] DND_DAYS={"Sunday","Monday","Tuesday","Wednesday","Thursday","Friday","Saturday"};
    private String dndRuleId(JSONObject state,String name)throws Exception{
        JSONObject rules=state.getJSONObject("rules");String found=null;
        java.util.Iterator<String> ids=rules.keys();while(ids.hasNext()){
            String id=ids.next();if(name.equals(rules.getJSONObject(id).getString("name"))){check(found==null,"Duplicate synthetic schedule name");found=id;}
        }
        return found;
    }
    private void dndRulesPage(){click(revealButton("Schedules and rules"));pane("Schedules and rules");}
    private void dndRulePage(String name){
        rolesTop();click(reveal(node->role(node,"Button")&&node.isEnabled()&&label(node).startsWith(name+"\n"),name+" schedule"));pane("Schedule");
    }
    private void dndEditorText(String name,String value){
        rolesTop();AccessibilityNodeInfo editor=reveal(node->role(node,"EditText")&&label(node).equals(name)&&node.isEnabled(),name);
        setText(editor,value);waitNode(node->role(node,"EditText")&&label(node).equals(name)&&value(node.getText()).equals(value),name+" edited value");
    }
    private void dndToggle(String name,boolean enabled){
        rolesTop();String target=name+": "+(enabled?"On":"Off");
        AccessibilityNodeInfo node=reveal(current->role(current,"Button")&&(label(current).equals(name+": On")||label(current).equals(name+": Off")),name);
        if(!label(node).equals(target)){check(node.isEnabled(),name+" toggle unavailable");click(node);}
        waitNode(current->role(current,"Button")&&label(current).equals(target),target);
    }
    private void dndDays(int... days){
        for(int day=1;day<=7;day++){boolean enabled=false;for(int value:days)if(value==day)enabled=true;dndToggle(DND_DAYS[day-1],enabled);}
    }
    private void dndSave(String name)throws Exception{
        click(enabledNotificationButton("Save schedule"));pane("Schedules and rules");
        long deadline=SystemClock.uptimeMillis()+10_000;
        do{if(dndRuleId(dndNativeState(),name)!=null){checks++;return;}SystemClock.sleep(100);}while(SystemClock.uptimeMillis()<deadline);
        throw new AssertionError("Saved schedule missing from native rules");
    }
    private void dndAssertSchedule(JSONObject state,String name,String days,String start,String end,boolean exit,boolean enabled)throws Exception{
        String id=dndRuleId(state,name);check(id!=null,"Missing native synthetic schedule");JSONObject rule=state.getJSONObject("rules").getJSONObject(id);
        check(rule.getString("owner").equals("android/com.android.server.notification.ScheduleConditionProvider"),"Schedule did not use Android's provider");
        check(rule.getInt("filter")==2&&rule.getBoolean("enabled")==enabled,"Unexpected native schedule filter/enabled state");
        android.net.Uri uri=android.net.Uri.parse(rule.getString("condition"));
        check("condition".equals(uri.getScheme())&&"android".equals(uri.getAuthority())&&"/schedule".equals(uri.getPath()),"Wrong native schedule condition");
        check(days.equals(uri.getQueryParameter("days"))&&start.equals(uri.getQueryParameter("start"))&&end.equals(uri.getQueryParameter("end"))
                &&Boolean.toString(exit).equals(uri.getQueryParameter("exitAtAlarm")),"Native schedule values differ from the reviewed draft");
    }
    private void dndDeleteSynthetic(String name)throws Exception{
        if(!name.startsWith(DND_SYNTHETIC))throw new AssertionError("Not a synthetic schedule");
        dndOpen();dndRulesPage();dndRulePage(name);click(enabledNotificationButton("Delete schedule"));
        click(enabledNotificationButton("Confirm delete schedule"));pane("Schedules and rules");
        long deadline=SystemClock.uptimeMillis()+10_000;
        do{if(dndRuleId(dndNativeState(),name)==null){checks++;return;}SystemClock.sleep(100);}while(SystemClock.uptimeMillis()<deadline);
        throw new AssertionError("Confirmed synthetic deletion did not remove its native rule");
    }
    private void dndRemoveSyntheticExternally(String id,JSONObject baseline)throws Exception{
        check(!baseline.getJSONObject("rules").has(id),"Refusing to remove an existing rule");
        automation.adoptShellPermissionIdentity("android.permission.MANAGE_NOTIFICATIONS");
        try{
            android.app.NotificationManager manager=getTargetContext().getSystemService(android.app.NotificationManager.class);
            android.app.AutomaticZenRule rule=manager.getAutomaticZenRules().get(id);
            check(rule!=null&&rule.getName().startsWith(DND_SYNTHETIC)&&!rule.isEnabled()
                    &&new android.content.ComponentName("android","com.android.server.notification.ScheduleConditionProvider").equals(rule.getOwner()),"External removal is restricted to a disabled synthetic rule");
            check(manager.removeAutomaticZenRule(id),"External synthetic removal failed");
        }finally{automation.dropShellPermissionIdentity();}
    }
    private void dndWaitFilter(int filter,long timeout){
        long deadline=SystemClock.uptimeMillis()+timeout;android.app.NotificationManager manager=getTargetContext().getSystemService(android.app.NotificationManager.class);
        do{if(manager.getCurrentInterruptionFilter()==filter){checks++;return;}SystemClock.sleep(500);}while(SystemClock.uptimeMillis()<deadline);
        throw new AssertionError("Android scheduler did not reach interruption filter "+filter);
    }
    private static String dndClock(java.util.Calendar time){return String.format(java.util.Locale.ROOT,"%02d:%02d",time.get(java.util.Calendar.HOUR_OF_DAY),time.get(java.util.Calendar.MINUTE));}
    private void dndSchedules(Bundle result)throws Exception{
        JSONObject baseline=dndNativeState();check(baseline.getInt("filter")==1,"Schedule acceptance requires the Off baseline");
        java.util.Iterator<String> existing=baseline.getJSONObject("rules").keys();while(existing.hasNext()){
            JSONObject rule=baseline.getJSONObject("rules").getJSONObject(existing.next());
            check(!rule.getString("name").startsWith(DND_SYNTHETIC),"Synthetic schedule already exists");
            check(!rule.getBoolean("enabled"),"Schedule timing acceptance requires the clone's disabled existing rules");
        }
        String first=DND_SYNTHETIC+"evening",renamed=DND_SYNTHETIC+"updated",stale=DND_SYNTHETIC+"stale";
        try{
            result.putString("dnd_stage","schedule_cancel_and_validation");dndOpen();dndRulesPage();click(enabledNotificationButton("Add time schedule"));pane("Edit schedule");
            dndEditorText("Schedule name",first);dndEditorText("Start time","25:99");
            AccessibilityNodeInfo save=revealButton("Save schedule");check(!save.isEnabled()&&!save.performAction(AccessibilityNodeInfo.ACTION_CLICK),"Invalid schedule time remained saveable");
            click(revealButton("Cancel schedule"));pane("Schedules and rules");check(dndNativeState().toString().equals(baseline.toString()),"Cancel created a rule");
            click(enabledNotificationButton("Add time schedule"));pane("Edit schedule");dndEditorText("Schedule name",first);
            dndEditorText("Start time","22:15");dndEditorText("End time","06:45");dndDays(2,4);dndToggle("Exit at next alarm",true);dndToggle("Schedule enabled",false);dndSave(first);
            JSONObject created=dndNativeState();dndAssertSchedule(created,first,"2.4","22.15","6.45",true,false);
            String createdId=dndRuleId(created,first);JSONObject original=created.getJSONObject("rules").getJSONObject(createdId);
            dndRulePage(first);click(revealButton("Edit schedule"));pane("Edit schedule");dndEditorText("Schedule name",renamed);
            click(revealButton("Cancel schedule"));pane("Schedules and rules");check(dndNativeState().getJSONObject("rules").getJSONObject(createdId).toString().equals(original.toString()),"Edit Cancel changed a native rule");
            dndRulePage(first);click(revealButton("Delete schedule"));check(dndRuleId(dndNativeState(),first)!=null,"Delete review removed a rule before confirmation");
            click(revealButton("Cancel schedule deletion"));pane("Schedule");click(revealButton("Edit schedule"));pane("Edit schedule");
            dndEditorText("Schedule name",renamed);dndEditorText("Start time","08:30");dndEditorText("End time","08:30");dndToggle("Exit at next alarm",false);dndSave(renamed);
            JSONObject edited=dndNativeState();dndAssertSchedule(edited,renamed,"2.4","8.30","8.30",false,false);
            JSONObject changed=edited.getJSONObject("rules").getJSONObject(createdId);
            for(String key:new String[]{"owner","configuration","filter","created","type","trigger","icon","manual_invocation","zen_policy","device_effects"})
                check(original.get(key).equals(changed.get(key)),"Schedule edit changed unreviewed native field "+key);
            dndRulePage(renamed);click(enabledNotificationButton("Enable schedule"));enabledNotificationButton("Disable schedule");
            check(dndNativeState().getJSONObject("rules").getJSONObject(createdId).getBoolean("enabled"),"Enable did not change native rule");
            click(enabledNotificationButton("Disable schedule"));enabledNotificationButton("Enable schedule");dndWaitFilter(1,5000);
            check(!dndNativeState().getJSONObject("rules").getJSONObject(createdId).getBoolean("enabled"),"Disable did not change native rule");
            click(revealButton("Edit schedule"));pane("Edit schedule");dndDays();dndSave(renamed);
            dndAssertSchedule(dndNativeState(),renamed,"","8.30","8.30",false,false);dndRulePage(renamed);
            AccessibilityNodeInfo emptyEnable=revealButton("Enable schedule");check(!emptyEnable.isEnabled()&&!emptyEnable.performAction(AccessibilityNodeInfo.ACTION_CLICK),"Empty-day schedule could be enabled");
            click(revealButton("Edit schedule"));pane("Edit schedule");dndToggle("Schedule enabled",true);
            AccessibilityNodeInfo emptySave=revealButton("Save schedule");check(!emptySave.isEnabled()&&!emptySave.performAction(AccessibilityNodeInfo.ACTION_CLICK),"Enabled empty-day draft could be saved");
            click(revealButton("Cancel schedule"));pane("Schedules and rules");dndRulePage(renamed);
            result.putString("dnd_stage","native_schedule_without_home");
            click(revealButton("Edit schedule"));pane("Edit schedule");dndDays(1,2,3,4,5,6,7);
            java.util.Calendar start=java.util.Calendar.getInstance();start.set(java.util.Calendar.SECOND,0);start.set(java.util.Calendar.MILLISECOND,0);start.add(java.util.Calendar.MINUTE,2);
            java.util.Calendar end=(java.util.Calendar)start.clone();end.add(java.util.Calendar.MINUTE,1);
            result.putLong("schedule_start_epoch",start.getTimeInMillis());result.putLong("schedule_end_epoch",end.getTimeInMillis());
            dndEditorText("Start time",dndClock(start));dndEditorText("End time",dndClock(end));dndToggle("Schedule enabled",true);dndSave(renamed);
            check(System.currentTimeMillis()<start.getTimeInMillis(),"Schedule setup missed the future start boundary");
            shell("am force-stop "+HOME);check(shell("pidof "+HOME).trim().isEmpty(),"Home remained running during native scheduler test");
            android.util.Log.i("OctoSenseDndAcceptance","Waiting for Android schedule start with Home stopped");
            dndWaitFilter(2,150_000);
            result.putLong("schedule_active_observed_epoch",System.currentTimeMillis());
            check(shell("pidof "+HOME).trim().isEmpty(),"Schedule activation required Home to restart");
            android.util.Log.i("OctoSenseDndAcceptance","Android schedule active; waiting for its end with Home stopped");
            dndWaitFilter(1,90_000);
            result.putLong("schedule_inactive_observed_epoch",System.currentTimeMillis());
            check(shell("pidof "+HOME).trim().isEmpty(),"Schedule end required Home to restart");
            dndOpen();dndRulesPage();dndDeleteSynthetic(renamed);
            result.putString("dnd_stage","stale_schedule_draft");click(enabledNotificationButton("Add time schedule"));pane("Edit schedule");
            dndEditorText("Schedule name",stale);dndSave(stale);String staleId=dndRuleId(dndNativeState(),stale);
            dndRulePage(stale);click(revealButton("Edit schedule"));pane("Edit schedule");dndEditorText("Schedule name","Keep this unsaved draft");
            AccessibilityNodeInfo oldSave=enabledNotificationButton("Save schedule");dndRemoveSyntheticExternally(staleId,baseline);
            waitNode(node->role(node,"Button")&&label(node).equals("Save schedule")&&!node.isEnabled(),"retired schedule Save");
            check(!oldSave.performAction(AccessibilityNodeInfo.ACTION_CLICK),"Deleted schedule's captured Save remained actionable");
            rolesTop();reveal(node->role(node,"EditText")&&label(node).equals("Schedule name")&&value(node.getText()).equals("Keep this unsaved draft"),"retained stale draft");
            click(revealButton("Cancel schedule"));pane("Schedules and rules");
            result.putBoolean("native_schedule_start_and_end_without_home",true);result.putBoolean("schedule_cancel_edit_delete_and_stale_draft",true);
        }catch(Exception|AssertionError failure){
            result.putString("dnd_primary_failure",failure.getClass().getSimpleName()+": "+failure.getMessage());
            throw failure;
        }finally{
            JSONObject remaining=dndNativeState().getJSONObject("rules");java.util.Iterator<String> ids=remaining.keys();ArrayList<String> remove=new ArrayList<>();
            while(ids.hasNext()){String id=ids.next();String name=remaining.getJSONObject(id).getString("name");if(!baseline.getJSONObject("rules").has(id)&&name.startsWith(DND_SYNTHETIC))remove.add(name);}
            for(String name:remove)dndDeleteSynthetic(name);
            if(dndNativeState().getInt("filter")!=1){dndOpen();dndMode("Off",1);}
            check(dndNativeState().toString().equals(baseline.toString()),"Schedule acceptance did not preserve original policy/rules");
            openRoute("overview","OctoSense Settings");
        }
        result.putString("dnd_stage","complete");
    }
    private void runtimePermissions(Bundle result)throws Exception {
        permissionStage(result,"modern_camera");
        JSONObject initial=permissionFixture(PERMISSION_FIXTURE);permissionGrant(initial,"CAMERA",false);
        permissionGrant(initial,"VIBRATE",true);
        permissionPage(PERMISSION_FIXTURE);permissionGroup("Camera");
        choosePermission("Camera","Allow only while using the app");
        JSONObject state=permissionFixture(PERMISSION_FIXTURE);permissionGrant(state,"CAMERA",true);permissionMode(state,"CAMERA",4);
        choosePermission("Camera","Ask every time");state=permissionFixture(PERMISSION_FIXTURE);
        permissionGrant(state,"CAMERA",false);permissionMode(state,"CAMERA",1);
        String flags=permissionFlags(PERMISSION_FIXTURE,"CAMERA");check(flags.contains("ONE_TIME")&&!flags.contains("USER_SET"),"Ask every time lost native one-time semantics");
        choosePermission("Camera","Don't allow");state=permissionFixture(PERMISSION_FIXTURE);permissionGrant(state,"CAMERA",false);
        flags=permissionFlags(PERMISSION_FIXTURE,"CAMERA");check(!flags.contains("ONE_TIME")&&flags.contains("USER_SET"),"Deny was confused with Ask every time");
        permissionStage(result,"modern_microphone");
        click(button("Back"));pane("App permissions");permissionGroup("Microphone");
        choosePermission("Microphone","Allow only while using the app");state=permissionFixture(PERMISSION_FIXTURE);permissionGrant(state,"RECORD_AUDIO",true);permissionMode(state,"RECORD_AUDIO",4);
        choosePermission("Microphone","Don't allow");state=permissionFixture(PERMISSION_FIXTURE);permissionGrant(state,"RECORD_AUDIO",false);permissionMode(state,"RECORD_AUDIO",1);
        permissionStage(result,"modern_location");
        click(button("Back"));pane("App permissions");permissionGroup("Location");
        choosePermission("Location","Allow only while using the app");state=permissionFixture(PERMISSION_FIXTURE);
        permissionGrant(state,"ACCESS_COARSE_LOCATION",true);permissionGrant(state,"ACCESS_BACKGROUND_LOCATION",false);permissionMode(state,"ACCESS_COARSE_LOCATION",4);
        // Test both accuracy mutations regardless of Android's initial accuracy default.
        if(!permissionRow(state,"ACCESS_FINE_LOCATION").getBoolean("granted"))choosePermission("Location","Use precise location");
        choosePermission("Location","Use approximate location");state=permissionFixture(PERMISSION_FIXTURE);
        permissionGrant(state,"ACCESS_FINE_LOCATION",false);permissionGrant(state,"ACCESS_COARSE_LOCATION",true);
        choosePermission("Location","Use precise location");state=permissionFixture(PERMISSION_FIXTURE);permissionGrant(state,"ACCESS_FINE_LOCATION",true);permissionMode(state,"ACCESS_FINE_LOCATION",4);
        choosePermission("Location","Allow all the time");state=permissionFixture(PERMISSION_FIXTURE);
        for(String permission:new String[]{"ACCESS_COARSE_LOCATION","ACCESS_FINE_LOCATION","ACCESS_BACKGROUND_LOCATION"})permissionGrant(state,permission,true);
        permissionMode(state,"ACCESS_COARSE_LOCATION",0);permissionMode(state,"ACCESS_FINE_LOCATION",0);
        choosePermission("Location","Allow only while using the app");state=permissionFixture(PERMISSION_FIXTURE);
        permissionGrant(state,"ACCESS_BACKGROUND_LOCATION",false);permissionMode(state,"ACCESS_COARSE_LOCATION",4);
        choosePermission("Location","Don't allow");state=permissionFixture(PERMISSION_FIXTURE);
        for(String permission:new String[]{"ACCESS_COARSE_LOCATION","ACCESS_FINE_LOCATION","ACCESS_BACKGROUND_LOCATION"})permissionGrant(state,permission,false);
        permissionMode(state,"ACCESS_COARSE_LOCATION",1);permissionMode(state,"ACCESS_FINE_LOCATION",1);
        click(button("Back"));pane("App permissions");
        permissionStage(result,"modern_other_groups");
        simplePermissionGroup("Contacts","READ_CONTACTS","WRITE_CONTACTS");
        simplePermissionGroup("Calendar","READ_CALENDAR","WRITE_CALENDAR");
        simplePermissionGroup("Phone","READ_PHONE_STATE","CALL_PHONE");
        permissionStage(result,"modern_call_logs_sms");
        result.putString("fixture_call_log_flags",permissionFlags(PERMISSION_FIXTURE,"READ_CALL_LOG"));
        result.putString("fixture_sms_flags",permissionFlags(PERMISSION_FIXTURE,"READ_SMS"));
        simplePermissionGroup("Call logs","READ_CALL_LOG");
        simplePermissionGroup("SMS","READ_SMS");
        permissionStage(result,"modern_nearby_activity_sensors");
        simplePermissionGroup("Nearby devices","BLUETOOTH_SCAN","BLUETOOTH_CONNECT","BLUETOOTH_ADVERTISE");
        simplePermissionGroup("Physical activity","ACTIVITY_RECOGNITION");
        simplePermissionGroup("Body sensors","BODY_SENSORS");
        permissionStage(result,"native_recovery");
        // Native recovery must return here even with an older Settings task.
        shell("am start -W -a android.settings.SOUND_SETTINGS -p com.android.settings");
        shell("am start -W -n "+HOME+"/.MakepadApp");pane("App permissions");
        click(enabledNotificationButton("Android app settings"));waitPackage("com.android.settings");shell("input keyevent 4");pane("App permissions");
        click(button("Back"));pane("App details");click(button("Back"));pane("Apps");
        waitNode(node->role(node,"EditText")&&value(node.getText()).equals(PERMISSION_FIXTURE),"Apps filter after permissions Back");

        legacyPermissionWarnings(result);
        result.putBoolean("native_common_permission_choices",true);result.putBoolean("ask_vs_deny",true);
        result.putBoolean("approximate_precise_background",true);
        result.putBoolean("permission_nested_back",true);
    }
    private void legacyPermissionWarnings(Bundle result)throws Exception {
        JSONObject state;
        permissionStage(result,"legacy_open");
        permissionPage(LEGACY_PERMISSION_FIXTURE);permissionGroup("Camera");
        for(boolean back:new boolean[]{false,true}){
            permissionStage(result,back?"legacy_warning_back":"legacy_warning_cancel");
            click(permissionChoice("Camera","Don't allow",false));waitPackage(CONTROLLER);
            AccessibilityNodeInfo approve=nativeRoleButton("android:id/button1");
            ArrayList<AccessibilityNodeInfo> warning=new ArrayList<>();collect(automation.getRootInActiveWindow(),warning,0);
            check(warning.stream().anyMatch(node->value(node.getText()).contains("older version of Android")),"Legacy warning was not native old-SDK consent");
            state=permissionFixture(LEGACY_PERMISSION_FIXTURE);permissionMode(state,"CAMERA",4);
            if(back)shell("input keyevent 4");else click(nativeRoleButton("android:id/button2"));pane("Camera permissions");
            state=permissionFixture(LEGACY_PERMISSION_FIXTURE);permissionMode(state,"CAMERA",4);
            check(!approve.performAction(AccessibilityNodeInfo.ACTION_CLICK),"Dismissed native warning remained actionable");
        }
        permissionStage(result,"legacy_warning_rotation");
        int originalRotation=permissionDisplay().getRotation();
        click(permissionChoice("Camera","Don't allow",false));waitPackage(CONTROLLER);
        int previousWarningWindow=nativeRoleButton("android:id/button1").getWindowId();
        String rotation=setting("system","user_rotation"),automatic=setting("system","accelerometer_rotation");
        try{
            int changedRotation=originalRotation==android.view.Surface.ROTATION_90?android.view.Surface.ROTATION_0:android.view.Surface.ROTATION_90;
            check(automation.setRotation(changedRotation==android.view.Surface.ROTATION_90?UiAutomation.ROTATION_FREEZE_90:UiAutomation.ROTATION_FREEZE_0),"Cannot rotate disposable permission warning");
            waitPermissionRotation(CONTROLLER,changedRotation,previousWarningWindow);
            waitPackage(CONTROLLER);nativeRoleButton("android:id/button1");
            state=permissionFixture(LEGACY_PERMISSION_FIXTURE);permissionMode(state,"CAMERA",4);
            click(nativeRoleButton("android:id/button2"));pane("Camera permissions");
            state=permissionFixture(LEGACY_PERMISSION_FIXTURE);permissionMode(state,"CAMERA",4);
        }finally{
            automation.setRotation(UiAutomation.ROTATION_UNFREEZE);
            restorePermissionRotation("user_rotation",rotation);restorePermissionRotation("accelerometer_rotation",automatic);
        }
        waitPermissionRotation(HOME,originalRotation,-1);
        check(setting("system","user_rotation").equals(rotation)&&setting("system","accelerometer_rotation").equals(automatic),"Permission warning rotation was not restored");
        permissionStage(result,"legacy_warning_reinstall");
        click(permissionChoice("Camera","Don't allow",false));waitPackage(CONTROLLER);
        AccessibilityNodeInfo stale=nativeRoleButton("android:id/button1");
        check(shell("pm uninstall "+LEGACY_PERMISSION_FIXTURE).contains("Success"),"Cannot remove permission fixture during warning");
        check(shell("pm install --bypass-low-target-sdk-block /data/local/tmp/octosense-permission-legacy.apk").contains("Success"),"Cannot reinstall permission fixture during warning");
        // A stale Android node may already be retired. Neither case permits the old grant target.
        stale.performAction(AccessibilityNodeInfo.ACTION_CLICK);pane("Camera permissions");
        state=permissionFixture(LEGACY_PERMISSION_FIXTURE);permissionGrant(state,"CAMERA",true);permissionMode(state,"CAMERA",4);
        permissionStage(result,"legacy_warning_confirm");
        permissionPage(LEGACY_PERMISSION_FIXTURE);permissionGroup("Camera");
        click(permissionChoice("Camera","Don't allow",false));waitPackage(CONTROLLER);click(nativeRoleButton("android:id/button1"));
        selectedPermission("Camera","Don't allow");state=permissionFixture(LEGACY_PERMISSION_FIXTURE);
        permissionGrant(state,"CAMERA",true);permissionMode(state,"CAMERA",1);
        check(permissionFlags(LEGACY_PERMISSION_FIXTURE,"CAMERA").contains("REVOKED_COMPAT"),"Legacy revoke did not apply native AppOp compatibility state");
        choosePermission("Camera","Allow only while using the app");state=permissionFixture(LEGACY_PERMISSION_FIXTURE);permissionMode(state,"CAMERA",4);
        check(!permissionFlags(LEGACY_PERMISSION_FIXTURE,"CAMERA").contains("REVOKED_COMPAT"),"Legacy allow did not clear compatibility denial");
        permissionGrant(permissionFixture(PERMISSION_FIXTURE),"VIBRATE",true);
        result.putBoolean("legacy_warning_cancel_back_and_confirm",true);
        result.putBoolean("permission_warning_rotation_preserves_choice",true);
        result.putBoolean("reinstalled_permission_target_denied",true);
        permissionStage(result,"complete");
        openRoute("overview","OctoSense Settings");
    }
    private void permissionsUnavailable(Bundle result){
        String before=shell("dumpsys package "+HOME);
        permissionPage(HOME);
        waitNode(node->role(node,"TextView")&&label(node).contains("Permission controls are unavailable on this installation"),"unavailable permission controller");
        check(nodes().stream().noneMatch(node->role(node,"Button")&&node.isEnabled()&&permissionLabel(node).contains(": Allow")),"Unavailable permission service offered a grant");
        shell("am start -W -a android.settings.SOUND_SETTINGS -p com.android.settings");
        shell("am start -W -n "+HOME+"/.MakepadApp");pane("App permissions");
        click(enabledNotificationButton("Android app settings"));waitPackage("com.android.settings");shell("input keyevent 4");pane("App permissions");
        click(button("Back"));pane("App details");click(button("Back"));pane("Apps");
        waitNode(node->role(node,"EditText")&&value(node.getText()).equals(HOME),"Apps filter after unavailable permission Back");
        String after=shell("dumpsys package "+HOME);
        for(String permission:new String[]{"CAMERA","RECORD_AUDIO","ACCESS_COARSE_LOCATION","ACCESS_FINE_LOCATION"}){
            Matcher first=Pattern.compile("android\\.permission\\."+permission+": granted=(true|false), flags=\\[([^]]*)\\]").matcher(before);
            Matcher last=Pattern.compile("android\\.permission\\."+permission+": granted=(true|false), flags=\\[([^]]*)\\]").matcher(after);
            check(first.find()&&last.find()&&first.group().equals(last.group()),"Unavailable navigation changed "+permission);
        }
        result.putBoolean("permission_service_unavailable",true);result.putBoolean("native_app_settings_recovery",true);
    }
    interface NotificationMatch {boolean accepts(JSONObject state)throws Exception;}
    private JSONObject notificationFixture(String pkg,String operation)throws Exception {
        if(!pkg.equals(NOTIFICATION_FIXTURE)&&!pkg.equals(LEGACY_NOTIFICATION_FIXTURE))throw new AssertionError("Not a disposable notification publisher");
        if(!operation.matches("state|post|cancel|delete|recreate"))throw new AssertionError("Unknown notification fixture operation");
        String report=shell("am instrument -w -e operation "+operation+" "+pkg+"/dev.makepad.octosense.notificationfixture.NotificationFixture");
        check(report.contains("INSTRUMENTATION_RESULT: passed=true"),"Notification fixture operation failed: "+operation);
        for(String line:report.split("\\r?\\n"))if(line.startsWith("INSTRUMENTATION_RESULT: state="))return new JSONObject(line.substring("INSTRUMENTATION_RESULT: state=".length()));
        throw new AssertionError("Notification fixture did not return observed state");
    }
    private JSONObject waitNotification(String pkg,NotificationMatch match,String description)throws Exception {
        long deadline=SystemClock.uptimeMillis()+15_000;JSONObject state;
        do {
            state=notificationFixture(pkg,"state");if(match.accepts(state)){checks++;return state;}
            SystemClock.sleep(100);
        }while(SystemClock.uptimeMillis()<deadline);
        throw new AssertionError("Notification state did not become "+description);
    }
    private void notificationDelivery(String pkg,boolean delivered)throws Exception {
        notificationFixture(pkg,"post");
        waitNotification(pkg,state->state.getJSONArray("active").length()==(delivered?1:0),delivered?"delivered":"blocked");
        notificationFixture(pkg,"cancel");
    }
    private void unchangedNotificationFields(JSONObject original,JSONObject observed,boolean soundMayChange)throws Exception {
        for(String field:new String[]{"name","description","group","badge","vibrate","lights","light_color","visibility","bubbles","bypass_dnd","pattern","sound"}) {
            if(soundMayChange&&field.equals("sound"))continue;
            check(String.valueOf(original.opt(field)).equals(String.valueOf(observed.opt(field))),"Unedited notification field changed: "+field);
        }
    }
    private void notificationAppDetails(String pkg) {
        openRoute("overview","OctoSense Settings");openRoute("apps","Apps");
        AccessibilityNodeInfo editor=waitNode(node->role(node,"EditText")&&label(node).equals("Search installed apps")&&node.isEnabled(),"Apps filter");
        check(editor.performAction(AccessibilityNodeInfo.ACTION_FOCUS),"Apps filter focus was not queued");waitKeyboard(true);
        setText(editor,pkg);waitNode(node->role(node,"EditText")&&value(node.getText()).equals(pkg),"exact app package filter");
        shell("input keyevent 4");waitKeyboard(false);
        // ACTION_CLICK returns when queued. Wait for the same-page Search
        // acknowledgment before examining rows, so the previous unfiltered
        // catalog cannot supply an apparently matching but retired target.
        clickAndWaitForAcknowledgment(button("Search"));
        // The synthetic publisher package is unique; the Home package is a
        // prefix of helper/fixture packages and can legitimately match more.
        if(!HOME.equals(pkg))waitNode(node->role(node,"TextView")&&label(node).equals("Apps 1–1 of 1"),"completed exact-package catalog");
        Match exact=node->role(node,"Button")&&node.isEnabled()
                &&(label(node).endsWith("\n"+pkg)||label(node).contains("\n"+pkg+" · "));
        AccessibilityNodeInfo target=waitNode(exact,"observed exact app package");
        for(int attempt=0;attempt<2;attempt++) {
            click(target);long deadline=SystemClock.uptimeMillis()+10_000;
            AccessibilityNodeInfo replacement=null;
            do {
                List<AccessibilityNodeInfo> current=nodes();
                if(current.stream().anyMatch(node->role(node,"TextView")&&label(node).equals("App details"))){checks++;return;}
                // The cached Android tree can outlive a filtered-catalog
                // generation for one publication interval. Retry navigation
                // only after the queued target actually retires and a new
                // semantic node for the same exact package is observed.
                for(AccessibilityNodeInfo node:current)if(exact.accepts(node)&&!node.equals(target))replacement=node;
                if(replacement!=null&&!target.refresh())break;
                replacement=null;SystemClock.sleep(100);
            }while(SystemClock.uptimeMillis()<deadline);
            if(replacement==null)throw new AssertionError("App details navigation did not complete for the current app row");
            target=replacement;navigationRetries++;
        }
        throw new AssertionError("App catalog repeatedly retired its navigation target");
    }
    private void notificationPage(String pkg) {
        notificationAppDetails(pkg);click(enabledNotificationButton("Notification settings"));pane("App notifications");
        waitNode(node->role(node,"Button")&&node.isEnabled()
                &&(label(node).equals("Turn app notifications off")||label(node).equals("Turn app notifications on")),"fresh app notification capability");
    }
    private AccessibilityNodeInfo enabledNotificationButton(String name) {
        AccessibilityNodeInfo node=revealButton(name);
        return node.isEnabled()?node:waitNode(candidate->role(candidate,"Button")&&label(candidate).equals(name)&&candidate.isEnabled(),"enabled "+name);
    }
    private void notificationRow(String pkg,String row,boolean secondPage) {
        notificationPage(pkg);
        if(secondPage) {
            click(enabledNotificationButton("Next"));
            waitNode(node->role(node,"Button")&&label(node).equals("Previous")&&node.isEnabled(),"second notification page");
        }
        click(reveal(node->role(node,"Button")&&label(node).startsWith(row)&&node.isEnabled(),row));pane("Notification channel");
    }
    private void notificationReview(String button) {
        click(enabledNotificationButton(button));
        waitNode(node->role(node,"Button")&&label(node).equals("Save notification change")&&node.isEnabled(),"reviewed notification Save");
    }
    private void saveNotification() {click(enabledNotificationButton("Save notification change"));}
    private int channelUserLocks() {
        String dump=shell("dumpsys notification --package "+NOTIFICATION_FIXTURE);
        boolean preferences=false,packages=false;
        for(String line:dump.split("\\r?\\n")) {
            String heading=line.trim();
            if(heading.equals("Notification Preferences:"))preferences=true;
            else if(preferences&&heading.equals("PackagePreferences:"))packages=true;
            else if(packages&&heading.equals("Restored without uid:"))break;
            if(!packages||!line.contains("mId='probe_channel_00'"))continue;
            // NotificationChannel.getFieldsString renders this bitset in hex.
            // Restrict inspection to current preferences, excluding historical
            // NotificationRecords that may contain a previous channel copy.
            java.util.regex.Matcher flags=java.util.regex.Pattern
                    .compile("mUserLockedFields=([0-9a-fA-F]+)").matcher(line);
            if(flags.find())return Integer.parseInt(flags.group(1),16);
        }
        throw new AssertionError("Synthetic channel user-lock flags unavailable");
    }
    private void waitRetiredNotificationSave(AccessibilityNodeInfo save) {
        long deadline=SystemClock.uptimeMillis()+15_000;
        do {
            if(!save.refresh()||!save.isEnabled()) {
                check(!save.performAction(AccessibilityNodeInfo.ACTION_CLICK),"Retired notification review remained actionable");return;
            }
            SystemClock.sleep(100);
        }while(SystemClock.uptimeMillis()<deadline);
        throw new AssertionError("Deleted channel did not retire its open review");
    }
    private void notificationChannels(Bundle result)throws Exception {
        check(shell("getprop ro.kernel.qemu").trim().equals("1"),"Notification acceptance requires an emulator");
        JSONObject initial=notificationFixture(NOTIFICATION_FIXTURE,"state");
        check(initial.getBoolean("enabled")&&initial.getInt("channel_count")==24&&initial.getInt("group_count")==2,"Synthetic notification setup mismatch");
        JSONObject original=initial.getJSONObject("channel");int originalLocks=channelUserLocks();
        try {
            notificationDelivery(NOTIFICATION_FIXTURE,true);
            notificationPage(NOTIFICATION_FIXTURE);
            notificationReview("Turn app notifications off");click(button("Cancel notification change"));
            check(notificationFixture(NOTIFICATION_FIXTURE,"state").getBoolean("enabled"),"Cancel changed app notification permission");
            notificationReview("Turn app notifications off");saveNotification();
            waitNotification(NOTIFICATION_FIXTURE,state->!state.getBoolean("enabled"),"app notifications Off");
            check(shell("dumpsys package "+NOTIFICATION_FIXTURE).contains("android.permission.POST_NOTIFICATIONS: granted=false"),"App Off did not revoke the actual notification permission");
            notificationDelivery(NOTIFICATION_FIXTURE,false);
            // A previous user denial is not an administrator policy lock.
            shell("pm set-permission-flags "+NOTIFICATION_FIXTURE+" android.permission.POST_NOTIFICATIONS user-fixed");
            notificationPage(NOTIFICATION_FIXTURE);notificationReview("Turn app notifications on");saveNotification();
            waitNotification(NOTIFICATION_FIXTURE,state->state.getBoolean("enabled"),"app notifications On after user-fixed denial");
            notificationDelivery(NOTIFICATION_FIXTURE,true);result.putBoolean("app_permission_delivery",true);

            notificationPage(NOTIFICATION_FIXTURE);click(enabledNotificationButton("Next"));
            waitNode(node->role(node,"Button")&&label(node).equals("Previous")&&node.isEnabled(),"notification page 2");
            reveal(node->role(node,"Button")&&label(node).startsWith("Channel: Probe channel 23"),"last synthetic channel");
            click(enabledNotificationButton("Previous"));
            waitNode(node->role(node,"Button")&&label(node).equals("Next")&&node.isEnabled(),"restored notification page 1");
            reveal(node->role(node,"Button")&&label(node).startsWith("Channel: Probe channel 00"),"first synthetic channel");
            result.putBoolean("paging",true);

            notificationRow(NOTIFICATION_FIXTURE,"Channel: Probe channel 00",false);
            notificationReview("Turn channel notifications off");saveNotification();
            waitNotification(NOTIFICATION_FIXTURE,state->state.getJSONObject("channel").getInt("importance")==0,"channel Off");
            notificationDelivery(NOTIFICATION_FIXTURE,false);
            notificationReview("Turn channel notifications on");saveNotification();
            waitNotification(NOTIFICATION_FIXTURE,state->state.getJSONObject("channel").getInt("importance")==2,"native original channel importance");
            notificationDelivery(NOTIFICATION_FIXTURE,true);
            for(String[] choice:new String[][]{{"Silent, minimized","1"},{"Silent","2"}}) {
                notificationReview(choice[0]);saveNotification();
                JSONObject state=waitNotification(NOTIFICATION_FIXTURE,s->s.getJSONObject("channel").getInt("importance")==Integer.parseInt(choice[1]),choice[0]);
                unchangedNotificationFields(original,state.getJSONObject("channel"),false);
            }
            notificationReview("Alerting, allow pop-up");
            waitNode(node->role(node,"TextView")&&label(node).toLowerCase().contains("default notification sound"),"review disclosure of restored sound");
            click(button("Cancel notification change"));
            JSONObject canceled=notificationFixture(NOTIFICATION_FIXTURE,"state").getJSONObject("channel");
            check(canceled.getInt("importance")==2&&canceled.isNull("sound"),"Importance Cancel changed sound or importance");
            notificationReview("Alerting, allow pop-up");saveNotification();
            JSONObject alerting=waitNotification(NOTIFICATION_FIXTURE,s->s.getJSONObject("channel").getInt("importance")==4,"high importance").getJSONObject("channel");
            check(alerting.getString("sound").equals("content://settings/system/notification_sound"),"Reviewed alerting did not restore default sound");
            unchangedNotificationFields(original,alerting,true);
            int locks=channelUserLocks();check((locks&36)==36&&(locks&~36)==(originalLocks&~36),
                    "Importance/sound user-lock bits: expected 0x"+Integer.toHexString(originalLocks|36)+" but observed 0x"+Integer.toHexString(locks));
            notificationReview("Silent");saveNotification();
            JSONObject silent=waitNotification(NOTIFICATION_FIXTURE,s->s.getJSONObject("channel").getInt("importance")==2,"silent importance").getJSONObject("channel");
            unchangedNotificationFields(alerting,silent,false);result.putBoolean("importance_review_preservation",true);

            notificationRow(NOTIFICATION_FIXTURE,"Group: Probe group A",true);
            notificationReview("Turn group notifications off");saveNotification();
            waitNotification(NOTIFICATION_FIXTURE,s->s.getBoolean("group_blocked"),"group blocked");notificationDelivery(NOTIFICATION_FIXTURE,false);
            notificationReview("Turn group notifications on");saveNotification();
            waitNotification(NOTIFICATION_FIXTURE,s->!s.getBoolean("group_blocked"),"group allowed");notificationDelivery(NOTIFICATION_FIXTURE,true);
            result.putBoolean("channel_group_delivery",true);

            notificationRow(NOTIFICATION_FIXTURE,"Channel: Probe channel 00",false);
            notificationReview("Alerting");AccessibilityNodeInfo staleSave=button("Save notification change");
            notificationFixture(NOTIFICATION_FIXTURE,"delete");waitRetiredNotificationSave(staleSave);
            notificationFixture(NOTIFICATION_FIXTURE,"recreate");
            check(!staleSave.performAction(AccessibilityNodeInfo.ACTION_CLICK),"Recreated channel rebound a retired review");
            result.putBoolean("deleted_target_denial",true);

            JSONObject legacy=notificationFixture(LEGACY_NOTIFICATION_FIXTURE,"state");
            check(legacy.getBoolean("enabled")&&legacy.getInt("channel_count")==1&&legacy.getJSONObject("channel").getInt("importance")==-1000,"Legacy fixture lacks platform default channel");
            notificationPage(LEGACY_NOTIFICATION_FIXTURE);notificationReview("Turn app notifications off");saveNotification();
            waitNotification(LEGACY_NOTIFICATION_FIXTURE,s->!s.getBoolean("enabled")&&s.getJSONObject("channel").getInt("importance")==0,"legacy app and channel Off");
            notificationDelivery(LEGACY_NOTIFICATION_FIXTURE,false);
            notificationReview("Turn app notifications on");saveNotification();
            waitNotification(LEGACY_NOTIFICATION_FIXTURE,s->s.getBoolean("enabled")&&s.getJSONObject("channel").getInt("importance")==-1000,"legacy app default restored");
            notificationRow(LEGACY_NOTIFICATION_FIXTURE,"Channel: "+legacy.getJSONObject("channel").getString("name"),false);
            notificationReview("Turn channel notifications off");saveNotification();
            waitNotification(LEGACY_NOTIFICATION_FIXTURE,s->!s.getBoolean("enabled")&&s.getJSONObject("channel").getInt("importance")==0,"legacy channel and app Off");
            notificationDelivery(LEGACY_NOTIFICATION_FIXTURE,false);
            notificationReview("Turn channel notifications on");saveNotification();
            waitNotification(LEGACY_NOTIFICATION_FIXTURE,s->s.getBoolean("enabled")&&s.getJSONObject("channel").getInt("importance")==-1000,"legacy channel and app restored");
            notificationDelivery(LEGACY_NOTIFICATION_FIXTURE,true);result.putBoolean("legacy_coupling",true);
            click(button("Back"));pane("App notifications");click(button("Back"));pane("App details");click(button("Back"));pane("Apps");
            waitNode(node->role(node,"EditText")&&value(node.getText()).equals(LEGACY_NOTIFICATION_FIXTURE),"preserved Apps filter after nested Back");
            result.putBoolean("nested_back",true);
        }finally {
            notificationFixture(NOTIFICATION_FIXTURE,"cancel");notificationFixture(LEGACY_NOTIFICATION_FIXTURE,"cancel");
        }
        openRoute("overview","OctoSense Settings");
    }
    private void notificationUnavailable(Bundle result) {
        check(shell("getprop ro.kernel.qemu").trim().equals("1"),"Ordinary-install notification probe requires an emulator");
        notificationAppDetails(HOME);click(enabledNotificationButton("Notification settings"));pane("App notifications");
        waitNode(node->role(node,"TextView")&&label(node).startsWith("App notification controls are unavailable on this installation."),"ordinary-install unavailable state");
        AccessibilityNodeInfo toggle=revealButton("Turn app notifications on");
        check(!toggle.isEnabled(),"Ordinary installation enabled a notification mutation");
        check(!toggle.performAction(AccessibilityNodeInfo.ACTION_CLICK),"Unavailable notification action was accepted");
        check(nodes().stream().noneMatch(node->role(node,"Button")&&label(node).startsWith("Channel: ")),"Unavailable state retained channel targets");
        click(enabledNotificationButton("Android notification settings"));waitPackage("com.android.settings");
        shell("input keyevent 4");pane("App notifications");
        click(button("Back"));pane("App details");click(button("Back"));pane("Apps");
        waitNode(node->role(node,"EditText")&&value(node.getText()).equals(HOME),"preserved app filter after native recovery and nested Back");
        openRoute("overview","OctoSense Settings");
        result.putBoolean("unavailable_denial",true);result.putBoolean("native_recovery",true);result.putBoolean("nested_back",true);
    }
    private void notificationEntry(String pkg,boolean cold) {
        if(cold)shell("am force-stop "+HOME);
        shell("am start -W -a android.settings.APP_NOTIFICATION_SETTINGS -p "+HOME
                +" --es android.provider.extra.APP_PACKAGE "+pkg
                +" --ei app_uid 12345 --ei android.intent.extra.UID 12345"
                +" --es android.provider.extra.CHANNEL_ID forged_channel"
                +" --es dev.makepad.octosense.extra.SETTINGS_ROUTE updates --es operation app_enabled --es value off");
        pane("App notifications");
        waitNode(node->role(node,"TextView")&&label(node).contains(pkg),"selected notification package");
        enabledNotificationButton("Android notification settings");
    }
    private void notificationEntries(Bundle result) {
        check(shell("getprop ro.kernel.qemu").trim().equals("1"),"Notification entry probe requires an emulator");
        String before=shell("dumpsys package "+HOME);
        boolean granted=before.contains("android.permission.POST_NOTIFICATIONS: granted=true");
        // Preserve a pre-existing native Settings task while testing recovery.
        shell("am start -W -a android.settings.SOUND_SETTINGS -p com.android.settings");
        notificationEntry(HOME,true);
        click(enabledNotificationButton("Android notification settings"));waitPackage("com.android.settings");
        shell("input keyevent 4");pane("App notifications");
        click(button("Back"));pane("App details");
        reveal(node->role(node,"TextView")&&label(node).endsWith("\n"+HOME),"fresh Home app identity");
        click(button("Back"));pane("Apps");
        shell("am start -W -a android.settings.SETTINGS -p com.android.settings");
        shell("am start -W -n "+HOME+"/.MakepadApp");pane("Apps");
        result.putBoolean("cold_entry_and_nonreplay",true);result.putBoolean("native_recovery",true);

        notificationEntry("com.android.settings",false);
        click(button("Back"));pane("App details");
        reveal(node->role(node,"TextView")&&label(node).endsWith("\ncom.android.settings"),"fresh system Settings app identity");
        result.putBoolean("warm_target_replacement",true);

        String missing="dev.makepad.octosense.missing_notification_entry";
        check(shell("pm list packages "+missing).trim().isEmpty(),"Missing-app selector unexpectedly installed");
        shell("am start -W -a android.settings.APP_NOTIFICATION_SETTINGS -p "+HOME+" --es android.provider.extra.APP_PACKAGE "+missing);
        pane("App details");
        reveal(node->role(node,"TextView")&&label(node).startsWith("This app is no longer installed for the current user.")
                &&label(node).endsWith("\n"+missing),"observed missing notification target");
        for(String name:new String[]{"Launch","Uninstall…","Notification settings"}) {
            AccessibilityNodeInfo target=revealButton(name);
            check(!target.isEnabled(),"Missing app retained enabled "+name);
            check(!target.performAction(AccessibilityNodeInfo.ACTION_CLICK),"Missing app retained actionable "+name);
        }
        click(button("Back"));pane("Apps");result.putBoolean("missing_target_denial",true);

        openRoute("overview","OctoSense Settings");
        String base="am start -W -n "+HOME+"/.MakepadApp -a android.settings.APP_NOTIFICATION_SETTINGS";
        for(String extra:new String[]{"", " --es android.provider.extra.APP_PACKAGE invalid/package",
                " --ei android.provider.extra.APP_PACKAGE 123", " --es android.provider.extra.APP_PACKAGE "+"a".repeat(256),
                " --es android.provider.extra.APP_PACKAGE "+HOME+" -d package:"+HOME}) {
            shell(base+extra);SystemClock.sleep(600);pane("OctoSense Settings");
        }
        shell("am start -W -a android.settings.DISPLAY_SETTINGS -p "+HOME+" --es android.provider.extra.APP_PACKAGE "+HOME);
        pane("Display");
        shell("am start -W -a dev.makepad.octosense.action.SETTINGS -p "+HOME
                +" --es dev.makepad.octosense.extra.SETTINGS_ROUTE app_notifications --es android.provider.extra.APP_PACKAGE "+HOME);
        SystemClock.sleep(600);pane("Display");
        check(shell("dumpsys package "+HOME).contains("android.permission.POST_NOTIFICATIONS: granted=true")==granted,
                "Navigation extras changed Home notification permission");
        result.putBoolean("malformed_entry_denial",true);result.putBoolean("untrusted_extras_not_commands",true);
        openRoute("overview","OctoSense Settings");
    }
    private static final String ROLE_BROWSER_A="dev.makepad.octosense.rolebrowserfixture";
    private static final String ROLE_BROWSER_B="dev.makepad.octosense.rolebrowsersecondfixture";
    private static final String ROLE_ASSISTANT="dev.makepad.octosense.roleassistantfixture";
    private static final String CONTROLLER="com.android.permissioncontroller";
    private String browserHolder(){return shell("cmd role get-role-holders --user 0 android.app.role.BROWSER").trim();}
    private String assistantHolder(){return shell("cmd role get-role-holders --user 0 android.app.role.ASSISTANT").trim();}
    private void waitAssistant(String expected){
        long deadline=SystemClock.uptimeMillis()+10_000;
        do{if(assistantHolder().equals(expected)){checks++;return;}SystemClock.sleep(100);}while(SystemClock.uptimeMillis()<deadline);
        throw new AssertionError("Assistant holder differs from reviewed selection");
    }
    private void waitBrowser(String expected){
        long deadline=SystemClock.uptimeMillis()+10_000;
        do{if(browserHolder().equals(expected)){checks++;return;}SystemClock.sleep(100);}while(SystemClock.uptimeMillis()<deadline);
        throw new AssertionError("Browser holder differs from reviewed selection: "+browserHolder());
    }
    private AccessibilityNodeInfo nativeRoleButton(String id){
        long deadline=SystemClock.uptimeMillis()+10_000;
        do{
            AccessibilityNodeInfo root=automation.getRootInActiveWindow();ArrayList<AccessibilityNodeInfo> current=new ArrayList<>();
            if(root!=null&&CONTROLLER.equals(value(root.getPackageName())))collect(root,current,0);
            for(AccessibilityNodeInfo node:current)if(id.equals(node.getViewIdResourceName())&&node.isEnabled())return node;
            SystemClock.sleep(100);
        }while(SystemClock.uptimeMillis()<deadline);
        throw new AssertionError("Missing native role confirmation "+id);
    }
    private void browserPage(){
        openRoute("default_apps","Default apps");
        click(reveal(node->role(node,"Button")&&label(node).startsWith("Default browser app\n")&&node.isEnabled(),"Browser role"));pane("Choose default app");
    }
    private void rolesTop(){
        for(int i=0;i<20;i++){
            AccessibilityNodeInfo scroll=null;for(AccessibilityNodeInfo node:nodes())if(role(node,"ScrollView")&&action(node,AccessibilityNodeInfo.ACTION_SCROLL_BACKWARD)){scroll=node;break;}
            if(scroll==null)return;check(scroll.performAction(AccessibilityNodeInfo.ACTION_SCROLL_BACKWARD),"Cannot reveal earlier role choices");SystemClock.sleep(150);
        }
        throw new AssertionError("Role choices did not reach top");
    }
    private AccessibilityNodeInfo browserCandidate(String pkg,boolean selected){
        rolesTop();
        return reveal(node->role(node,"Button")&&label(node).contains("\n"+pkg+"\n")
                &&(selected?label(node).endsWith("Selected"):node.isEnabled()),"browser candidate "+pkg);
    }
    private void reviewBrowser(String pkg){click(browserCandidate(pkg,false));waitPackage(CONTROLLER);nativeRoleButton("android:id/button1");}
    private void confirmBrowser(String pkg){reviewBrowser(pkg);click(nativeRoleButton("android:id/button1"));pane("Choose default app");waitBrowser(pkg);browserCandidate(pkg,true);}
    private void assistantPage(){
        rolesTop();click(reveal(node->role(node,"Button")&&label(node).startsWith("Default digital assistant app\n")&&node.isEnabled(),"Assistant role"));pane("Choose default app");
    }
    private void clearAssistant(){
        rolesTop();click(enabledNotificationButton("Choose none…"));waitPackage(CONTROLLER);click(nativeRoleButton("android:id/button1"));pane("Choose default app");waitAssistant("");
    }
    private String nativeDefaultsPackage(){
        String resolved=shell("cmd package resolve-activity --brief --user 0 -a android.settings.MANAGE_DEFAULT_APPS_SETTINGS").trim();
        String nativeController=null;
        for(String pkg:new String[]{CONTROLLER,"com.google.android.permissioncontroller"})
            if(resolved.endsWith(pkg+"/com.android.permissioncontroller.role.ui.DefaultAppListActivity"))nativeController=pkg;
        if(resolved.endsWith(CONTROLLER+"/.role.ui.DefaultAppListActivity"))nativeController=CONTROLLER;
        check(nativeController!=null,"Unexpected native default-app list component: "+resolved);
        return nativeController;
    }
    private void rolesUnavailable(Bundle result){
        String nativeController=nativeDefaultsPackage();
        openRoute("default_apps","Default apps");
        waitNode(node->role(node,"TextView")&&label(node).contains("Default-app choices are unavailable on this installation"),"unavailable role service");
        check(nodes().stream().noneMatch(node->role(node,"Button")&&node.isEnabled()&&label(node).endsWith("Choose…")),"Unavailable role service offered a write");
        shell("am start -W -a android.settings.MANAGE_DEFAULT_APPS_SETTINGS -p "+nativeController);waitPackage(nativeController);
        openRoute("default_apps","Default apps");click(revealButton("Android default-app settings"));waitPackage(nativeController);
        shell("input keyevent 4");pane("Default apps");click(button("Back"));pane("Apps");
        result.putBoolean("role_service_unavailable",true);result.putBoolean("native_defaults_recovery",true);
    }
    private String roleHoldersSnapshot(){
        StringBuilder snapshot=new StringBuilder();
        for(String name:new String[]{"BROWSER","HOME","ASSISTANT","DIALER","SMS","CALL_SCREENING","CALL_REDIRECTION","WALLET"})
            snapshot.append(name).append(':').append(shell("cmd role get-role-holders --user 0 android.app.role."+name).trim()).append('\n');
        return snapshot.toString();
    }
    private void rolesEntries(Bundle result){
        String before=roleHoldersSnapshot();
        String base="am start -W -n "+HOME+"/.MakepadApp -a android.settings.MANAGE_DEFAULT_APPS_SETTINGS";
        String extras=" --es android.app.role.extra.ROLE_NAME android.app.role.HOME --es android.intent.extra.PACKAGE_NAME "+HOME
                +" --es android.provider.extra.APP_PACKAGE invalid/package --ei android.intent.extra.USER_ID 10 --ei android.intent.extra.UID 1000"
                +" --es dev.makepad.octosense.extra.SETTINGS_ROUTE sound --es target invalid --es key invalid --es value invalid";
        shell("am force-stop "+HOME);shell(base+extras);pane("Default apps");
        check(roleHoldersSnapshot().equals(before),"Cold default-app entry extras changed a role");
        click(button("Back"));pane("Apps");
        shell("am start -W -a android.settings.SOUND_SETTINGS -p com.android.settings");
        shell("am start -W -n "+HOME+"/.MakepadApp");pane("Apps");
        shell(base+extras);pane("Default apps");
        result.putBoolean("default_entry_cold_warm_nonreplay",true);
        openRoute("overview","OctoSense Settings");
        for(String malformed:new String[]{" -d package:"+HOME," -t text/plain"}){
            shell(base+malformed);SystemClock.sleep(600);pane("OctoSense Settings");
        }
        shell("am start -W -a dev.makepad.octosense.action.SETTINGS -p "+HOME+" --es dev.makepad.octosense.extra.SETTINGS_ROUTE role_choices");
        SystemClock.sleep(600);pane("OctoSense Settings");
        check(roleHoldersSnapshot().equals(before),"Default-app navigation mutated role state");
        result.putBoolean("role_extras_are_not_commands",true);result.putBoolean("malformed_role_entry_denied",true);
    }
    private void defaultRoles(Bundle result)throws Exception{
        String original=browserHolder();check(original.equals("org.chromium.webview_shell"),"Unexpected disposable emulator browser baseline");
        check(assistantHolder().isEmpty(),"Unexpected disposable emulator assistant baseline");
        try{
            // An older controller task must not capture the confirmation Back.
            shell("am start -W -a android.settings.MANAGE_DEFAULT_APPS_SETTINGS -p "+CONTROLLER);waitPackage(CONTROLLER);
            openRoute("apps","Apps");
            AccessibilityNodeInfo editor=waitNode(node->role(node,"EditText")&&label(node).equals("Search installed apps")&&node.isEnabled(),"Apps filter");
            check(editor.performAction(AccessibilityNodeInfo.ACTION_FOCUS),"Apps filter focus failed");waitKeyboard(true);setText(editor,"rolebrowser");
            shell("input keyevent 4");waitKeyboard(false);clickAndWaitForAcknowledgment(button("Search"));
            click(revealButton("Default apps"));pane("Default apps");
            click(reveal(node->role(node,"Button")&&label(node).startsWith("Default browser app\n")&&node.isEnabled(),"Browser role"));pane("Choose default app");
            AccessibilityNodeInfo current=browserCandidate(original,true);
            check(!current.isEnabled()&&!current.performAction(AccessibilityNodeInfo.ACTION_CLICK),"Current browser remained selectable");
            reviewBrowser(ROLE_BROWSER_A);waitBrowser(original);click(nativeRoleButton("android:id/button2"));pane("Choose default app");waitBrowser(original);
            reviewBrowser(ROLE_BROWSER_A);shell("input keyevent 4");pane("Choose default app");waitBrowser(original);
            confirmBrowser(ROLE_BROWSER_A);
            String first=shell("am instrument -w "+ROLE_BROWSER_A+"/"+ROLE_BROWSER_A+".RoleBrowserProbe");
            check(first.contains("INSTRUMENTATION_RESULT: held=true")&&first.contains("INSTRUMENTATION_RESULT: passed=true"),"Selected browser did not observe its own public role");
            reviewBrowser(ROLE_BROWSER_B);
            check(shell("pm uninstall "+ROLE_BROWSER_B).contains("Success"),"Could not remove disposable candidate during review");
            check(shell("pm install /data/local/tmp/octosense-role-browser-b.apk").contains("Success"),"Could not reinstall disposable candidate during review");
            click(nativeRoleButton("android:id/button1"));pane("Choose default app");waitBrowser(ROLE_BROWSER_A);
            rolesTop();click(enabledNotificationButton("Refresh choices"));browserCandidate(ROLE_BROWSER_B,false);confirmBrowser(ROLE_BROWSER_B);
            String second=shell("am instrument -w "+ROLE_BROWSER_B+"/"+ROLE_BROWSER_A+".RoleBrowserProbe");
            check(second.contains("INSTRUMENTATION_RESULT: held=true")&&second.contains("INSTRUMENTATION_RESULT: passed=true"),"Second browser did not observe its own role");
            // None is tested only if the native role actually offers it.
            rolesTop();AccessibilityNodeInfo none=revealButton("Choose none…");
            if(none.isEnabled()){click(none);waitPackage(CONTROLLER);click(nativeRoleButton("android:id/button1"));pane("Choose default app");waitBrowser("");result.putBoolean("native_none",true);}
            confirmBrowser(original);
            click(revealButton("Android default-app settings"));waitPackage(CONTROLLER);shell("input keyevent 4");pane("Choose default app");
            click(button("Back"));pane("Default apps");assistantPage();
            click(browserCandidate(ROLE_ASSISTANT,false));waitPackage(CONTROLLER);click(nativeRoleButton("android:id/button1"));pane("Choose default app");waitAssistant(ROLE_ASSISTANT);
            String assistant=shell("am instrument -w -e role assistant "+ROLE_ASSISTANT+"/"+ROLE_BROWSER_A+".RoleBrowserProbe");
            check(assistant.contains("INSTRUMENTATION_RESULT: held=true")&&assistant.contains("INSTRUMENTATION_RESULT: role=android.app.role.ASSISTANT")&&assistant.contains("INSTRUMENTATION_RESULT: passed=true"),"Assistant did not observe its own public role");
            clearAssistant();result.putBoolean("native_none",true);result.putBoolean("assistant_role_readback",true);
            click(button("Back"));pane("Default apps");click(button("Back"));pane("Apps");
            waitNode(node->role(node,"EditText")&&value(node.getText()).equals("rolebrowser"),"Apps filter after nested role Back");
            result.putBoolean("native_qualification_and_consent",true);result.putBoolean("cancel_and_back_preserve_holder",true);
            result.putBoolean("reinstalled_candidate_denied",true);result.putBoolean("public_role_readback",true);result.putBoolean("role_nested_back",true);
        }finally{
            if(!assistantHolder().isEmpty()){openRoute("default_apps","Default apps");assistantPage();clearAssistant();}
            if(!browserHolder().equals(original)){browserPage();confirmBrowser(original);}
            waitBrowser(original);waitAssistant("");
        }
    }
    private int appNetworkUid() {
        Matcher m=Pattern.compile("uid:(\\d+)").matcher(shell("pm list packages -U dev.makepad.octosense.networkfixture"));
        if(!m.find())throw new AssertionError("Synthetic network UID missing");return Integer.parseInt(m.group(1));
    }
    private int appNetworkPolicy(int uid) {
        String dump=shell("dumpsys netpolicy");
        String section=dump.substring(dump.indexOf("Policy for UIDs:"),dump.indexOf("Power save whitelist"));
        Matcher m=Pattern.compile("UID="+uid+" policy=(\\d+)").matcher(section);return m.find()?Integer.parseInt(m.group(1)):0;
    }
    private void waitAppNetworkPolicy(int uid,int expected) {
        long deadline=SystemClock.uptimeMillis()+10000;
        do{if(appNetworkPolicy(uid)==expected){checks++;return;}SystemClock.sleep(100);}while(SystemClock.uptimeMillis()<deadline);
        throw new AssertionError("Native UID policy did not become "+expected);
    }
    private void appNetworkPage(String pkg){
        notificationAppDetails(pkg);click(reveal(n->role(n,"Button")&&label(n).equals("Network access")&&n.isEnabled(),"enabled Network access after app details settle"));pane("App network access");
    }
    private void appNetworkToggle(String label,boolean current,int uid,int policy){
        String name=label+": "+(current?"On":"Off");click(enabledNotificationButton(name));waitAppNetworkPolicy(uid,policy);
        waitNode(n->role(n,"Button")&&label(n).equals(label+": "+(current?"Off":"On")),"observed network change");
    }
    private void appNetwork(Bundle result){
        String pkg="dev.makepad.octosense.networkfixture";int uid=appNetworkUid();check(appNetworkPolicy(uid)==0,"Unexpected synthetic policy baseline");
        appNetworkPage(pkg);waitNode(n->role(n,"Button")&&label(n).equals("Background data: On")&&n.isEnabled(),"network write capability");
        for(String label:new String[]{"Network access","Wi-Fi access","Mobile data access","VPN access"}){
            AccessibilityNodeInfo unsupported=revealButton(label+": Unavailable");check(!unsupported.isEnabled(),"AOSP must not invent Lineage policy support");
        }
        appNetworkToggle("Background data",true,uid,1);
        AccessibilityNodeInfo unavailable=revealButton("Unrestricted data with Data Saver: Off");check(!unavailable.isEnabled(),"Exception must be disabled while background data is blocked");
        // Native enforcement is observed independently from Home's settings response.
        String dump=shell("dumpsys netpolicy");Matcher blocked=Pattern.compile("UID="+uid+" state=.*effective=([^}]+)").matcher(dump);
        check(blocked.find()&&blocked.group(1).contains("USER_RESTRICTED"),"Native policy did not enforce the metered background restriction");
        appNetworkToggle("Background data",false,uid,0);
        appNetworkToggle("Unrestricted data with Data Saver",false,uid,4);
        appNetworkToggle("Background data",true,uid,1); // also clears the allow bit, as native Settings does.
        appNetworkToggle("Background data",false,uid,0);
        appNetworkToggle("Unrestricted data with Data Saver",false,uid,4);
        shell("am force-stop "+HOME);appNetworkPage(pkg);
        waitNode(n->role(n,"Button")&&label(n).equals("Unrestricted data with Data Saver: On")&&n.isEnabled(),"policy persisted across Home restart");
        appNetworkToggle("Unrestricted data with Data Saver",true,uid,0);
        // External edits of the synthetic UID must be reflected by the active page.
        shell("cmd netpolicy add restrict-background-blacklist "+uid);waitAppNetworkPolicy(uid,1);
        click(revealButton("Refresh network access"));waitNode(n->role(n,"Button")&&label(n).equals("Background data: Off")&&n.isEnabled(),"external background restriction");
        appNetworkToggle("Background data",false,uid,0);
        click(button("Back"));pane("App details");click(button("Back"));pane("Apps");
        waitNode(n->role(n,"EditText")&&value(n.getText()).equals(pkg),"retained app filter after network Back");
        appNetworkPage(HOME);AccessibilityNodeInfo protectedRow=revealButton("Background data: On");check(!protectedRow.isEnabled(),"Settings host must remain protected");
        check(appNetworkPolicy(uid)==0,"Synthetic UID policy not restored");
        result.putBoolean("native_uid_policy",true);result.putBoolean("native_metered_enforcement",true);
        result.putBoolean("coupled_bits",true);result.putBoolean("home_restart_persistence",true);result.putBoolean("external_change_readback",true);
        result.putBoolean("lineage_unavailable_on_aosp",true);result.putBoolean("protected_host",true);
    }
    private void appNetworkUnavailable(Bundle result){
        appNetworkPage(HOME);
        waitNode(n->role(n,"TextView")&&label(n).equals("App network controls require the OctoSense system service."),"unavailable network service");
        for(String field:new String[]{"Background data","Unrestricted data with Data Saver","Network access","Wi-Fi access","Mobile data access","VPN access"}){
            AccessibilityNodeInfo node=revealButton(field+": Unavailable");
            check(!node.isEnabled()&&!node.performAction(AccessibilityNodeInfo.ACTION_CLICK),"Unavailable network control was actionable");
        }
        shell("am start -W -a android.settings.SOUND_SETTINGS -p com.android.settings");waitPackage("com.android.settings");
        shell("am start -W -n "+HOME+"/.MakepadApp");pane("App network access");
        click(enabledNotificationButton("Android app settings"));waitPackage("com.android.settings");shell("input keyevent 4");pane("App network access");
        click(button("Back"));pane("App details");click(button("Back"));pane("Apps");
        waitNode(n->role(n,"EditText")&&value(n.getText()).equals(HOME),"filter retained after network recovery");
        result.putBoolean("unavailable_denial",true);result.putBoolean("native_recovery",true);result.putBoolean("nested_back",true);
    }
    private static final String BATTERY_FIXTURE="dev.makepad.octosense.networkfixture";
    private boolean batteryAllowlisted(String pkg){
        for(String line:shell("cmd deviceidle whitelist").split("\\r?\\n"))if(line.trim().matches("[^,]+,"+Pattern.quote(pkg)+",[0-9]+"))return true;
        return false;
    }
    private String batteryOp(String pkg,String op){
        String response=shell("cmd appops get "+pkg+" "+op);
        Matcher mode=Pattern.compile(Pattern.quote(op)+": (allow|ignore|deny|default|foreground)").matcher(response);
        if(mode.find())return mode.group(1);
        if(response.contains("No operations."))return "default";
        throw new AssertionError("Native battery AppOp readback unavailable for "+op);
    }
    private void batteryPage(String pkg){notificationAppDetails(pkg);click(enabledNotificationButton("Battery usage"));pane("App battery usage");}
    private void waitBatteryPolicy(String pkg,String mode,boolean legacy){
        long deadline=SystemClock.uptimeMillis()+10000;
        do{
            String op=batteryOp(pkg,"RUN_ANY_IN_BACKGROUND");
            boolean correct=(mode.equals("Restricted")?op.equals("ignore"):op.equals("allow")||op.equals("default"))
                    &&batteryAllowlisted(pkg)==mode.equals("Unrestricted");
            if(legacy){String old=batteryOp(pkg,"RUN_IN_BACKGROUND");correct&=mode.equals("Restricted")?old.equals("ignore"):old.equals("allow")||old.equals("default");}
            if(correct){checks++;return;}SystemClock.sleep(100);
        }while(SystemClock.uptimeMillis()<deadline);
        throw new AssertionError("Native battery policy did not become "+mode);
    }
    private void batteryChoice(String pkg,String mode,boolean legacy){
        click(enabledNotificationButton("Battery policy: "+mode));
        if(mode.equals("Restricted")){
            waitNode(n->role(n,"TextView")&&label(n).startsWith("Restrict background activity"),"new restricted policy review");
            click(enabledNotificationButton("Battery policy: Confirm restricted battery policy"));
        }
        waitBatteryPolicy(pkg,mode,legacy);
        waitNode(n->role(n,"Button")&&label(n).equals("Battery policy: "+mode+" · Selected")&&!n.isEnabled(),"observed battery choice "+mode);
    }
    private void cancelBatteryReview(){
        click(enabledNotificationButton("Cancel battery policy change"));
        // performAction only queues a renderer action. Native policy remains
        // unchanged both before and after Cancel, so that readback cannot
        // acknowledge removal of the review before opening another one.
        long deadline=SystemClock.uptimeMillis()+10000;
        do{
            List<AccessibilityNodeInfo> visible=nodes();
            boolean observed=visible.stream().anyMatch(n->role(n,"TextView")&&label(n).startsWith("Observed policy:"));
            boolean review=visible.stream().anyMatch(n->role(n,"Button")&&label(n).equals("Cancel battery policy change"));
            if(observed&&!review){checks++;return;}SystemClock.sleep(100);
        }while(SystemClock.uptimeMillis()<deadline);
        throw new AssertionError("Battery review did not close after Cancel");
    }
    private void appBatteryUnavailable(Bundle result){
        batteryPage(HOME);
        waitNode(n->role(n,"TextView")&&label(n).contains("App battery controls are unavailable on this installation"),"unavailable battery service");
        for(String mode:new String[]{"Restricted","Optimized","Unrestricted"}){
            AccessibilityNodeInfo node=revealButton("Battery policy: "+mode);check(!node.isEnabled()&&!node.performAction(AccessibilityNodeInfo.ACTION_CLICK),"Unavailable battery control was actionable");
        }
        shell("am start -W -a android.settings.SOUND_SETTINGS -p com.android.settings");waitPackage("com.android.settings");
        shell("am start -W -n "+HOME+"/.MakepadApp");pane("App battery usage");
        click(enabledNotificationButton("Android app settings"));waitPackage("com.android.settings");shell("input keyevent 4");pane("App battery usage");
        click(button("Back"));pane("App details");click(button("Back"));pane("Apps");
        waitNode(n->role(n,"EditText")&&value(n.getText()).equals(HOME),"filter retained after battery recovery");
        result.putBoolean("unavailable_denial",true);result.putBoolean("native_recovery",true);result.putBoolean("nested_back",true);
    }
    private void appBattery(Bundle result){
        String pkg=BATTERY_FIXTURE;check(!batteryAllowlisted(pkg),"Unexpected synthetic battery allowlist baseline");
        batteryPage(pkg);enabledNotificationButton("Battery policy: Restricted");
        click(enabledNotificationButton("Battery policy: Restricted"));
        waitNode(n->role(n,"TextView")&&label(n).startsWith("Restrict background activity"),"restricted policy review");
        waitBatteryPolicy(pkg,"Optimized",false);cancelBatteryReview();waitBatteryPolicy(pkg,"Optimized",false);
        batteryChoice(pkg,"Restricted",false);batteryChoice(pkg,"Optimized",false);batteryChoice(pkg,"Unrestricted",false);
        shell("am force-stop "+HOME);batteryPage(pkg);
        waitNode(n->role(n,"Button")&&label(n).equals("Battery policy: Unrestricted · Selected"),"battery policy persisted across Home restart");
        batteryChoice(pkg,"Optimized",false);
        // A reviewed target cannot survive an independent policy change.
        click(enabledNotificationButton("Battery policy: Restricted"));
        waitNode(n->role(n,"TextView")&&label(n).startsWith("Restrict background activity"),"review before external policy change");
        shell("cmd deviceidle whitelist +"+pkg);
        click(enabledNotificationButton("Refresh battery policy"));
        waitNode(n->role(n,"TextView")&&label(n).startsWith("App policy or access changed"),"retired battery review");
        AccessibilityNodeInfo stale=revealButton("Battery policy: Confirm restricted battery policy");check(!stale.isEnabled()&&!stale.performAction(AccessibilityNodeInfo.ACTION_CLICK),"Stale battery review remained actionable");
        waitBatteryPolicy(pkg,"Unrestricted",false);cancelBatteryReview();batteryChoice(pkg,"Optimized",false);
        // Unexpected native modes are observed honestly, then explicitly repaired.
        shell("cmd appops set "+pkg+" RUN_ANY_IN_BACKGROUND foreground");click(enabledNotificationButton("Refresh battery policy"));
        waitNode(n->role(n,"TextView")&&label(n).equals("Observed policy: Custom Android state"),"custom native battery state");
        batteryChoice(pkg,"Optimized",false);
        click(button("Back"));pane("App details");click(button("Back"));pane("Apps");
        waitNode(n->role(n,"EditText")&&value(n.getText()).equals(pkg),"filter retained after battery Back");
        String legacy="dev.makepad.octosense.batterylegacyfixture";batteryPage(legacy);enabledNotificationButton("Battery policy: Restricted");
        batteryChoice(legacy,"Restricted",true);batteryChoice(legacy,"Unrestricted",true);batteryChoice(legacy,"Optimized",true);
        batteryPage("dev.makepad.octosense.batterysharedfixture");
        waitNode(n->role(n,"TextView")&&label(n).startsWith("This app shares an Android identity"),"shared UID battery policy gate");
        for(AccessibilityNodeInfo node:nodes())if(role(node,"Button")&&label(node).startsWith("Battery policy:"))check(!node.isEnabled()&&!node.performAction(AccessibilityNodeInfo.ACTION_CLICK),"Shared UID battery mutation was enabled");
        // Android's protected system-UID broker is read-only even though its state is observable.
        openRoute("apps","Apps");AccessibilityNodeInfo hidden=nodes().stream().filter(n->role(n,"Button")&&label(n).equals("System apps: Hidden")).findFirst().orElse(null);
        if(hidden!=null)clickAndWaitForAcknowledgment(hidden);
        batteryPage("dev.makepad.octosense.settingsbroker");waitNode(n->role(n,"TextView")&&label(n).startsWith("Android manages the battery policy"),"protected native package battery gate");
        for(AccessibilityNodeInfo node:nodes())if(role(node,"Button")&&label(node).startsWith("Battery policy:"))check(!node.isEnabled()&&!node.performAction(AccessibilityNodeInfo.ACTION_CLICK),"Protected battery mutation was enabled");
        openRoute("apps","Apps");AccessibilityNodeInfo shown=nodes().stream().filter(n->role(n,"Button")&&label(n).equals("System apps: Shown")).findFirst().orElse(null);if(shown!=null)clickAndWaitForAcknowledgment(shown);
        waitBatteryPolicy(pkg,"Optimized",false);waitBatteryPolicy(legacy,"Optimized",true);
        result.putBoolean("appops_and_allowlist_readback",true);result.putBoolean("review_cancel",true);result.putBoolean("stale_review_denied",true);
        result.putBoolean("legacy_appop_coupling",true);result.putBoolean("protected_and_shared_uid",true);result.putBoolean("home_restart_persistence",true);
    }
    private static final String STORAGE_FIXTURE="dev.makepad.octosense.storagefixture";
    private static final String STORAGE_SPACE="dev.makepad.octosense.storagespacefixture";
    private JSONObject storageFixture(String pkg,String operation)throws Exception{
        if(!pkg.equals(STORAGE_FIXTURE)&&!pkg.equals(STORAGE_SPACE))throw new AssertionError("Not a disposable storage fixture");
        if(!operation.equals("seed")&&!operation.equals("state")&&!operation.equals("grow_cache"))throw new AssertionError("Unknown storage fixture operation");
        return storageBroadcast(pkg+"/dev.makepad.octosense.storagefixture.StorageFixture",operation);
    }
    private JSONObject storageBroadcast(String component,String operation)throws Exception{
        String report=shell("am broadcast --receiver-foreground --include-stopped-packages -n "+component+" -a dev.makepad.octosense.storagefixture.COMMAND --es operation "+operation);
        Matcher reply=Pattern.compile("Broadcast completed: result=1, data=\"([A-Za-z0-9+/=]+)\"").matcher(report);
        check(reply.find(),"Synthetic storage broadcast failed");
        return new JSONObject(new String(android.util.Base64.decode(reply.group(1),android.util.Base64.DEFAULT),StandardCharsets.UTF_8));
    }
    private boolean storageUriGranted()throws Exception{return storageBroadcast("dev.makepad.octosense.storagereaderfixture/.StorageReader","state").getBoolean("uri_granted");}
    private boolean storageAlarmScheduled(){
        String report=shell("dumpsys alarm");
        // A PendingIntent can remain valid after AlarmManager removes its
        // scheduled alarm. Inspect the actual pending-alarm store instead.
        Matcher start=Pattern.compile("(?m)^  [0-9]+ pending alarms: *$").matcher(report);
        check(start.find(),"Native pending-alarm store was not reported");
        int end=report.indexOf("  LazyAlarmStore stats:",start.end());
        check(end>start.end(),"Native pending-alarm store was not bounded");
        String pending=report.substring(start.end(),end);
        return Pattern.compile("(?m)^\\s*(?:RTC|ELAPSED)(?:_WAKEUP)? #[0-9]+: Alarm\\{[^\\n]* "+Pattern.quote(STORAGE_FIXTURE)+"\\}").matcher(pending).find();
    }
    private AccessibilityNodeInfo storageEnabled(String name){return reveal(n->role(n,"Button")&&label(n).equals(name)&&n.isEnabled(),"enabled storage action "+name);}
    private void storagePage(String pkg){notificationAppDetails(pkg);click(storageEnabled("Storage & cache"));pane("App storage & cache");}
    private void storageReview(String button,String confirm){click(storageEnabled(button));storageEnabled(confirm);}
    private void storageCancel(){click(storageEnabled("Cancel storage change"));long deadline=SystemClock.uptimeMillis()+10000;do{List<AccessibilityNodeInfo> current=nodes();if(current.stream().anyMatch(n->role(n,"TextView")&&label(n).startsWith("App: "))&&current.stream().noneMatch(n->role(n,"Button")&&label(n).equals("Cancel storage change"))){checks++;return;}SystemClock.sleep(100);}while(SystemClock.uptimeMillis()<deadline);throw new AssertionError("Storage review did not close");}
    private void storageCompleted(boolean cache){reveal(n->role(n,"TextView")&&label(n).startsWith(cache?"Android completed the cache clear.":"Android completed the storage clear."),"native storage completion");}
    private void storageUnavailable(Bundle result){
        storagePage(HOME);waitNode(n->role(n,"TextView")&&label(n).startsWith("Storage controls are unavailable on this installation."),"ordinary unavailable storage service");
        for(String name:new String[]{"Clear cache…","Clear storage…","Manage space…"}){AccessibilityNodeInfo node=revealButton(name);check(!node.isEnabled()&&!node.performAction(AccessibilityNodeInfo.ACTION_CLICK),"Unavailable storage action was enabled");}
        shell("am start -W -a android.settings.SOUND_SETTINGS -p com.android.settings");waitPackage("com.android.settings");shell("am start -W -n "+HOME+"/.MakepadApp");pane("App storage & cache");
        click(storageEnabled("Android app settings"));waitPackage("com.android.settings");shell("input keyevent 4");pane("App storage & cache");click(button("Back"));pane("App details");click(button("Back"));pane("Apps");
        waitNode(n->role(n,"EditText")&&value(n.getText()).equals(HOME),"storage filter after recovery");result.putBoolean("unavailable_denial",true);result.putBoolean("native_recovery",true);result.putBoolean("nested_back",true);
    }
    private void appStorage(Bundle result)throws Exception{
        JSONObject initial=storageFixture(STORAGE_FIXTURE,"seed");shell("pm grant "+STORAGE_FIXTURE+" android.permission.CAMERA");initial=storageFixture(STORAGE_FIXTURE,"state");
        check(initial.getLong("data")==1048576&&initial.getLong("cache")==2097152&&initial.getBoolean("camera")&&initial.getBoolean("channel")&&initial.getBoolean("job")&&initial.getBoolean("alarm_token"),"Synthetic durable/cache/system-state seed failed");check(storageUriGranted(),"Synthetic URI permission seed failed");check(storageAlarmScheduled(),"Synthetic alarm was not scheduled");
        storagePage(STORAGE_FIXTURE);storageReview("Clear cache…","Confirm clear cache");storageCancel();
        JSONObject cancelled=storageFixture(STORAGE_FIXTURE,"state");for(String field:new String[]{"data","cache","code_cache","external_data","external_cache","camera","channel","job","alarm_token"})check(String.valueOf(initial.get(field)).equals(String.valueOf(cancelled.get(field))),"Cache Cancel changed "+field);
        check(storageAlarmScheduled(),"Cache Cancel removed the scheduled alarm");
        storageReview("Clear cache…","Confirm clear cache");
        check(storageFixture(STORAGE_FIXTURE,"grow_cache").getLong("cache")==3145728,"Synthetic app did not grow cache during review");
        click(storageEnabled("Confirm clear cache"));storageCompleted(true);
        JSONObject cached=storageFixture(STORAGE_FIXTURE,"state");
        check(cached.getLong("cache")==0&&cached.getLong("code_cache")==0&&cached.getLong("external_cache")==0,"Native cache clear left synthetic cache files");
        for(String field:new String[]{"data","external_data","camera","channel","job","alarm_token"})check(String.valueOf(initial.get(field)).equals(String.valueOf(cached.get(field))),"Cache clear changed durable state: "+field);check(storageUriGranted(),"Cache clear revoked durable URI state");
        check(storageAlarmScheduled(),"Cache clear removed the scheduled alarm");
        storageReview("Clear storage…","Confirm clear storage");storageCancel();check(storageFixture(STORAGE_FIXTURE,"state").getLong("data")==1048576,"Data Cancel cleared durable file");
        storageReview("Clear storage…","Confirm clear storage");
        check(shell("pm uninstall "+STORAGE_FIXTURE).contains("Success"),"Could not replace disposable storage fixture during review");
        check(shell("pm install /data/local/tmp/octosense-storage-fixture.apk").contains("Success"),"Could not reinstall disposable storage fixture");
        storageFixture(STORAGE_FIXTURE,"seed");shell("pm grant "+STORAGE_FIXTURE+" android.permission.CAMERA");
        check(storageAlarmScheduled(),"Reinstalled fixture alarm was not scheduled");
        click(storageEnabled("Refresh storage"));waitNode(n->role(n,"TextView")&&label(n).startsWith("This app, its storage or access changed."),"reinstalled storage review retirement");
        AccessibilityNodeInfo retired=revealButton("Confirm clear storage");check(!retired.isEnabled()&&!retired.performAction(AccessibilityNodeInfo.ACTION_CLICK),"Old storage review remained actionable after reinstall");
        check(storageFixture(STORAGE_FIXTURE,"state").getLong("data")==1048576,"Old review affected new installation");storageCancel();
        storageReview("Clear storage…","Confirm clear storage");click(storageEnabled("Confirm clear storage"));storageCompleted(false);
        JSONObject cleared=storageFixture(STORAGE_FIXTURE,"state");for(String field:new String[]{"data","cache","code_cache","external_data","external_cache"})check(cleared.getLong(field)==0,"Native data clear retained synthetic file "+field);
        for(String field:new String[]{"camera","channel","job"})check(!cleared.getBoolean(field),"Native data clear omitted Android state reset "+field);check(!storageUriGranted(),"Native data clear did not revoke URI permission");check(!storageAlarmScheduled(),"Native data clear did not cancel the scheduled alarm");
        click(button("Back"));pane("App details");click(button("Back"));pane("Apps");waitNode(n->role(n,"EditText")&&value(n.getText()).equals(STORAGE_FIXTURE),"storage filter after nested Back");
        storageFixture(STORAGE_SPACE,"seed");storagePage(STORAGE_SPACE);storageEnabled("Manage space…");AccessibilityNodeInfo direct=revealButton("Clear storage…");check(!direct.isEnabled()&&!direct.performAction(AccessibilityNodeInfo.ACTION_CLICK),"App-owned manage-space was bypassed");
        shell("am start -W -n "+STORAGE_SPACE+"/.ManageSpaceActivity");waitPackage(STORAGE_SPACE);shell("am start -W -n "+HOME+"/.MakepadApp");pane("App storage & cache");
        click(storageEnabled("Manage space…"));waitPackage(STORAGE_SPACE);shell("input keyevent 4");pane("App storage & cache");check(storageFixture(STORAGE_SPACE,"state").getLong("data")==1048576,"Provider Back cleared durable storage");
        storagePage(HOME);waitNode(n->role(n,"TextView")&&label(n).startsWith("Android protects storage for this essential app."),"protected Home storage");
        for(String name:new String[]{"Clear cache…","Clear storage…","Manage space…"}){AccessibilityNodeInfo node=revealButton(name);check(!node.isEnabled()&&!node.performAction(AccessibilityNodeInfo.ACTION_CLICK),"Protected Home storage clear was enabled");}
        result.putBoolean("cache_growth_keeps_review_valid",true);result.putBoolean("cache_only_preserves_data",true);result.putBoolean("clear_data_framework_resets",true);result.putBoolean("cancel_preserves_state",true);result.putBoolean("reinstalled_review_denied",true);result.putBoolean("app_owned_manage_space",true);result.putBoolean("protected_home",true);
    }
    // Integrate into SettingsAccessibilityProbe after storage fixture editing is finished.
    private static final String[] VISION_KEYS={"accessibility_display_inversion_enabled","accessibility_display_daltonizer_enabled","accessibility_display_daltonizer"};
    private static final String[][] VISION_MODES={{"12","Red-green, green weak"},{"11","Red-green, red weak"},{"13","Blue-yellow"},{"0","Grayscale"}};
    private void visionPage(){openRoute("overview","OctoSense Settings");openRoute("accessibility_vision","Accessibility: colors");}
    private String visionRaw(int index){return android.provider.Settings.Secure.getString(getTargetContext().getContentResolver(),VISION_KEYS[index]);}
    private String visionEffective(int index){String value=visionRaw(index);return value==null?(index==2?"12":"0"):value;}
    private String visionMatrix(){
        Matcher match=Pattern.compile("(?m)^\\s*colorTransformMatrix=(\\[\\[.*\\]\\])\\s*$").matcher(shell("dumpsys SurfaceFlinger"));
        if(!match.find())throw new AssertionError("Actual SurfaceFlinger color matrix unavailable");return match.group(1);
    }
    private String waitVisionMatrix(String prior,boolean same){
        long deadline=SystemClock.uptimeMillis()+10000;
        do{String current=visionMatrix();if(current.equals(prior)==same){checks++;return current;}SystemClock.sleep(100);}while(SystemClock.uptimeMillis()<deadline);
        throw new AssertionError("SurfaceFlinger color matrix did not "+(same?"return":"change"));
    }
    private void visionSet(int index,String title,String choice,String stored){
        visionPage();waitNode(n->role(n,"Button")&&n.isEnabled()&&label(n).startsWith("Color inversion: "),"fresh writable vision snapshot");
        if(visionEffective(index).equals(stored))return;
        final String semantic=title+": "+choice;
        click(reveal(n->role(n,"Button")&&label(n).equals(semantic)&&n.isEnabled(),"enabled "+semantic));waitSetting("secure",VISION_KEYS[index],stored);
        if(index==2)waitNode(n->role(n,"Button")&&label(n).equals(semantic+" · Selected"),"observed correction mode");
    }
    private void visionUnavailable(Bundle result){
        String[] original=new String[3];for(int i=0;i<3;i++)original[i]=visionRaw(i);
        visionPage();waitNode(n->role(n,"TextView")&&(label(n).equals("Read only")||label(n).equals("Unavailable")),"ordinary vision observation");
        for(String name:new String[]{"Color inversion: Off","Color inversion: On","Use color correction: Off","Use color correction: On"}){
            AccessibilityNodeInfo node=revealButton(name);check(!node.isEnabled()&&!node.performAction(AccessibilityNodeInfo.ACTION_CLICK),"Ordinary vision write enabled: "+name);
        }
        reveal(n->role(n,"TextView")&&label(n).equals("Color correction mode"),"observed mode row");
        check(nodes().stream().noneMatch(n->role(n,"Button")&&label(n).startsWith("Color correction mode: ")&&n.isEnabled()),"Ordinary color mode write enabled");
        shell("am start -W -a android.settings.SOUND_SETTINGS -p com.android.settings");waitPackage("com.android.settings");shell("am start -W -n "+HOME+"/.MakepadApp");pane("Accessibility: colors");
        click(revealButton("More in Android Settings"));waitPackage("com.android.settings");shell("input keyevent 4");pane("Accessibility: colors");
        click(button("Back"));pane("System");
        for(int i=0;i<3;i++)check(java.util.Objects.equals(original[i],visionRaw(i)),"Ordinary vision probe changed saved state");
        openRoute("overview","OctoSense Settings");result.putBoolean("read_only_denial",true);result.putBoolean("native_recovery",true);result.putBoolean("nested_back",true);
    }
    private void visionControls(Bundle result){
        check(shell("getprop ro.kernel.qemu").trim().equals("1"),"Vision probe requires an emulator");
        String[] original=new String[3];for(int i=0;i<3;i++){original[i]=visionRaw(i);check(original[i]==null||original[i].matches("-?[0-9]+")||original[i].isEmpty(),"Unexpected original color value");}
        // All writes belong to this reversible emulator scenario. Unknown baselines are refused.
        check(visionEffective(0).equals("0")&&visionEffective(1).equals("0"),"Restore initial accessibility transforms Off before this acceptance");
        String baseline=visionMatrix();
        try{
            visionSet(0,"Color inversion","On","1");String inverted=waitVisionMatrix(baseline,false);
            check(visionEffective(1).equals("0"),"Inversion enabled correction");visionSet(0,"Color inversion","Off","0");waitVisionMatrix(baseline,true);
            for(String[] mode:VISION_MODES){visionSet(2,"Color correction mode",mode[1],mode[0]);check(visionEffective(1).equals("0"),"Mode selection enabled correction");check(visionMatrix().equals(baseline),"Disabled correction altered compositor");}
            visionSet(1,"Use color correction","On","1");String grayscale=waitVisionMatrix(baseline,false);java.util.HashSet<String> matrices=new java.util.HashSet<>();
            String previous=grayscale;
            for(String[] mode:VISION_MODES){visionSet(2,"Color correction mode",mode[1],mode[0]);String current=waitVisionMatrix(previous,false);check(!current.equals(baseline),"Enabled correction lost its transform");check(matrices.add(current),"Native correction modes unexpectedly share a matrix");previous=current;}
            visionSet(0,"Color inversion","On","1");String combined=waitVisionMatrix(previous,false);check(!combined.equals(inverted),"Correction was overwritten by inversion");
            visionSet(0,"Color inversion","Off","0");waitVisionMatrix(previous,true);
            shell("am force-stop "+HOME);visionPage();waitNode(n->role(n,"Button")&&label(n).equals("Color correction mode: Grayscale · Selected"),"persisted correction after restart");check(visionEffective(1).equals("1"),"Correction lost on Home restart");check(visionMatrix().equals(previous),"Home restart changed compositor state");
            visionSet(1,"Use color correction","Off","0");waitVisionMatrix(baseline,true);
            // Unknown raw mode remains unknown while correction is off; no unsupported native transform is activated.
            shell("settings put secure "+VISION_KEYS[2]+" 99");click(reveal(n->role(n,"Button")&&label(n).equals("Refresh")&&n.isEnabled(),"enabled vision Refresh"));
            waitNode(n->role(n,"TextView")&&label(n).equals("Unavailable"),"unknown native correction mode");
            check(nodes().stream().noneMatch(n->role(n,"Button")&&label(n).startsWith("Color correction mode: ")&&n.isEnabled()),"Unknown mode offered mutation");check(visionRaw(2).equals("99"),"Reading custom mode rewrote it");
            shell("settings put secure "+VISION_KEYS[2]+" 11");click(reveal(n->role(n,"Button")&&label(n).equals("Refresh")&&n.isEnabled(),"enabled vision Refresh"));waitNode(n->role(n,"Button")&&label(n).equals("Color correction mode: Red-green, red weak · Selected"),"external mode observation");
            visionSet(1,"Use color correction","Off","0");waitVisionMatrix(baseline,true);
            result.putBoolean("independent_modes_and_enable",true);result.putBoolean("all_four_native_transforms",true);result.putBoolean("combined_inversion",true);result.putBoolean("unknown_mode_denial",true);result.putBoolean("restart_and_external_readback",true);
        } finally {
            automation.adoptShellPermissionIdentity("android.permission.WRITE_SECURE_SETTINGS");
            try{for(int i=0;i<3;i++)check(android.provider.Settings.Secure.putString(getTargetContext().getContentResolver(),VISION_KEYS[i],original[i]),"Could not restore color preference");}
            finally{automation.dropShellPermissionIdentity();}
        }
        for(int i=0;i<3;i++)check(java.util.Objects.equals(original[i],visionRaw(i)),"Color preference restoration failed");waitVisionMatrix(baseline,true);
        openRoute("overview","OctoSense Settings");result.putBoolean("restored",true);
    }

    private static final String LANGUAGE_FIXTURE="dev.makepad.octosense.languagefixture";
    private JSONObject languageFixture(String operation)throws Exception{
        if(!operation.equals("state")&&!operation.equals("override_subset")&&!operation.equals("override_clear"))throw new AssertionError("Unknown language fixture command");
        String report=shell("am broadcast --receiver-foreground --include-stopped-packages -n "+LANGUAGE_FIXTURE+"/.LanguageFixture -a "+LANGUAGE_FIXTURE+".COMMAND --es operation "+operation);
        Matcher reply=Pattern.compile("Broadcast completed: result=1, data=\"([A-Za-z0-9+/=]+)\"").matcher(report);
        check(reply.find(),"Synthetic language broadcast failed");
        return new JSONObject(new String(android.util.Base64.decode(reply.group(1),android.util.Base64.DEFAULT),StandardCharsets.UTF_8));
    }
    private void languagePage(String pkg){notificationAppDetails(pkg);click(storageEnabled("Language"));pane("App language");}
    private void languageReady(){rolesTop();waitNode(n->role(n,"TextView")&&(label(n).startsWith("Current languages\n")||label(n).equals("Current language: System default")),"actual app locale list");}
    private void languageRefresh(){rolesTop();click(storageEnabled("Refresh languages"));languageReady();}
    private AccessibilityNodeInfo languageRow(String prefix,boolean branch){
        rolesTop();return reveal(n->role(n,"Button")&&n.isEnabled()&&label(n).toLowerCase(java.util.Locale.ROOT).startsWith(prefix.toLowerCase(java.util.Locale.ROOT))&&label(n).contains("Choose region or numbering")==branch,"language choice "+prefix);
    }
    private void languageBranch(String name){click(languageRow(name,true));rolesTop();waitNode(n->role(n,"TextView")&&(label(n).startsWith("Regions ")||label(n).startsWith("Numbering systems ")),"native language branch");}
    private void languageTags(String expected)throws Exception{
        long deadline=SystemClock.uptimeMillis()+10000;
        do{if(languageFixture("state").getString("application_tags").equals(expected)){checks++;return;}SystemClock.sleep(150);}while(SystemClock.uptimeMillis()<deadline);
        throw new AssertionError("Native app LocaleList did not become "+expected);
    }
    private void languageRender(String tag,String text)throws Exception{
        languageTags(tag);int before=languageFixture("state").getInt("created");
        shell("am start -W -n "+LANGUAGE_FIXTURE+"/.LanguageActivity");waitPackage(LANGUAGE_FIXTURE);
        long deadline=SystemClock.uptimeMillis()+10000;JSONObject state;
        do{state=languageFixture("state");if(state.getString("activity_tags").startsWith(tag)&&state.getString("activity_greeting").equals(text))break;SystemClock.sleep(100);}while(SystemClock.uptimeMillis()<deadline);
        check(state.getString("greeting").equals(text)&&state.getString("activity_greeting").equals(text),"Real target Activity did not render selected translation");
        check(state.getInt("created")>before,"Locale change did not recreate target Activity");
        shell("am start -W -n "+HOME+"/.MakepadApp");pane("App language");languageRefresh();
    }
    private void appLanguage(Bundle result)throws Exception{
        check(languageFixture("state").getString("application_tags").isEmpty(),"Unexpected target language baseline");
        shell("am start -W -n "+LANGUAGE_FIXTURE+"/.LanguageActivity");waitPackage(LANGUAGE_FIXTURE);
        languagePage(LANGUAGE_FIXTURE);languageReady();
        AccessibilityNodeInfo editor=waitNode(n->role(n,"EditText")&&label(n).equals("Filter language choices"),"stable language filter");
        setText(editor,"Français");waitNode(n->role(n,"EditText")&&value(n.getText()).equals("Français"),"language filter draft");
        SystemClock.sleep(6000);AccessibilityNodeInfo polled=waitNode(n->role(n,"EditText")&&value(n.getText()).equals("Français"),"language draft after polling");check(editor.equals(polled),"Language polling replaced editor");
        click(storageEnabled("Search languages"));rolesTop();waitNode(n->role(n,"TextView")&&label(n).equals("Languages 1–1 of 1"),"filtered French native branch");
        languageBranch("Français");check(languageFixture("state").getString("application_tags").isEmpty(),"Opening language branch changed locale");
        click(button("Back"));pane("App language");rolesTop();waitNode(n->role(n,"EditText")&&value(n.getText()).equals("Français"),"branch Back retained language filter");
        languageBranch("Français");click(languageRow("Canada",false));languageRender("fr-CA","Bonjour depuis le test de langue canadien");
        languageBranch("简体中文");click(languageRow("中国",false));languageRender("zh-Hans-CN","简体中文语言测试");
        languageBranch("繁體中文");click(languageRow("台灣",false));languageRender("zh-Hant-TW","繁體中文語言測試");
        shell("am force-stop "+HOME);languagePage(LANGUAGE_FIXTURE);languageReady();
        waitNode(n->role(n,"TextView")&&label(n).contains("zh-Hant-TW"),"persisted language after Home restart");languageTags("zh-Hant-TW");
        // Existing ordered custom lists are observations, never normalized by a read.
        shell("cmd locale set-app-locales "+LANGUAGE_FIXTURE+" --user 0 --locales fr-CA,zh-Hant-TW --delegate true");languageRefresh();
        waitNode(n->role(n,"TextView")&&label(n).contains("fr-CA")&&label(n).contains("zh-Hant-TW"),"complete native custom LocaleList");languageTags("fr-CA,zh-Hant-TW");
        click(languageRow("System default",false));languageTags("");languageReady();waitNode(n->role(n,"TextView")&&label(n).equals("Current language: System default"),"empty native LocaleList readback");
        languageBranch("Français");AccessibilityNodeInfo retired=languageRow("Canada",false);
        check(languageFixture("override_subset").getBoolean("override"),"Synthetic runtime LocaleConfig was not applied");
        rolesTop();waitNode(n->role(n,"TextView")&&label(n).startsWith("This app or its language configuration changed."),"runtime config invalidates old branch");
        check(!retired.performAction(AccessibilityNodeInfo.ACTION_CLICK),"Retired runtime locale choice remained actionable");languageTags("");
        languageRefresh();languageBranch("Français");click(languageRow("France",false));languageTags("fr-FR");
        languageFixture("override_clear");languageRefresh();
        click(languageRow("System default",false));languageTags("");languageReady();
        languageBranch("Français");AccessibilityNodeInfo reinstalled=languageRow("Canada",false);
        check(shell("pm uninstall "+LANGUAGE_FIXTURE).contains("Success"),"Cannot replace disposable language fixture");
        check(shell("pm install /data/local/tmp/octosense-language-fixture.apk").contains("Success"),"Cannot reinstall disposable language fixture");
        rolesTop();waitNode(n->role(n,"TextView")&&label(n).startsWith("This app or its language configuration changed."),"reinstall invalidates old language branch");
        check(!reinstalled.performAction(AccessibilityNodeInfo.ACTION_CLICK),"Old language choice rebound after reinstall");languageTags("");languageRefresh();
        shell("am start -W -a android.settings.SOUND_SETTINGS -p com.android.settings");waitPackage("com.android.settings");shell("am start -W -n "+HOME+"/.MakepadApp");pane("App language");
        click(storageEnabled("Android language settings"));waitPackage("com.android.settings");check(shell("dumpsys activity activities").contains("AppLocalePickerActivity"),"Native language recovery wrong destination");
        shell("input keyevent 4");pane("App language");click(button("Back"));pane("App details");click(button("Back"));pane("Apps");
        waitNode(n->role(n,"EditText")&&value(n.getText()).equals(LANGUAGE_FIXTURE),"Apps filter after nested language Back");
        languagePage(HOME);waitNode(n->role(n,"TextView")&&label(n).equals("Android does not offer app language choices for this app."),"platform-signed app language exclusion");
        AccessibilityNodeInfo nativeButton=revealButton("Android language settings");check(!nativeButton.isEnabled()&&!nativeButton.performAction(AccessibilityNodeInfo.ACTION_CLICK),"Ineligible platform app offered native language mutation");
        languageTags("");result.putBoolean("actual_french_and_chinese_resources",true);result.putBoolean("native_activity_recreation",true);result.putBoolean("custom_list_preserved",true);result.putBoolean("runtime_config_and_reinstall_denial",true);result.putBoolean("branch_filter_back",true);result.putBoolean("native_recovery",true);result.putBoolean("protected_host",true);
        openRoute("overview","OctoSense Settings");
    }
    private void languageInspect(Bundle result)throws Exception{
        languagePage(LANGUAGE_FIXTURE);languageReady();StringBuilder rows=new StringBuilder();
        for(int i=0;i<6;i++){
            for(AccessibilityNodeInfo n:nodes())if(role(n,"TextView")||role(n,"Button"))rows.append(label(n)).append(" enabled=").append(n.isEnabled()).append('\n');
            AccessibilityNodeInfo scroll=nodes().stream().filter(n->role(n,"ScrollView")&&action(n,AccessibilityNodeInfo.ACTION_SCROLL_FORWARD)).findFirst().orElse(null);
            if(scroll==null)break;check(scroll.performAction(AccessibilityNodeInfo.ACTION_SCROLL_FORWARD),"Language scroll failed");SystemClock.sleep(400);
        }
        result.putString("language_choices",rows.toString());result.putString("native_fixture",languageFixture("state").toString());
    }
    private void languageUnavailable(Bundle result)throws Exception{
        languagePage(LANGUAGE_FIXTURE);waitNode(n->role(n,"TextView")&&label(n).startsWith("App language controls are unavailable on this installation."),"ordinary missing language service");
        check(nodes().stream().noneMatch(n->role(n,"Button")&&label(n).contains("Selected")),"Missing service invented selected locale");
        check(languageFixture("state").getString("application_tags").isEmpty(),"Ordinary read changed target locales");
        shell("am start -W -a android.settings.SOUND_SETTINGS -p com.android.settings");waitPackage("com.android.settings");shell("am start -W -n "+HOME+"/.MakepadApp");pane("App language");
        click(storageEnabled("Android language settings"));waitPackage("com.android.settings");
        check(shell("dumpsys activity activities").contains("AppLocalePickerActivity"),"Recovery did not open exact app language picker");
        shell("input keyevent 4");pane("App language");click(button("Back"));pane("App details");click(button("Back"));pane("Apps");
        waitNode(n->role(n,"EditText")&&value(n.getText()).equals(LANGUAGE_FIXTURE),"language filter after recovery");
        check(languageFixture("state").getString("application_tags").isEmpty(),"Recovery changed locale without selection");
        result.putBoolean("unavailable_no_fabricated_locale",true);result.putBoolean("native_language_recovery",true);result.putBoolean("nested_back",true);
    }

    private static final String HEARING_FIXTURE="dev.makepad.octosense.hearingfixture";
    private boolean hearingPlaybackStarted;
    private static final String[] HEARING_SYSTEM={"master_mono","master_balance"};
    private static final String[] HEARING_SECURE={"accessibility_captioning_enabled","accessibility_captioning_font_scale","accessibility_captioning_preset","accessibility_captioning_locale","accessibility_captioning_foreground_color","accessibility_captioning_background_color","accessibility_captioning_window_color","accessibility_captioning_edge_type","accessibility_captioning_edge_color","accessibility_captioning_typeface"};
    private String hearingRaw(String table,String key){return table.equals("system")?android.provider.Settings.System.getString(getTargetContext().getContentResolver(),key):android.provider.Settings.Secure.getString(getTargetContext().getContentResolver(),key);}
    private static String hearingQuote(String value){return "'"+value.replace("'","'\"'\"'")+"'";}
    private void hearingRestore(java.util.Map<String,String> system,java.util.Map<String,String> secure){
        StringBuilder script=new StringBuilder("set -e\n");
        for(String table:new String[]{"system","secure"})for(java.util.Map.Entry<String,String> entry:(table.equals("system")?system:secure).entrySet()){
            script.append("settings ").append(entry.getValue()==null?"delete ":"put ").append(table).append(' ').append(hearingQuote(entry.getKey()));
            if(entry.getValue()!=null)script.append(' ').append(hearingQuote(entry.getValue()));
            script.append(" >/dev/null\n");
        }
        script.append("echo hearing-restored\n");
        // Instrumentation is an ordinary APK: adopting WRITE_SECURE_SETTINGS does
        // not permit private System rows. Use shell stdin, without argument reparsing.
        ParcelFileDescriptor[] descriptors=automation.executeShellCommandRw("sh");
        try(ParcelFileDescriptor.AutoCloseInputStream input=new ParcelFileDescriptor.AutoCloseInputStream(descriptors[0]);
                ByteArrayOutputStream output=new ByteArrayOutputStream()){
            try(ParcelFileDescriptor.AutoCloseOutputStream writer=new ParcelFileDescriptor.AutoCloseOutputStream(descriptors[1])){writer.write(script.toString().getBytes(StandardCharsets.UTF_8));}
            byte[] bytes=new byte[4096];int count;while((count=input.read(bytes))!=-1)output.write(bytes,0,count);
            check(new String(output.toByteArray(),StandardCharsets.UTF_8).trim().equals("hearing-restored"),"Shell could not restore hearing rows");
        }catch(IOException failure){throw new AssertionError("Cannot restore hearing rows",failure);}
    }
    private void hearingTop(){
        long deadline=SystemClock.uptimeMillis()+10000;
        do{
            List<AccessibilityNodeInfo> visible=nodes();AccessibilityNodeInfo scroll=null;
            for(AccessibilityNodeInfo node:visible)if(role(node,"ScrollView")&&action(node,AccessibilityNodeInfo.ACTION_SCROLL_BACKWARD)){scroll=node;break;}
            if(scroll==null&&visible.stream().anyMatch(n->role(n,"Button")&&label(n).equals("Refresh")))return;
            // Snapshot publication can retire a scroll node between lookup and action.
            // Re-query the tree; never retry a setting mutation this way.
            if(scroll!=null&&scroll.performAction(AccessibilityNodeInfo.ACTION_SCROLL_BACKWARD))checks++;
            SystemClock.sleep(150);
        }while(SystemClock.uptimeMillis()<deadline);throw new AssertionError("Hearing page did not reach top");
    }
    private AccessibilityNodeInfo hearingReveal(Match match,String description){
        long deadline=SystemClock.uptimeMillis()+20000;
        do{
            List<AccessibilityNodeInfo> visible=nodes();for(AccessibilityNodeInfo node:visible)if(match.accepts(node))return node;
            AccessibilityNodeInfo scroll=null;for(AccessibilityNodeInfo node:visible)if(role(node,"ScrollView")&&action(node,AccessibilityNodeInfo.ACTION_SCROLL_FORWARD)){scroll=node;break;}
            if(scroll==null)hearingTop();else if(scroll.performAction(AccessibilityNodeInfo.ACTION_SCROLL_FORWARD))checks++;
            SystemClock.sleep(200);
        }while(SystemClock.uptimeMillis()<deadline);throw new AssertionError("Missing hearing observation: "+description);
    }
    private void hearingPage(){openRoute("accessibility_hearing","Accessibility: hearing");}
    private void hearingRefresh(){hearingTop();click(storageEnabled("Refresh"));}
    private void hearingChoice(String name){hearingTop();hearingReveal(n->role(n,"Button")&&label(n).equals(name),name);click(waitNode(n->role(n,"Button")&&n.isEnabled()&&label(n).equals(name),"enabled "+name));}
    private void hearingBalance(float value){
        hearingAudio(false,value);hearingTop();String expected=value==0?"Centered":new java.text.DecimalFormat("0.######",java.text.DecimalFormatSymbols.getInstance(java.util.Locale.ROOT)).format(Math.abs(value)*100)+(value<0?"% left":"% right");
        waitNode(n->role(n,"TextView")&&label(n).equals(expected),"observed balance "+expected);
    }
    private void hearingSelected(String name){hearingTop();hearingReveal(n->role(n,"Button")&&label(n).equals(name+" · Selected"),"observed "+name);}
    private void hearingAudio(boolean mono,float balance){
        long deadline=SystemClock.uptimeMillis()+10000;
        do{
            String report=shell("dumpsys media.audio_flinger");Matcher m=Pattern.compile("Master mono: (on|off)").matcher(report);int outputs=0;boolean match=true;
            while(m.find()){outputs++;match&=m.group(1).equals(mono?"on":"off");}
            Matcher b=Pattern.compile("Master balance: (-?[0-9.]+)").matcher(report);int balances=0;
            while(b.find()){balances++;match&=Math.abs(Float.parseFloat(b.group(1))-balance)<0.00001f;}
            if(hearingPlaybackStarted&&(balance==0||Math.abs(balance)==1)){
                int stereo=0;
                for(String thread:report.split("Output thread "))if(thread.contains("type 0 (MIXER)")&&thread.contains("Standby: no")){
                    Matcher gains=Pattern.compile("Master balance: [^\\n]*channelCount 2 volumes: ([0-9.eE+-]+) ([0-9.eE+-]+)").matcher(thread);
                    if(gains.find()){stereo++;match&=Math.abs(Float.parseFloat(gains.group(1))-(balance==1?0:1))<0.00001f&&Math.abs(Float.parseFloat(gains.group(2))-(balance==-1?0:1))<0.00001f;}
                }
                match&=stereo>0;
            }
            if(outputs>0&&balances==outputs&&match){checks++;return;}SystemClock.sleep(100);
        }while(SystemClock.uptimeMillis()<deadline);throw new AssertionError("AudioFlinger outputs did not apply mono/balance");
    }
    private JSONObject hearingFixture()throws Exception{
        String report=shell("am broadcast --receiver-foreground --include-stopped-packages -n "+HEARING_FIXTURE+"/.CaptionFixture -a "+HEARING_FIXTURE+".COMMAND --es operation state");
        Matcher reply=Pattern.compile("Broadcast completed: result=1, data=\"([A-Za-z0-9+/=]+)\"").matcher(report);check(reply.find(),"Caption consumer observation failed");
        return new JSONObject(new String(android.util.Base64.decode(reply.group(1),android.util.Base64.DEFAULT),StandardCharsets.UTF_8));
    }
    private void hearingConsumer(boolean enabled,float scale)throws Exception{
        shell("am start -W -n "+HEARING_FIXTURE+"/.CaptionActivity");waitPackage(HEARING_FIXTURE);
        long deadline=SystemClock.uptimeMillis()+10000;JSONObject state=null,drawn=null,observed=null;
        do{
            state=hearingFixture();observed=state.getJSONObject("observed");drawn=state.optJSONObject("drawn");
            if(drawn!=null&&drawn.getBoolean("visible")==enabled&&Math.abs(drawn.getDouble("font_scale")-scale)<0.00001&&observed.getBoolean("enabled")==enabled&&Math.abs(observed.getDouble("font_scale")-scale)<0.00001)break;
            SystemClock.sleep(100);
        }while(SystemClock.uptimeMillis()<deadline);
        check(state!=null&&state.getString("error").isEmpty()&&drawn!=null,"Caption consumer did not draw");
        check(drawn.getBoolean("visible")==enabled&&observed.getBoolean("enabled")==enabled,"Caption enable not reflected by real consumer");
        check(Math.abs(drawn.getDouble("font_scale")-scale)<0.00001&&Math.abs(observed.getDouble("font_scale")-scale)<0.00001,"Caption size not propagated");
        check(Math.abs(drawn.getDouble("text_size_px")-20.0*scale*drawn.getDouble("scaled_density"))<0.01,"Actual TextView size does not follow caption preference");
        int foreground=observed.getBoolean("has_foreground")?observed.getInt("foreground"):-1,background=observed.getBoolean("has_background")?observed.getInt("background"):-16777216;
        check(drawn.getInt("foreground")==foreground&&drawn.getInt("background")==background,"Actual caption colors do not match native style");
        if(enabled)check(drawn.getInt("width")>0&&drawn.getInt("height")>0,"Enabled subtitle not laid out");
        shell("am start -W -n "+HOME+"/.MakepadApp");pane("Accessibility: hearing");
    }
    private void hearingUnavailable(Bundle result){
        java.util.Map<String,String> before=new java.util.LinkedHashMap<>();for(String key:HEARING_SYSTEM)before.put(key,hearingRaw("system",key));for(String key:HEARING_SECURE)before.put(key,hearingRaw("secure",key));
        hearingPage();waitNode(n->role(n,"TextView")&&label(n).equals("Unavailable"),"ordinary unavailable hearing state");
        for(String name:new String[]{"Mono audio: On","Mono audio: Off","Audio balance: Left 1%","Audio balance: Right 1%","Audio balance: Center","Use captions: On","Use captions: Off"}){hearingTop();AccessibilityNodeInfo row=hearingReveal(n->role(n,"Button")&&label(n).equals(name),name);check(!row.isEnabled()&&!row.performAction(AccessibilityNodeInfo.ACTION_CLICK),"Ordinary hearing control enabled: "+name);}
        hearingTop();hearingReveal(n->role(n,"TextView")&&label(n).equals("Caption style"),"caption style observation");check(nodes().stream().noneMatch(n->role(n,"Button")&&n.isEnabled()&&label(n).startsWith("Caption style: ")),"Missing service invented caption choice");
        shell("am start -W -a android.settings.SOUND_SETTINGS -p com.android.settings");waitPackage("com.android.settings");shell("am start -W -n "+HOME+"/.MakepadApp");pane("Accessibility: hearing");
        click(revealButton("More in Android Settings"));waitPackage("com.android.settings");shell("input keyevent 4");pane("Accessibility: hearing");click(button("Back"));pane("System");
        for(String key:HEARING_SYSTEM)check(java.util.Objects.equals(before.get(key),hearingRaw("system",key)),"Ordinary path changed audio setting");for(String key:HEARING_SECURE)check(java.util.Objects.equals(before.get(key),hearingRaw("secure",key)),"Ordinary path changed caption setting");
        result.putBoolean("readonly_and_recovery",true);openRoute("overview","OctoSense Settings");
    }
    private void hearingControls(Bundle result)throws Exception{
        java.util.Map<String,String> system=new java.util.LinkedHashMap<>(),secure=new java.util.LinkedHashMap<>();for(String key:HEARING_SYSTEM)system.put(key,hearingRaw("system",key));for(String key:HEARING_SECURE)secure.put(key,hearingRaw("secure",key));
        boolean originalMono=system.get("master_mono")!=null&&system.get("master_mono").equals("1");float originalBalance=system.get("master_balance")==null?0:Float.parseFloat(system.get("master_balance"));
        check(Float.isFinite(originalBalance)&&Math.abs(originalBalance)<=1,"Unknown original audio balance");
        Throwable originalFailure=null;
        try{
            shell("settings put system master_mono 0");shell("settings put system master_balance 0");shell("settings put secure accessibility_captioning_enabled 0");shell("settings put secure accessibility_captioning_font_scale 1.0");shell("settings put secure accessibility_captioning_preset 0");
            shell("am start -W -n "+HEARING_FIXTURE+"/.CaptionActivity");waitPackage(HEARING_FIXTURE);hearingPlaybackStarted=true;
            hearingAudio(false,0);hearingPage();hearingChoice("Mono audio: On");waitSetting("system","master_mono","1");hearingAudio(true,0);
            hearingChoice("Mono audio: Off");waitSetting("system","master_mono","0");hearingAudio(false,0);
            for(String[] step:new String[][]{{"Right 1%","0.01"},{"Left 1%","0.0"},{"Left 1%","-0.01"},{"Center","0.0"}}){hearingChoice("Audio balance: "+step[0]);waitSetting("system","master_balance",step[1]);hearingBalance(Float.parseFloat(step[1]));}
            for(String[] edge:new String[][]{{"0.99","Right 1%","1.0"},{"-0.99","Left 1%","-1.0"}}){shell("settings put system master_balance "+edge[0]);hearingRefresh();hearingBalance(Float.parseFloat(edge[0]));hearingChoice("Audio balance: "+edge[1]);waitSetting("system","master_balance",edge[2]);hearingBalance(Float.parseFloat(edge[2]));AccessibilityNodeInfo limit=revealButton("Audio balance: "+edge[1]);check(!limit.isEnabled()&&!limit.performAction(AccessibilityNodeInfo.ACTION_CLICK),"Balance endpoint remained actionable");}
            shell("settings put system master_balance 0.123");hearingRefresh();hearingAudio(false,0.123f);hearingTop();waitNode(n->role(n,"TextView")&&label(n).startsWith("12.3% right"),"custom balance readback");
            check(hearingRaw("system","master_balance").equals("0.123"),"Custom balance coerced by read");hearingChoice("Audio balance: Left 1%");waitSetting("system","master_balance","0.12");hearingAudio(false,0.12f);hearingChoice("Audio balance: Center");hearingAudio(false,0);
            // Native size/style selection also enables captions and preserves all custom fields.
            hearingChoice("Caption style: Yellow on blue");waitSetting("secure","accessibility_captioning_preset","3");hearingSelected("Caption style: Yellow on blue");waitSetting("secure","accessibility_captioning_enabled","1");
            hearingConsumer(true,1);hearingChoice("Use captions: Off");waitSetting("secure","accessibility_captioning_enabled","0");hearingConsumer(false,1);
            for(String[] size:new String[][]{{"25%","0.25"},{"50%","0.5"},{"100%","1.0"},{"150%","1.5"},{"200%","2.0"}}){hearingChoice("Caption text size: "+size[0]);waitSetting("secure","accessibility_captioning_font_scale",size[1]);hearingSelected("Caption text size: "+size[0]);hearingConsumer(true,Float.parseFloat(size[1]));}
            for(String[] style:new String[][]{{"Set by app","4"},{"White on black","0"},{"Black on white","1"},{"Yellow on black","2"},{"Yellow on blue","3"},{"Custom","-1"}}){hearingChoice("Caption style: "+style[0]);waitSetting("secure","accessibility_captioning_preset",style[1]);hearingSelected("Caption style: "+style[0]);hearingConsumer(true,2);}
            JSONObject callbacks=hearingFixture();check(callbacks.getInt("enabled_callbacks")>0&&callbacks.getInt("scale_callbacks")>0&&callbacks.getInt("style_callbacks")>0,"Real consumer did not receive native caption callbacks");
            for(String key:HEARING_SECURE)if(!key.equals("accessibility_captioning_enabled")&&!key.equals("accessibility_captioning_font_scale")&&!key.equals("accessibility_captioning_preset"))check(java.util.Objects.equals(secure.get(key),hearingRaw("secure",key)),"Preset rewrote custom caption field "+key);
            shell("settings put secure accessibility_captioning_font_scale 1.25");hearingRefresh();hearingTop();hearingReveal(n->role(n,"TextView")&&label(n).equals("125%"),"custom font size readback");check(hearingRaw("secure","accessibility_captioning_font_scale").equals("1.25"),"Read normalized custom caption size");hearingConsumer(true,1.25f);
            shell("am force-stop "+HOME);hearingPage();hearingSelected("Caption style: Custom");hearingTop();hearingReveal(n->role(n,"TextView")&&label(n).equals("125%"),"caption persistence after Home restart");
            hearingChoice("Use captions: Off");waitSetting("secure","accessibility_captioning_enabled","0");hearingConsumer(false,1.25f);
            AccessibilityNodeInfo retiredPreset=hearingReveal(n->role(n,"Button")&&n.isEnabled()&&label(n).equals("Caption style: Set by app"),"known preset before external change");
            // Native CaptioningManager dispatch itself indexes preset arrays. Stop the
            // synthetic listener before deliberate malformed-provider fault injection.
            shell("am force-stop "+HEARING_FIXTURE);
            hearingPlaybackStarted=false;
            shell("settings put secure accessibility_captioning_preset 99");hearingRefresh();hearingTop();hearingReveal(n->role(n,"TextView")&&label(n).equals("Unavailable"),"unknown preset readback");
            check(nodes().stream().noneMatch(n->role(n,"Button")&&n.isEnabled()&&label(n).startsWith("Caption style: ")),"Unknown caption preset offered mutations");check(!retiredPreset.performAction(AccessibilityNodeInfo.ACTION_CLICK),"Retired preset remained actionable");check(hearingRaw("secure","accessibility_captioning_preset").equals("99"),"Unknown preset normalized by read");
            shell("settings put secure accessibility_captioning_preset 0");hearingRefresh();hearingSelected("Caption style: White on black");
            result.putBoolean("native_audio_outputs",true);result.putBoolean("native_caption_callbacks_and_rendering",true);result.putBoolean("all_five_sizes_and_six_presets",true);result.putBoolean("native_caption_enable_coupling",true);result.putBoolean("custom_values_and_fields_preserved",true);result.putBoolean("unknown_preset_denied",true);result.putBoolean("restart_persistence",true);
        }catch(Exception|Error failure){originalFailure=failure;throw failure;
        }finally{
            try{hearingRestore(system,secure);}catch(RuntimeException|Error cleanupFailure){if(originalFailure==null)throw cleanupFailure;originalFailure.addSuppressed(cleanupFailure);}
        }
        for(String key:HEARING_SYSTEM)check(java.util.Objects.equals(system.get(key),hearingRaw("system",key)),"Audio raw restoration failed");for(String key:HEARING_SECURE)check(java.util.Objects.equals(secure.get(key),hearingRaw("secure",key)),"Caption raw restoration failed");hearingAudio(originalMono,originalBalance);openRoute("overview","OctoSense Settings");result.putBoolean("restored",true);
    }
    private void captionTop(){
        long deadline=SystemClock.uptimeMillis()+10000;
        do{AccessibilityNodeInfo scroll=null;boolean any=false;for(AccessibilityNodeInfo n:nodes())if(role(n,"ScrollView")){any=true;if(action(n,AccessibilityNodeInfo.ACTION_SCROLL_BACKWARD))scroll=n;}
            if(any&&scroll==null)return;if(scroll!=null&&scroll.performAction(AccessibilityNodeInfo.ACTION_SCROLL_BACKWARD))checks++;SystemClock.sleep(150);
        }while(SystemClock.uptimeMillis()<deadline);throw new AssertionError("Caption page did not reach top");
    }
    private AccessibilityNodeInfo captionFind(Match match,String description){
        long deadline=SystemClock.uptimeMillis()+20000;
        do{List<AccessibilityNodeInfo> visible=nodes();for(AccessibilityNodeInfo n:visible)if(match.accepts(n))return n;
            AccessibilityNodeInfo scroll=null;for(AccessibilityNodeInfo n:visible)if(role(n,"ScrollView")&&action(n,AccessibilityNodeInfo.ACTION_SCROLL_FORWARD)){scroll=n;break;}
            if(scroll==null)captionTop();else if(scroll.performAction(AccessibilityNodeInfo.ACTION_SCROLL_FORWARD))checks++;SystemClock.sleep(200);
        }while(SystemClock.uptimeMillis()<deadline);throw new AssertionError("Missing caption control: "+description);
    }
    private void captionTap(String name){captionTop();captionFind(n->role(n,"Button")&&label(n).equals(name),name);click(waitNode(n->role(n,"Button")&&n.isEnabled()&&label(n).equals(name),"enabled "+name));}
    private void captionLanguagePage(){hearingPage();hearingChoice("Caption language");pane("Caption language");}
    private void captionLanguageQuery(String query){captionTop();setText(waitNode(n->role(n,"EditText")&&label(n).equals("Filter caption languages"),"caption language filter"),query);captionTap("Search languages");}
    private AccessibilityNodeInfo captionLocaleRow(String locale){captionTop();return captionFind(n->role(n,"Button")&&n.isEnabled()&&(locale.isEmpty()?label(n).startsWith("System default"):label(n).contains("\n"+locale)),"native caption locale "+locale);}
    private void captionLocaleSelection(String locale){
        captionTop();captionFind(n->role(n,"Button")&&n.isEnabled()&&label(n).endsWith(" · Selected")&&(locale.isEmpty()?label(n).startsWith("System default"):label(n).contains("\n"+locale)),"fresh selected caption locale "+locale);
    }
    private void captionLocaleObserved(String locale)throws Exception{
        long end=SystemClock.uptimeMillis()+10000;
        do{JSONObject observed=hearingFixture().getJSONObject("observed");if(locale.isEmpty()?observed.isNull("locale"):locale.equals(observed.optString("locale"))){checks++;return;}SystemClock.sleep(100);}while(SystemClock.uptimeMillis()<end);throw new AssertionError("CaptioningManager locale did not propagate");
    }
    private void captionLanguageUnavailable(Bundle result){
        java.util.Map<String,String> before=new java.util.LinkedHashMap<>();for(String key:HEARING_SECURE)before.put(key,hearingRaw("secure",key));captionLanguagePage();
        waitNode(n->role(n,"TextView")&&label(n).equals("Caption language controls are unavailable on this installation."),"unavailable native caption language service");
        check(nodes().stream().noneMatch(n->role(n,"Button")&&label(n).contains("Selected")),"Ordinary installation fabricated a selected caption locale");
        shell("am start -W -a android.settings.SOUND_SETTINGS -p com.android.settings");waitPackage("com.android.settings");shell("am start -W -n "+HOME+"/.MakepadApp");pane("Caption language");captionTap("Android caption settings");waitPackage("com.android.settings");shell("input keyevent 4");pane("Caption language");click(button("Back"));pane("Accessibility: hearing");click(button("Back"));pane("System");
        for(String key:HEARING_SECURE)check(java.util.Objects.equals(before.get(key),hearingRaw("secure",key)),"Ordinary caption language path wrote preference");result.putBoolean("unavailable_and_native_back",true);
    }
    private void captionLanguage(Bundle result)throws Exception{
        java.util.Map<String,String> system=new java.util.LinkedHashMap<>(),secure=new java.util.LinkedHashMap<>();for(String key:HEARING_SYSTEM)system.put(key,hearingRaw("system",key));for(String key:HEARING_SECURE)secure.put(key,hearingRaw("secure",key));
        String systemLocales=hearingRaw("system","system_locales"),configurationLocales=getTargetContext().getResources().getConfiguration().getLocales().toLanguageTags();Throwable originalFailure=null;
        try{
            shell("settings put secure accessibility_captioning_enabled 0");
            // Empty values need shell stdin; executeShellCommand does not parse quotes.
            java.util.Map<String,String> initial=new java.util.LinkedHashMap<>();initial.put("accessibility_captioning_locale","");hearingRestore(java.util.Collections.emptyMap(),initial);
            shell("am start -W -n "+HEARING_FIXTURE+"/.CaptionActivity");waitPackage(HEARING_FIXTURE);int callbacks=hearingFixture().getInt("locale_callbacks");captionLanguagePage();
            captionLocaleRow("");captionTap("Next languages");captionTap("Previous languages");captionLanguageQuery("fr_FR");click(captionLocaleRow("fr_FR"));waitSetting("secure","accessibility_captioning_locale","fr_FR");captionLocaleObserved("fr_FR");captionLocaleSelection("fr_FR");check(hearingRaw("secure","accessibility_captioning_enabled").equals("0"),"Caption locale unexpectedly enabled captions");
            captionLanguageQuery("zh_CN");click(captionLocaleRow("zh_CN"));waitSetting("secure","accessibility_captioning_locale","zh_CN");captionLocaleObserved("zh_CN");captionLocaleSelection("zh_CN");check(hearingRaw("secure","accessibility_captioning_enabled").equals("0"),"Chinese caption locale enabled captions");
            captionTap("Clear language filter");click(captionLocaleRow(""));waitSetting("secure","accessibility_captioning_locale","");captionLocaleObserved("");captionLocaleSelection("");
            captionLanguageQuery("fr_FR");AccessibilityNodeInfo retired=captionLocaleRow("fr_FR");shell("settings put secure accessibility_captioning_locale en_GB");captionTop();waitNode(n->role(n,"TextView")&&label(n).startsWith("Caption language or its available choices changed."),"stale caption locale catalog");check(!retired.performAction(AccessibilityNodeInfo.ACTION_CLICK),"Retired caption language target remained actionable");check(hearingRaw("secure","accessibility_captioning_locale").equals("en_GB"),"Stale caption selection replaced external language");
            shell("settings put secure accessibility_captioning_locale custom_NATIVE");captionTap("Refresh languages");captionTop();waitNode(n->role(n,"TextView")&&label(n).equals("Current caption language: custom_NATIVE · Custom"),"custom raw caption locale");check(hearingRaw("secure","accessibility_captioning_locale").equals("custom_NATIVE"),"Caption read normalized custom raw language");
            shell("am force-stop "+HOME);captionLanguagePage();waitNode(n->role(n,"TextView")&&label(n).equals("Current caption language: custom_NATIVE · Custom"),"custom caption language after Home restart");
            captionLanguageQuery("fr_FR");click(captionLocaleRow("fr_FR"));waitSetting("secure","accessibility_captioning_locale","fr_FR");captionLocaleObserved("fr_FR");captionLocaleSelection("fr_FR");check(hearingFixture().getInt("locale_callbacks")>callbacks,"Independent caption app did not receive locale callbacks");
            check(java.util.Objects.equals(systemLocales,hearingRaw("system","system_locales"))&&configurationLocales.equals(getTargetContext().getResources().getConfiguration().getLocales().toLanguageTags()),"Caption locale changed system/application configuration");
            for(String key:HEARING_SECURE)if(!key.equals("accessibility_captioning_locale")&&!key.equals("accessibility_captioning_enabled"))check(java.util.Objects.equals(secure.get(key),hearingRaw("secure",key)),"Caption language changed unrelated preference");
            shell("am start -W -a android.settings.SOUND_SETTINGS -p com.android.settings");waitPackage("com.android.settings");shell("am start -W -n "+HOME+"/.MakepadApp");pane("Caption language");captionTap("Android caption settings");waitPackage("com.android.settings");shell("input keyevent 4");pane("Caption language");click(button("Back"));pane("Accessibility: hearing");click(button("Back"));pane("System");
            result.putBoolean("native_locale_callbacks",true);result.putBoolean("default_french_chinese_custom",true);result.putBoolean("no_caption_enable_or_system_locale_change",true);result.putBoolean("pagination_search_stale_denial_and_back",true);
        }catch(Exception|Error failure){originalFailure=failure;throw failure;}finally{try{hearingRestore(system,secure);}catch(RuntimeException|Error cleanupFailure){if(originalFailure==null)throw cleanupFailure;originalFailure.addSuppressed(cleanupFailure);}}
        for(String key:HEARING_SECURE)check(java.util.Objects.equals(secure.get(key),hearingRaw("secure",key)),"Caption language restoration failed");openRoute("overview","OctoSense Settings");result.putBoolean("restored",true);
    }
    private void captionField(String name){captionTap(name+": Choose");pane(name);}
    private void captionPick(String field,String choice){
        String name=field+": "+choice;captionTop();click(captionFind(n->role(n,"Button")&&n.isEnabled()&&(label(n).equals(name)||label(n).equals(name+" · Selected")),name));
        captionTop();captionFind(n->role(n,"Button")&&n.isEnabled()&&label(n).equals(name+" · Selected"),"observed "+name);
    }
    private void captionFieldBack(){click(button("Back"));pane("Caption appearance");}
    private void captionStyleObserved(String field,int value)throws Exception{
        long end=SystemClock.uptimeMillis()+10000;
        do{if(hearingFixture().getJSONObject("observed").getInt(field)==value){checks++;return;}SystemClock.sleep(100);}while(SystemClock.uptimeMillis()<end);throw new AssertionError("Native CaptionStyle did not apply "+field);
    }
    private void captionTypefaceObserved(String family)throws Exception{
        long end=SystemClock.uptimeMillis()+10000;
        do{JSONObject observed=hearingFixture().getJSONObject("observed");org.json.JSONArray matches=observed.getJSONArray("typeface_matches");boolean match=family.isEmpty()&&observed.getBoolean("typeface_default");for(int i=0;i<matches.length();i++)match|=matches.getString(i).equals(family);if(match){checks++;return;}SystemClock.sleep(100);}while(SystemClock.uptimeMillis()<end);throw new AssertionError("Native CaptionStyle typeface did not match selected family");
    }
    private void captionCustomUnavailable(Bundle result){
        java.util.Map<String,String> before=new java.util.LinkedHashMap<>();for(String key:HEARING_SECURE)before.put(key,hearingRaw("secure",key));openRoute("caption_custom","Caption appearance");
        captionFind(n->role(n,"TextView")&&label(n).equals("Custom caption appearance is unavailable from the connected Android service."),"ordinary custom caption gate");
        for(String field:new String[]{"Typeface","Text color","Text opacity","Edge type","Edge color","Background color","Background opacity","Window color","Window opacity"}){captionTop();AccessibilityNodeInfo node=captionFind(n->role(n,"Button")&&label(n).equals(field+": Choose"),field);check(!node.isEnabled()&&!node.performAction(AccessibilityNodeInfo.ACTION_CLICK),"Missing service enabled "+field);}
        click(button("Back"));pane("Accessibility: hearing");for(String key:HEARING_SECURE)check(java.util.Objects.equals(before.get(key),hearingRaw("secure",key)),"Ordinary custom caption path changed preference");result.putBoolean("all_nine_fields_unavailable",true);
    }
    private void captionCustom(Bundle result)throws Exception{
        java.util.Map<String,String> system=new java.util.LinkedHashMap<>(),secure=new java.util.LinkedHashMap<>();for(String key:HEARING_SYSTEM)system.put(key,hearingRaw("system",key));for(String key:HEARING_SECURE)secure.put(key,hearingRaw("secure",key));Throwable originalFailure=null;
        try{
            java.util.Map<String,String> initial=new java.util.LinkedHashMap<>();String[][] rows={{"preset","-1"},{"enabled","0"},{"font_scale","1.0"},{"foreground_color","-1"},{"background_color","-16777216"},{"window_color","255"},{"edge_type","0"},{"edge_color","-16777216"},{"typeface",""}};for(String[] row:rows)initial.put("accessibility_captioning_"+row[0],row[1]);hearingRestore(java.util.Collections.emptyMap(),initial);
            shell("am start -W -n "+HEARING_FIXTURE+"/.CaptionActivity");waitPackage(HEARING_FIXTURE);openRoute("caption_custom","Caption appearance");captionField("Typeface");
            for(String[] face:new String[][]{{"Sans-serif","sans-serif"},{"Sans-serif condensed","sans-serif-condensed"},{"Sans-serif monospace","sans-serif-monospace"},{"Serif","serif"},{"Serif monospace","serif-monospace"},{"Casual","casual"},{"Cursive","cursive"},{"Small capitals","sans-serif-smallcaps"},{"Default",""}}){captionPick("Typeface",face[0]);waitSetting("secure","accessibility_captioning_typeface",face[1]);captionTypefaceObserved(face[1]);waitSetting("secure","accessibility_captioning_enabled","1");}captionFieldBack();
            captionField("Text opacity");for(String[] alpha:new String[][]{{"25%","64"},{"50%","128"},{"75%","192"},{"100%","255"}}){captionPick("Text opacity",alpha[0]);int packed=(Integer.parseInt(alpha[1])<<24)|0xffffff;waitSetting("secure","accessibility_captioning_foreground_color",Integer.toString(packed));captionStyleObserved("foreground",packed);}captionFieldBack();
            captionField("Text color");java.util.Set<String> palette=new java.util.HashSet<>();
            for(int page=0;page<4;page++){
                captionTop();final String pageLabel="Choices "+(page*20+1)+"–"+Math.min(65,(page+1)*20)+" of 65";waitNode(n->role(n,"TextView")&&label(n).equals(pageLabel),"fresh palette page "+pageLabel);for(int scroll=0;scroll<12;scroll++){
                    for(AccessibilityNodeInfo n:nodes())if(role(n,"Button")&&label(n).startsWith("Text color: "))palette.add(label(n).replace(" · Selected",""));
                    AccessibilityNodeInfo forward=null;for(AccessibilityNodeInfo n:nodes())if(role(n,"ScrollView")&&action(n,AccessibilityNodeInfo.ACTION_SCROLL_FORWARD)){forward=n;break;}
                    if(forward==null)break;if(forward.performAction(AccessibilityNodeInfo.ACTION_SCROLL_FORWARD))checks++;SystemClock.sleep(200);
                }
                if(page<3)captionTap("Next choices");
            }
            check(palette.size()==65,"Text color palette choices "+palette.size()+": "+new java.util.TreeSet<>(palette));captionPick("Text color","RGB 255, 255, 170");waitSetting("secure","accessibility_captioning_foreground_color","-86");captionStyleObserved("foreground",-86);captionFieldBack();
            // Native opacity caching survives field subpages within a single appearance visit.
            captionField("Text opacity");captionPick("Text opacity","50%");captionFieldBack();captionField("Text color");captionPick("Text color","Default (set by app)");waitSetting("secure","accessibility_captioning_foreground_color",Integer.toString(0x00ffff80));captionFieldBack();
            captionTop();AccessibilityNodeInfo opacity=captionFind(n->role(n,"Button")&&label(n).equals("Text opacity: Choose"),"inherited text opacity");check(!opacity.isEnabled()&&!opacity.performAction(AccessibilityNodeInfo.ACTION_CLICK),"Default color exposed independent opacity mutation");
            captionField("Text color");captionPick("Text color","Blue");waitSetting("secure","accessibility_captioning_foreground_color",Integer.toString(0x800000ff));captionStyleObserved("foreground",0x800000ff);
            captionPick("Text color","Default (set by app)");waitSetting("secure","accessibility_captioning_foreground_color",Integer.toString(0x00ffff80));captionFieldBack();click(button("Back"));pane("Accessibility: hearing");hearingChoice("Custom caption appearance");pane("Caption appearance");captionField("Text color");captionPick("Text color","Blue");waitSetting("secure","accessibility_captioning_foreground_color",Integer.toString(0xff0000ff));captionFieldBack();
            captionField("Edge type");for(String[] edge:new String[][]{{"Default","-1"},{"None","0"},{"Outline","1"},{"Drop shadow","2"},{"Raised","3"},{"Depressed","4"}}){captionPick("Edge type",edge[0]);waitSetting("secure","accessibility_captioning_edge_type",edge[1]);captionStyleObserved("edge_type",Math.max(0,Integer.parseInt(edge[1])));}captionFieldBack();
            captionField("Edge color");captionPick("Edge color","Yellow");waitSetting("secure","accessibility_captioning_edge_color",Integer.toString(0xffffff00));captionStyleObserved("edge_color",0xffffff00);captionFieldBack();
            for(String[] field:new String[][]{{"Background","background"},{"Window","window"}}){captionField(field[0]+" color");captionPick(field[0]+" color","Red");captionFieldBack();captionField(field[0]+" opacity");captionPick(field[0]+" opacity","50%");waitSetting("secure","accessibility_captioning_"+field[1]+"_color",Integer.toString(0x80ff0000));captionStyleObserved(field[1],0x80ff0000);captionFieldBack();captionField(field[0]+" color");captionPick(field[0]+" color","None");waitSetting("secure","accessibility_captioning_"+field[1]+"_color","128");captionFieldBack();captionTop();AccessibilityNodeInfo disabled=captionFind(n->role(n,"Button")&&label(n).equals(field[0]+" opacity: Choose"),field[0]+" inherited opacity");check(!disabled.isEnabled()&&!disabled.performAction(AccessibilityNodeInfo.ACTION_CLICK),"None background exposed opacity setter");}
            shell("am start -W -n "+HEARING_FIXTURE+"/.CaptionActivity");waitPackage(HEARING_FIXTURE);JSONObject consumer=hearingFixture();check(consumer.getInt("style_callbacks")>0&&consumer.getJSONObject("observed").getBoolean("enabled"),"Native custom caption callbacks absent");shell("am start -W -n "+HOME+"/.MakepadApp");pane("Caption appearance");
            shell("am force-stop "+HOME);openRoute("caption_custom","Caption appearance");captionField("Text color");captionFind(n->role(n,"Button")&&label(n).equals("Text color: Blue · Selected"),"custom color persisted after Home process restart");AccessibilityNodeInfo retired=captionFind(n->role(n,"Button")&&n.isEnabled()&&label(n).equals("Text color: Red"),"color before preset change");shell("settings put secure accessibility_captioning_preset 0");captionTop();waitNode(n->role(n,"TextView")&&label(n).startsWith("No authorized choices."),"preset change clears custom editor choices");check(!retired.performAction(AccessibilityNodeInfo.ACTION_CLICK),"Old custom color survived preset change");check(hearingRaw("secure","accessibility_captioning_foreground_color").equals(Integer.toString(0xff0000ff)),"Retired color action mutated new preset");
            check(java.util.Objects.equals(secure.get("accessibility_captioning_locale"),hearingRaw("secure","accessibility_captioning_locale")),"Custom style rewrote caption language");result.putBoolean("all_nine_fields",true);result.putBoolean("all_typefaces_edges_opacities",true);result.putBoolean("complete_native_color_palette",true);result.putBoolean("native_custom_style_readback",true);result.putBoolean("opacity_cache_visit_lifetime",true);result.putBoolean("restart_and_preset_stale_denial",true);
        }catch(Exception|Error failure){originalFailure=failure;throw failure;}finally{try{hearingRestore(system,secure);}catch(RuntimeException|Error cleanupFailure){if(originalFailure==null)throw cleanupFailure;originalFailure.addSuppressed(cleanupFailure);}}
        for(String key:HEARING_SECURE)check(java.util.Objects.equals(secure.get(key),hearingRaw("secure",key)),"Custom caption restoration failed");openRoute("overview","OctoSense Settings");result.putBoolean("restored",true);
    }
    private static final String INTERACTION="dev.makepad.octosense.interactionfixture";
    private static final String INTERACTION_PANE="Accessibility: text and interaction";
    private static final String[] INTERACTION_SECURE={"high_text_contrast_enabled","font_weight_adjustment","long_press_timeout","accessibility_interactive_ui_timeout_ms","accessibility_non_interactive_ui_timeout_ms","accessibility_autoclick_enabled","accessibility_autoclick_delay","accessibility_large_pointer_icon"};
    private static final String[] INTERACTION_GLOBAL={"window_animation_scale","transition_animation_scale","animator_duration_scale"};
    private String interactionRaw(String table,String key){return table.equals("global")?android.provider.Settings.Global.getString(getTargetContext().getContentResolver(),key):android.provider.Settings.Secure.getString(getTargetContext().getContentResolver(),key);}
    private java.util.Map<String,String> interactionRows(String table,String[] keys){java.util.Map<String,String> rows=new java.util.LinkedHashMap<>();for(String key:keys)rows.put(key,interactionRaw(table,key));return rows;}
    private void interactionRestore(java.util.Map<String,String> secure,java.util.Map<String,String> global){
        StringBuilder script=new StringBuilder("set -e\n");for(String table:new String[]{"secure","global"})for(java.util.Map.Entry<String,String> row:(table.equals("secure")?secure:global).entrySet()){
            script.append("settings ").append(row.getValue()==null?"delete ":"put ").append(table).append(' ').append(hearingQuote(row.getKey()));if(row.getValue()!=null)script.append(' ').append(hearingQuote(row.getValue()));script.append(" >/dev/null\n");}script.append("echo interaction-restored\n");
        ParcelFileDescriptor[] descriptors=automation.executeShellCommandRw("sh");try(ParcelFileDescriptor.AutoCloseInputStream input=new ParcelFileDescriptor.AutoCloseInputStream(descriptors[0]);ByteArrayOutputStream out=new ByteArrayOutputStream()){
            try(ParcelFileDescriptor.AutoCloseOutputStream writer=new ParcelFileDescriptor.AutoCloseOutputStream(descriptors[1])){writer.write(script.toString().getBytes(StandardCharsets.UTF_8));}byte[] bytes=new byte[4096];int n;while((n=input.read(bytes))!=-1)out.write(bytes,0,n);check(new String(out.toByteArray(),StandardCharsets.UTF_8).trim().equals("interaction-restored"),"Cannot restore text and interaction preferences");
        }catch(IOException failure){throw new AssertionError("Cannot restore interaction settings",failure);}
    }
    private void interactionPage(){openRoute("accessibility_text_interaction",INTERACTION_PANE);}
    private AccessibilityNodeInfo interactionFind(Match match,String name){captionTop();return captionFind(match,name);}
    private void interactionChoice(String name){click(interactionFind(n->role(n,"Button")&&n.isEnabled()&&(label(n).equals(name)||label(n).equals(name+" · Selected")),name));}
    private void interactionSelected(String name){interactionFind(n->role(n,"Button")&&n.isEnabled()&&label(n).equals(name+" · Selected"),"fresh selected "+name);checks++;}
    private void interactionToggleObserved(String name,boolean enabled){
        String selected=name+": "+(enabled?"On":"Off"),other=name+": "+(enabled?"Off":"On");
        // Both choices are disabled while a request is pending. Keep this row
        // visible and require its selected AND enabled alternate in one tree;
        // searching for a disabled choice alone can accept that pending frame
        // and scroll the alternate out of view before fresh readback arrives.
        captionTop();long end=SystemClock.uptimeMillis()+20000;
        do{
            List<AccessibilityNodeInfo> visible=nodes();AccessibilityNodeInfo current=null,alternate=null,forward=null;
            for(AccessibilityNodeInfo node:visible){
                if(role(node,"Button")&&label(node).equals(selected))current=node;
                if(role(node,"Button")&&label(node).equals(other))alternate=node;
                if(role(node,"ScrollView")&&action(node,AccessibilityNodeInfo.ACTION_SCROLL_FORWARD))forward=node;
            }
            if(current!=null&&alternate!=null&&!current.isEnabled()&&alternate.isEnabled()){checks++;return;}
            if(current==null&&alternate==null){if(forward==null)captionTop();else if(forward.performAction(AccessibilityNodeInfo.ACTION_SCROLL_FORWARD))checks++;}
            SystemClock.sleep(200);
        }while(SystemClock.uptimeMillis()<end);
        throw new AssertionError("Missing fresh toggle readback: "+selected);
    }
    private JSONObject interactionFixture(String operation)throws Exception{
        String report=shell("am broadcast --receiver-foreground --include-stopped-packages -n "+INTERACTION+"/.InteractionFixture -a "+INTERACTION+".COMMAND --es operation "+operation);
        Matcher reply=Pattern.compile("Broadcast completed: result=1, data=\"([A-Za-z0-9+/=]+)\"").matcher(report);check(reply.find(),"Independent interaction consumer did not report state");return new JSONObject(new String(android.util.Base64.decode(reply.group(1),android.util.Base64.DEFAULT),StandardCharsets.UTF_8));
    }
    private JSONObject interactionConsumer()throws Exception{
        shell("am start -W -n "+INTERACTION+"/.InteractionActivity");waitPackage(INTERACTION);long end=SystemClock.uptimeMillis()+10000;
        do{JSONObject report=interactionFixture("state"),state=report.getJSONObject("state"),drawn=report.getJSONObject("drawn");if(report.getBoolean("running")&&state.optBoolean("focused")&&state.optBoolean("resumed")&&drawn.has("target")&&state.getJSONArray("target").getInt(2)>0){checks++;return state;}SystemClock.sleep(100);}while(SystemClock.uptimeMillis()<end);throw new AssertionError("Real Android consumer did not draw a focused target");
    }
    private JSONObject interactionNative(String key,int expected)throws Exception{
        long end=SystemClock.uptimeMillis()+10000;do{JSONObject state=interactionFixture("state").getJSONObject("state");if(state.has(key)&&state.getInt(key)==expected){checks++;return state;}SystemClock.sleep(100);}while(SystemClock.uptimeMillis()<end);throw new AssertionError("Native interaction getter did not apply "+key+"="+expected);
    }
    private void interactionMotion(int action,long down,long at,float x,float y,int source){
        android.view.MotionEvent.PointerProperties property=new android.view.MotionEvent.PointerProperties();property.id=0;property.toolType=source==android.view.InputDevice.SOURCE_MOUSE?android.view.MotionEvent.TOOL_TYPE_MOUSE:android.view.MotionEvent.TOOL_TYPE_FINGER;
        android.view.MotionEvent.PointerCoords point=new android.view.MotionEvent.PointerCoords();point.x=x;point.y=y;point.pressure=action==android.view.MotionEvent.ACTION_UP?0:1;point.size=1;
        android.view.MotionEvent event=android.view.MotionEvent.obtain(down,at,action,1,new android.view.MotionEvent.PointerProperties[]{property},new android.view.MotionEvent.PointerCoords[]{point},0,0,1,1,0,0,source,0);
        try{check(automation.injectInputEvent(event,true),"Native interaction event was rejected");}finally{event.recycle();}
    }
    private void interactionHold(int timeout)throws Exception{
        interactionConsumer();JSONObject state=interactionNative("long_press_ms",timeout);interactionFixture("reset_counters");org.json.JSONArray target=state.getJSONArray("target");float x=target.getInt(0)+target.getInt(2)/2f,y=target.getInt(1)+target.getInt(3)/2f;long down=SystemClock.uptimeMillis();
        interactionMotion(android.view.MotionEvent.ACTION_DOWN,down,down,x,y,android.view.InputDevice.SOURCE_TOUCHSCREEN);
        try{long end=down+timeout+5000;boolean delivered=false;do{JSONObject observed=interactionFixture("state").getJSONObject("state");if(observed.getInt("long_clicks")==1){long actual=observed.getLong("last_long_delay_ms");check(actual>=timeout-50&&actual<=timeout+2000,"Real View long press fired outside native delay tolerance");delivered=true;break;}SystemClock.sleep(40);}while(SystemClock.uptimeMillis()<end);check(delivered,"Real View did not deliver a long press");}
        finally{interactionMotion(android.view.MotionEvent.ACTION_UP,down,SystemClock.uptimeMillis(),x,y,android.view.InputDevice.SOURCE_TOUCHSCREEN);}
        interactionFixture("reset_counters");down=SystemClock.uptimeMillis();interactionMotion(android.view.MotionEvent.ACTION_DOWN,down,down,x,y,android.view.InputDevice.SOURCE_TOUCHSCREEN);interactionMotion(android.view.MotionEvent.ACTION_UP,down,SystemClock.uptimeMillis(),x,y,android.view.InputDevice.SOURCE_TOUCHSCREEN);
        JSONObject tapped=interactionFixture("state").getJSONObject("state");check(tapped.getInt("clicks")==1&&tapped.getInt("long_clicks")==0,"A short tap became a long press");interactionPage();
    }
    private void interactionMouse(boolean enabled,int delay)throws Exception{
        JSONObject state=interactionConsumer();org.json.JSONArray target=state.getJSONArray("target");float x=target.getInt(0)+target.getInt(2)/2f,y=target.getInt(1)+target.getInt(3)/2f;long now=SystemClock.uptimeMillis();
        // UiAutomation injection bypasses InputReader's accessibility filter.
        // A temporary uinput mouse sends movement only through the real path.
        try(KernelMouse mouse=new KernelMouse(automation)){
            mouse.move(1,1);SystemClock.sleep(100);
            boolean positioned=false;
            for(int i=0;i<40;i++){
                JSONObject observed=interactionFixture("state").getJSONObject("state");
                check(observed.getInt("mouse_device")>=0,"Consumer did not receive kernel mouse hover");
                double dx=x-observed.getDouble("mouse_x"),dy=y-observed.getDouble("mouse_y");
                if(Math.abs(dx)<20&&Math.abs(dy)<20){positioned=true;break;}
                mouse.move((int)Math.copySign(Math.max(1,Math.abs(dx)/10),dx),(int)Math.copySign(Math.max(1,Math.abs(dy)/10),dy));SystemClock.sleep(60);
            }
            check(positioned,"Kernel mouse did not reach the measured target");
            interactionFixture("reset_counters");mouse.move(25,0);
        long end=SystemClock.uptimeMillis()+delay+2000;boolean click=false;do{JSONObject observed=interactionFixture("state").getJSONObject("state");if(observed.getInt("mouse_downs")>0||observed.getInt("clicks")>0){if(!enabled){click=true;break;}check(observed.getInt("mouse_downs")<=1&&observed.getInt("clicks")<=1,"Automatic click delivered repeated native clicks");if(observed.getInt("mouse_downs")==1&&observed.getInt("clicks")==1){click=true;check(observed.getLong("last_mouse_delay_ms")>=delay-80&&observed.getLong("last_mouse_delay_ms")<=delay+2000,"Automatic click fired outside configured delay tolerance");break;}}SystemClock.sleep(60);}while(SystemClock.uptimeMillis()<end);check(click==enabled,enabled?"Native automatic click never arrived":"Automatic click fired while Off");}
        check(!KernelMouse.present(),"Kernel mouse not removed");interactionPage();
    }
    private void interactionScales(float expected){
        String apk=getContext().getPackageCodePath();check(apk.matches("/data/app/[A-Za-z0-9_./=+~-]+\\.apk"),"Unexpected fixture APK path");
        long end=SystemClock.uptimeMillis()+10000;String report="";
        do{
            // Invoke the actual read-only Binder getter independently of Agent.
            // Current API35 WindowManager dumps no longer print these fields.
            report=shell("app_process -Djava.class.path="+apk+" / dev.makepad.octosense.settingsa11yfixture.WindowAnimationObserver");
            Matcher values=Pattern.compile("window_animation_scales=([0-9.eE+-]+),([0-9.eE+-]+),([0-9.eE+-]+)").matcher(report);
            if(values.find()){boolean all=true;for(int i=1;i<=3;i++)all&=Math.abs(Float.parseFloat(values.group(i))-expected)<0.00001f;if(all){checks++;return;}}
            SystemClock.sleep(100);
        }while(SystemClock.uptimeMillis()<end);
        throw new AssertionError("WindowManager did not observe all three animation scales: "+report);
    }
    private void interactionUnavailable(Bundle result)throws Exception{
        java.util.Map<String,String> secure=interactionRows("secure",INTERACTION_SECURE),global=interactionRows("global",INTERACTION_GLOBAL);interactionPage();
        for(String name:new String[]{"High contrast text","Bold text","Remove animations","Touch & hold delay","Time to take action","Automatic click","Large mouse pointer"}){interactionFind(n->role(n,"TextView")&&label(n).equals(name),"unavailable "+name);check(nodes().stream().noneMatch(n->role(n,"Button")&&n.isEnabled()&&label(n).startsWith(name+": ")),"Unavailable row advertised an enabled choice: "+name);}
        for(String name:new String[]{"High contrast text: On","Bold text: On","Remove animations: On","Automatic click: Choose","Large mouse pointer: On"}){AccessibilityNodeInfo disabled=interactionFind(n->role(n,"Button")&&!n.isEnabled()&&label(n).equals(name),"disabled "+name);check(!disabled.performAction(AccessibilityNodeInfo.ACTION_CLICK),"Unavailable interaction action was queued");}
        check(nodes().stream().noneMatch(n->role(n,"Button")&&n.isEnabled()&&(label(n).startsWith("Touch & hold delay: ")||label(n).startsWith("Time to take action: "))),"Unavailable native timing fabricated choices");
        shell("am start -W -a android.settings.SOUND_SETTINGS -p com.android.settings");shell("am start -W -n "+HOME+"/.MakepadApp");pane(INTERACTION_PANE);click(interactionFind(n->role(n,"Button")&&n.isEnabled()&&label(n).equals("More in Android Settings"),"native accessibility recovery"));waitPackage("com.android.settings");automation.performGlobalAction(android.accessibilityservice.AccessibilityService.GLOBAL_ACTION_BACK);pane(INTERACTION_PANE);click(button("Back"));pane("System");
        check(secure.equals(interactionRows("secure",INTERACTION_SECURE))&&global.equals(interactionRows("global",INTERACTION_GLOBAL)),"Unavailable path changed preferences");result.putBoolean("unavailable_no_choices",true);result.putBoolean("native_back_and_parent",true);openRoute("overview","OctoSense Settings");
    }
    private void interactionControls(Bundle result)throws Exception{
        java.util.Map<String,String> secure=interactionRows("secure",INTERACTION_SECURE),global=interactionRows("global",INTERACTION_GLOBAL);Throwable original=null;
        try{
            // Establish a reversible known starting state; all writes under test
            // below use built-in controls. Exact original raw rows are restored.
            java.util.Map<String,String> start=new java.util.LinkedHashMap<>();start.put("high_text_contrast_enabled","0");start.put("font_weight_adjustment","0");start.put("accessibility_autoclick_enabled","0");start.put("accessibility_autoclick_delay","0");java.util.Map<String,String> animations=new java.util.LinkedHashMap<>();for(String key:INTERACTION_GLOBAL)animations.put(key,"1.0");interactionRestore(start,animations);interactionPage();
            interactionToggleObserved("High contrast text",false);interactionChoice("High contrast text: On");waitSetting("secure","high_text_contrast_enabled","1");interactionToggleObserved("High contrast text",true);interactionConsumer();JSONObject contrast=interactionFixture("state");if(!contrast.isNull("native_high_contrast"))check(contrast.getBoolean("native_high_contrast"),"Independent high contrast getter remained Off");result.putBoolean("public_contrast_getter",!contrast.isNull("native_high_contrast"));interactionPage();interactionChoice("High contrast text: Off");interactionToggleObserved("High contrast text",false);
            interactionConsumer();JSONObject regular=interactionNative("configuration_weight",0);int normalWeight=regular.getInt("typeface_weight");interactionPage();interactionChoice("Bold text: On");waitSetting("secure","font_weight_adjustment","300");interactionToggleObserved("Bold text",true);interactionConsumer();JSONObject bold=interactionNative("configuration_weight",300);check(bold.getInt("typeface_weight")>normalWeight,"Real Android TextView did not apply heavier font");interactionPage();interactionChoice("Bold text: Off");interactionToggleObserved("Bold text",false);interactionConsumer();interactionNative("configuration_weight",0);interactionPage();
            interactionChoice("Remove animations: On");for(String key:INTERACTION_GLOBAL)waitSetting("global",key,"0.0");interactionToggleObserved("Remove animations",true);interactionScales(0);interactionChoice("Remove animations: Off");for(String key:INTERACTION_GLOBAL)waitSetting("global",key,"1.0");interactionToggleObserved("Remove animations",false);interactionScales(1);
            for(int i=0;i<3;i++){int delay=new int[]{400,1000,1500}[i];String name="Touch & hold delay: "+new String[]{"Short","Medium","Long"}[i];interactionChoice(name);waitSetting("secure","long_press_timeout",Integer.toString(delay));interactionSelected(name);interactionHold(delay);}
            for(int i=0;i<5;i++){int duration=new int[]{0,10000,30000,60000,120000}[i];String name="Time to take action: "+new String[]{"Default","10 seconds","30 seconds","1 minute","2 minutes"}[i];interactionChoice(name);waitSetting("secure","accessibility_interactive_ui_timeout_ms",Integer.toString(duration));waitSetting("secure","accessibility_non_interactive_ui_timeout_ms",Integer.toString(duration));interactionSelected(name);interactionConsumer();long end=SystemClock.uptimeMillis()+10000;boolean applied=false;do{JSONObject state=interactionFixture("state").getJSONObject("state");if(state.getInt("recommended_controls_ms")>=duration&&state.getInt("recommended_other_ms")>=duration){applied=true;break;}SystemClock.sleep(100);}while(SystemClock.uptimeMillis()<end);check(applied,"Native recommended timeouts did not include selected duration");interactionPage();}
            interactionChoice("Automatic click: Choose");pane("Automatic click");for(int delay=200;delay<=1000;delay+=100){String name="Automatic click: "+delay+" ms";interactionChoice(name);waitSetting("secure","accessibility_autoclick_enabled","1");waitSetting("secure","accessibility_autoclick_delay",Integer.toString(delay));interactionSelected(name);}interactionChoice("Automatic click: Off");waitSetting("secure","accessibility_autoclick_enabled","0");waitSetting("secure","accessibility_autoclick_delay","0");interactionSelected("Automatic click: Off");click(button("Back"));pane(INTERACTION_PANE);interactionMouse(false,1000);
            interactionChoice("Automatic click: Choose");pane("Automatic click");interactionChoice("Automatic click: 600 ms");interactionSelected("Automatic click: 600 ms");click(button("Back"));pane(INTERACTION_PANE);interactionMouse(true,600);
            interactionChoice("Automatic click: Choose");pane("Automatic click");interactionChoice("Automatic click: Off");interactionSelected("Automatic click: Off");AccessibilityNodeInfo old=interactionFind(n->role(n,"Button")&&n.isEnabled()&&label(n).equals("Automatic click: 200 ms"),"old automatic click target");shell("settings put secure accessibility_autoclick_delay invalid");interactionFind(n->role(n,"TextView")&&label(n).startsWith("No authorized choices."),"malformed native delay retired choices");check(!old.performAction(AccessibilityNodeInfo.ACTION_CLICK),"Retired automatic click target remained writable");check(setting("secure","accessibility_autoclick_delay").equals("invalid"),"Reading malformed delay normalized it");shell("settings put secure accessibility_autoclick_delay 625");shell("settings put secure accessibility_autoclick_enabled 1");interactionFind(n->role(n,"TextView")&&label(n).equals("Automatic click: On · 625 ms"),"custom delay readback");interactionChoice("Automatic click: Off");interactionSelected("Automatic click: Off");click(button("Back"));pane(INTERACTION_PANE);
            AccessibilityNodeInfo large=interactionFind(n->role(n,"Button")&&label(n).equals("Large mouse pointer: On")&&(n.isEnabled()||nodes().stream().anyMatch(other->role(other,"Button")&&other.isEnabled()&&label(other).equals("Large mouse pointer: Off"))||nodes().stream().anyMatch(other->role(other,"TextView")&&label(other).equals("Not supported on this device"))),"large pointer capability");if(large.isEnabled()){click(large);waitSetting("secure","accessibility_large_pointer_icon","1");interactionToggleObserved("Large mouse pointer",true);interactionChoice("Large mouse pointer: Off");waitSetting("secure","accessibility_large_pointer_icon","0");interactionToggleObserved("Large mouse pointer",false);result.putBoolean("large_pointer_supported",true);}else{AccessibilityNodeInfo off=waitNode(n->role(n,"Button")&&label(n).equals("Large mouse pointer: Off"),"large pointer Off");if(off.isEnabled()){click(off);interactionToggleObserved("Large mouse pointer",false);interactionChoice("Large mouse pointer: On");interactionToggleObserved("Large mouse pointer",true);interactionChoice("Large mouse pointer: Off");interactionToggleObserved("Large mouse pointer",false);result.putBoolean("large_pointer_supported",true);}else{check(!large.performAction(AccessibilityNodeInfo.ACTION_CLICK),"Unsupported pointer action queued");check(java.util.Objects.equals(secure.get("accessibility_large_pointer_icon"),interactionRaw("secure","accessibility_large_pointer_icon")),"Unsupported pointer value changed");result.putBoolean("large_pointer_supported",false);}}
            shell("settings put secure long_press_timeout 735");interactionFind(n->role(n,"TextView")&&label(n).equals("Custom: 735 ms"),"custom external hold delay");interactionChoice("Touch & hold delay: Medium");interactionSelected("Touch & hold delay: Medium");shell("am force-stop "+HOME);interactionPage();interactionSelected("Touch & hold delay: Medium");interactionChoice("Automatic click: Choose");pane("Automatic click");interactionSelected("Automatic click: Off");click(button("Back"));pane(INTERACTION_PANE);click(button("Back"));pane("System");result.putBoolean("all_offered_choices",true);result.putBoolean("native_view_hold_and_mouse",true);result.putBoolean("native_configuration_and_recommended_timeout",true);result.putBoolean("window_manager_scales",true);result.putBoolean("stale_external_restart_back",true);
        }catch(Exception|Error failure){original=failure;throw failure;}finally{try{interactionRestore(secure,global);}catch(RuntimeException|Error cleanup){if(original==null)throw cleanup;original.addSuppressed(cleanup);}}
        check(secure.equals(interactionRows("secure",INTERACTION_SECURE))&&global.equals(interactionRows("global",INTERACTION_GLOBAL)),"Exact interaction preferences not restored");result.putBoolean("restored",true);openRoute("overview","OctoSense Settings");
    }

    @Override public void onStart() {
        Bundle result=new Bundle();boolean passed=false;
        try {
            automation=getUiAutomation(UiAutomation.FLAG_DONT_SUPPRESS_ACCESSIBILITY_SERVICES);
            if(scenario.equals("default_roles")||scenario.equals("roles_unavailable")||scenario.equals("runtime_permissions")||scenario.equals("permissions_legacy")){
                android.accessibilityservice.AccessibilityServiceInfo info=automation.getServiceInfo();
                info.flags|=android.accessibilityservice.AccessibilityServiceInfo.FLAG_REPORT_VIEW_IDS
                        |android.accessibilityservice.AccessibilityServiceInfo.FLAG_RETRIEVE_INTERACTIVE_WINDOWS;automation.setServiceInfo(info);
            }
            if(scenario.equals("inspect"))inspect(result);else if(scenario.equals("smoke"))smoke(result);
            else if(scenario.equals("entries"))entries(result);else if(scenario.equals("cold_entries"))coldEntries(result);
            else if(scenario.equals("rom_defaults"))romDefaults(result);else if(scenario.equals("preferred_fallback"))preferredFallback(result);
            else if(scenario.equals("density"))density(result);
            else if(scenario.equals("display_controls"))displayControls(result);
            else if(scenario.equals("night_unavailable"))nightUnavailable(result);
            else if(scenario.equals("sound_feedback"))soundFeedback(result);
            else if(scenario.equals("notification_channels"))notificationChannels(result);
            else if(scenario.equals("notification_unavailable"))notificationUnavailable(result);
            else if(scenario.equals("notification_entries"))notificationEntries(result);
            else if(scenario.equals("roles_unavailable"))rolesUnavailable(result);
            else if(scenario.equals("default_roles"))defaultRoles(result);
            else if(scenario.equals("roles_entries"))rolesEntries(result);
            else if(scenario.equals("runtime_permissions"))runtimePermissions(result);
            else if(scenario.equals("permissions_legacy"))legacyPermissionWarnings(result);
            else if(scenario.equals("permissions_unavailable"))permissionsUnavailable(result);
            else if(scenario.equals("dnd_observer"))dndObserver(result);
            else if(scenario.equals("dnd_policy"))dndPolicy(result);
            else if(scenario.equals("dnd_schedules"))dndSchedules(result);
            else if(scenario.equals("dnd_unavailable"))dndUnavailable(result);
            else if(scenario.equals("dnd_entries"))dndEntries(result);
            else if(scenario.equals("app_network"))appNetwork(result);
            else if(scenario.equals("app_network_unavailable"))appNetworkUnavailable(result);
            else if(scenario.equals("app_battery"))appBattery(result);
            else if(scenario.equals("app_battery_unavailable"))appBatteryUnavailable(result);
            else if(scenario.equals("hearing_controls"))hearingControls(result);
            else if(scenario.equals("hearing_unavailable"))hearingUnavailable(result);
            else if(scenario.equals("caption_language"))captionLanguage(result);
            else if(scenario.equals("caption_language_unavailable"))captionLanguageUnavailable(result);
            else if(scenario.equals("caption_custom"))captionCustom(result);
            else if(scenario.equals("caption_custom_unavailable"))captionCustomUnavailable(result);
            else if(scenario.equals("text_interaction"))interactionControls(result);
            else if(scenario.equals("text_interaction_unavailable"))interactionUnavailable(result);
            else if(scenario.equals("app_language"))appLanguage(result);
            else if(scenario.equals("app_language_inspect"))languageInspect(result);
            else if(scenario.equals("app_language_unavailable"))languageUnavailable(result);
            else if(scenario.equals("app_storage"))appStorage(result);
            else if(scenario.equals("vision_controls"))visionControls(result);
            else if(scenario.equals("vision_unavailable"))visionUnavailable(result);
            else if(scenario.equals("app_storage_unavailable"))storageUnavailable(result);
            else throw new IllegalArgumentException("Unknown scenario");
            passed=true;
        }catch(Throwable failure){
            result.putString("failure",failure.getClass().getSimpleName()+": "+failure.getMessage());
            if(automation!=null)try {
                AccessibilityNodeInfo root=automation.getRootInActiveWindow();result.putString("root_package",root==null?"none":value(root.getPackageName()));
                int text=0,buttons=0,scroll=0;for(AccessibilityNodeInfo node:nodes()) {
                    if(role(node,"TextView"))text++;if(role(node,"Button"))buttons++;if(role(node,"ScrollView"))scroll++;
                }
                result.putInt("text_nodes",text);result.putInt("buttons",buttons);result.putInt("scroll_containers",scroll);
                if(scenario.equals("runtime_permissions")||scenario.equals("permissions_legacy")||scenario.equals("permissions_unavailable")||scenario.startsWith("dnd_")||scenario.equals("app_network")||scenario.startsWith("app_battery")||scenario.startsWith("app_storage")||scenario.startsWith("app_language")||scenario.startsWith("hearing_")||scenario.startsWith("caption_")){
                    StringBuilder visible=new StringBuilder();for(AccessibilityNodeInfo node:nodes()){
                        if(visible.length()>4096)break;
                        if(role(node,"TextView")||role(node,"Button"))visible.append(node.getClassName()).append(" enabled=").append(node.isEnabled()).append(" ").append(label(node)).append('\n');
                    }
                    result.putString(scenario.startsWith("dnd_")?"dnd_fixture_visible_nodes":"permission_fixture_visible_nodes",visible.toString());
                }
            }catch(RuntimeException ignored) { }
        }
        result.putInt("checks",checks);if(navigationRetries>0)result.putInt("retired_navigation_retries",navigationRetries);
        result.putBoolean("passed",passed);finish(passed?0:1,result);
    }
}
