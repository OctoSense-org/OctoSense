package com.android.permissioncontroller.octosense;

import android.app.KeyguardManager;
import android.app.PendingIntent;
import android.app.role.RoleManager;
import android.content.Context;
import android.content.Intent;
import android.content.pm.ApplicationInfo;
import android.content.pm.PackageInfo;
import android.content.pm.PackageManager;
import android.os.Process;
import android.os.SystemClock;
import android.os.UserHandle;
import android.os.UserManager;
import com.android.role.controller.model.Role;
import com.android.role.controller.model.Roles;
import dev.makepad.octosense.roles.RolesSettingsContract;
import dev.makepad.octosense.roles.RolesSettingsContract.RoleId;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.UUID;
import org.json.JSONArray;
import org.json.JSONObject;

/** Runs inside PermissionController, using its own native role model and restrictions. */
final class RoleSettingsBackend {
    private static RoleSettingsBackend instance;
    static synchronized RoleSettingsBackend get(Context context){if(instance==null)instance=new RoleSettingsBackend(context.getApplicationContext());return instance;}
    private final Context context;
    private final RolesSettingsContract.Lease lease=new RolesSettingsContract.Lease();
    private final Set<String> observedTargets=new HashSet<>();
    private RoleId observedRole;
    private final Map<String,Ticket> tickets=new HashMap<>();
    private RoleSettingsBackend(Context context){this.context=context;}
    boolean owner(){UserManager users=context.getSystemService(UserManager.class);KeyguardManager lock=context.getSystemService(KeyguardManager.class);
        return Process.myUserHandle().equals(UserHandle.SYSTEM)&&users!=null&&users.isUserForeground()&&users.isUserUnlocked()&&lock!=null&&!lock.isKeyguardLocked();}
    private static String hash(String text){return RolesSettingsContract.hash(text);}
    private static String text(CharSequence text){return RolesSettingsContract.text(text,256);}
    private static final class Candidate {
        String key,pkg,label,identity;boolean selected,allowed;
        JSONObject json()throws Exception{return new JSONObject().put("target",key).put("package",pkg).put("label",label).put("selected",selected).put("can_select",allowed);}
    }
    private static final class State {
        RoleId id;Role role;String label,holderIdentity="",holderPackage,holderLabel,generation,fingerprint,noneTarget;
        boolean available,restricted,truncated;final List<Candidate> candidates=new ArrayList<>();
        Candidate candidate(String key){for(Candidate candidate:candidates)if(candidate.key.equals(key))return candidate;return null;}
        JSONObject overview()throws Exception{return new JSONObject().put("role",id.wire).put("label",label).put("available",available).put("restricted",restricted)
            .put("holder",holderPackage==null?JSONObject.NULL:new JSONObject().put("package",holderPackage).put("label",holderLabel));}
    }
    private String identity(PackageInfo info){StringBuilder value=new StringBuilder(info.packageName+":"+info.applicationInfo.uid+":"+info.firstInstallTime+":"+info.lastUpdateTime+":"+info.getLongVersionCode());
        if(info.signingInfo==null)throw new IllegalStateException("Missing candidate signer");
        ArrayList<String> signatures=new ArrayList<>();for(android.content.pm.Signature signer:info.signingInfo.getApkContentsSigners())signatures.add(hash(signer.toCharsString()));signatures.sort(String::compareTo);for(String signer:signatures)value.append(':').append(signer);return hash(value.toString());}
    private PackageInfo application(String pkg)throws Exception{PackageInfo info=context.getPackageManager().getPackageInfo(pkg,PackageManager.GET_SIGNING_CERTIFICATES);
        if((info.applicationInfo.flags&ApplicationInfo.FLAG_INSTALLED)==0||!Process.myUserHandle().equals(UserHandle.getUserHandleForUid(info.applicationInfo.uid)))throw new IllegalStateException("Candidate is not installed for owner");return info;}
    private String label(ApplicationInfo app){String label=text(app.loadLabel(context.getPackageManager()));return label.isEmpty()?app.packageName:label;}
    private State read(RoleId id,boolean candidates)throws Exception{
        State state=new State();state.id=id;state.role=Roles.get(context).get(id.nativeName);state.label=id.label;
        Role role=state.role;
        if(role!=null&&role.getLabelResource()!=0)state.label=text(context.getString(role.getLabelResource()));
        state.available=role!=null&&role.isAvailableAsUser(UserHandle.SYSTEM,context)&&role.isVisibleAsUser(UserHandle.SYSTEM,context)&&role.isExclusive();
        if(!state.available)return state;
        state.restricted=role.getRestrictionIntentAsUser(UserHandle.SYSTEM,context)!=null;
        List<String> holders=context.getSystemService(RoleManager.class).getRoleHolders(id.nativeName);
        if(holders.size()>1)throw new IllegalStateException("Exclusive role has multiple holders");
        if(!holders.isEmpty()){PackageInfo holder=application(holders.get(0));state.holderPackage=holder.packageName;state.holderLabel=label(holder.applicationInfo);state.holderIdentity=identity(holder);}
        if(!candidates)return state;
        Set<String> seen=new HashSet<>();
        for(String pkg:role.getQualifyingPackagesAsUser(UserHandle.SYSTEM,context)){
            if(!seen.add(pkg))continue;
            PackageInfo info;try{info=application(pkg);}catch(PackageManager.NameNotFoundException removed){continue;}
            ApplicationInfo app=info.applicationInfo;if(!role.isApplicationVisibleAsUser(app,UserHandle.SYSTEM,context))continue;
            Candidate candidate=new Candidate();candidate.pkg=pkg;candidate.label=label(app);candidate.identity=identity(info);
            candidate.key=hash(id.wire+":"+candidate.identity);candidate.selected=pkg.equals(state.holderPackage);
            candidate.allowed=!state.restricted&&app.enabled&&(app.flags&ApplicationInfo.FLAG_SUSPENDED)==0
                    &&role.getApplicationRestrictionIntentAsUser(app,UserHandle.SYSTEM,context)==null&&!candidate.selected;
            state.candidates.add(candidate);
        }
        state.candidates.sort(Comparator.comparing((Candidate candidate)->candidate.label,String.CASE_INSENSITIVE_ORDER).thenComparing(candidate->candidate.pkg));
        StringBuilder generation=new StringBuilder(id.wire),fingerprint=new StringBuilder(id.wire+":"+state.holderIdentity+":"+state.restricted);
        for(Candidate candidate:state.candidates){generation.append(':').append(candidate.key).append(':').append(candidate.label);fingerprint.append(':').append(candidate.key).append(':').append(candidate.allowed);}
        if(role.shouldShowNone()&&!state.restricted&&state.holderPackage!=null){state.noneTarget=hash(id.wire+":none:"+state.holderIdentity);fingerprint.append(':').append(state.noneTarget);}
        state.generation=hash(generation.toString());state.fingerprint=hash(fingerprint.toString());
        if(state.candidates.size()>RolesSettingsContract.MAX_ROWS){state.truncated=true;state.candidates.subList(RolesSettingsContract.MAX_ROWS,state.candidates.size()).clear();}
        return state;
    }
    synchronized JSONObject snapshot(long id,RoleId selected,int offset,String expected)throws Exception{
        RolesSettingsContract.page(id,selected,offset,expected);
        if(!owner()){retire();return RolesSettingsContract.unavailable(id,selected,"restricted");}
        JSONArray overview=new JSONArray();State state=null;
        for(RoleId role:RoleId.values()){State current=read(role,role==selected);overview.put(current.overview());if(role==selected)state=current;}
        if(!owner()){retire();return RolesSettingsContract.unavailable(id,selected,"restricted");}
        JSONObject packet=RolesSettingsContract.unavailable(id,selected,state!=null&&!state.available?"unsupported":"available").put("roles",overview);
        observedTargets.clear();observedRole=selected;
        if(state==null||!state.available){lease.retire();return packet;}
        boolean stale=expected!=null&&!expected.equals(state.generation)||offset>0&&offset>=state.candidates.size();if(stale)offset=0;
        JSONArray rows=new JSONArray();for(int i=offset;i<Math.min(state.candidates.size(),offset+RolesSettingsContract.PAGE_SIZE);i++){Candidate candidate=state.candidates.get(i);rows.put(candidate.json());if(candidate.allowed)observedTargets.add(candidate.key);}
        if(state.noneTarget!=null)observedTargets.add(state.noneTarget);
        return packet.put("key",lease.observe(state.fingerprint,SystemClock.elapsedRealtime())).put("generation",state.generation).put("offset",offset).put("total",state.candidates.size())
                .put("truncated",state.truncated).put("stale",stale).put("candidates",rows).put("none_target",state.noneTarget==null?JSONObject.NULL:state.noneTarget);
    }
    static final class Ticket {
        final String id,target,fingerprint;final RoleId role;final long expires;boolean claimed;
        Ticket(RoleId role,String target,String fingerprint,long now){id=UUID.randomUUID().toString();this.role=role;this.target=target;this.fingerprint=fingerprint;expires=now+RolesSettingsContract.TICKET_MS;}
    }
    synchronized PendingIntent confirmation(RoleId role,String key,String target)throws Exception{
        RolesSettingsContract.key(key);RolesSettingsContract.key(target);
        if(!owner()||observedRole!=role||!observedTargets.contains(target))return null;
        State state=read(role,true);if(!offered(state,target)||!owner()||!lease.claim(key,state.fingerprint,SystemClock.elapsedRealtime()))return null;
        long now=SystemClock.elapsedRealtime();tickets.values().removeIf(ticket->ticket.claimed||now>=ticket.expires);
        if(tickets.size()>=8)return null;Ticket ticket=new Ticket(role,target,state.fingerprint,now);tickets.put(ticket.id,ticket);
        Intent intent=new Intent(context,OctoSenseRoleConfirmationActivity.class).setIdentifier(ticket.id).putExtra("ticket",ticket.id)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK|Intent.FLAG_ACTIVITY_MULTIPLE_TASK|Intent.FLAG_ACTIVITY_EXCLUDE_FROM_RECENTS);
        return PendingIntent.getActivity(context,0,intent,PendingIntent.FLAG_IMMUTABLE|PendingIntent.FLAG_ONE_SHOT);
    }
    private boolean offered(State state,String target){if(state==null||!state.available||state.restricted)return false;Candidate candidate=state.candidate(target);return target.equals(state.noneTarget)||(candidate!=null&&candidate.allowed);}
    static final class Reviewed {
        final Role role;final RoleId id;final String packageName,label,identity;
        Reviewed(State state,Candidate candidate){role=state.role;id=state.id;packageName=candidate==null?null:candidate.pkg;label=candidate==null?"":candidate.label;identity=candidate==null?"":candidate.identity;}
    }
    synchronized Reviewed review(String id,boolean claim)throws Exception{
        Ticket ticket=tickets.get(id);if(ticket==null||ticket.claimed||SystemClock.elapsedRealtime()>=ticket.expires||!owner())return null;
        State state=read(ticket.role,true);if(!offered(state,ticket.target)||!ticket.fingerprint.equals(state.fingerprint)||!owner())return null;
        if(claim){ticket.claimed=true;tickets.remove(id);}return new Reviewed(state,state.candidate(ticket.target));
    }
    synchronized void cancel(String id){tickets.remove(id);}
    synchronized boolean isApplied(Reviewed reviewed)throws Exception{
        if(!owner())return false;
        State current=read(reviewed.id,false);
        return owner()&&current.available&&java.util.Objects.equals(reviewed.packageName,current.holderPackage)
                &&reviewed.identity.equals(current.holderIdentity);
    }
    private void retire(){lease.retire();observedTargets.clear();observedRole=null;tickets.clear();}
}
