package dev.makepad.octosense.controls;

import java.util.Arrays;
import java.util.Collections;
import java.util.EnumMap;
import java.util.Map;
import java.util.Objects;
import java.util.function.LongSupplier;
import java.util.function.Supplier;

/** Native finite caption choices. Raw observations are never accepted as writes. */
public final class CaptionCustomSettings {
    private CaptionCustomSettings(){throw new AssertionError();}
    public static final long SCOPE_TTL_MS=600_000;
    public static final int DEFAULT_COLOR=0x00ffffff;
    public enum Key {
        ENABLED("accessibility_captioning_enabled"), PRESET("accessibility_captioning_preset"),
        FOREGROUND("accessibility_captioning_foreground_color"), BACKGROUND("accessibility_captioning_background_color"),
        WINDOW("accessibility_captioning_window_color"), EDGE_TYPE("accessibility_captioning_edge_type"),
        EDGE_COLOR("accessibility_captioning_edge_color"), TYPEFACE("accessibility_captioning_typeface"),
        FONT_SCALE("accessibility_captioning_font_scale"), LOCALE("accessibility_captioning_locale");
        public final String key;Key(String key){this.key=key;}
    }
    public enum Field {
        TYPEFACE("caption_typeface",Key.TYPEFACE), FOREGROUND_COLOR("caption_foreground_color",Key.FOREGROUND),
        FOREGROUND_OPACITY("caption_foreground_opacity",Key.FOREGROUND), EDGE_TYPE("caption_edge_type",Key.EDGE_TYPE),
        EDGE_COLOR("caption_edge_color",Key.EDGE_COLOR), BACKGROUND_COLOR("caption_background_color",Key.BACKGROUND),
        BACKGROUND_OPACITY("caption_background_opacity",Key.BACKGROUND), WINDOW_COLOR("caption_window_color",Key.WINDOW),
        WINDOW_OPACITY("caption_window_opacity",Key.WINDOW);
        public final String id;public final Key key;Field(String id,Key key){this.id=id;this.key=key;}
        public static Field parse(String id){for(Field f:values())if(f.id.equals(id))return f;throw new IllegalArgumentException("Unknown caption field");}
        public boolean opacity(){return this==FOREGROUND_OPACITY||this==BACKGROUND_OPACITY||this==WINDOW_OPACITY;}
        public boolean color(){return this==FOREGROUND_COLOR||this==BACKGROUND_COLOR||this==WINDOW_COLOR||this==EDGE_COLOR;}
        public boolean none(){return this==BACKGROUND_COLOR||this==WINDOW_COLOR;}
        public String[] choices(){
            if(opacity())return new String[]{"caption_alpha:64","caption_alpha:128","caption_alpha:192","caption_alpha:255"};
            if(this==TYPEFACE)return Arrays.stream(FACE_NAMES).map(name->"caption_typeface:"+name).toArray(String[]::new);
            if(this==EDGE_TYPE)return Arrays.stream(EDGE_NAMES).map(name->"caption_edge:"+name).toArray(String[]::new);
            String[] out=new String[none()?66:65];int at=0;if(none())out[at++]="caption_swatch:none";out[at++]="caption_swatch:default";
            // Native palette: eight named colors, then the remaining RGB cube entries.
            int[] named={63,0,48,60,12,15,3,51};for(int n:named)out[at++]="caption_swatch:"+n;
            outer:for(int n=0;n<64;n++){for(int known:named)if(n==known)continue outer;out[at++]="caption_swatch:"+n;}
            return out;
        }
        public void validate(String choice){if(choice==null||!Arrays.asList(choices()).contains(choice))throw new IllegalArgumentException("Not a finite caption choice");}
    }
    private static final String[] FACE_NAMES={"default","sans","condensed","mono_sans","serif","mono_serif","casual","cursive","small_caps"};
    private static final String[] FACES={"","sans-serif","sans-serif-condensed","sans-serif-monospace","serif","serif-monospace","casual","cursive","sans-serif-smallcaps"};
    private static final String[] EDGE_NAMES={"default","none","outline","shadow","raised","depressed"};
    public enum Access { WRITABLE, READ_ONLY, RESTRICTED }

