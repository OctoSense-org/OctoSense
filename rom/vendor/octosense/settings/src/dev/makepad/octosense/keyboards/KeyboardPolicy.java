package dev.makepad.octosense.keyboards;

import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.util.ArrayList;
import java.util.Collections;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Objects;
import java.util.UUID;
import java.util.function.LongSupplier;

/** Finite observed authority; only the native preference callback can claim a keyboard change. */
public final class KeyboardPolicy {
    public static final int PAGE_SIZE=20,MAX_METHODS=128;
    public static final long CATALOG_MS=600000,OBSERVED_MS=30000,REVIEW_MS=120000;
    public enum Action {ENABLE,DISABLE,SETTINGS,SUBTYPES,PICK_DEFAULT}
    public enum Warning {SECURITY,DIRECT_BOOT}
    public enum Result {APPLIED,OPENED,CHANGED,RESTRICTED,UNAVAILABLE,UNCONFIRMED,BUSY}
    public static final class Method {
        public final String id,incarnation,label,packageName,summary,restriction;
        public final boolean enabled,selected,system,directBoot,tv,canEnable,canDisable,settings,subtypes;
        public Method(String id,String incarnation,String label,String pkg,String summary,String restriction,
                boolean enabled,boolean selected,boolean system,boolean directBoot,boolean tv,
                boolean canEnable,boolean canDisable,boolean settings,boolean subtypes){
            this.id=id;this.incarnation=incarnation;this.label=label;packageName=pkg;this.summary=summary;
            this.restriction=restriction;this.enabled=enabled;this.selected=selected;this.system=system;
            this.directBoot=directBoot;this.tv=tv;this.canEnable=canEnable;this.canDisable=canDisable;
            this.settings=settings;this.subtypes=subtypes;
        }
        boolean permits(Action action){switch(action){case ENABLE:return !enabled&&canEnable;case DISABLE:return enabled&&canDisable;case SETTINGS:return settings;case SUBTYPES:return subtypes;default:return false;}}
        public List<Warning> warnings(){List<Warning> result=new ArrayList<>();if(!system)result.add(Warning.SECURITY);if(!directBoot&&!tv)result.add(Warning.DIRECT_BOOT);return Collections.unmodifiableList(result);}
    }
    public static final class State {
        public final String revision,availability,reason,defaultId;
        public final boolean canPickDefault;
        public final List<Method> methods;
        public State(String revision,String availability,String reason,String defaultId,boolean canPickDefault,List<Method> methods){
            this.revision=revision;this.availability=availability;this.reason=reason;this.defaultId=defaultId;
            this.canPickDefault=canPickDefault;this.methods=Collections.unmodifiableList(new ArrayList<>(methods));
        }
    }
    public interface Platform {State read()throws Exception;}
    public static final class Row {public final String target;public final Method method;Row(String target,Method method){this.target=target;this.method=method;}}
    public static final class Page {
        public final String key,availability,reason,defaultId,query;public final boolean canPickDefault;
        public final int offset,total;public final List<Row> rows;
        Page(String key,State state,String query,int offset,int total,List<Row> rows){this.key=key;availability=state.availability;reason=state.reason;defaultId=state.defaultId;canPickDefault=state.canPickDefault;this.query=query;this.offset=offset;this.total=total;this.rows=Collections.unmodifiableList(rows);}
    }
    /** Only the nonexported native Activity receives the opaque ticket in an immutable PendingIntent. */
    public static final class Review {
        public final String ticket;public final Action action;public final Method method;public final List<Warning> warnings;
        Review(String ticket,Action action,Method method){this.ticket=ticket;this.action=action;this.method=method;warnings=action==Action.ENABLE?method.warnings():Collections.emptyList();}
    }
    /** Claimed only from the native preference's positive save callback. It carries no free-form setter. */
    public static final class Claim {
        public final Action action;public final Method method;public final State observed;
        Claim(Action action,Method method,State observed){this.action=action;this.method=method;this.observed=observed;}
    }
    private static final class Catalog {
        final State state;final String key;final long created;final Map<String,Method> targets=new HashMap<>();final Map<String,Long> seen=new HashMap<>();
        Catalog(State state,long now){this.state=state;created=now;key=hash(state.revision,UUID.randomUUID().toString());for(Method row:state.methods)targets.put(hash(key,row.id,row.incarnation),row);}
    }
    private static final class Pending {final Review review;final State state;final long created;Pending(Review review,State state,long created){this.review=review;this.state=state;this.created=created;}}
    private final Platform platform;private final LongSupplier clock;private Catalog catalog;private Pending pending;
    public KeyboardPolicy(Platform platform,LongSupplier clock){this.platform=platform;this.clock=clock;}
    public static String hash(String...values){try{MessageDigest digest=MessageDigest.getInstance("SHA-256");for(String value:values){byte[] bytes=value.getBytes(StandardCharsets.UTF_8);digest.update(java.nio.ByteBuffer.allocate(4).putInt(bytes.length).array());digest.update(bytes);}StringBuilder text=new StringBuilder();for(byte value:digest.digest())text.append(String.format(Locale.ROOT,"%02x",value&255));return text.toString();}catch(Exception impossible){throw new AssertionError(impossible);}}
    private static boolean text(String value,int max,boolean empty){return value!=null&&(empty||!value.trim().isEmpty())&&value.codePointCount(0,value.length())<=max&&!value.codePoints().anyMatch(Character::isISOControl);}
    private static boolean key(String value){return value!=null&&value.matches("[0-9a-f]{64}");}
    private static boolean live(long now,long created,long limit){return now>=created&&now-created<=limit;}
    private static State unavailable(){return new State(null,"unavailable","service_unavailable",null,false,Collections.emptyList());}
    private static boolean valid(State state){
        if(state==null)return false;
        if(!"available".equals(state.availability))return ("restricted".equals(state.availability)||"unavailable".equals(state.availability))&&!state.canPickDefault&&state.methods.isEmpty();
        if(!text(state.revision,1024,false)||state.methods.size()>MAX_METHODS)return false;
        HashSet<String> ids=new HashSet<>();int selected=0;
        for(Method row:state.methods){
            if(row==null||!text(row.id,512,false)||!text(row.incarnation,1024,false)||!text(row.label,256,false)||!text(row.packageName,256,false)||!text(row.summary,2048,true)||!text(row.restriction,256,true)||!ids.add(row.id))return false;
            if(row.canEnable&&row.enabled||row.canDisable&&!row.enabled)return false;
            if(row.selected){if(!row.enabled||!Objects.equals(state.defaultId,row.id)||++selected>1)return false;}
        }
        // Unknown/default-not-listed state is observed as such, never an invented selection.
        return state.defaultId==null||text(state.defaultId,512,true);
    }
    private State read(){try{State state=platform.read();return valid(state)?state:unavailable();}catch(Exception|LinkageError failure){return unavailable();}}
    private static Method find(State state,String id){for(Method row:state.methods)if(row.id.equals(id))return row;return null;}
    private static String fingerprint(State state){
        List<String> values=new ArrayList<>();Collections.addAll(values,state.revision,String.valueOf(state.defaultId),String.valueOf(state.canPickDefault));
        for(Method row:state.methods)values.add(hash(row.id,row.incarnation,row.label,row.packageName,row.summary,row.restriction,
                String.valueOf(row.enabled),String.valueOf(row.selected),String.valueOf(row.system),String.valueOf(row.directBoot),
                String.valueOf(row.tv),String.valueOf(row.canEnable),String.valueOf(row.canDisable),String.valueOf(row.settings),String.valueOf(row.subtypes)));
        return hash(values.toArray(new String[0]));
    }
    private static boolean same(State before,State now){return "available".equals(now.availability)&&fingerprint(before).equals(fingerprint(now));}
    public synchronized Page snapshot(String query,int offset){
        if(!text(query,80,true)||offset<0||offset>=MAX_METHODS||offset%PAGE_SIZE!=0)throw new IllegalArgumentException("Invalid keyboard page");
        long now=clock.getAsLong();State state=read();if(!"available".equals(state.availability)){catalog=null;pending=null;return new Page(null,state,query,0,0,new ArrayList<>());}
        if(catalog==null||!same(catalog.state,state)||!live(now,catalog.created,CATALOG_MS))catalog=new Catalog(state,now);
        List<Method> rows=new ArrayList<>();String filter=query.toLowerCase(Locale.ROOT).trim();for(Method row:state.methods)if((row.label+" "+row.packageName).toLowerCase(Locale.ROOT).contains(filter))rows.add(row);
        if(offset>=rows.size())offset=0;List<Row> result=new ArrayList<>();for(int i=offset;i<Math.min(rows.size(),offset+PAGE_SIZE);i++){Method row=rows.get(i);String target=hash(catalog.key,row.id,row.incarnation);catalog.seen.put(target,now);result.add(new Row(target,row));}
        return new Page(catalog.key,state,query,offset,rows.size(),result);
    }
    public synchronized Review prepare(String observed,String target,Action action){
        long now=clock.getAsLong();if(action==null||!key(observed)||catalog==null||!catalog.key.equals(observed)||!live(now,catalog.created,CATALOG_MS))return null;
        Method row=null;if(action==Action.PICK_DEFAULT){if(!"".equals(target)||!catalog.state.canPickDefault)return null;}
        else {row=catalog.targets.get(target);Long seen=catalog.seen.get(target);if(row==null||seen==null||!live(now,seen,OBSERVED_MS)||!row.permits(action))return null;}
        State current=read();if(!same(catalog.state,current)){catalog=null;return null;}
        if(row!=null){Method actual=find(current,row.id);if(actual==null||!actual.incarnation.equals(row.incarnation)||!actual.permits(action))return null;row=actual;}
        else if(!current.canPickDefault)return null;
        // A fresh observed user flow retires an older unclaimed review, including a failed
        // PendingIntent launch. The old native callback can never claim the replacement.
        Review review=new Review(hash(observed,UUID.randomUUID().toString()),action,row);pending=new Pending(review,current,now);catalog=null;return review;
    }
    public synchronized Review review(String ticket){if(!key(ticket)||pending==null||!pending.review.ticket.equals(ticket)||!live(clock.getAsLong(),pending.created,REVIEW_MS))return null;return pending.review;}
    public synchronized Review checkedReview(String ticket){Review review=review(ticket);if(review==null)return null;if(!same(pending.state,read())){pending=null;return null;}return review;}
    public synchronized void cancel(String ticket){if(pending!=null&&Objects.equals(pending.review.ticket,ticket))pending=null;}
    public synchronized void invalidate(){catalog=null;pending=null;}
    public synchronized Claim claimAfterNativeConsent(String ticket){
        Review review=review(ticket);if(review==null)return null;Pending captured=pending;pending=null; // retire before any native side effect
        State current=read();if(!same(captured.state,current))return null;
        Method method=review.method;if(method!=null){method=find(current,method.id);if(method==null||!method.incarnation.equals(review.method.incarnation)||!method.permits(review.action))return null;}
        else if(review.action!=Action.PICK_DEFAULT||!current.canPickDefault)return null;
        return new Claim(review.action,method,current);
    }
    public synchronized Result observeCompletion(Claim claim){
        if(claim==null)return Result.CHANGED;State state=read();if(!"available".equals(state.availability))return Result.UNCONFIRMED;
        if(claim.action!=Action.ENABLE&&claim.action!=Action.DISABLE)return Result.OPENED;
        Method actual=find(state,claim.method.id);if(actual==null||!actual.incarnation.equals(claim.method.incarnation))return Result.UNCONFIRMED;
        if(claim.action==Action.DISABLE&&Objects.equals(state.defaultId,actual.id))return Result.UNCONFIRMED;
        return actual.enabled==(claim.action==Action.ENABLE)?Result.APPLIED:Result.UNCONFIRMED;
    }
}
