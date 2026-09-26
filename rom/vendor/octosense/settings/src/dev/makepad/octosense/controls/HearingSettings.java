package dev.makepad.octosense.controls;

import java.util.Arrays;
import java.util.function.Supplier;

/** Finite hearing preferences; custom observations are never silently coerced into choices. */
public final class HearingSettings {
    private HearingSettings(){throw new AssertionError();}
    public enum Setting {
        MONO("mono_audio","master_mono",false,"0"),
        BALANCE("audio_balance","master_balance",false,"0"),
        CAPTIONS_ENABLED("captions_enabled","accessibility_captioning_enabled",true,"0"),
        CAPTIONS_FONT_SCALE("captions_font_scale","accessibility_captioning_font_scale",true,"1"),
        CAPTIONS_PRESET("captions_preset","accessibility_captioning_preset",true,"0");
        public final String id,key;public final boolean secure;private final String missing;
        Setting(String id,String key,boolean secure,String missing){this.id=id;this.key=key;this.secure=secure;this.missing=missing;}
        public static Setting parse(String id){for(Setting value:values())if(value.id.equals(id))return value;throw new IllegalArgumentException("Unknown hearing setting");}
        public String[] choices(){
            switch(this){
                case BALANCE:{String[] choices=new String[201];for(int value=-100;value<=100;value++)choices[value+100]="balance_percent:"+value;return choices;}
                case CAPTIONS_FONT_SCALE:return new String[]{"caption_scale:0.25","caption_scale:0.5","caption_scale:1.0","caption_scale:1.5","caption_scale:2.0"};
                case CAPTIONS_PRESET:return new String[]{"caption_app","caption_white_black","caption_black_white","caption_yellow_black","caption_yellow_blue","caption_custom"};
                default:return new String[]{"off","on"};
            }
        }
        /** Only the native finite choices may be written. */
        public String stored(String choice){
            if(choice==null||!Arrays.asList(choices()).contains(choice))throw new IllegalArgumentException("Unknown hearing choice");
            switch(this){
                case BALANCE:return Float.toString(Integer.parseInt(choice.substring("balance_percent:".length()))/100f);
                case CAPTIONS_FONT_SCALE:return choice.substring("caption_scale:".length());
                case CAPTIONS_PRESET:switch(choice){case "caption_app":return "4";case "caption_white_black":return "0";case "caption_black_white":return "1";case "caption_yellow_black":return "2";case "caption_yellow_blue":return "3";default:return "-1";}
                default:return choice.equals("on")?"1":"0";
            }
        }
        public String observed(String raw){
            String value=raw==null?missing:raw;
            if(this==BALANCE||this==CAPTIONS_FONT_SCALE){
                Float parsed=finiteFloat(value);if(parsed==null)return null;
                if(this==BALANCE&&(parsed < -1f||parsed > 1f)||this==CAPTIONS_FONT_SCALE&&parsed<=0f)return null;
                // Both signs of zero represent centered audio; other custom values stay exact.
                if(parsed==0f)parsed=0f;
                return (this==BALANCE?"balance:":"caption_scale:")+Float.toString(parsed);
            }
            if(!value.matches("-?[0-9]+"))return null;int n;try{n=Integer.parseInt(value);}catch(NumberFormatException unknown){return null;}
            if(this!=CAPTIONS_PRESET)return n==0?"off":n==1?"on":null;
            switch(n){case -1:return "caption_custom";case 0:return "caption_white_black";case 1:return "caption_black_white";case 2:return "caption_yellow_black";case 3:return "caption_yellow_blue";case 4:return "caption_app";default:return null;}
        }
        public String expected(String choice){return observed(stored(choice));}
        public boolean enablesCaptions(){return this==CAPTIONS_FONT_SCALE||this==CAPTIONS_PRESET;}
    }
    private static Float finiteFloat(String value){
        // Provider content is observation only, never a way to smuggle a new mutation value.
        if(value==null||value.isEmpty()||value.length()>64||!value.equals(value.trim()))return null;
        try{float parsed=Float.parseFloat(value);return Float.isFinite(parsed)?parsed:null;}catch(NumberFormatException unknown){return null;}
    }
    public enum Access{WRITABLE,READ_ONLY,RESTRICTED}
    public interface Store{
        String raw(Setting setting);
        /** Canonical observed value from AudioSystem or CaptioningManager; null if unknown. */
        String service(Setting setting);
        boolean write(Setting setting,String finiteStoredValue);
    }
    public static final class Backend{
        private final Store store;private final Supplier<Access> access;
        public Backend(Store store,Supplier<Access> access){this.store=store;this.access=access;}
        public String read(Setting setting){String stored=setting.observed(store.raw(setting));return stored!=null&&stored.equals(store.service(setting))?stored:null;}
        public boolean writable(){return access.get()==Access.WRITABLE;}
        public boolean writable(Setting setting){
            try{return writable()&&(!setting.enablesCaptions()||read(Setting.CAPTIONS_ENABLED)!=null)&&writable();}
            catch(SecurityException|IllegalStateException|IndexOutOfBoundsException|LinkageError unavailable){return false;}
        }
        private String denial(){Access current=access.get();return current==Access.RESTRICTED?"policy_restricted":current==Access.READ_ONLY?"control_unavailable":null;}
        public synchronized String apply(Setting setting,String choice){
            String stored=setting.stored(choice),expected=setting.expected(choice);
            try{
                String denied=denial();if(denied!=null)return denied;
                if(read(setting)==null||setting.enablesCaptions()&&read(Setting.CAPTIONS_ENABLED)==null)return "control_unavailable";
                denied=denial();if(denied!=null)return denied;
                if(!store.write(setting,stored))return "control_unavailable";
            }catch(SecurityException|IllegalStateException|IndexOutOfBoundsException|LinkageError unavailable){return "control_unavailable";}
            // Native font/preset controllers enable captions after writing the chosen field.
            // These writes are not atomic: never replay or call a partial change applied.
            if(setting.enablesCaptions())try{
                if(denial()!=null)return "control_partial";
                String enabled=read(Setting.CAPTIONS_ENABLED);
                if(enabled==null)return "control_partial";
                if(enabled.equals("off")){
                    if(denial()!=null||!store.write(Setting.CAPTIONS_ENABLED,"1"))return "control_partial";
                }
            }catch(SecurityException|IllegalStateException|IndexOutOfBoundsException|LinkageError unavailable){return "control_partial";}
            try{
                if(denial()!=null)return setting.enablesCaptions()?"control_partial":"control_requested";
                boolean confirmed=expected.equals(read(setting))&&(!setting.enablesCaptions()||"on".equals(read(Setting.CAPTIONS_ENABLED)));
                if(denial()!=null)return setting.enablesCaptions()?"control_partial":"control_requested";
                return confirmed?"control_applied":"control_requested";
            }
            catch(SecurityException|IllegalStateException|IndexOutOfBoundsException|LinkageError unavailable){return "control_requested";}
        }
    }
}
