package dev.makepad.octosense.dnd;

import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;
import java.util.function.LongSupplier;
import org.json.JSONArray;
import org.json.JSONObject;
import static dev.makepad.octosense.dnd.DndSettingsContract.*;

/** Serialized observed leases. The platform rechecks owner/policy immediately before its write. */
public final class DndSettingsBackend {
    public static final class Rule {
        public String target,name,revision; public boolean enabled,editable,deletable;
        public Boolean active; public Schedule schedule; public Object nativeRule;
    }
    public static final class State {
        public boolean available,writable,modesApi,modesUi,policyWritable,canCreate;
        public String manualMode,currentMode,revision; public int repeatMinutes;
        public Policy policy,effectivePolicy; public List<Rule> rules=new ArrayList<>();
    }
    public interface Platform {
        State read() throws Exception;
        boolean policy(State observed,Policy value) throws Exception;
        boolean schedule(State observed,Rule rule,Schedule value) throws Exception;
        boolean enabled(State observed,Rule rule,boolean value) throws Exception;
        boolean delete(State observed,Rule rule) throws Exception;
    }
    private final Platform platform;private final LongSupplier clock;
    private String observedKey,observedRevision;private long observedAt=-1;
    public DndSettingsBackend(Platform platform,LongSupplier clock) {this.platform=platform;this.clock=clock;}
    private State read() throws Exception {
        State state=platform==null?new State():platform.read();
        state.rules.sort(Comparator.comparing((Rule rule)->rule.name,String.CASE_INSENSITIVE_ORDER).thenComparing(rule->rule.target));
        return state;
    }
    private String revision(State state) {
        ArrayList<String> fields=new ArrayList<>();fields.add("dnd-v1");fields.add(state.revision);
        fields.add(String.valueOf(state.available));fields.add(String.valueOf(state.writable));fields.add(String.valueOf(state.modesApi));fields.add(String.valueOf(state.modesUi));
        fields.add(String.valueOf(state.policyWritable));fields.add(String.valueOf(state.canCreate));
        fields.add(state.policy==null?null:state.policy.fingerprint());
        for(Rule rule:state.rules) {fields.add(rule.target);fields.add(rule.revision);fields.add(String.valueOf(rule.editable));fields.add(String.valueOf(rule.deletable));}
        return hash(fields.toArray(new String[0]));
    }
    private JSONArray policyJson(Policy policy,boolean writable) throws Exception {
        JSONArray result=new JSONArray();for(Field field:Field.values()) {
            String value=policy==null?null:policy.value(field);
            result.put(new JSONObject().put("field",field.wire).put("value",value==null?JSONObject.NULL:value).put("can_set",writable&&value!=null));
        }return result;
    }
    public synchronized JSONObject snapshot(long id,int offset,String generation) throws Exception {
        if(id<=0)throw new IllegalArgumentException("Positive request ID required");offset(offset,generation);
        State state=read();String revision=revision(state);String current=hash("catalog",revision);
        long now=clock.getAsLong();
        if(observedAt<0||now<observedAt||now-observedAt>20000||!revision.equals(observedRevision))
            observedKey=hash("lease",revision,java.util.UUID.randomUUID().toString());
        String key=observedKey;
        int total=Math.min(MAX_RULES,state.rules.size());boolean stale=generation!=null&&!generation.equals(current);
        if(stale||offset>=total)offset=0;
        JSONArray rows=new JSONArray();
        if(state.available)for(int i=offset;i<Math.min(total,offset+PAGE_SIZE);i++) {
            Rule rule=state.rules.get(i);JSONObject row=new JSONObject().put("target",rule.target).put("name",rule.name)
                .put("kind",rule.schedule==null?"other":"time").put("enabled",rule.enabled)
                .put("active",rule.active==null?JSONObject.NULL:rule.active)
                .put("can_edit",state.writable&&rule.editable).put("can_delete",state.writable&&rule.deletable);
            if(rule.schedule!=null) {Schedule schedule=rule.schedule;JSONArray days=new JSONArray();for(int day:schedule.days)days.put(day);
                row.put("schedule",new JSONObject().put("days",days).put("start_minute",schedule.start).put("end_minute",schedule.end).put("exit_at_alarm",schedule.exitAtAlarm));}
            rows.put(row);
        }
        observedRevision=revision;observedAt=now;
        JSONObject result=new JSONObject().put("schema",1).put("request_id",id).put("availability",state.available?"available":"unavailable")
            .put("key",key).put("modes_api",state.modesApi).put("modes_ui",state.modesUi)
            .put("manual_mode",state.manualMode==null?JSONObject.NULL:state.manualMode).put("current_mode",state.currentMode==null?JSONObject.NULL:state.currentMode)
            .put("policy",policyJson(state.policy,state.available&&state.writable&&state.policyWritable)).put("effective_policy",policyJson(state.effectivePolicy,false))
            .put("generation",current).put("offset",offset).put("total",state.available?total:0).put("stale",stale)
            .put("truncated",state.rules.size()>MAX_RULES).put("can_create",state.available&&state.writable&&state.canCreate&&state.rules.size()<MAX_RULES).put("rules",rows);
        if(state.repeatMinutes>0)result.put("repeat_callers_minutes",state.repeatMinutes);return result;
    }
    private State claim(String key) throws Exception {
        key(key);long now=clock.getAsLong();
        if(!key.equals(observedKey)||observedAt<0||now<observedAt||now-observedAt>20000)return null;
        State state=read();if(!java.util.Objects.equals(observedRevision,revision(state)))return null;
        // Claim before any call into Android. Readback cannot turn this into replayable authority.
        observedAt=-1;return state;
    }
    private Rule target(State state,String target) {
        key(target);for(Rule rule:state.rules)if(rule.target.equals(target))return rule;return null;
    }
    public synchronized String policy(String key,Field field,String value) throws Exception {
        field.value(value);State state=claim(key);if(state==null)return "dnd_target_changed";
        if(!state.available||!state.writable)return "dnd_restricted";
        if(!state.policyWritable||state.policy==null||state.policy.value(field)==null)return "dnd_unavailable";
        if(!platform.policy(state,state.policy.change(field,value)))return "dnd_restricted";
        State after=read();return after.available&&after.policy!=null&&value.equals(after.policy.value(field))?"dnd_applied":"dnd_requested";
    }
    public synchronized String schedule(String key,String target,Schedule value) throws Exception {
        if(value.enabled&&value.days.length==0)throw new IllegalArgumentException("Enabled schedule requires days");
        if(target!=null)key(target);State state=claim(key);if(state==null)return "dnd_target_changed";
        if(!state.available||!state.writable)return "dnd_restricted";
        Rule rule=target==null?null:target(state,target);
        if(target==null?!state.canCreate||state.rules.size()>=MAX_RULES:rule==null||!rule.editable||rule.schedule==null)return "dnd_unavailable";
        if(!platform.schedule(state,rule,value))return "dnd_restricted";
        // Creation has no caller-chosen native identity; compare against only newly observed targets.
        State after=read();for(Rule candidate:after.rules) {
            if(target!=null?!candidate.target.equals(target):target(state,candidate.target)!=null)continue;
            Schedule actual=candidate.schedule;
            if(actual!=null&&actual.name.equals(value.name)&&java.util.Arrays.equals(actual.days,value.days)&&actual.start==value.start&&actual.end==value.end
                &&actual.exitAtAlarm==value.exitAtAlarm&&candidate.enabled==value.enabled)return "dnd_applied";
        }return "dnd_requested";
    }
    public synchronized String enabled(String key,String target,boolean enabled) throws Exception {
        key(target);State state=claim(key);if(state==null)return "dnd_target_changed";
        if(!state.available||!state.writable)return "dnd_restricted";
        Rule rule=target(state,target);if(rule==null||!rule.editable||rule.schedule==null||enabled&&rule.schedule.days.length==0)return "dnd_unavailable";
        if(!platform.enabled(state,rule,enabled))return "dnd_restricted";
        State after=read();Rule actual=target(after,target);return actual!=null&&actual.enabled==enabled?"dnd_applied":"dnd_requested";
    }
    public synchronized String delete(String key,String target) throws Exception {
        key(target);State state=claim(key);if(state==null)return "dnd_target_changed";
        if(!state.available||!state.writable)return "dnd_restricted";
        Rule rule=target(state,target);if(rule==null||!rule.deletable||rule.schedule==null)return "dnd_unavailable";
        if(!platform.delete(state,rule))return "dnd_restricted";
        return target(read(),target)==null?"dnd_applied":"dnd_requested";
    }
}
