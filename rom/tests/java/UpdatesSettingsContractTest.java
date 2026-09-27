import dev.makepad.octosense.updates.UpdatesSettingsContract;
import dev.makepad.octosense.updates.UpdatesSettingsContract.Review;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.atomic.AtomicInteger;

public final class UpdatesSettingsContractTest {
    static void check(boolean value) {if(!value) throw new AssertionError();}
    static void rejects(Runnable run) {try {run.run();throw new AssertionError("Invalid target accepted");}catch(IllegalArgumentException expected) {}}
    static Review review() {return new Review("manifest-a","https://updates.example/update.json","rom1",4,100,true,true);}
    public static void main(String[] args) throws Exception {
        for(String part:new String[]{null,"all","system","","ROM"}) rejects(()->UpdatesSettingsContract.part(part));
        Review a=review();
        check(a.available("rom","https://updates.example/update.json","rom1",4,100));
        check(!a.available("rom","https://other.example/update.json","rom1",4,101));
        check(!a.available("rom","https://updates.example/update.json","rom2",4,101));
        check(!a.available("rom","https://updates.example/update.json","rom1",5,101));
        check(!a.available("rom","https://updates.example/update.json","rom1",4,99));
        check(!a.available("rom","https://updates.example/update.json","rom1",4,100+UpdatesSettingsContract.REVIEW_TTL_MS+1));
        check(!a.key.equals(new Review("manifest-b","https://updates.example/update.json","rom1",4,100,true,true).key));
        check(!a.claim(UpdatesSettingsContract.fingerprint("forged"),"rom","https://updates.example/update.json","rom1",4,101));
        check(a.claim(a.key,"rom","https://updates.example/update.json","rom1",4,101));
        check(!a.claim(a.key,"home","https://updates.example/update.json","rom1",4,101));
        check(!new Review("m","s","r",1,0,false,true).available("rom","s","r",1,0));
        check(!UpdatesSettingsContract.fingerprint("ab","c").equals(UpdatesSettingsContract.fingerprint("a","bc")));
        rejects(()->UpdatesSettingsContract.key(a.key.toUpperCase()));
        // Binder calls can arrive concurrently. Exactly one may consume a review.
        Review concurrent=review();CountDownLatch start=new CountDownLatch(1);AtomicInteger accepted=new AtomicInteger();
        Thread[] threads=new Thread[12];
        for(int i=0;i<threads.length;i++) {threads[i]=new Thread(()->{try {start.await();
            if(concurrent.claim(concurrent.key,"home","https://updates.example/update.json","rom1",4,101)) accepted.incrementAndGet();
        }catch(InterruptedException e) {throw new AssertionError(e);}});threads[i].start();}
        start.countDown();for(Thread t:threads) t.join();check(accepted.get()==1);
    }
}
