import dev.makepad.octosense.controls.CaptionCustomSettings;
import dev.makepad.octosense.controls.CaptionCustomSettings.*;
import java.util.EnumMap;
import java.util.Map;
import java.util.Objects;

public final class CaptionCustomSettingsTest {
    static int checks;
    static void check(boolean value){checks++;if(!value)throw new AssertionError("Check "+checks);}
    static final String A="a".repeat(64),B="b".repeat(64);
    static int number(Map<Key,String> raw,Key key,int fallback){String value=raw.get(key);return value==null?fallback:Integer.parseInt(value);}
    // Fake framework: tests also assert known packed constants independently below.
    static boolean has(int n){return n>>>24!=0||(n&0xffff00)==0;}
    static NativeStyle nativeStyle(Map<Key,String> raw){
        int fg=number(raw,Key.FOREGROUND,-1),bg=number(raw,Key.BACKGROUND,0xff000000),win=number(raw,Key.WINDOW,255),ec=number(raw,Key.EDGE_COLOR,0xff000000),edge=number(raw,Key.EDGE_TYPE,0);
        return new NativeStyle(has(fg)?fg:-1,has(bg)?bg:0xff000000,has(win)?win:255,edge==-1?0:edge,has(ec)?ec:0xff000000,has(fg),has(bg),has(win),edge!=-1,has(ec),raw.get(Key.TYPEFACE));
    }
    static final class Store implements CaptionCustomSettings.Store {
        final EnumMap<Key,String> raw=new EnumMap<>(Key.class),actual=new EnumMap<>(Key.class);
        int writes,reads;boolean propagate=true;Key reject;Runnable afterWrite=()->{},onRead=()->{};
        Store(){raw.put(Key.PRESET,"-1");sync();}
        void sync(){actual.clear();actual.putAll(raw);}
        public State read(){reads++;onRead.run();return new State(raw,number(actual,Key.ENABLED,0)==1,number(actual,Key.PRESET,0),nativeStyle(actual));}
        public boolean write(Key key,String value){writes++;if(key==reject)return false;raw.put(key,value);if(propagate)sync();afterWrite.run();return true;}
    }
    static final class Case {
        final Store store=new Store();final Access[] access={Access.WRITABLE};final long[] clock={0};
        final Backend backend=new Backend(store,()->access[0],()->clock[0]);
        Case(){backend.replaceSession(A);check(backend.enterScope(A,1));}
        String apply(Field field,String choice){return backend.apply(A,1,field,choice);}
        void raw(Key key,String value){if(value==null)store.raw.remove(key);else store.raw.put(key,value);store.sync();}
    }
    static void packingAndFiniteChoices(){
        check(Field.FOREGROUND_COLOR.choices().length==65&&Field.EDGE_COLOR.choices().length==65);
        check(Field.BACKGROUND_COLOR.choices().length==66&&Field.WINDOW_COLOR.choices().length==66);
        check(Field.TYPEFACE.choices().length==9&&Field.EDGE_TYPE.choices().length==6&&Field.FOREGROUND_OPACITY.choices().length==4);
        check(CaptionCustomSettings.swatch("caption_swatch:48")==0xffff0000);
        check(CaptionCustomSettings.swatch("caption_swatch:27")==0xff55aaff);
        check(CaptionCustomSettings.merge(CaptionCustomSettings.DEFAULT_COLOR,128)==0x00ffff80);
        check(CaptionCustomSettings.merge(0,64)==0x00000040);
        check(CaptionCustomSettings.merge(0xffff0000,192)==0xc0ff0000);
        check(!CaptionCustomSettings.hasColor(0x00000100)&&CaptionCustomSettings.parseColor(0x00000100)==CaptionCustomSettings.DEFAULT_COLOR);
        check(!CaptionCustomSettings.hasColor(0x00ffff80)&&CaptionCustomSettings.parseOpacity(0x00ffff80)==128);
        check(CaptionCustomSettings.hasColor(0x00000080)&&CaptionCustomSettings.parseColor(0x00000080)==0&&CaptionCustomSettings.parseOpacity(0x00000080)==128);
        for(int index=0;index<64;index++)for(int alpha=0;alpha<256;alpha++){
            int color=CaptionCustomSettings.swatch("caption_swatch:"+index),packed=CaptionCustomSettings.merge(color,alpha);
            check(packed==((color&0xffffff)|(alpha<<24)));
            if(alpha>0){check(CaptionCustomSettings.parseColor(packed)==color);check(CaptionCustomSettings.parseOpacity(packed)==alpha);}
        }
        for(Field field:Field.values())for(String invalid:new String[]{"caption_color_argb:ff123456","caption_opacity:127","caption_swatch:64","caption_swatch:01","caption_alpha:127","caption_typeface:custom","caption_edge_value:2","provider_key"}){
            try{field.validate(invalid);throw new AssertionError("accepted "+invalid);}catch(IllegalArgumentException expected){checks++;}
        }
        try{Field.FOREGROUND_COLOR.validate("caption_swatch:none");throw new AssertionError();}catch(IllegalArgumentException expected){checks++;}
    }
    static void nativeWritesAndPreservation(){
        for(Field field:Field.values())for(String choice:field.choices()){
            Case c=new Case();c.raw(Key.EDGE_TYPE,"1");c.raw(Key.WINDOW,Integer.toString(0x80808080));c.raw(Key.FOREGROUND,Integer.toString(0x80446688));c.raw(Key.LOCALE,"fr_CA");c.raw(Key.FONT_SCALE,"1.25");
            Map<Key,String> before=new EnumMap<>(c.store.raw);
            check(c.apply(field,choice).equals("control_applied"));
            check(c.store.raw.get(Key.ENABLED).equals("1"));check(c.store.writes==2);
            for(Key key:Key.values())if(key!=field.key&&key!=Key.ENABLED)check(Objects.equals(before.get(key),c.store.raw.get(key)));
        }
        Case c=new Case();c.raw(Key.ENABLED,"1");check(c.apply(Field.TYPEFACE,"caption_typeface:serif").equals("control_applied"));check(c.store.writes==1);
        c.raw(Key.TYPEFACE,"vendor-custom-face");check(c.backend.read(Field.TYPEFACE).equals("caption_typeface_custom"));
        c.raw(Key.FOREGROUND,Integer.toString(0x73445566));check(c.backend.read(Field.FOREGROUND_COLOR).equals("caption_color_argb:73445566"));check(c.backend.read(Field.FOREGROUND_OPACITY).equals("caption_opacity:115"));
        c.raw(Key.EDGE_TYPE,"13");check(c.backend.read(Field.EDGE_TYPE).equals("caption_edge_value:13"));
        c.raw(Key.PRESET,"0");check(c.backend.read(Field.FOREGROUND_COLOR)!=null&&c.backend.choices(A,1,Field.TYPEFACE).length==0);int writes=c.store.writes;check(c.apply(Field.TYPEFACE,"caption_typeface:sans").equals("control_unavailable")&&c.store.writes==writes);
        c.raw(Key.PRESET,"-1");c.raw(Key.FOREGROUND,Integer.toString(0x00ffff80));check(c.backend.choices(A,1,Field.FOREGROUND_OPACITY).length==0);
        c.raw(Key.WINDOW,"128");check(c.backend.choices(A,1,Field.WINDOW_OPACITY).length==0);
        c.raw(Key.EDGE_TYPE,"0");check(c.backend.choices(A,1,Field.EDGE_COLOR).length==0);
        c.raw(Key.EDGE_TYPE,"-1");check(c.backend.choices(A,1,Field.EDGE_COLOR).length==65);
    }
    static void scopeAndOpacity(){
        Case c=new Case();c.raw(Key.FOREGROUND,Integer.toString(0x8000ff00));
        check(c.apply(Field.FOREGROUND_COLOR,"caption_swatch:default").equals("control_applied"));check(c.store.raw.get(Key.FOREGROUND).equals(Integer.toString(0x00ffff80)));
        check(c.backend.read(Field.FOREGROUND_OPACITY).equals("caption_opacity:255"));check(c.backend.enterScope(A,1));
        check(c.apply(Field.FOREGROUND_COLOR,"caption_swatch:48").equals("control_applied"));check(c.store.raw.get(Key.FOREGROUND).equals(Integer.toString(0x80ff0000)));
        check(c.apply(Field.FOREGROUND_COLOR,"caption_swatch:default").equals("control_applied"));
        c.raw(Key.TYPEFACE,"serif");check(c.apply(Field.FOREGROUND_COLOR,"caption_swatch:3").equals("control_applied"));check(c.store.raw.get(Key.FOREGROUND).equals(Integer.toString(0xff0000ff)));
        c.raw(Key.FOREGROUND,Integer.toString(0x400000ff));check(c.apply(Field.FOREGROUND_COLOR,"caption_swatch:default").equals("control_applied"));c.backend.leaveScope(A,1);
        check(!c.backend.enterScope(A,1));check(c.apply(Field.FOREGROUND_COLOR,"caption_swatch:48").equals("control_unavailable"));check(c.backend.enterScope(A,2));
        check(c.backend.apply(A,2,Field.FOREGROUND_COLOR,"caption_swatch:48").equals("control_applied"));check(c.store.raw.get(Key.FOREGROUND).equals(Integer.toString(0xffff0000)));
        c.backend.leaveScope(A,1);check(c.backend.choices(A,2,Field.TYPEFACE).length==9);
        c.backend.replaceSession(B);check(c.backend.enterScope(B,1));c.backend.invalidateSession(A);c.backend.leaveScope(A,2);check(c.backend.choices(B,1,Field.TYPEFACE).length==9);
        check(!c.backend.enterScope(A,3));check(c.backend.apply(A,2,Field.TYPEFACE,"caption_typeface:sans").equals("control_unavailable"));
        c.backend.replaceSession(B);check(c.backend.choices(B,1,Field.TYPEFACE).length==9);
        c.clock[0]=599_999;check(c.backend.choices(B,1,Field.TYPEFACE).length==9);c.clock[0]=600_000;check(c.backend.choices(B,1,Field.TYPEFACE).length==0);check(!c.backend.enterScope(B,1));
        check(c.backend.enterScope(B,2));c.access[0]=Access.RESTRICTED;check(c.backend.read(Field.TYPEFACE)==null);c.access[0]=Access.WRITABLE;check(!c.backend.enterScope(B,2));check(c.backend.enterScope(B,3));
        c.access[0]=Access.READ_ONLY;check(c.backend.read(Field.TYPEFACE)!=null);check(c.backend.choices(B,3,Field.TYPEFACE).length==0);c.access[0]=Access.WRITABLE;check(!c.backend.enterScope(B,3));
        c.backend.invalidateSession(B);check(!c.backend.enterScope(B,4));
    }
    static void staleAndPartial(){
        Case changed=new Case();changed.store.onRead=()->{if(changed.store.reads==2){changed.store.raw.put(Key.TYPEFACE,"serif");changed.store.sync();}};
        check(changed.apply(Field.FOREGROUND_COLOR,"caption_swatch:48").equals("control_unavailable")&&changed.store.writes==0);
        Case locked=new Case();locked.store.afterWrite=()->locked.access[0]=Access.RESTRICTED;
        check(locked.apply(Field.TYPEFACE,"caption_typeface:serif").equals("control_partial")&&locked.store.writes==1);
        Case failed=new Case();failed.store.reject=Key.ENABLED;
        check(failed.apply(Field.TYPEFACE,"caption_typeface:serif").equals("control_partial")&&failed.store.writes==2);check(failed.store.raw.get(Key.ENABLED)==null);
        Case replaced=new Case();replaced.store.afterWrite=()->{replaced.store.raw.put(Key.PRESET,"0");replaced.store.sync();};
        check(replaced.apply(Field.TYPEFACE,"caption_typeface:serif").equals("control_partial")&&replaced.store.writes==1);
        Case lag=new Case();lag.store.propagate=false;check(lag.apply(Field.TYPEFACE,"caption_typeface:serif").equals("control_requested"));check(lag.store.writes==2);lag.store.sync();check(lag.backend.read(Field.TYPEFACE).equals("caption_typeface:serif"));
        Case retired=new Case();retired.store.onRead=()->{if(retired.store.writes==2)retired.access[0]=Access.RESTRICTED;};check(retired.apply(Field.TYPEFACE,"caption_typeface:serif").equals("control_partial"));
        Case malformed=new Case();malformed.store.raw.put(Key.ENABLED,"2");check(malformed.apply(Field.TYPEFACE,"caption_typeface:serif").equals("control_unavailable")&&malformed.store.writes==0);
        Case disagreement=new Case();disagreement.store.raw.put(Key.FOREGROUND,"42");check(disagreement.backend.read(Field.FOREGROUND_COLOR)==null);check(disagreement.apply(Field.TYPEFACE,"caption_typeface:serif").equals("control_unavailable")&&disagreement.store.writes==0);
    }
    public static void main(String[] ignored){packingAndFiniteChoices();nativeWritesAndPreservation();scopeAndOpacity();staleAndPartial();System.out.println("PASS custom captions: "+checks);}
}
