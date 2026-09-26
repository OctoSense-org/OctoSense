package dev.makepad.octosense.controls;

import java.util.Arrays;
import java.util.EnumMap;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.Objects;
import java.util.function.Supplier;

/** Isolated finite policy model. Shared transport/UI integration is intentionally separate. */
public final class AccessibilityTextMotorSettings {
    private AccessibilityTextMotorSettings(){throw new AssertionError();}
    public enum Access {READ_ONLY,WRITABLE,RESTRICTED}
    public enum Key {
        HIGH_CONTRAST(false,"high_text_contrast_enabled"),FONT_WEIGHT(false,"font_weight_adjustment"),
        WINDOW_SCALE(true,"window_animation_scale"),TRANSITION_SCALE(true,"transition_animation_scale"),ANIMATOR_SCALE(true,"animator_duration_scale"),
        LONG_PRESS(false,"long_press_timeout"),INTERACTIVE_TIMEOUT(false,"accessibility_interactive_ui_timeout_ms"),NON_INTERACTIVE_TIMEOUT(false,"accessibility_non_interactive_ui_timeout_ms"),
        AUTOCLICK_ENABLED(false,"accessibility_autoclick_enabled"),AUTOCLICK_DELAY(false,"accessibility_autoclick_delay"),LARGE_POINTER(false,"accessibility_large_pointer_icon");
        public final boolean global;public final String key;Key(boolean global,String key){this.global=global;this.key=key;}
    }
    public enum Field {HIGH_CONTRAST("high_contrast_text"),BOLD_TEXT("bold_text"),REMOVE_ANIMATIONS("remove_animations"),TOUCH_HOLD("touch_hold_delay"),ACTION_TIMEOUT("action_timeout"),AUTOCLICK("autoclick"),LARGE_POINTER("large_pointer");
        public final String id;Field(String id){this.id=id;}
        public static Field parse(String id){for(Field field:values())if(field.id.equals(id))return field;throw new IllegalArgumentException("Unknown accessibility field");}
        public String[] choices(){switch(this){case TOUCH_HOLD:return new String[]{"hold:short","hold:medium","hold:long"};case ACTION_TIMEOUT:return new String[]{"timeout:default","timeout:seconds10","timeout:seconds30","timeout:minute1","timeout:minutes2"};case AUTOCLICK:return new String[]{"off","autoclick_delay:200","autoclick_delay:300","autoclick_delay:400","autoclick_delay:500","autoclick_delay:600","autoclick_delay:700","autoclick_delay:800","autoclick_delay:900","autoclick_delay:1000"};default:return new String[]{"off","on"};}}
        public void validate(String value){if(!Arrays.asList(choices()).contains(value))throw new IllegalArgumentException("Invalid accessibility choice");}
    }
    /** Optional independent native observations; absence never becomes an invented value. */
    public static final class Effects {
        public Boolean highContrast;public Integer fontWeight,longPress,interactiveRecommendation,nonInteractiveRecommendation;public float[] animationScales;
        public final java.util.EnumSet<Field> required=java.util.EnumSet.noneOf(Field.class);
        private Effects copy(){Effects copy=new Effects();copy.highContrast=highContrast;copy.fontWeight=fontWeight;copy.longPress=longPress;copy.interactiveRecommendation=interactiveRecommendation;copy.nonInteractiveRecommendation=nonInteractiveRecommendation;copy.animationScales=animationScales==null?null:animationScales.clone();copy.required.addAll(required);return copy;}
        private boolean available(Field field){if(!required.contains(field))return true;switch(field){case HIGH_CONTRAST:return highContrast!=null;case BOLD_TEXT:return fontWeight!=null;case REMOVE_ANIMATIONS:return animationScales!=null&&animationScales.length==3;case TOUCH_HOLD:return longPress!=null;case ACTION_TIMEOUT:return interactiveRecommendation!=null&&nonInteractiveRecommendation!=null;default:return true;}}
    }
    public static final class State {
        public final Map<Key,String> raw;public final Boolean vectorCursor;public final Effects effects;
        public State(Map<Key,String> raw,Boolean vectorCursor,Effects effects){EnumMap<Key,String> copy=new EnumMap<>(Key.class);for(Key key:Key.values())copy.put(key,raw.get(key));this.raw=java.util.Collections.unmodifiableMap(copy);this.vectorCursor=vectorCursor;this.effects=effects==null?new Effects():effects.copy();}
        public boolean supported(Field field){return field!=Field.LARGE_POINTER||Boolean.FALSE.equals(vectorCursor);}
        public String observed(Field field){switch(field){
            case HIGH_CONTRAST:return bit(raw.get(Key.HIGH_CONTRAST),0);
            case BOLD_TEXT:{Integer value=integer(raw.get(Key.FONT_WEIGHT),0);return value==null?null:value==0?"off":value==300?"on":"font_weight:"+value;}
            case REMOVE_ANIMATIONS:{float[] values=scales();if(values==null)return null;if(values[0]==0&&values[1]==0&&values[2]==0)return "on";if(values[0]==1&&values[1]==1&&values[2]==1)return "off";return "animation_scales:"+values[0]+","+values[1]+","+values[2];}
            case TOUCH_HOLD:{Integer value=nonnegative(raw.get(Key.LONG_PRESS),400);return value==null?null:"hold_ms:"+value;}
            case ACTION_TIMEOUT:{Integer interactive=nonnegative(raw.get(Key.INTERACTIVE_TIMEOUT),0),nonInteractive=nonnegative(raw.get(Key.NON_INTERACTIVE_TIMEOUT),0);if(interactive==null||nonInteractive==null)return null;return interactive.equals(nonInteractive)?"timeout_ms:"+interactive:"timeout_pair_ms:"+interactive+","+nonInteractive;}
            case AUTOCLICK:{String enabled=bit(raw.get(Key.AUTOCLICK_ENABLED),0);Integer delay=nonnegative(raw.get(Key.AUTOCLICK_DELAY),600);if(enabled==null||delay==null)return null;return (enabled.equals("on")?"autoclick_ms:":"autoclick_off_ms:")+delay;}
            case LARGE_POINTER:return bit(raw.get(Key.LARGE_POINTER),0);default:throw new AssertionError();
        }}
        private float[] scales(){Key[] keys={Key.WINDOW_SCALE,Key.TRANSITION_SCALE,Key.ANIMATOR_SCALE};float[] values=new float[3];for(int i=0;i<keys.length;i++){String raw=this.raw.get(keys[i]);try{values[i]=raw==null?1:Float.parseFloat(raw);if(raw!=null&&!raw.equals(raw.trim())||!Float.isFinite(values[i])||values[i]<0)return null;}catch(NumberFormatException invalid){return null;}}return values;}
        private boolean effectConfirmed(Field field){if(!effects.available(field))return false;switch(field){
            case HIGH_CONTRAST:return effects.highContrast==null||effects.highContrast==Objects.equals(observed(field),"on");
            case BOLD_TEXT:{Integer expected=integer(raw.get(Key.FONT_WEIGHT),0);return effects.fontWeight==null||Objects.equals(effects.fontWeight,expected)||(raw.get(Key.FONT_WEIGHT)==null&&effects.fontWeight==Integer.MAX_VALUE);}
            case REMOVE_ANIMATIONS:return effects.animationScales==null||Arrays.equals(effects.animationScales,scales());
            case TOUCH_HOLD:return effects.longPress==null||Objects.equals(effects.longPress,nonnegative(raw.get(Key.LONG_PRESS),400));
            case ACTION_TIMEOUT:{int interactive=nonnegative(raw.get(Key.INTERACTIVE_TIMEOUT),0),nonInteractive=nonnegative(raw.get(Key.NON_INTERACTIVE_TIMEOUT),0);return (effects.interactiveRecommendation==null||effects.interactiveRecommendation>=interactive)&&(effects.nonInteractiveRecommendation==null||effects.nonInteractiveRecommendation>=nonInteractive);}
            default:return true;
        }}
    }
    public static final class Observation {public final String value;public final String[] choices;public final String availability;Observation(String value,String[] choices,String availability){this.value=value;this.choices=choices;this.availability=availability;}}
    public interface Store {State read();boolean write(Key key,String value);}
    public static final class Backend {
        private final Store store;private final Supplier<Access> access;
        public Backend(Store store,Supplier<Access> access){this.store=store;this.access=access;}
        public synchronized Observation read(Field field){try{if(access.get()==Access.RESTRICTED)return unavailable("restricted");State state=store.read();Access observedAccess=access.get();if(observedAccess==Access.RESTRICTED)return unavailable("restricted");if(state==null)return unavailable("unavailable");String value=state.observed(field);if(!state.supported(field))return new Observation(value,new String[0],state.vectorCursor==null?"unavailable":"unsupported");if(value!=null&&!state.effectConfirmed(field))value=null;return new Observation(value,value!=null&&observedAccess==Access.WRITABLE?field.choices():new String[0],value==null?"unavailable":"available");}catch(SecurityException|IllegalStateException|LinkageError failed){return unavailable("unavailable");}}
        private Observation unavailable(String reason){return new Observation(null,new String[0],reason);}
        public synchronized String apply(Field field,String choice){field.validate(choice);boolean written=false;try{
            if(access.get()!=Access.WRITABLE)return "control_unavailable";State before=store.read();if(before==null||!before.supported(field)||before.observed(field)==null||!before.effectConfirmed(field))return "control_unavailable";
            Map<Key,String> expected=new EnumMap<>(before.raw);Map<Key,String> writes=writes(field,choice);
            for(Map.Entry<Key,String> write:writes.entrySet()){
                State current=store.read();if(access.get()!=Access.WRITABLE||current==null||!Objects.equals(before.vectorCursor,current.vectorCursor)||!current.supported(field)||!expected.equals(current.raw))return written?"control_partial":"control_unavailable";
                if(!store.write(write.getKey(),write.getValue()))return written?"control_partial":"control_unavailable";
                written=true;expected.put(write.getKey(),write.getValue());
            }
            if(access.get()!=Access.WRITABLE)return "control_partial";State after=store.read();if(access.get()!=Access.WRITABLE||after!=null&&(!Objects.equals(before.vectorCursor,after.vectorCursor)||!after.supported(field)))return "control_partial";
            return after!=null&&expected.equals(after.raw)&&after.observed(field)!=null&&after.effectConfirmed(field)?"control_applied":"control_requested";
        }catch(SecurityException|IllegalStateException|LinkageError failure){return written?"control_partial":"control_unavailable";}}
    }
    private static Map<Key,String> writes(Field field,String choice){Map<Key,String> values=new LinkedHashMap<>();String bit=choice.equals("on")?"1":"0";switch(field){
        case HIGH_CONTRAST:values.put(Key.HIGH_CONTRAST,bit);break;
        case BOLD_TEXT:values.put(Key.FONT_WEIGHT,choice.equals("on")?"300":"0");break;
        case REMOVE_ANIMATIONS:String scale=choice.equals("on")?"0.0":"1.0";values.put(Key.WINDOW_SCALE,scale);values.put(Key.TRANSITION_SCALE,scale);values.put(Key.ANIMATOR_SCALE,scale);break;
        case TOUCH_HOLD:values.put(Key.LONG_PRESS,choice.equals("hold:short")?"400":choice.equals("hold:medium")?"1000":"1500");break;
        case ACTION_TIMEOUT:String timeout=choice.equals("timeout:default")?"0":choice.equals("timeout:seconds10")?"10000":choice.equals("timeout:seconds30")?"30000":choice.equals("timeout:minute1")?"60000":"120000";values.put(Key.NON_INTERACTIVE_TIMEOUT,timeout);values.put(Key.INTERACTIVE_TIMEOUT,timeout);break;
        case AUTOCLICK:values.put(Key.AUTOCLICK_ENABLED,choice.equals("off")?"0":"1");values.put(Key.AUTOCLICK_DELAY,choice.equals("off")?"0":choice.substring("autoclick_delay:".length()));break;
        case LARGE_POINTER:values.put(Key.LARGE_POINTER,bit);break;default:throw new AssertionError();
    }return values;}
    private static String bit(String raw,int fallback){Integer value=integer(raw,fallback);return value==null||value!=0&&value!=1?null:value==1?"on":"off";}
    private static Integer nonnegative(String raw,int fallback){Integer value=integer(raw,fallback);return value!=null&&value>=0?value:null;}
    private static Integer integer(String raw,int fallback){if(raw==null)return fallback;if(!raw.matches("-?[0-9]+")||raw.length()>11)return null;try{return Integer.valueOf(raw);}catch(NumberFormatException invalid){return null;}}
}
