import dev.makepad.octosense.controls.AccessibilityTextMotorSettings;
import dev.makepad.octosense.controls.AccessibilityTextMotorSettings.*;
import java.util.ArrayList;
import java.util.EnumMap;
import java.util.List;
import java.util.Map;
import java.util.Objects;

/** Pure tests, deliberately outside native build inputs until the slice is integrated. */
public final class AccessibilityTextMotorSettingsTest {
    static int checks;
    static void check(boolean condition){checks++;if(!condition)throw new AssertionError("Check "+checks);}
    static void equal(Object actual,Object expected){checks++;if(!Objects.equals(actual,expected))throw new AssertionError("Check "+checks+": "+actual+" != "+expected);}
    static final class Store implements AccessibilityTextMotorSettings.Store {
        final EnumMap<Key,String> raw=new EnumMap<>(Key.class);
        final List<Key> written=new ArrayList<>();
        Boolean vectorCursor=false;Effects effects=new Effects();
        int reads,failWrite=-1;boolean apply=true;Runnable onRead=()->{},afterWrite=()->{};
        public State read(){reads++;onRead.run();return new State(raw,vectorCursor,effects);}
        public boolean write(Key key,String value){written.add(key);if(written.size()==failWrite)return false;if(apply)raw.put(key,value);afterWrite.run();return true;}
    }
    static final class Case {
        final Store store=new Store();final Access[] access={Access.WRITABLE};final Backend backend=new Backend(store,()->access[0]);
    }
    static void missingDefaultsAndCustomObservations(){
        Case c=new Case();
        equal(c.backend.read(Field.HIGH_CONTRAST).value,"off");equal(c.backend.read(Field.BOLD_TEXT).value,"off");
        equal(c.backend.read(Field.REMOVE_ANIMATIONS).value,"off");equal(c.backend.read(Field.TOUCH_HOLD).value,"hold_ms:400");
        equal(c.backend.read(Field.ACTION_TIMEOUT).value,"timeout_ms:0");equal(c.backend.read(Field.AUTOCLICK).value,"autoclick_off_ms:600");equal(c.backend.read(Field.LARGE_POINTER).value,"off");
        c.store.raw.put(Key.FONT_WEIGHT,"125");equal(c.backend.read(Field.BOLD_TEXT).value,"font_weight:125");
        c.store.raw.put(Key.LONG_PRESS,"735");equal(c.backend.read(Field.TOUCH_HOLD).value,"hold_ms:735");
        c.store.raw.put(Key.WINDOW_SCALE,"0.5");c.store.raw.put(Key.ANIMATOR_SCALE,"0");equal(c.backend.read(Field.REMOVE_ANIMATIONS).value,"animation_scales:0.5,1.0,0.0");
        c.store.raw.put(Key.INTERACTIVE_TIMEOUT,"30000");c.store.raw.put(Key.NON_INTERACTIVE_TIMEOUT,"12500");equal(c.backend.read(Field.ACTION_TIMEOUT).value,"timeout_pair_ms:30000,12500");
        c.store.raw.put(Key.AUTOCLICK_ENABLED,"1");c.store.raw.put(Key.AUTOCLICK_DELAY,"625");equal(c.backend.read(Field.AUTOCLICK).value,"autoclick_ms:625");
        check(c.store.written.isEmpty());
        for(Key key:new Key[]{Key.HIGH_CONTRAST,Key.LARGE_POINTER,Key.AUTOCLICK_ENABLED}){
            Case invalid=new Case();invalid.store.raw.put(key,"2");Field field=key==Key.HIGH_CONTRAST?Field.HIGH_CONTRAST:key==Key.LARGE_POINTER?Field.LARGE_POINTER:Field.AUTOCLICK;
            equal(invalid.backend.read(field).value,null);check(invalid.backend.read(field).choices.length==0);equal(invalid.backend.apply(field,field.choices()[0]),"control_unavailable");check(invalid.store.written.isEmpty());
        }
        for(String invalid:new String[]{"NaN","Infinity","-1"," 1","garbage"}){
            Case bad=new Case();bad.store.raw.put(Key.TRANSITION_SCALE,invalid);equal(bad.backend.read(Field.REMOVE_ANIMATIONS).value,null);equal(bad.backend.apply(Field.REMOVE_ANIMATIONS,"on"),"control_unavailable");check(bad.store.written.isEmpty());
        }
        for(String invalid:new String[]{"-1","", "2147483648","1.5"," 400"}){
            Case bad=new Case();bad.store.raw.put(Key.LONG_PRESS,invalid);equal(bad.backend.read(Field.TOUCH_HOLD).value,null);check(bad.store.written.isEmpty());
        }
    }
    static void finiteChoicesAndNativeWrites(){
        equal(Field.AUTOCLICK.choices().length,10);equal(Field.ACTION_TIMEOUT.choices().length,5);equal(Field.TOUCH_HOLD.choices().length,3);
        for(Field field:Field.values())for(String bad:new String[]{"provider_key","autoclick_delay:2000","autoclick_delay:201","600","1","on\n","minutes3"}){
            Case c=new Case();try{c.backend.apply(field,bad);throw new AssertionError("Arbitrary choice accepted");}catch(IllegalArgumentException expected){checks++;}check(c.store.written.isEmpty());
        }
        for(Field field:Field.values())for(String choice:field.choices()){
            Case c=new Case();c.store.raw.put(Key.FONT_WEIGHT,"125");c.store.raw.put(Key.LONG_PRESS,"735");c.store.raw.put(Key.AUTOCLICK_DELAY,"625");
            Map<Key,String> before=new EnumMap<>(c.store.raw);equal(c.backend.apply(field,choice),"control_applied");
            for(Key key:Key.values())if(!c.store.written.contains(key))equal(c.store.raw.get(key),before.get(key));
        }
        Case c=new Case();equal(c.backend.apply(Field.BOLD_TEXT,"on"),"control_applied");equal(c.store.raw.get(Key.FONT_WEIGHT),"300");
        c=new Case();equal(c.backend.apply(Field.REMOVE_ANIMATIONS,"on"),"control_applied");equal(c.store.written,List.of(Key.WINDOW_SCALE,Key.TRANSITION_SCALE,Key.ANIMATOR_SCALE));for(Key key:c.store.written)equal(c.store.raw.get(key),"0.0");
        c=new Case();c.store.raw.put(Key.WINDOW_SCALE,"0.25");equal(c.backend.apply(Field.REMOVE_ANIMATIONS,"off"),"control_applied");for(Key key:c.store.written)equal(c.store.raw.get(key),"1.0");
        for(String[] choice:new String[][]{{"hold:short","400"},{"hold:medium","1000"},{"hold:long","1500"}}){c=new Case();equal(c.backend.apply(Field.TOUCH_HOLD,choice[0]),"control_applied");equal(c.store.raw.get(Key.LONG_PRESS),choice[1]);}
        for(String[] choice:new String[][]{{"timeout:default","0"},{"timeout:seconds10","10000"},{"timeout:seconds30","30000"},{"timeout:minute1","60000"},{"timeout:minutes2","120000"}}){c=new Case();equal(c.backend.apply(Field.ACTION_TIMEOUT,choice[0]),"control_applied");equal(c.store.written,List.of(Key.NON_INTERACTIVE_TIMEOUT,Key.INTERACTIVE_TIMEOUT));equal(c.store.raw.get(Key.NON_INTERACTIVE_TIMEOUT),choice[1]);equal(c.store.raw.get(Key.INTERACTIVE_TIMEOUT),choice[1]);}
        c=new Case();equal(c.backend.apply(Field.AUTOCLICK,"off"),"control_applied");equal(c.store.written,List.of(Key.AUTOCLICK_ENABLED,Key.AUTOCLICK_DELAY));equal(c.store.raw.get(Key.AUTOCLICK_ENABLED),"0");equal(c.store.raw.get(Key.AUTOCLICK_DELAY),"0");
        for(int delay=200;delay<=1000;delay+=100){c=new Case();equal(c.backend.apply(Field.AUTOCLICK,"autoclick_delay:"+delay),"control_applied");equal(c.store.raw.get(Key.AUTOCLICK_ENABLED),"1");equal(c.store.raw.get(Key.AUTOCLICK_DELAY),Integer.toString(delay));}
    }
    static void capabilityAuthorityAndLinkedFailure(){
        Case c=new Case();c.store.vectorCursor=true;equal(c.backend.read(Field.LARGE_POINTER).availability,"unsupported");check(c.backend.read(Field.LARGE_POINTER).choices.length==0);equal(c.backend.apply(Field.LARGE_POINTER,"on"),"control_unavailable");check(c.store.written.isEmpty());
        c.store.vectorCursor=null;equal(c.backend.read(Field.LARGE_POINTER).availability,"unavailable");equal(c.backend.read(Field.HIGH_CONTRAST).availability,"available");
        for(Access access:new Access[]{Access.READ_ONLY,Access.RESTRICTED}){
            c=new Case();c.access[0]=access;for(Field field:Field.values()){check(c.backend.read(field).choices.length==0);equal(c.backend.apply(field,field.choices()[0]),"control_unavailable");}check(c.store.written.isEmpty());
        }
        Case lockedDuringRead=new Case();lockedDuringRead.store.onRead=()->lockedDuringRead.access[0]=Access.RESTRICTED;equal(lockedDuringRead.backend.read(Field.BOLD_TEXT).availability,"restricted");
        Case changedBeforeWrite=new Case();changedBeforeWrite.store.onRead=()->{if(changedBeforeWrite.store.reads==2)changedBeforeWrite.store.raw.put(Key.FONT_WEIGHT,"150");};equal(changedBeforeWrite.backend.apply(Field.BOLD_TEXT,"on"),"control_unavailable");check(changedBeforeWrite.store.written.isEmpty());
        Case flagChanged=new Case();flagChanged.store.onRead=()->{if(flagChanged.store.reads==2)flagChanged.store.vectorCursor=true;};equal(flagChanged.backend.apply(Field.LARGE_POINTER,"on"),"control_unavailable");check(flagChanged.store.written.isEmpty());
        for(Field field:new Field[]{Field.REMOVE_ANIMATIONS,Field.ACTION_TIMEOUT,Field.AUTOCLICK}){
            int total=field==Field.REMOVE_ANIMATIONS?3:2;
            for(int fail=1;fail<=total;fail++){c=new Case();c.store.failWrite=fail;equal(c.backend.apply(field,field.choices()[0]),fail==1?"control_unavailable":"control_partial");equal(c.store.written.size(),fail);}
            Case loseOwner=new Case();loseOwner.store.afterWrite=()->loseOwner.access[0]=Access.RESTRICTED;equal(loseOwner.backend.apply(field,field.choices()[0]),"control_partial");equal(loseOwner.store.written.size(),1);
            Case external=new Case();external.store.afterWrite=()->external.store.raw.put(Key.LONG_PRESS,"777");equal(external.backend.apply(field,field.choices()[0]),"control_partial");equal(external.store.written.size(),1);
        }
        Case losesAfterWrite=new Case();losesAfterWrite.store.afterWrite=()->losesAfterWrite.access[0]=Access.RESTRICTED;equal(losesAfterWrite.backend.apply(Field.BOLD_TEXT,"on"),"control_partial");equal(losesAfterWrite.store.written.size(),1);
    }
    static void delayedIndependentReadbackAndNoRetry(){
        Case c=new Case();c.store.effects.highContrast=false;equal(c.backend.apply(Field.HIGH_CONTRAST,"on"),"control_requested");equal(c.store.written.size(),1);equal(c.store.raw.get(Key.HIGH_CONTRAST),"1");
        equal(c.backend.read(Field.HIGH_CONTRAST).value,null);check(c.backend.read(Field.HIGH_CONTRAST).choices.length==0);
        c.store.effects.highContrast=true;equal(c.backend.read(Field.HIGH_CONTRAST).value,"on");equal(c.store.written.size(),1);
        c=new Case();c.store.effects.fontWeight=0;equal(c.backend.apply(Field.BOLD_TEXT,"on"),"control_requested");equal(c.store.written.size(),1);
        c=new Case();c.store.effects.longPress=400;equal(c.backend.apply(Field.TOUCH_HOLD,"hold:long"),"control_requested");equal(c.store.written.size(),1);
        c=new Case();c.store.effects.animationScales=new float[]{1,1,1};equal(c.backend.apply(Field.REMOVE_ANIMATIONS,"on"),"control_requested");equal(c.store.written.size(),3);
        c=new Case();c.store.effects.interactiveRecommendation=10000;c.store.effects.nonInteractiveRecommendation=5000;equal(c.backend.apply(Field.ACTION_TIMEOUT,"timeout:seconds30"),"control_requested");equal(c.store.written.size(),2);
        c=new Case();c.store.effects.interactiveRecommendation=90000;c.store.effects.nonInteractiveRecommendation=60000;equal(c.backend.apply(Field.ACTION_TIMEOUT,"timeout:seconds30"),"control_applied");equal(c.store.written.size(),2);
        c=new Case();c.store.apply=false;equal(c.backend.apply(Field.BOLD_TEXT,"on"),"control_requested");equal(c.store.written.size(),1);
        c=new Case();c.store.apply=false;equal(c.backend.apply(Field.REMOVE_ANIMATIONS,"on"),"control_partial");equal(c.store.written.size(),1);
        c=new Case();c.store.effects.animationScales=new float[]{1,1,1};State snapshot=c.store.read();c.store.effects.animationScales[0]=0;equal(snapshot.effects.animationScales[0],1f);
        for(Field field:new Field[]{Field.HIGH_CONTRAST,Field.BOLD_TEXT,Field.REMOVE_ANIMATIONS,Field.TOUCH_HOLD,Field.ACTION_TIMEOUT}){
            Case missing=new Case();missing.store.effects.required.add(field);equal(missing.backend.read(field).value,null);check(missing.backend.read(field).choices.length==0);equal(missing.backend.apply(field,field.choices()[0]),"control_unavailable");check(missing.store.written.isEmpty());
        }
        Case lostAfterWrite=new Case();lostAfterWrite.store.effects.required.add(Field.BOLD_TEXT);lostAfterWrite.store.effects.fontWeight=0;lostAfterWrite.store.afterWrite=()->lostAfterWrite.store.effects.fontWeight=null;
        equal(lostAfterWrite.backend.apply(Field.BOLD_TEXT,"on"),"control_requested");equal(lostAfterWrite.store.written.size(),1);equal(lostAfterWrite.backend.read(Field.BOLD_TEXT).value,null);
    }
    public static void main(String[] args){missingDefaultsAndCustomObservations();finiteChoicesAndNativeWrites();capabilityAuthorityAndLinkedFailure();delayedIndependentReadbackAndNoRetry();System.out.println("PASS "+checks+" text/motor policy checks");}
}
