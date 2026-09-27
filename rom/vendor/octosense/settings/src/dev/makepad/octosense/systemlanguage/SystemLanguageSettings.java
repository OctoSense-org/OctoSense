package dev.makepad.octosense.systemlanguage;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import java.util.UUID;
import java.util.function.LongSupplier;
import org.json.JSONArray;
import org.json.JSONObject;

/** Fixed-lifetime catalog authority survives browsing, but every write consumes it once. */
public final class SystemLanguageSettings {
    public static final long CATALOG_MS=600000;
    public static final class Current {
        public final String tag,label,nativeLabel;public final boolean translated;
        public Current(String tag,String label,String nativeLabel,boolean translated){this.tag=tag;this.label=label;this.nativeLabel=nativeLabel;this.translated=translated;}
    }
    public static final class Node {
        public final String id,parent,label,secondary,kind,level,tag;public final boolean translated,suggested;
        public Node(String id,String parent,String label,String secondary,String kind,String level,String tag,boolean translated,boolean suggested){this.id=id;this.parent=parent;this.label=label;this.secondary=secondary;this.kind=kind;this.level=level;this.tag=tag;this.translated=translated;this.suggested=suggested;}
    }
    public static class State {
        public String revision,availability="unavailable",reason="service_unavailable";public boolean writable;
        public final List<Current> current=new ArrayList<>();public final List<Node> nodes=new ArrayList<>();
    }
    /** A native current locale or native selectable leaf, resolved by the backend, not the caller. */
    public static final class Selection {
        public final String id,tag;public final boolean existing;
        Selection(String id,String tag,boolean existing){this.id=id;this.tag=tag;this.existing=existing;}
    }
    public enum Write {APPLIED,REQUESTED,TARGET_CHANGED,RESTRICTED,UNAVAILABLE,UNCONFIRMED}
    public interface Platform {State read()throws Exception;Write apply(State observed,List<Selection> order)throws Exception;}
    private static final class Catalog {
        final State state;final String key;final long created;
        final Map<String,Node> nodes=new LinkedHashMap<>();final Map<String,Selection> selections=new HashMap<>();final Set<String> observed=new HashSet<>();
        Catalog(State state,long now){this.state=state;created=now;key=SystemLanguageContract.hash(state.revision,UUID.randomUUID().toString());
            for(Current row:state.current){String target=currentToken(row.tag);selections.put(target,new Selection(row.tag,row.tag,true));observed.add(target);}
            for(Node node:state.nodes){String target=nodeToken(node.id);nodes.put(target,node);if(node.kind.equals("select"))selections.put(target,new Selection(node.id,node.tag,false));}
        }
        String currentToken(String tag){return SystemLanguageContract.hash(key,"current",tag);}String nodeToken(String id){return SystemLanguageContract.hash(key,"catalog",id);}
    }
    private final Platform platform;private final LongSupplier clock;private Catalog catalog;
    public SystemLanguageSettings(Platform platform,LongSupplier clock){this.platform=platform;this.clock=clock;}
    private static boolean printable(String value){return value!=null&&!value.trim().isEmpty()&&value.codePointCount(0,value.length())<=256&&!value.codePoints().anyMatch(Character::isISOControl);}
    private boolean valid(State state){
        if(state==null||!"available".equals(state.availability)||state.revision==null||state.current.isEmpty()||state.current.size()>SystemLanguageContract.MAX_CURRENT||state.nodes.size()>SystemLanguageContract.MAX_NODES)return false;
        Set<String> tags=new HashSet<>();for(Current row:state.current)if(!printable(row.tag)||!printable(row.label)||!printable(row.nativeLabel)||!tags.add(row.tag))return false;
        Map<String,Node> ids=new HashMap<>();for(Node node:state.nodes){if(node.id==null||node.parent==null||!printable(node.label)||node.secondary==null||(!node.secondary.isEmpty()&&!printable(node.secondary))||!printable(node.tag)||(!"region".equals(node.level)&&!"numbering".equals(node.level))||(!"open".equals(node.kind)&&!"select".equals(node.kind))||ids.put(node.id,node)!=null)return false;}
        for(Node node:state.nodes){int depth=1;String parent=node.parent;while(!parent.isEmpty()){Node ancestor=ids.get(parent);if(ancestor==null||!ancestor.kind.equals("open")||++depth>SystemLanguageContract.MAX_DEPTH)return false;parent=ancestor.parent;}}
        return true;
    }
    private boolean live(Catalog value,long now){return value!=null&&now>=value.created&&now-value.created<=CATALOG_MS;}
    private boolean same(State a,State b){return valid(b)&&a.revision.equals(b.revision)&&a.writable==b.writable;}
    public synchronized JSONObject snapshot(long id,String key,String parent,String query,int offset)throws Exception{
        SystemLanguageContract.read(id,key,parent,query,offset);long now=clock.getAsLong();State state;
        try{state=platform.read();}catch(Exception|LinkageError unavailable){catalog=null;return SystemLanguageContract.unavailable(id,query,"unavailable","service_unavailable");}
        if(state==null){catalog=null;return SystemLanguageContract.unavailable(id,query,"unavailable","service_unavailable");}
        if(!"available".equals(state.availability)){catalog=null;return SystemLanguageContract.unavailable(id,query,state.availability,state.reason);}
        if(!valid(state)){catalog=null;return SystemLanguageContract.unavailable(id,query,"unavailable","invalid_catalog");}
        if(key.isEmpty())catalog=new Catalog(state,now);
        else if(catalog==null||!catalog.key.equals(key)||!same(catalog.state,state)){catalog=null;return SystemLanguageContract.unavailable(id,query,"stale","target_changed");}
        else if(!live(catalog,now)){catalog=null;return SystemLanguageContract.unavailable(id,query,"stale","catalog_expired");}
        Catalog active=catalog;Node branch=parent.isEmpty()?null:active.nodes.get(parent);
        if(!parent.isEmpty()&&(branch==null||!branch.kind.equals("open")))return SystemLanguageContract.unavailable(id,query,"stale","target_changed");
        String parentId=branch==null?"":branch.id,filter=query.trim().toLowerCase(Locale.ROOT);List<Node> rows=new ArrayList<>();
        for(Node node:active.state.nodes)if(node.parent.equals(parentId)&&(filter.isEmpty()||(node.label+" "+node.secondary+" "+node.tag).toLowerCase(Locale.ROOT).contains(filter)))rows.add(node);
        if(offset>0&&offset>=rows.size())offset=0;
        JSONArray values=new JSONArray();for(int index=offset;index<Math.min(rows.size(),offset+SystemLanguageContract.PAGE_SIZE);index++){
            Node node=rows.get(index);String target=active.nodeToken(node.id);JSONObject row=new JSONObject().put("target",target).put("label",node.label).put("tag",node.tag).put("kind",node.kind).put("translated",node.translated).put("suggested",node.suggested);
            if(!node.secondary.isEmpty())row.put("secondary",node.secondary);values.put(row);if(node.kind.equals("select"))active.observed.add(target);
        }
        JSONArray current=new JSONArray();for(Current row:state.current)current.put(new JSONObject().put("target",active.currentToken(row.tag)).put("tag",row.tag).put("label",row.label).put("native_label",row.nativeLabel).put("translated",row.translated));
        JSONObject result=new JSONObject().put("schema",1).put("request_id",id).put("availability","available").put("reason","none").put("key",active.key).put("can_apply",state.writable).put("current",current)
            .put("parent",parent.isEmpty()?JSONObject.NULL:parent).put("level",branch==null?"language":branch.level).put("query",query).put("offset",offset).put("total",rows.size()).put("page_size",SystemLanguageContract.PAGE_SIZE).put("rows",values);
        if(branch!=null)result.put("parent_label",branch.label);return result;
    }
    public synchronized String apply(String key,String[] targets){
        SystemLanguageContract.order(key,targets);long now=clock.getAsLong();Catalog observed=catalog;
        if(!live(observed,now)||!observed.key.equals(key)||!observed.state.writable)return "languages_target_changed";
        List<Selection> order=new ArrayList<>();Set<String> tags=new HashSet<>();
        for(String target:targets){Selection row=observed.selections.get(target);if(row==null||!observed.observed.contains(target)||!tags.add(row.tag))return "languages_target_changed";order.add(row);}
        catalog=null; // Consume before native work; uncertainty never permits replay.
        try{
            State current=platform.read();if(current!=null&&"restricted".equals(current.availability))return "languages_restricted";
            if(!same(observed.state,current)||!current.writable)return "languages_target_changed";
            // Re-resolve semantic values, even if a buggy provider reused its revision.
            for(Selection selected:order){boolean found=false;if(selected.existing){for(Current row:current.current)if(row.tag.equals(selected.tag))found=true;}else{for(Node node:current.nodes)if(node.id.equals(selected.id)&&node.kind.equals("select")&&node.tag.equals(selected.tag))found=true;}if(!found)return "languages_target_changed";}
            switch(platform.apply(current,order)){case APPLIED:return "languages_applied";case REQUESTED:return "languages_requested";case TARGET_CHANGED:return "languages_target_changed";case RESTRICTED:return "languages_restricted";case UNAVAILABLE:return "languages_unavailable";default:return "languages_unconfirmed";}
        }catch(Exception|LinkageError unknown){return "languages_unconfirmed";}
    }
    public synchronized void invalidate(){catalog=null;}
}
