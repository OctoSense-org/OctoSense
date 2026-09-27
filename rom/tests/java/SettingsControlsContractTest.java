import dev.makepad.octosense.controls.SettingsControlsContract.Page;
import dev.makepad.octosense.controls.SettingsControlsContract.Control;
import dev.makepad.octosense.controls.DndMode;

public final class SettingsControlsContractTest {
    private static void require(boolean value) {if(!value) throw new AssertionError();}
    private static void rejects(Runnable fn) {
        try {fn.run();throw new AssertionError("Invalid control was accepted");}
        catch(IllegalArgumentException expected) { }
    }
    public static void main(String[] args) {
        for(String page:new String[]{null,"system","global","secure","privacy;settings put", "LOCATION"}) rejects(() -> Page.parse(page));
        for(Page page:Page.values()) {
            require(Page.parse(page.wire)==page);
            for(Control control:Control.values()) {
                if(control.page!=page) rejects(() -> Control.parse(page,control.wire));
                else require(Control.parse(page,control.wire)==control);
            }
        }
        for(Control control:Control.values()) {
            for(String choice:control.choices()) require(control.validate(choice).equals(choice));
            for(String bad:new String[]{null,"true","false","NaN","shell","-1","101","on\n"}) rejects(() -> control.validate(bad));
        }
        require(Control.AUTOCLICK.choices().length==10);
        require(Control.TOUCH_HOLD_DELAY.validate("hold:long").equals("hold:long"));
        require(Control.ACTION_TIMEOUT.validate("timeout:default").equals("timeout:default"));
        rejects(()->Control.AUTOCLICK.validate("autoclick_delay:2000"));
        rejects(()->Control.BOLD_TEXT.stored("on"));
        require(Control.BOLD_TEXT.observedRaw(null)==null);
        rejects(() -> Control.BATTERY_THRESHOLD.validate("23"));
        require(Control.BATTERY_THRESHOLD.observed(23).equals("23"));
        require(Control.BATTERY_THRESHOLD.observed(101)==null);
        require(Control.LOCATION_ENABLED.observed(3)==null);
        require(Control.CAMERA_ACCESS.stored("on")==1);
        require(Control.MICROPHONE_ACCESS.stored("off")==0);
        require(Control.BATTERY_THRESHOLD.stored("0")==0);
        require(Control.BATTERY_THRESHOLD.stored("75")==75);
        require(Control.LOCKSCREEN_NOTIFICATIONS.observedRaw(null)==null);
        require(Control.LOCKSCREEN_SENSITIVE.observedRaw(null)==null);
        require(Control.NOTIFICATION_HISTORY.observedRaw(null).equals("off"));
        require(Control.ADAPTIVE_BATTERY.observedRaw(null).equals("on"));
        for(Control control:Control.values()) for(String raw:new String[]{"bad"," 1","NaN","Infinity"})
            require(control.observedRaw(raw)==null);
        for(Control control:Control.values()) if(control!=Control.AUDIO_BALANCE&&control!=Control.CAPTIONS_FONT_SCALE)
            for(String raw:new String[]{"1.0","99999999999999999999","2e0"})require(control.observedRaw(raw)==null);
        require(Control.AUDIO_BALANCE.observedRaw("0.123").equals("balance:0.123"));
        require(Control.CAPTIONS_FONT_SCALE.observedRaw("1.25").equals("caption_scale:1.25"));
        rejects(()->Control.AUDIO_BALANCE.validate("balance:0.123"));
        rejects(()->Control.CAPTIONS_FONT_SCALE.validate("caption_scale:1.25"));
        require(Control.BATTERY_THRESHOLD.observedRaw("23").equals("23"));
        require(Control.DND_MODE.table==null&&Control.DND_MODE.key==null);
        for(DndMode mode:DndMode.values()) {
            require(DndMode.parse(mode.wire)==mode);
            require(DndMode.fromZen(mode.zen)==mode);
            require(DndMode.fromInterruptionFilter(mode.interruptionFilter)==mode);
            require(Control.DND_MODE.validate(mode.wire).equals(mode.wire));
            rejects(() -> Control.DND_MODE.stored(mode.wire));
        }
        require(DndMode.ALARMS.zen==3&&DndMode.ALARMS.interruptionFilter==4);
        require(DndMode.SILENCE.zen==2&&DndMode.SILENCE.interruptionFilter==3);
        require(DndMode.fromZen(-1)==null&&DndMode.fromZen(4)==null);
        require(DndMode.fromInterruptionFilter(0)==null&&DndMode.fromInterruptionFilter(5)==null);
        for(String bad:new String[]{null,"on","1","all","priority ","OFF","alarms;reset"})
            rejects(() -> DndMode.parse(bad));
        // Mutations never accept an arbitrary SettingsProvider key or Intent.
        rejects(() -> Control.parse(Page.LOCATION,"location_mode"));
        rejects(() -> Control.parse(Page.PRIVACY,"android.settings.PRIVACY_SETTINGS"));
    }
}
