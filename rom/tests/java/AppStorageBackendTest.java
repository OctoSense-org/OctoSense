import dev.makepad.octosense.appstorage.AppStorageBackend;
import dev.makepad.octosense.appstorage.AppStorageContract;
import dev.makepad.octosense.appstorage.AppStorageContract.Action;
import java.util.EnumSet;
import org.json.JSONObject;

public final class AppStorageBackendTest {
    static int checks;
    static void check(boolean value){checks++;if(!value)throw new AssertionError("Check "+checks);}
    static final class Platform implements AppStorageBackend.Platform {
        String incarnation="install-A",availability="available";boolean mutable=true,shared,accepted=true,throwing,reject,sync,manage;int requests;
        long data=300,cache=100;AppStorageBackend.Completion completion;
        public AppStorageBackend.State read(String pkg){AppStorageBackend.State s=new AppStorageBackend.State();s.packageName=pkg;s.label="Fixture";s.incarnation=incarnation;s.availability=availability;s.reason="none";s.sharedUid=shared;s.appBytes=100L;s.dataBytes=data;s.cacheBytes=cache;if(mutable&&!shared&&availability.equals("available")&&s.statsAvailable()){if(cache>0)s.actions.add(Action.CLEAR_CACHE);if(data>cache)s.actions.add(manage?Action.MANAGE_SPACE:Action.CLEAR_DATA);}s.revision=AppStorageContract.hash(incarnation,""+mutable,""+shared,""+manage,""+s.statsAvailable(),s.actions.toString());return s;}
        public AppStorageBackend.Started request(AppStorageBackend.State s,Action action,AppStorageBackend.Completion callback)throws Exception{requests++;if(reject)throw new AppStorageBackend.Rejected(AppStorageBackend.Failure.TARGET_CHANGED);if(throwing)throw new IllegalStateException("Uncertain Binder result");completion=callback;if(sync)callback.complete(s.packageName,true);return new AppStorageBackend.Started(accepted,action==Action.MANAGE_SPACE?new Object():null);}
    }
    static final String PKG="fixture.storage";
    static String key(AppStorageBackend b,long id)throws Exception{return b.snapshot(id,PKG).getString("key");}
    static String state(AppStorageBackend b)throws Exception{return b.snapshot(90,PKG).getJSONObject("operation").getString("state");}
    public static void main(String[] args)throws Exception{
        long[] now={100};Platform p=new Platform();AppStorageBackend b=new AppStorageBackend(p,()->now[0]);String key=key(b,1);check(key.equals(key(b,2)));
        check(b.action(PKG,key,"clear_cache").reason.equals("app_storage_requested"));check(p.requests==1);check(state(b).equals("pending"));check(b.snapshot(3,PKG).getJSONArray("actions").length()==0);
        check(b.action(PKG,key,"clear_cache").reason.equals("app_storage_target_changed"));check(p.requests==1);
        check(b.snapshot(31,"other.package").getString("reason").equals("operation_pending"));check(b.snapshot(32,"other.package").getJSONArray("actions").length()==0);p.completion.complete("wrong.package",true);check(state(b).equals("uncertain"));check(b.snapshot(33,PKG).getJSONArray("actions").length()==0);
        p.cache=0;p.data=200;p.completion.complete(PKG,true);check(state(b).equals("succeeded"));check(p.requests==1);check(b.snapshot(4,PKG).getJSONObject("stats").getInt("data_bytes")==200);
        check(b.action(PKG,key,"clear_data").reason.equals("app_storage_target_changed"));
        key=key(b,5);check(b.action(PKG,key,"clear_data").reason.equals("app_storage_requested"));check(p.requests==2);p.completion.complete(PKG,false);check(state(b).equals("failed"));
        key=key(b,6);p.incarnation="install-B";check(b.action(PKG,key,"clear_data").reason.equals("app_storage_target_changed"));check(p.requests==2);
        key=key(b,7);p.data++;check(key.equals(key(b,71)));
        key=key(b,8);now[0]+=20001;check(b.action(PKG,key,"clear_data").reason.equals("app_storage_target_changed"));
        key=key(b,9);p.mutable=false;check(b.action(PKG,key,"clear_data").reason.equals("app_storage_restricted"));check(p.requests==2);p.mutable=true;
        p.shared=true;check(b.snapshot(10,PKG).getJSONArray("actions").length()==0);p.shared=false;
        key=key(b,11);check(b.action("other.package",key,"clear_data").reason.equals("app_storage_target_changed"));
        check(b.action(PKG,"0".repeat(64),"clear_data").reason.equals("app_storage_target_changed"));check(p.requests==2);
        key=key(b,12);check(b.action(PKG,key,"clear_data").reason.equals("app_storage_requested"));now[0]+=60001;check(state(b).equals("uncertain"));check(b.snapshot(13,PKG).getJSONArray("actions").length()==0);
        key=key(b,14);check(b.action(PKG,key,"clear_data").reason.equals("app_storage_busy"));check(p.requests==3);p.completion.complete(PKG,true);check(state(b).equals("succeeded"));
        key=key(b,15);b.action(PKG,key,"clear_data");p.incarnation="install-C";p.completion.complete(PKG,true);check(b.snapshot(16,PKG).opt("operation")==null);
        p.manage=true;key=key(b,17);AppStorageBackend.Result flow=b.action(PKG,key,"manage_space");check(flow.reason.equals("app_storage_flow_opened")&&flow.nativeFlow!=null);int count=p.requests;
        check(b.action(PKG,key,"manage_space").reason.equals("app_storage_target_changed"));check(p.requests==count);
        p.manage=false;p.accepted=false;key=key(b,18);check(b.action(PKG,key,"clear_data").reason.equals("app_storage_unavailable"));check(state(b).equals("failed"));p.accepted=true;
        p.reject=true;key=key(b,19);check(b.action(PKG,key,"clear_data").reason.equals("app_storage_target_changed"));check(state(b).equals("failed"));p.reject=false;
        p.throwing=true;key=key(b,20);check(b.action(PKG,key,"clear_data").reason.equals("app_storage_unconfirmed"));check(state(b).equals("uncertain"));check(b.snapshot(21,PKG).getJSONArray("actions").length()==0);
        Platform second=new Platform();AppStorageBackend c=new AppStorageBackend(second,()->now[0]);second.sync=true;check(c.action(PKG,key(c,22),"clear_data").reason.equals("app_storage_requested"));check(state(c).equals("succeeded"));
        Platform growing=new Platform();AppStorageBackend growth=new AppStorageBackend(growing,()->now[0]);growing.sync=true;
        for(String action:new String[]{"clear_cache","clear_data"}){String review=key(growth,25);growing.data+=40;growing.cache+=20;check(review.equals(key(growth,26)));growing.data+=10;check(growth.action(PKG,review,action).reason.equals("app_storage_requested"));check(state(growth).equals("succeeded"));}
        int requested=growing.requests;String review=key(growth,27);growing.cache=0;check(growth.action(PKG,review,"clear_cache").reason.equals("app_storage_restricted"));check(growing.requests==requested);
        review=key(growth,28);growing.data=-1;check(growth.action(PKG,review,"clear_data").reason.equals("app_storage_restricted"));check(growing.requests==requested);
        for(String bad:new String[]{"delete_path","clear_all","","CLEAR_DATA"})try{Action.parse(bad);throw new AssertionError();}catch(IllegalArgumentException expected){checks++;}
        for(String bad:new String[]{"../app","no space allowed","com..app","app"})try{AppStorageContract.packageName(bad);throw new AssertionError();}catch(IllegalArgumentException expected){checks++;}
        AppStorageBackend unavailable=new AppStorageBackend(null,()->now[0]);check(unavailable.snapshot(23,PKG).getString("availability").equals("unavailable"));check(unavailable.snapshot(24,PKG).getJSONArray("actions").length()==0);
        System.out.println("PASS app storage: "+checks);
    }
}