    /** Actual CaptionStyle values/has-flags supplied independently by the platform adapter. */
    public static final class NativeStyle {
        public final int foreground,background,window,edgeType,edgeColor;
        public final boolean hasForeground,hasBackground,hasWindow,hasEdgeType,hasEdgeColor;
        public final String typeface;
        public NativeStyle(int fg,int bg,int win,int edge,int edgeColor,boolean hasFg,boolean hasBg,
                boolean hasWin,boolean hasEdge,boolean hasEdgeColor,String face){
            foreground=fg;background=bg;window=win;edgeType=edge;this.edgeColor=edgeColor;
            hasForeground=hasFg;hasBackground=hasBg;hasWindow=hasWin;hasEdgeType=hasEdge;this.hasEdgeColor=hasEdgeColor;typeface=face;
        }
    }
    public static final class State {
        public final Map<Key,String> raw;public final Boolean enabled;public final Integer preset;public final NativeStyle style;
        public State(Map<Key,String> raw,Boolean enabled,Integer preset,NativeStyle style){
            EnumMap<Key,String> copy=new EnumMap<>(Key.class);for(Key key:Key.values())copy.put(key,raw.get(key));
            this.raw=Collections.unmodifiableMap(copy);this.enabled=enabled;this.preset=preset;this.style=style;
        }
        public boolean enabledConsistent(){Integer n=integer(raw.get(Key.ENABLED),0);return n!=null&&(n==0||n==1)&&enabled!=null&&enabled==(n==1);}
        public boolean presetConsistent(){Integer n=integer(raw.get(Key.PRESET),0);return n!=null&&n>=-1&&n<=4&&n.equals(preset);}
        public Integer packed(Key key){return integer(raw.get(key),key==Key.FOREGROUND?-1:key==Key.WINDOW?255:0xff000000);}
        public boolean consistent(){
            if(!enabledConsistent()||!presetConsistent()||style==null)return false;
            Integer fg=packed(Key.FOREGROUND),bg=packed(Key.BACKGROUND),win=packed(Key.WINDOW),ec=packed(Key.EDGE_COLOR),edge=integer(raw.get(Key.EDGE_TYPE),0);
            return fg!=null&&bg!=null&&win!=null&&ec!=null&&edge!=null
                &&matchesColor(fg,style.foreground,style.hasForeground,-1)&&matchesColor(bg,style.background,style.hasBackground,0xff000000)
                &&matchesColor(win,style.window,style.hasWindow,255)&&matchesColor(ec,style.edgeColor,style.hasEdgeColor,0xff000000)
                &&style.hasEdgeType==(edge!=-1)&&style.edgeType==(edge==-1?0:edge)&&Objects.equals(raw.get(Key.TYPEFACE),style.typeface);
        }
        public String observed(Field field){
            if(!consistent())return null;
            if(field.color())return "caption_color_argb:"+hex(packed(field.key));
            if(field.opacity())return "caption_opacity:"+parseOpacity(nativeColor(this,field.key));
            if(field==Field.EDGE_TYPE)return "caption_edge_value:"+integer(raw.get(Key.EDGE_TYPE),0);
            String face=raw.get(Key.TYPEFACE);if(face==null)face="";
            for(int i=0;i<FACES.length;i++)if(FACES[i].equals(face))return "caption_typeface:"+FACE_NAMES[i];
            return "caption_typeface_custom";
        }
    }
    public interface Store { State read(); boolean write(Key key,String finiteStoredValue); }
    public static final class Backend {
        private final Store store;private final Supplier<Access> access;private final LongSupplier clock;
        private String session;private long newestVisit,visit,opened;
        private Map<Key,String> cacheRaw;private final EnumMap<Key,Integer> opacityCache=new EnumMap<>(Key.class);
        public Backend(Store store,Supplier<Access> access,LongSupplier clock){this.store=store;this.access=access;this.clock=clock;}
        /** Called only when transport authenticates a new Home process/session. */
        public synchronized void replaceSession(String next){
            if(next==null||!next.matches("[0-9a-f]{64}"))throw new IllegalArgumentException("Invalid caption session");
            if(next.equals(session))return;invalidate();session=next;newestVisit=0;
        }
        public synchronized boolean enterScope(String owner,long next){
            if(!Objects.equals(session,owner)||session==null||next<=0||next<newestVisit)return false;
            if(next==newestVisit)return active(owner,next);
            invalidate();newestVisit=next;
            if(access.get()!=Access.WRITABLE)return false;
            visit=next;opened=clock.getAsLong();return true;
        }
        public synchronized void leaveScope(String owner,long old){if(Objects.equals(session,owner)&&visit==old)invalidate();}
        public synchronized void invalidateSession(String owner){if(Objects.equals(session,owner)){invalidate();session=null;newestVisit=0;}}
        public synchronized void invalidate(){visit=0;opacityCache.clear();cacheRaw=null;}
        private boolean active(String owner,long requested){
            if(visit==0||session==null||!session.equals(owner)||visit!=requested)return false;
            long elapsed=clock.getAsLong()-opened;
            if(elapsed<0||elapsed>=SCOPE_TTL_MS||access.get()!=Access.WRITABLE){invalidate();return false;}
            return true;
        }
        private State observe(){
            if(access.get()==Access.RESTRICTED){invalidate();return null;}
            State state=store.read();
            if(access.get()==Access.RESTRICTED){invalidate();return null;}
            if(state==null||!state.consistent()){opacityCache.clear();cacheRaw=null;return null;}
            if(cacheRaw!=null&&!cacheRaw.equals(state.raw))opacityCache.clear();
            cacheRaw=state.raw;
            if(visit!=0)active(session,visit);
            return state;
        }
        public synchronized String read(Field field){try{State state=observe();return state==null?null:state.observed(field);}catch(SecurityException|IllegalStateException|IndexOutOfBoundsException|LinkageError unavailable){invalidate();return null;}}
        public synchronized Boolean customSelected(){try{State state=observe();return state==null?null:state.preset==-1;}catch(SecurityException|IllegalStateException|IndexOutOfBoundsException|LinkageError unavailable){invalidate();return null;}}
        public synchronized boolean scopeActive(String owner,long requested){return active(owner,requested);}
        public synchronized String[] choices(String owner,long requested,Field field){
            try{State state=observe();return active(owner,requested)&&allowed(state,field)?field.choices():new String[0];}
            catch(SecurityException|IllegalStateException|IndexOutOfBoundsException|LinkageError unavailable){invalidate();return new String[0];}
        }
        private boolean allowed(State state,Field field){
            if(state==null||!state.consistent()||state.preset!=-1)return false;
            if(field.opacity())return (parseColor(nativeColor(state,field.key))>>>24)!=0;
            return field!=Field.EDGE_COLOR||integer(state.raw.get(Key.EDGE_TYPE),0)!=0;
        }
        public synchronized String apply(String owner,long requested,Field field,String choice){
            field.validate(choice);boolean written=false;
            try{
                if(!active(owner,requested))return "control_unavailable";
                State before=observe();if(!allowed(before,field))return "control_unavailable";
                String stored=stored(before,field,choice);Integer remembered=null;
                if(field.color()&&field!=Field.EDGE_COLOR&&!hasColor(swatch(choice)))remembered=parseOpacity(nativeColor(before,field.key));
                State latest=store.read();
                if(!active(owner,requested)||latest==null||!latest.consistent()||!before.raw.equals(latest.raw)){opacityCache.clear();cacheRaw=null;return "control_unavailable";}
                if(!store.write(field.key,stored))return "control_unavailable";written=true;
                Map<Key,String> expected=new EnumMap<>(before.raw);expected.put(field.key,stored);
                if(!active(owner,requested))return partial();
                State afterFirst=store.read();
                if(afterFirst==null||!expected.equals(afterFirst.raw)||!afterFirst.enabledConsistent()||!afterFirst.presetConsistent()||afterFirst.preset!=-1)return partial();
                if(!afterFirst.enabled){
                    if(!active(owner,requested)||!store.write(Key.ENABLED,"1"))return partial();
                    expected.put(Key.ENABLED,"1");
                }
                if(!active(owner,requested))return partial();
                State after=store.read();if(!active(owner,requested))return partial();
                if(after==null||!after.consistent()||!expected.equals(after.raw)){opacityCache.clear();cacheRaw=null;return "control_requested";}
                cacheRaw=after.raw;
                if(field.color()&&field!=Field.EDGE_COLOR){if(remembered==null)opacityCache.remove(field.key);else opacityCache.put(field.key,remembered);}
                return "control_applied";
            }catch(SecurityException|IllegalStateException|IndexOutOfBoundsException|LinkageError unavailable){return written?partial():"control_unavailable";}
        }
        private String partial(){opacityCache.clear();cacheRaw=null;return "control_partial";}
        private String stored(State state,Field field,String choice){
            if(field==Field.TYPEFACE){String name=choice.substring("caption_typeface:".length());for(int i=0;i<FACE_NAMES.length;i++)if(FACE_NAMES[i].equals(name))return FACES[i];}
            if(field==Field.EDGE_TYPE){String name=choice.substring("caption_edge:".length());for(int i=0;i<EDGE_NAMES.length;i++)if(EDGE_NAMES[i].equals(name))return Integer.toString(i-1);}
            if(field==Field.EDGE_COLOR)return Integer.toString(swatch(choice));
            int current=nativeColor(state,field.key);
            if(field.opacity())return Integer.toString(merge(parseColor(current),Integer.parseInt(choice.substring("caption_alpha:".length()))));
            int color=swatch(choice);int alpha=hasColor(color)&&opacityCache.containsKey(field.key)?opacityCache.get(field.key):parseOpacity(current);
            return Integer.toString(merge(color,alpha));
        }
    }
    /** These three operations mirror Settings CaptionUtils and framework CaptionStyle. */
    public static boolean hasColor(int packed){return (packed>>>24)!=0||(packed&0x00ffff00)==0;}
    public static int parseColor(int packed){return !hasColor(packed)?DEFAULT_COLOR:(packed>>>24)==0?0:packed|0xff000000;}
    public static int parseOpacity(int packed){return !hasColor(packed)||(packed>>>24)==0?packed&255:packed>>>24;}
    public static int merge(int color,int alpha){if(alpha<0||alpha>255)throw new IllegalArgumentException("Invalid alpha");return !hasColor(color)?0x00ffff00|alpha:color==0?alpha:(color&0x00ffffff)|(alpha<<24);}
    public static int swatch(String choice){
        if("caption_swatch:default".equals(choice))return DEFAULT_COLOR;if("caption_swatch:none".equals(choice))return 0;
        if(choice==null||!choice.startsWith("caption_swatch:"))throw new IllegalArgumentException("Invalid swatch");String number=choice.substring(15);int n;
        try{n=Integer.parseInt(number);}catch(NumberFormatException invalid){throw new IllegalArgumentException("Invalid swatch");}
        if(n<0||n>=64||!number.equals(Integer.toString(n)))throw new IllegalArgumentException("Invalid swatch");
        return 0xff000000|((n/16)*85<<16)|(((n/4)%4)*85<<8)|(n%4)*85;
    }
    private static int nativeColor(State state,Key key){int raw=state.packed(key);return hasColor(raw)?raw:DEFAULT_COLOR;}
    private static boolean matchesColor(int raw,int actual,boolean has,int fallback){return has==hasColor(raw)&&actual==(has?raw:fallback);}
    private static Integer integer(String raw,int missing){if(raw==null)return missing;if(!raw.matches("-?[0-9]+")||raw.length()>11)return null;try{return Integer.valueOf(raw);}catch(NumberFormatException invalid){return null;}}
    private static String hex(int value){return String.format(java.util.Locale.ROOT,"%08x",value);}
}
