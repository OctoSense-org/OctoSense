package dev.makepad.octosense.applanguage;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Objects;
import java.util.UUID;
import java.util.function.LongSupplier;
import org.json.JSONArray;
import org.json.JSONObject;

/** Current native hierarchy with expiring observed choices; never accepts a locale tag. */
public final class AppLanguageBackend {
    public static final long CATALOG_MS=600000,OBSERVED_MS=20000;
    public static final class Current {public final String tag,label;public Current(String tag,String label){this.tag=tag;this.label=label;}}
    public static final class Node {
        public final String id,parent,label,secondary,kind,level,target;
        public final boolean selected,systemDefault,suggested;
        public Node(String id,String parent,String label,String secondary,String kind,String level,String target,boolean selected,boolean systemDefault,boolean suggested){this.id=id;this.parent=parent;this.label=label;this.secondary=secondary;this.kind=kind;this.level=level;this.target=target;this.selected=selected;this.systemDefault=systemDefault;this.suggested=suggested;}
    }
    public static class State {
        public String packageName,label,incarnation,revision,availability="unavailable",reason="service_unavailable";
        public boolean writable;
        public final List<Current> current=new ArrayList<>();
        public final List<Node> nodes=new ArrayList<>();
    }
    public enum Write {APPLIED,REQUESTED,TARGET_CHANGED,RESTRICTED,UNAVAILABLE,UNCONFIRMED}
    public interface Platform {State read(String pkg)throws Exception;Write select(State observed,Node node)throws Exception;}
    private static final class Catalog {
        final State state;final String key;final long created;
        final Map<String,Node> nodes=new LinkedHashMap<>();final Map<String,Long> observed=new HashMap<>();
        Catalog(State state,long now){this.state=state;created=now;key=AppLanguageContract.hash(state.packageName,state.revision,UUID.randomUUID().toString());for(Node node:state.nodes)nodes.put(token(node.id),node);}
        String token(String id){return AppLanguageContract.hash(key,id);}
    }
    private final Platform platform;private final LongSupplier clock;private Catalog catalog;
    public AppLanguageBackend(Platform platform,LongSupplier clock){this.platform=platform;this.clock=clock;}
    private boolean valid(State state,String pkg) {
        if(state==null||!pkg.equals(state.packageName)||!"available".equals(state.availability)||state.incarnation==null||state.revision==null||state.label==null||state.label.isEmpty()||state.current.size()>AppLanguageContract.MAX_CURRENT||state.nodes.size()>AppLanguageContract.MAX_NODES)return false;
        for(Current value:state.current)if(value.tag==null||value.tag.isEmpty()||value.label==null||value.label.isEmpty())return false;
        Map<String,Node> ids=new HashMap<>();for(Node node:state.nodes){if(node.id==null||node.parent==null||node.label==null||node.label.isEmpty()||node.target==null||(!"region".equals(node.level)&&!"numbering".equals(node.level))||(!"open".equals(node.kind)&&!"select".equals(node.kind))||ids.put(node.id,node)!=null)return false;}
        for(Node node:state.nodes){int depth=1;String parent=node.parent;while(!parent.isEmpty()){Node ancestor=ids.get(parent);if(ancestor==null||!ancestor.kind.equals("open")||++depth>AppLanguageContract.MAX_DEPTH)return false;parent=ancestor.parent;}}
        return true;
    }
    private JSONObject unavailable(long id,String pkg,String query,String availability,String reason)throws Exception{return AppLanguageContract.unavailable(id,pkg,query,availability,reason);}
    public synchronized JSONObject snapshot(long id,String pkg,String key,String parent,String query,int offset)throws Exception {
        AppLanguageContract.read(id,pkg,key,parent,query,offset);long now=clock.getAsLong();State state;
        try{state=platform.read(pkg);}catch(Exception|LinkageError unavailable){catalog=null;return unavailable(id,pkg,query,"unavailable","service_unavailable");}
        if(state==null){catalog=null;return unavailable(id,pkg,query,"unavailable","service_unavailable");}
        if(!"available".equals(state.availability)){catalog=null;return unavailable(id,pkg,query,state.availability,state.reason);}
        if(!valid(state,pkg)){catalog=null;return unavailable(id,pkg,query,"unavailable","catalog_limit");}
        if(key.isEmpty()){catalog=new Catalog(state,now);}
        else if(catalog==null||!catalog.key.equals(key)||!catalog.state.packageName.equals(pkg)||!catalog.state.incarnation.equals(state.incarnation)||!catalog.state.revision.equals(state.revision)||catalog.state.writable!=state.writable){
            catalog=null;return unavailable(id,pkg,query,"stale","target_changed");
        } else if(now<catalog.created||now-catalog.created>CATALOG_MS){catalog=null;return unavailable(id,pkg,query,"stale","catalog_expired");}
        Catalog active=catalog;Node branch=parent.isEmpty()?null:active.nodes.get(parent);
        if(!parent.isEmpty()&&(branch==null||!branch.kind.equals("open")))return unavailable(id,pkg,query,"stale","target_changed");
        String parentId=branch==null?"":branch.id;String filter=query.toLowerCase(Locale.ROOT).trim();List<Node> rows=new ArrayList<>();
        for(Node node:active.state.nodes)if(node.parent.equals(parentId)&&(filter.isEmpty()||(node.label+" "+(node.secondary==null?"":node.secondary)+" "+node.target).toLowerCase(Locale.ROOT).contains(filter)))rows.add(node);
        if(offset>0&&offset>=rows.size())offset=0;
        JSONArray values=new JSONArray();for(int index=offset;index<Math.min(rows.size(),offset+AppLanguageContract.PAGE_SIZE);index++){
            Node node=rows.get(index);String target=active.token(node.id);
            JSONObject row=new JSONObject().put("key",target).put("label",node.label).put("kind",node.kind).put("selected",node.selected).put("system_default",node.systemDefault).put("suggested",node.suggested);
            if(node.secondary!=null&&!node.secondary.isEmpty())row.put("secondary",node.secondary);values.put(row);active.observed.put(target,now);
        }
        JSONArray current=new JSONArray();for(Current locale:state.current)current.put(new JSONObject().put("tag",locale.tag).put("label",locale.label));
        JSONObject result=new JSONObject().put("schema",1).put("request_id",id).put("package",pkg).put("availability","available").put("reason","none")
            .put("key",active.key).put("parent",parent.isEmpty()?JSONObject.NULL:parent).put("label",state.label).put("current",current).put("system_default",state.current.isEmpty())
            .put("can_set",state.writable).put("level",branch==null?"language":branch.level).put("query",query).put("offset",offset).put("total",rows.size()).put("page_size",AppLanguageContract.PAGE_SIZE).put("rows",values);
        if(branch!=null)result.put("parent_label",branch.label);
        return result;
    }
    public synchronized String select(String pkg,String key,String choice) {
        AppLanguageContract.packageName(pkg);AppLanguageContract.key(key);AppLanguageContract.key(choice);
        long now=clock.getAsLong();Catalog observed=catalog;
        if(observed==null||!observed.key.equals(key)||!observed.state.packageName.equals(pkg)||now<observed.created||now-observed.created>CATALOG_MS)return "app_language_target_changed";
        Node node=observed.nodes.get(choice);Long seen=observed.observed.get(choice);
        if(node==null||!node.kind.equals("select")||seen==null||now<seen||now-seen>OBSERVED_MS||!observed.state.writable)return "app_language_target_changed";
        catalog=null; // One claim, including failed or uncertain operations. Never replay.
        try {
            State current=platform.read(pkg);
            if(current!=null&&"restricted".equals(current.availability))return "app_language_restricted";
            if(!valid(current,pkg)||!current.writable||!observed.state.incarnation.equals(current.incarnation)||!observed.state.revision.equals(current.revision))return "app_language_target_changed";
            Node fresh=null;for(Node candidate:current.nodes)if(candidate.id.equals(node.id)){fresh=candidate;break;}
            if(fresh==null||!fresh.kind.equals("select")||!Objects.equals(fresh.target,node.target)||fresh.systemDefault!=node.systemDefault)return "app_language_target_changed";
            Write result=platform.select(current,fresh);
            switch(result){case APPLIED:return "app_language_applied";case REQUESTED:return "app_language_requested";case TARGET_CHANGED:return "app_language_target_changed";case RESTRICTED:return "app_language_restricted";case UNAVAILABLE:return "app_language_unavailable";default:return "app_language_unconfirmed";}
        }catch(Exception|LinkageError unknown){return "app_language_unconfirmed";}
    }
    public synchronized void invalidate(){catalog=null;}
}
