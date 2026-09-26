package dev.makepad.octosense.keyboards;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;

public final class KeyboardPolicyTest {
    static int checks;
    static void check(boolean value){if(!value)throw new AssertionError("Keyboard policy check "+checks);checks++;}
    static void rejects(Runnable code){try{code.run();throw new AssertionError("Malformed operation accepted");}catch(IllegalArgumentException expected){checks++;}}
    static final class Platform implements KeyboardPolicy.Platform {
        String revision="state-A",availability="available",incarnation="installed-A";boolean enabled,allowed=true,throwing,directBoot;int count=25;
        public KeyboardPolicy.State read(){
            if(throwing)throw new IllegalStateException("Native read failed");
            List<KeyboardPolicy.Method> rows=new ArrayList<>();if(availability.equals("available")){
                rows.add(new KeyboardPolicy.Method("system/.Ime","system-A","Native keyboard","system","English","Last keyboard",true,true,true,true,false,false,false,true,true));
                for(int i=1;i<count;i++)rows.add(new KeyboardPolicy.Method("fixture"+i+"/.Ime",incarnation,"Fixture "+i,"fixture"+i,"English","",enabled,false,false,directBoot,false,!enabled&&allowed,enabled,true,true));
            }
            return new KeyboardPolicy.State(revision,availability,availability.equals("available")?"none":"policy","system/.Ime",availability.equals("available"),rows);
        }
    }
    public static void main(String[] args){
        KeyboardContract.read(1,"😀".repeat(80),0);checks++;
        rejects(()->KeyboardContract.read(1,"😀".repeat(81),0));rejects(()->KeyboardContract.read(0,"",0));rejects(()->KeyboardContract.read(1,"",19));
        check(KeyboardContract.flow(1,"a".repeat(64),"","choose_default")==KeyboardPolicy.Action.PICK_DEFAULT);
        rejects(()->KeyboardContract.flow(1,"a".repeat(64),"native/.Ime","enable"));rejects(()->KeyboardContract.flow(1,"a".repeat(64),"b".repeat(64),"choose_default"));
        rejects(()->KeyboardContract.flow(1,"a".repeat(64),"b".repeat(64),"set_secure_row"));
        long[] now={100};Platform platform=new Platform();KeyboardPolicy backend=new KeyboardPolicy(platform,()->now[0]);
        KeyboardPolicy.Page first=backend.snapshot("",0);check(first.total==25&&first.rows.size()==20);
        KeyboardPolicy.Page second=backend.snapshot("",20);check(second.key.equals(first.key)&&second.rows.size()==5);
        check(backend.prepare(first.key,first.rows.get(0).target,KeyboardPolicy.Action.DISABLE)==null);
        check(backend.prepare(first.key,"forged",KeyboardPolicy.Action.ENABLE)==null);
        KeyboardPolicy.Review review=backend.prepare(first.key,first.rows.get(1).target,KeyboardPolicy.Action.ENABLE);
        check(review.warnings.equals(Arrays.asList(KeyboardPolicy.Warning.SECURITY,KeyboardPolicy.Warning.DIRECT_BOOT)));
        check(backend.prepare(first.key,first.rows.get(2).target,KeyboardPolicy.Action.ENABLE)==null);
        check(backend.review(review.ticket)==review);backend.cancel(review.ticket);check(backend.claimAfterNativeConsent(review.ticket)==null&&!platform.enabled);
        first=backend.snapshot("",0);KeyboardPolicy.Review abandoned=backend.prepare(first.key,first.rows.get(1).target,KeyboardPolicy.Action.ENABLE);
        first=backend.snapshot("",0);review=backend.prepare(first.key,first.rows.get(2).target,KeyboardPolicy.Action.ENABLE);
        check(review!=null&&backend.claimAfterNativeConsent(abandoned.ticket)==null);backend.cancel(review.ticket);
        // Rotation can redisplay the same native review; only its positive save callback claims.
        first=backend.snapshot("",0);review=backend.prepare(first.key,first.rows.get(1).target,KeyboardPolicy.Action.ENABLE);
        check(backend.review(review.ticket)==review&&backend.review(review.ticket)==review);
        check(backend.checkedReview(review.ticket)==review);
        KeyboardPolicy.Claim claim=backend.claimAfterNativeConsent(review.ticket);check(claim!=null&&backend.claimAfterNativeConsent(review.ticket)==null);
        check(backend.observeCompletion(claim)==KeyboardPolicy.Result.UNCONFIRMED);platform.enabled=true;platform.revision="state-B";
        check(backend.observeCompletion(claim)==KeyboardPolicy.Result.APPLIED);
        first=backend.snapshot("",0);review=backend.prepare(first.key,first.rows.get(1).target,KeyboardPolicy.Action.DISABLE);check(review.warnings.isEmpty());
        platform.incarnation="replacement-B";check(backend.claimAfterNativeConsent(review.ticket)==null);platform.incarnation="installed-A";
        first=backend.snapshot("",0);review=backend.prepare(first.key,first.rows.get(1).target,KeyboardPolicy.Action.DISABLE);
        platform.incarnation="replacement-before-dialog";check(backend.checkedReview(review.ticket)==null);platform.incarnation="installed-A";
        first=backend.snapshot("",0);review=backend.prepare(first.key,first.rows.get(1).target,KeyboardPolicy.Action.DISABLE);
        platform.availability="restricted";check(backend.claimAfterNativeConsent(review.ticket)==null);platform.availability="available";
        first=backend.snapshot("",0);review=backend.prepare(first.key,first.rows.get(1).target,KeyboardPolicy.Action.DISABLE);platform.revision="raw-subtype-change";
        check(backend.claimAfterNativeConsent(review.ticket)==null);
        first=backend.snapshot("",0);review=backend.prepare(first.key,first.rows.get(1).target,KeyboardPolicy.Action.DISABLE);platform.directBoot=true;
        check(backend.claimAfterNativeConsent(review.ticket)==null);platform.directBoot=false;
        first=backend.snapshot("",0);review=backend.prepare(first.key,first.rows.get(1).target,KeyboardPolicy.Action.SETTINGS);backend.invalidate();check(backend.claimAfterNativeConsent(review.ticket)==null);
        first=backend.snapshot("",0);review=backend.prepare(first.key,first.rows.get(1).target,KeyboardPolicy.Action.SUBTYPES);now[0]+=KeyboardPolicy.REVIEW_MS+1;check(backend.claimAfterNativeConsent(review.ticket)==null);
        first=backend.snapshot("",0);now[0]+=KeyboardPolicy.OBSERVED_MS+1;check(backend.prepare(first.key,first.rows.get(1).target,KeyboardPolicy.Action.DISABLE)==null);
        first=backend.snapshot("",0);now[0]+=KeyboardPolicy.CATALOG_MS+1;check(!backend.snapshot("",0).key.equals(first.key));
        first=backend.snapshot("",0);review=backend.prepare(first.key,"",KeyboardPolicy.Action.PICK_DEFAULT);check(review!=null&&review.method==null&&review.warnings.isEmpty());claim=backend.claimAfterNativeConsent(review.ticket);check(claim!=null&&backend.observeCompletion(claim)==KeyboardPolicy.Result.OPENED);
        first=backend.snapshot("Fixture 24",20);check(first.offset==0&&first.rows.size()==1);
        platform.throwing=true;check(backend.snapshot("",0).availability.equals("unavailable"));platform.throwing=false;
        platform.count=129;check(backend.snapshot("",0).availability.equals("unavailable"));platform.count=25;
        KeyboardPolicy.Method systemUnaware=new KeyboardPolicy.Method("native/.Ime","A","Native","native","","",false,false,true,false,false,true,false,false,false);
        check(systemUnaware.warnings().equals(Arrays.asList(KeyboardPolicy.Warning.DIRECT_BOOT)));
        KeyboardPolicy.Method thirdPartyAware=new KeyboardPolicy.Method("third/.Ime","A","Third","third","","",false,false,false,true,false,true,false,false,false);
        check(thirdPartyAware.warnings().equals(Arrays.asList(KeyboardPolicy.Warning.SECURITY)));
        KeyboardPolicy.Method tvUnaware=new KeyboardPolicy.Method("tv/.Ime","A","TV","tv","","",false,false,false,false,true,true,false,false,false);
        check(tvUnaware.warnings().equals(Arrays.asList(KeyboardPolicy.Warning.SECURITY)));
        System.out.println("PASS keyboard authority "+checks);
    }
}
