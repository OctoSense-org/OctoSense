package dev.makepad.octosense.appstorage;

import java.util.EnumSet;
import java.util.Objects;
import java.util.function.LongSupplier;
import org.json.JSONArray;
import org.json.JSONObject;
import static dev.makepad.octosense.appstorage.AppStorageContract.*;

/** Reviewed one-use requests and asynchronous native completion are distinct states. */
public final class AppStorageBackend {
    public static final long TTL_MS=20000, COMPLETION_MS=60000;
    public static class State {
        public String packageName,label,incarnation,revision,availability="unavailable",reason="service_unavailable";
        public boolean sharedUid;
        public Long appBytes,dataBytes,cacheBytes;
        public EnumSet<Action> actions=EnumSet.noneOf(Action.class);
        public boolean statsAvailable(){return appBytes!=null&&dataBytes!=null&&cacheBytes!=null&&appBytes>=0&&dataBytes>=0&&cacheBytes>=0&&dataBytes>=cacheBytes;}
    }
    public interface Completion {void complete(String packageName,boolean succeeded);}
    public enum Failure {RESTRICTED,TARGET_CHANGED,UNAVAILABLE}
    public static final class Rejected extends Exception {
        public final Failure failure;
        public Rejected(Failure failure){this.failure=failure;}
    }
    public static final class Started {
        public final boolean accepted;public final Object nativeFlow;
        public Started(boolean accepted,Object nativeFlow){this.accepted=accepted;this.nativeFlow=nativeFlow;}
    }
    public interface Platform {
        State read(String packageName)throws Exception;
        Started request(State observed,Action action,Completion completion)throws Exception;
    }
    public static final class Result {
        public final String reason;public final Object nativeFlow;
        Result(String reason,Object nativeFlow){this.reason=reason;this.nativeFlow=nativeFlow;}
    }
    private static final class Operation {
        final String id,pkg,incarnation;final Action action;final long started;
        String state="pending";boolean outstanding=true;
        Operation(State observed,Action action,long started){this.id=hash("storage-operation",java.util.UUID.randomUUID().toString());pkg=observed.packageName;incarnation=observed.incarnation;this.action=action;this.started=started;}
    }
    private final Platform platform;private final LongSupplier clock;
    private State observed;private String observedKey;private long observedAt=-1;
    private Operation operation;
    public AppStorageBackend(Platform platform,LongSupplier clock){this.platform=platform;this.clock=clock;}
    private State read(String pkg)throws Exception{packageName(pkg);State state=platform==null?new State():platform.read(pkg);if(state==null)state=new State();state.packageName=pkg;return state;}
    public synchronized void invalidate(){observed=null;observedKey=null;observedAt=-1;}
    private void timeout(){if(operation!=null&&operation.outstanding&&clock.getAsLong()-operation.started>COMPLETION_MS)operation.state="uncertain";}
    public synchronized JSONObject snapshot(long id,String pkg)throws Exception{
        if(id<=0)throw new IllegalArgumentException("Positive request ID required");State state=read(pkg);long now=clock.getAsLong();timeout();
        if(observed==null||!pkg.equals(observed.packageName)||!Objects.equals(state.incarnation,observed.incarnation)||!Objects.equals(state.revision,observed.revision)||!state.actions.equals(observed.actions)||observedAt<0||now<observedAt||now-observedAt>TTL_MS)observedKey=hash("storage-review",pkg,state.incarnation,state.revision,java.util.UUID.randomUUID().toString());
        observed=state;observedAt=now;boolean available="available".equals(state.availability);
        String reason=operation!=null&&operation.outstanding&&!pkg.equals(operation.pkg)&&available?"operation_pending":state.reason;
        JSONObject out=new JSONObject().put("schema",1).put("request_id",id).put("package",pkg).put("availability",state.availability).put("reason",reason).put("shared_uid",available&&state.sharedUid);
        JSONArray actions=new JSONArray();if(available&&!state.sharedUid&&(operation==null||!operation.outstanding))for(Action action:state.actions)actions.put(action.wire);out.put("actions",actions);
        if(available){out.put("key",observedKey);if(state.label!=null)out.put("label",state.label);if(state.statsAvailable())out.put("stats",new JSONObject().put("app_bytes",state.appBytes).put("data_bytes",state.dataBytes).put("cache_bytes",state.cacheBytes));}
        if(operation!=null&&pkg.equals(operation.pkg)&&Objects.equals(state.incarnation,operation.incarnation))out.put("operation",new JSONObject().put("id",operation.id).put("action",operation.action.wire).put("state",operation.state));
        return out;
    }
    public synchronized Result action(String pkg,String key,String value)throws Exception{
        packageName(pkg);key(key);Action action=Action.parse(value);long now=clock.getAsLong();State before=observed;
        if(before==null||!pkg.equals(before.packageName)||!key.equals(observedKey)||observedAt<0||now<observedAt||now-observedAt>TTL_MS)return new Result("app_storage_target_changed",null);
        invalidate();if(operation!=null&&operation.outstanding)return new Result("app_storage_busy",null);
        State current=read(pkg);
        if(!"available".equals(current.availability)||current.sharedUid||!current.actions.contains(action))return new Result("app_storage_restricted",null);
        if(!Objects.equals(before.incarnation,current.incarnation)||!Objects.equals(before.revision,current.revision)||!before.actions.contains(action))return new Result("app_storage_target_changed",null);
        Operation next=action==Action.MANAGE_SPACE?null:new Operation(current,action,now);if(next!=null)operation=next;
        try{
            Started started=platform.request(current,action,(completed,succeeded)->complete(next,completed,succeeded));
            if(!started.accepted){if(next!=null){next.state="failed";next.outstanding=false;}return new Result("app_storage_unavailable",null);}
            if(action==Action.MANAGE_SPACE){if(started.nativeFlow==null)return new Result("app_storage_unavailable",null);return new Result("app_storage_flow_opened",started.nativeFlow);}
            return new Result("app_storage_requested",null);
        }catch(Rejected rejected){if(next!=null){next.state="failed";next.outstanding=false;}return new Result("app_storage_"+rejected.failure.name().toLowerCase(java.util.Locale.ROOT),null);}
        catch(Exception|LinkageError error){if(next!=null)next.state="uncertain";return new Result("app_storage_unconfirmed",null);}
    }
    private synchronized void complete(Operation expected,String pkg,boolean succeeded){
        if(expected==null||operation!=expected||!expected.outstanding)return;
        invalidate();
        if(!expected.pkg.equals(pkg)){expected.state="uncertain";return;}
        expected.outstanding=false;
        try{State current=read(pkg);expected.state=Objects.equals(current.incarnation,expected.incarnation)&&"available".equals(current.availability)?(succeeded?"succeeded":"failed"):"uncertain";}
        catch(Exception|LinkageError unavailable){expected.state="uncertain";}
    }
}
