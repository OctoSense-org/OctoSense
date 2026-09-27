package dev.makepad.octosense.controls;

/** Compiled control vocabulary; renderer inputs can never select a provider key. */
public final class SettingsControlsContract {
    private SettingsControlsContract() {}
    public enum Page {
        LOCATION("location"), PRIVACY("privacy"), NOTIFICATIONS("notifications"), BATTERY_POLICY("battery_policy"), SOUND_FEEDBACK("sound_feedback"), ACCESSIBILITY_VISION("accessibility_vision"), ACCESSIBILITY_HEARING("accessibility_hearing"), ACCESSIBILITY_TEXT_INTERACTION("accessibility_text_interaction");
        public final String wire;
        Page(String wire) {this.wire=wire;}
        public static Page parse(String value) {
            for(Page page:values()) if(page.wire.equals(value)) return page;
            throw new IllegalArgumentException("Unknown controls page");
        }
    }
    public enum Control {
        LOCATION_ENABLED("location_enabled",Page.LOCATION,null,null,0),
        WIFI_SCANNING("wifi_scanning",Page.LOCATION,null,null,0),
        BLUETOOTH_SCANNING("bluetooth_scanning",Page.LOCATION,"global","ble_scan_always_enabled",0),
        CAMERA_ACCESS("camera_access",Page.PRIVACY,null,null,0),
        MICROPHONE_ACCESS("microphone_access",Page.PRIVACY,null,null,0),
        DND_MODE("dnd_mode",Page.NOTIFICATIONS,null,null,0),
        LOCKSCREEN_NOTIFICATIONS("lockscreen_notifications",Page.NOTIFICATIONS,"secure","lock_screen_show_notifications",1),
        LOCKSCREEN_SENSITIVE("lockscreen_sensitive",Page.NOTIFICATIONS,"secure","lock_screen_allow_private_notifications",1),
        NOTIFICATION_HISTORY("notification_history",Page.NOTIFICATIONS,"secure","notification_history_enabled",0),
        NOTIFICATION_BUBBLES("notification_bubbles",Page.NOTIFICATIONS,"secure","notification_bubbles",1),
        BATTERY_SAVER("battery_saver",Page.BATTERY_POLICY,null,null,0),
        BATTERY_THRESHOLD("battery_threshold",Page.BATTERY_POLICY,"global","low_power_trigger_level",0),
        BATTERY_DISABLE_AT_90("battery_disable_at_90",Page.BATTERY_POLICY,"global","low_power_sticky_auto_disable_enabled",1),
        ADAPTIVE_BATTERY("adaptive_battery",Page.BATTERY_POLICY,"global","adaptive_battery_management_enabled",1),
        CHARGING_SOUNDS("charging_sounds",Page.SOUND_FEEDBACK,null,null,1),
        CHARGING_VIBRATION("charging_vibration",Page.SOUND_FEEDBACK,null,null,1),
        LOCK_SOUNDS("lock_sounds",Page.SOUND_FEEDBACK,null,null,1),
        DIALPAD_TONES("dialpad_tones",Page.SOUND_FEEDBACK,null,null,1),
        VIBRATION_ENABLED("vibration_enabled",Page.SOUND_FEEDBACK,null,null,1),
        KEYBOARD_VIBRATION("keyboard_vibration",Page.SOUND_FEEDBACK,null,null,1),
        RING_VIBRATION("ring_vibration",Page.SOUND_FEEDBACK,null,null,1),
        NOTIFICATION_VIBRATION("notification_vibration",Page.SOUND_FEEDBACK,null,null,1),
        ALARM_VIBRATION("alarm_vibration",Page.SOUND_FEEDBACK,null,null,1),
        MEDIA_VIBRATION("media_vibration",Page.SOUND_FEEDBACK,null,null,1),
        TOUCH_VIBRATION("touch_vibration",Page.SOUND_FEEDBACK,null,null,1),
        COLOR_INVERSION("color_inversion",Page.ACCESSIBILITY_VISION,null,null,0),
        COLOR_CORRECTION("color_correction",Page.ACCESSIBILITY_VISION,null,null,0),
        COLOR_CORRECTION_MODE("color_correction_mode",Page.ACCESSIBILITY_VISION,null,null,12),
        MONO_AUDIO("mono_audio",Page.ACCESSIBILITY_HEARING,null,null,0),
        AUDIO_BALANCE("audio_balance",Page.ACCESSIBILITY_HEARING,null,null,0),
        CAPTIONS_ENABLED("captions_enabled",Page.ACCESSIBILITY_HEARING,null,null,0),
        CAPTIONS_FONT_SCALE("captions_font_scale",Page.ACCESSIBILITY_HEARING,null,null,1),
        CAPTIONS_PRESET("captions_preset",Page.ACCESSIBILITY_HEARING,null,null,0),
        HIGH_CONTRAST_TEXT("high_contrast_text",Page.ACCESSIBILITY_TEXT_INTERACTION,null,null,0),
        BOLD_TEXT("bold_text",Page.ACCESSIBILITY_TEXT_INTERACTION,null,null,0),
        REMOVE_ANIMATIONS("remove_animations",Page.ACCESSIBILITY_TEXT_INTERACTION,null,null,0),
        TOUCH_HOLD_DELAY("touch_hold_delay",Page.ACCESSIBILITY_TEXT_INTERACTION,null,null,0),
        ACTION_TIMEOUT("action_timeout",Page.ACCESSIBILITY_TEXT_INTERACTION,null,null,0),
        AUTOCLICK("autoclick",Page.ACCESSIBILITY_TEXT_INTERACTION,null,null,0),
        LARGE_POINTER("large_pointer",Page.ACCESSIBILITY_TEXT_INTERACTION,null,null,0);
        public final String wire,table,key; public final Page page; public final int defaultValue;
        Control(String wire,Page page,String table,String key,int defaultValue) {
            this.wire=wire;this.page=page;this.table=table;this.key=key;this.defaultValue=defaultValue;
        }
        public static Control parse(Page page,String value) {
            for(Control control:values()) if(control.wire.equals(value)&&control.page==page) return control;
            throw new IllegalArgumentException("Control does not belong to page");
        }
        public AccessibilityTextMotorSettings.Field textInteraction(){return page==Page.ACCESSIBILITY_TEXT_INTERACTION?AccessibilityTextMotorSettings.Field.parse(wire):null;}
        public HearingSettings.Setting hearingSetting() {
            switch(this){case MONO_AUDIO:return HearingSettings.Setting.MONO;case AUDIO_BALANCE:return HearingSettings.Setting.BALANCE;case CAPTIONS_ENABLED:return HearingSettings.Setting.CAPTIONS_ENABLED;case CAPTIONS_FONT_SCALE:return HearingSettings.Setting.CAPTIONS_FONT_SCALE;case CAPTIONS_PRESET:return HearingSettings.Setting.CAPTIONS_PRESET;default:return null;}
        }
        public ColorAccessibility.Setting colorSetting() {
            switch(this) {case COLOR_INVERSION:return ColorAccessibility.Setting.INVERSION;case COLOR_CORRECTION:return ColorAccessibility.Setting.CORRECTION;case COLOR_CORRECTION_MODE:return ColorAccessibility.Setting.MODE;default:return null;}
        }
        public boolean intensity() {return this==RING_VIBRATION||this==NOTIFICATION_VIBRATION||this==ALARM_VIBRATION||this==MEDIA_VIBRATION||this==TOUCH_VIBRATION;}
        public String[] choices() {
            if(textInteraction()!=null)return textInteraction().choices();
            if(hearingSetting()!=null)return hearingSetting().choices();
            if(colorSetting()!=null)return colorSetting().choices();
            if(intensity())return new String[]{"off","default","low","medium","high"};
            if(this==DND_MODE) return DndMode.choices();
            return this==BATTERY_THRESHOLD?new String[]{"0","5","10","15","20","25","30","40","50","75"}:new String[]{"off","on"};
        }
        public String validate(String value) {
            for(String choice:choices()) if(choice.equals(value)) return value;
            throw new IllegalArgumentException("Invalid control value");
        }
        public int stored(String value) {
            validate(value);
            if(textInteraction()!=null)throw new IllegalArgumentException("Text and interaction require native readback");
            if(hearingSetting()!=null)throw new IllegalArgumentException("Hearing requires typed native service readback");
            if(colorSetting()!=null)return colorSetting().stored(value);
            if(intensity())throw new IllegalArgumentException("Intensity requires actual device levels");
            if(this==DND_MODE) throw new IllegalArgumentException("DND requires the system service");
            return this==BATTERY_THRESHOLD?Integer.parseInt(value):value.equals("on")?1:0;
        }
        public String observed(int value) {
            if(textInteraction()!=null)return null;
            if(hearingSetting()!=null)return hearingSetting().observed(Integer.toString(value));
            if(colorSetting()!=null)return colorSetting().observed(Integer.toString(value));
            if(this==DND_MODE) {DndMode mode=DndMode.fromZen(value);return mode==null?null:mode.wire;}
            if(this==BATTERY_THRESHOLD) return value>=0&&value<=100?Integer.toString(value):null;
            return value==0?"off":value==1?"on":null;
        }
        public String observedRaw(String raw) {
            if(textInteraction()!=null)return null;
            if(hearingSetting()!=null)return hearingSetting().observed(raw);
            if(colorSetting()!=null)return colorSetting().observed(raw);
            // Only verified Android effective defaults are used. Legacy and
            // current lock-screen controllers disagree for absent rows.
            if(intensity())return null;
            if(raw==null) return this==LOCKSCREEN_NOTIFICATIONS||this==LOCKSCREEN_SENSITIVE?null:observed(defaultValue);
            if(!raw.matches("-?[0-9]+")) return null;
            try {return observed(Integer.parseInt(raw));} catch(NumberFormatException invalid) {return null;}
        }
    }
}
