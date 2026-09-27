package com.android.permissioncontroller.octosense;
import android.os.*;
import androidx.lifecycle.*;
import com.android.permissioncontroller.permission.ui.model.*;
import dev.makepad.octosense.permissions.PermissionsSettingsContract.Group;
import java.util.*;
public class RuntimePermissionModelTest{
 static void require(boolean value,String message){if(!value)throw new AssertionError(message);}
 public static void main(String[] args)throws Exception{
  Looper.IS_MAIN.set(true);NativePermissionModel source=new NativePermissionModel(new android.app.Application());
  List<NativePermissionModel.State> observations=new ArrayList<>();
  // Native non-safety-label rationale initializes to false before activation,
  // leaving aggregate isStale true. Its actual newly computed button map is valid.
  NativePermissionModel.Session session=source.observe("fixture.permissions",Group.CAMERA,observations::add);session.start();
  require(observations.size()==1&&observations.get(0).choices.size()==1,"new native button map must be delivered even with a static stale rationale source");
  require(observations.get(0).choices.get(0).enabled,"native policy-enabled choice remains enabled");session.close();require(TestLiveData.active==0,"completed observer leak");
  observations.clear();AppPermissionViewModel.initialNull=true;session=source.observe("fixture.permissions",Group.CAMERA,observations::add);session.start();
  require(observations.isEmpty()&&session.current()==null,"uninitialized button data must not invent choices");
  AppPermissionViewModel.last.getButtonStateLiveData().emit(AppPermissionViewModel.choices(),true);require(observations.size()==1,"newly emitted map should unblock detail read");
  AppPermissionGroupsViewModel.last.getPackagePermGroupsLiveData().stale=true;require(session.current()==null,"inventory freshness remains mandatory");session.close();require(TestLiveData.active==0,"detail observer leak");
  AppPermissionGroupsViewModel.initialStale=true;observations.clear();session=source.observe("fixture.permissions",Group.CAMERA,observations::add);session.start();require(observations.isEmpty(),"stale inventory must not create a detail model");session.close();
  boolean denied=false;try{source.read("fixture.permissions",Group.CAMERA);}catch(IllegalStateException expected){denied=true;}require(denied,"main-thread blocking forbidden");
  // Interrupt a worker after it installs observers but before native values arrive.
  java.util.concurrent.atomic.AtomicReference<Throwable> failure=new java.util.concurrent.atomic.AtomicReference<>();
  Thread worker=new Thread(()->{try{source.read("fixture.permissions",Group.CAMERA);failure.set(new AssertionError("interrupted read succeeded"));}catch(InterruptedException expected){}catch(Throwable wrong){failure.set(wrong);}});
  worker.start();long until=System.nanoTime()+1_000_000_000L;while(Handler.queue.isEmpty()&&System.nanoTime()<until)Thread.yield();Handler.drain();require(TestLiveData.active>0,"worker did not install observers");worker.interrupt();worker.join(1000);Handler.drain();require(!worker.isAlive()&&TestLiveData.active==0&&failure.get()==null,"interrupted worker leaked observers or failed: "+failure.get());
  Thread timeout=new Thread(()->{try{source.read("fixture.permissions",Group.CAMERA);failure.set(new AssertionError("uninitialized read succeeded"));}catch(IllegalStateException expected){if(!expected.getMessage().contains("timed out"))failure.set(expected);}catch(Throwable wrong){failure.set(wrong);}});
  timeout.start();until=System.nanoTime()+1_000_000_000L;while(Handler.queue.isEmpty()&&System.nanoTime()<until)Thread.yield();Handler.drain();require(TestLiveData.active>0,"timeout reader did not install observers");timeout.join(6500);Handler.drain();require(!timeout.isAlive()&&failure.get()==null&&TestLiveData.active==0,"timed-out read leaked model observers: "+failure.get());
 }
}
