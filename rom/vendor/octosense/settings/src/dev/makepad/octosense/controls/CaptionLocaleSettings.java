package dev.makepad.octosense.controls;

import java.util.ArrayList;
import java.util.Collections;
import java.util.HashSet;
import java.util.List;
import java.util.Locale;
import java.util.Objects;
import java.util.Set;
import java.util.function.LongSupplier;
import java.util.function.Supplier;

/** Caption language uses the device's native asset locale catalog, not application locales. */
public final class CaptionLocaleSettings {
    private CaptionLocaleSettings(){throw new AssertionError();}
    public static final int PAGE_SIZE=24,MAX_LOCALES=1024;
    public static final long CATALOG_TTL_MS=600_000;
    public enum Access{WRITABLE,READ_ONLY,RESTRICTED}
    public static final class LocaleEntry {
        public final String value,label;
        public LocaleEntry(String value,String label){this.value=value;this.label=label;}
        @Override public boolean equals(Object value){if(!(value instanceof LocaleEntry))return false;LocaleEntry other=(LocaleEntry)value;return Objects.equals(this.value,other.value)&&Objects.equals(label,other.label);}
        @Override public int hashCode(){return Objects.hash(value,label);}
    }
    public static final class State {
        public final String raw,service;public final List<LocaleEntry> locales;
        public State(String raw,String service,List<LocaleEntry> locales){this.raw=raw;this.service=service;this.locales=locales==null?null:Collections.unmodifiableList(new ArrayList<>(locales));}
    }
    public interface Store {
        /** Includes an exact provider/native raw-locale comparison; null means unavailable. */
        State read();
        /** Called only with a reviewed native catalog entry, including empty System default. */
        boolean write(String nativeValue);
    }
    public static final class Row {
        public final String choice,label,locale;public final boolean selected;
        Row(String choice,LocaleEntry entry,boolean selected){this.choice=choice;this.label=entry.label;this.locale=entry.value;this.selected=selected;}
    }
    public static final class Snapshot {
        public final String reason,key,current,query;public final boolean systemDefault,custom;public final int offset,total;public final List<Row> rows;
        Snapshot(String reason,String key,String current,boolean systemDefault,boolean custom,String query,int offset,int total,List<Row> rows){this.reason=reason;this.key=key;this.current=current;this.systemDefault=systemDefault;this.custom=custom;this.query=query;this.offset=offset;this.total=total;this.rows=Collections.unmodifiableList(rows);}
    }
    public static void validateRead(String requestedKey,String query,int offset){if(requestedKey==null||!requestedKey.isEmpty()&&!token(requestedKey)||query==null||query.length()>80||query.codePoints().anyMatch(Character::isISOControl)||offset<0||offset>MAX_LOCALES||offset%PAGE_SIZE!=0||requestedKey.isEmpty()&&offset!=0)throw new IllegalArgumentException("Invalid caption language query");}
    public static void validateKey(String key){if(!token(key))throw new IllegalArgumentException("Invalid caption language key");}
    private static boolean token(String value){return value!=null&&value.matches("[0-9a-f]{64}");}
    private static boolean text(String value,int max){return value!=null&&!value.isEmpty()&&value.length()<=max&&value.codePoints().noneMatch(Character::isISOControl);}
    private static boolean valid(State state){
        if(state==null||!Objects.equals(state.raw,state.service)||state.raw!=null&&(!state.raw.isEmpty()&&!text(state.raw,128))||state.locales==null||state.locales.isEmpty()||state.locales.size()>MAX_LOCALES)return false;
        Set<String> values=new HashSet<>();
        for(LocaleEntry entry:state.locales)if(entry==null||!text(entry.value,128)||!text(entry.label,256)||!values.add(entry.value))return false;
        return true;
    }
    private static boolean same(State a,State b){return valid(b)&&Objects.equals(a.raw,b.raw)&&a.locales.equals(b.locales);}
    public static final class Backend {
        private final Store store;private final Supplier<Access> access;private final LongSupplier clock;private final Supplier<String> nonce;
        private State captured;private String key="";private long created;private List<LocaleEntry> entries=Collections.emptyList();private List<String> choices=Collections.emptyList();private final Set<String> offered=new HashSet<>();
        public Backend(Store store,Supplier<Access> access,LongSupplier clock,Supplier<String> nonce){this.store=store;this.access=access;this.clock=clock;this.nonce=nonce;}
        public synchronized void invalidate(){captured=null;key="";entries=Collections.emptyList();choices=Collections.emptyList();offered.clear();}
        private boolean fresh(){long now=clock.getAsLong();return captured!=null&&now>=created&&now-created<CATALOG_TTL_MS;}
        private Snapshot empty(String reason,String query,int offset){return new Snapshot(reason,"","",false,false,query,offset,0,Collections.emptyList());}
        public synchronized Snapshot snapshot(String requestedKey,String query,int offset){
            validateRead(requestedKey,query,offset);
            try{
                Access authority=access.get();if(authority!=Access.WRITABLE){invalidate();return empty(authority==Access.RESTRICTED?"policy_restricted":"control_unavailable",query,offset);}
                State current=store.read();if(!valid(current)){invalidate();return empty("control_unavailable",query,offset);}
                if(requestedKey.isEmpty()){
                    invalidate();captured=current;created=clock.getAsLong();key=nonce.get();if(!token(key))throw new IllegalStateException("Invalid caption catalog nonce");
                    entries=new ArrayList<>();entries.add(new LocaleEntry("","System default"));entries.addAll(current.locales);choices=new ArrayList<>();Set<String> unique=new HashSet<>();unique.add(key);
                    for(int i=0;i<entries.size();i++){String choice=nonce.get();if(!token(choice)||!unique.add(choice))throw new IllegalStateException("Invalid caption choice nonce");choices.add(choice);}
                }else if(!requestedKey.equals(key)||!fresh()||!same(captured,current)){invalidate();return empty("caption_language_changed",query,offset);}
                if(access.get()!=Access.WRITABLE){invalidate();return empty("policy_restricted",query,offset);}
                String selected=current.raw==null?"":current.raw,needle=query.toLowerCase(Locale.ROOT);List<Integer> filtered=new ArrayList<>();boolean custom=!selected.isEmpty();
                for(int i=0;i<entries.size();i++){LocaleEntry entry=entries.get(i);if(entry.value.equals(selected))custom=false;if(needle.isEmpty()||entry.label.toLowerCase(Locale.ROOT).contains(needle)||entry.value.toLowerCase(Locale.ROOT).contains(needle))filtered.add(i);}
                if(offset>filtered.size())return empty("caption_language_page_changed",query,offset);
                List<Row> rows=new ArrayList<>();for(int i=offset;i<Math.min(filtered.size(),offset+PAGE_SIZE);i++){int index=filtered.get(i);String choice=choices.get(index);offered.add(choice);rows.add(new Row(choice,entries.get(index),entries.get(index).value.equals(selected)));}
                return new Snapshot("ready",key,selected,selected.isEmpty(),custom,query,offset,filtered.size(),rows);
            }catch(SecurityException|IllegalStateException|LinkageError unavailable){invalidate();return empty("control_unavailable",query,offset);}
        }
        public synchronized String select(String requestedKey,String choice){
            if(!token(requestedKey)||!token(choice))throw new IllegalArgumentException("Invalid caption language choice");
            boolean written=false;
            try{
                Access authority=access.get();if(authority!=Access.WRITABLE){invalidate();return authority==Access.RESTRICTED?"policy_restricted":"control_unavailable";}
                if(!requestedKey.equals(key)||!fresh()||!offered.contains(choice)||!same(captured,store.read())){invalidate();return "caption_language_changed";}
                int index=choices.indexOf(choice);if(index<0){invalidate();return "caption_language_changed";}String value=entries.get(index).value;
                // A mutation consumes the review before crossing the provider boundary.
                invalidate();if(access.get()!=Access.WRITABLE)return "policy_restricted";
                if(!store.write(value))return "control_unavailable";
                written=true;
                State observed=store.read();boolean applied=valid(observed)&&Objects.equals(observed.raw,value);
                return access.get()==Access.WRITABLE&&applied?"control_applied":"control_requested";
            }catch(SecurityException|IllegalStateException|LinkageError unavailable){invalidate();return written?"control_requested":"control_unavailable";}
        }
    }
}
