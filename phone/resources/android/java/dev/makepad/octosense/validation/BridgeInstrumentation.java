package dev.makepad.octosense.validation;

import android.app.Activity;
import android.app.Instrumentation;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.ServiceConnection;
import android.content.pm.LauncherApps;
import android.media.AudioManager;
import android.os.Bundle;
import android.os.IBinder;
import android.os.SystemClock;
import dev.makepad.octosense.LauncherPlacements;
import dev.makepad.octosense.WidgetPlacements;
import dev.makepad.octosense.contracts.ISystemBridge;
import dev.makepad.octosense.contracts.ISystemBridgeCallback;
import dev.makepad.octosense.contracts.Protocol;
import dev.makepad.octosense.contracts.HomeLayout;
import java.util.UUID;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;

/** Shell-launched protocol checks. Waits run on Instrumentation's worker,
 * never the activity or renderer. No notification content leaves the app. */
public final class BridgeInstrumentation extends Instrumentation {
    // This can be set only by this opt-in instrumentation runner. It prevents
    // automatic Home binding from replacing the protocol test subscription.
    public static volatile boolean homeTransportTest;
    public static volatile boolean notificationRoundtripTest;
    private final CountDownLatch connected=new CountDownLatch(1);
    private final ArrayBlockingQueue<Bundle> snapshots=new ArrayBlockingQueue<>(64);
    private final ArrayBlockingQueue<Bundle> results=new ArrayBlockingQueue<>(64);
    private final String session=UUID.randomUUID().toString();
    private volatile ISystemBridge bridge;
    private volatile boolean overflow;
    private boolean bound;
    private Bundle arguments;
    private long revision=-1;
    private String epoch="";
    private int checks;
    private final ServiceConnection connection=new ServiceConnection() {
        @Override public void onServiceConnected(ComponentName name,IBinder binder) {
            bridge=ISystemBridge.Stub.asInterface(binder); connected.countDown();
        }
        @Override public void onServiceDisconnected(ComponentName name) {bridge=null;}
    };
    private final ISystemBridgeCallback.Stub callback=new ISystemBridgeCallback.Stub() {
        @Override public void onSnapshot(String e,long r,Bundle state) {
            Bundle packet=new Bundle();packet.putString("epoch",e);packet.putLong("revision",r);packet.putBundle("state",state);
            overflow|=!snapshots.offer(packet);
        }
        @Override public void onDelta(String e,long r,Bundle state) {overflow=true;}
        @Override public void onResyncRequired(String e) {overflow=true;}
        @Override public void onCommandResult(String s,long id,int status,String reason) {
            if(!session.equals(s)) {overflow=true;return;}
            Bundle value=new Bundle();value.putLong("id",id);value.putInt("status",status);value.putString("reason",reason);
            overflow|=!results.offer(value);
        }
    };
    @Override public void onCreate(Bundle args) {
        arguments=args==null?new Bundle():args;
        notificationRoundtripTest="notification_roundtrip".equals(arguments.getString("mode"));
        homeTransportTest="home_transport".equals(arguments.getString("mode"))
                ||notificationRoundtripTest;start();
    }
    private void check(boolean value,String reason) {
        if(!value) throw new AssertionError(reason);checks++;
    }
    private Bundle snapshot() throws Exception {
        Bundle packet=snapshots.poll(10,TimeUnit.SECONDS);
        check(packet!=null,"snapshot_timeout");check(!overflow,"callback_overflow");
        String e=packet.getString("epoch");long r=packet.getLong("revision");
        check(e!=null&&!e.isEmpty(),"snapshot_epoch_missing");
        check(!e.equals(epoch)||r>=revision,"snapshot_revision_regressed");
        epoch=e;revision=r;return packet.getBundle("state");
    }
    private void completion(long id,int expected,boolean accepted) throws Exception {
        boolean sawAcceptance=false;
        long deadline=SystemClock.elapsedRealtime()+10_000;
        while(SystemClock.elapsedRealtime()<deadline) {
            Bundle result=results.poll(1,TimeUnit.SECONDS);
            if(result==null) continue;
            check(result.getLong("id")==id,"uncorrelated_command_result");
            int status=result.getInt("status");
            if(status==Protocol.ACCEPTED) {sawAcceptance=true;continue;}
            check(status==expected,"unexpected_status_"+status+"_"+result.getString("reason"));
            check(!accepted||sawAcceptance,"completion_without_acceptance");return;
        }
        throw new AssertionError("completion_timeout");
    }
    private boolean accessible(Bundle capabilities,String operation) {
        Bundle value=capabilities.getBundle(operation);return value!=null&&value.getBoolean("accessible");
    }
    private void widgets(Context context,Bundle report) throws Exception {
        // A separate validation host never touches IDs owned by Home's host.
        android.appwidget.AppWidgetHost host=new android.appwidget.AppWidgetHost(context,0x4f4355);
        java.io.File directory=new java.io.File(context.getCacheDir(),"widget-validation-"+session);
        if(!directory.mkdirs()) throw new AssertionError("widget_test_directory_failed");
        java.io.File path=new java.io.File(directory,"widgets.json");
        java.util.ArrayList<Integer> allocated=new java.util.ArrayList<>();
        int[] before=host.getAppWidgetIds();
        try {
            android.appwidget.AppWidgetManager manager=android.appwidget.AppWidgetManager.getInstance(context);
            java.util.List<android.appwidget.AppWidgetProviderInfo> providers=manager.getInstalledProviders();
            report.putInt("widget_providers",providers.size());
            check(!providers.isEmpty(),"no_widget_providers_on_target");
            android.appwidget.AppWidgetProviderInfo provider=providers.get(0);
            long serial=context.getSystemService(android.os.UserManager.class).getSerialNumberForUser(provider.getProfile());
            int id=host.allocateAppWidgetId();allocated.add(id);check(id>0,"widget_allocation_failed");
            check(java.util.Arrays.stream(host.getAppWidgetIds()).anyMatch(value -> value==id),"host_does_not_own_allocated_widget");
            WidgetPlacements journal=new WidgetPlacements(path);
            WidgetPlacements.State empty=journal.read();check(empty.entries.isEmpty()&&empty.pending==null,"widget_store_not_empty");
            WidgetPlacements.Entry entry=new WidgetPlacements.Entry(id,provider.provider.flattenToString(),serial,240,160);
            journal.write(new WidgetPlacements.State(empty.entries,entry,"bind"));
            WidgetPlacements.State pending=new WidgetPlacements(path).read();
            check(pending.pending.id==id&&pending.stage.equals("bind")&&pending.owns(id),"pending_binding_not_recoverable");
            check(pending.pending.user==serial&&pending.pending.provider.equals(entry.provider),"widget_profile_or_provider_lost");
            journal.write(new WidgetPlacements.State(empty.entries,entry,"configure"));
            check(new WidgetPlacements(path).read().stage.equals("configure"),"configuration_stage_not_recoverable");
            java.util.ArrayList<WidgetPlacements.Entry> entries=new java.util.ArrayList<>();entries.add(entry);
            journal.write(new WidgetPlacements.State(entries,null,""));
            WidgetPlacements.State placed=new WidgetPlacements(path).read();
            check(placed.entries.size()==1&&placed.pending==null&&placed.owns(id),"widget_placement_not_persisted");
            entries.set(0,new WidgetPlacements.Entry(id,entry.provider,serial,320,200));
            journal.write(new WidgetPlacements.State(entries,null,""));
            placed=new WidgetPlacements(path).read();
            check(placed.entries.get(0).width==320&&placed.entries.get(0).height==200,"widget_resize_not_persisted");
            try {new WidgetPlacements.State(entries,entry,"bind");throw new AssertionError("duplicate_widget_id_accepted");}
            catch(IllegalArgumentException expected) {checks++;}
            byte[] future=placed.json().put("version",2).toString().getBytes(java.nio.charset.StandardCharsets.UTF_8);
            try(java.io.FileOutputStream output=new java.io.FileOutputStream(path)) {output.write(future);}
            try {new WidgetPlacements(path).read();throw new AssertionError("future_widget_schema_accepted");}
            catch(java.io.IOException expected) {checks++;}
            check(java.util.Arrays.equals(future,new android.util.AtomicFile(path).readFully()),"future_widget_schema_overwritten");
            journal.write(empty);host.deleteAppWidgetId(id);allocated.remove(Integer.valueOf(id));
            check(!new WidgetPlacements(path).read().owns(id),"removed_widget_retained_in_journal");
            check(java.util.Arrays.stream(host.getAppWidgetIds()).noneMatch(value -> value==id),"removed_widget_id_leaked");
            report.putBoolean("widget_ids_and_recovery_journal_verified",true);
            report.putBoolean("widget_binding_and_rendering_tested",false);
        } finally {
            for(int id:allocated) host.deleteAppWidgetId(id);
            int[] after=host.getAppWidgetIds();java.util.Arrays.sort(before);java.util.Arrays.sort(after);
            check(java.util.Arrays.equals(before,after),"validation_widget_host_changed_after_cleanup");
            new android.util.AtomicFile(path).delete();new java.io.File(path.getPath()+".new").delete();directory.delete();
        }
    }
    private void widgetUi(Context context,Bundle report) throws Exception {
        // This host and journal belong only to an explicitly launched validation
        // activity. Binding uses a temporary shell permission, not a user grant.
        android.appwidget.AppWidgetHost host=new android.appwidget.AppWidgetHost(context,0x4f4356);
        java.io.File path=new java.io.File(context.getCacheDir(),"widget-ui-validation.json");
        java.io.File placementPath=new java.io.File(context.getCacheDir(),"home-geometry-placements.json");
        if(path.exists()) {
            WidgetPlacements.State interrupted=new WidgetPlacements(path).read();
            for(int id:host.getAppWidgetIds()) {
                check(interrupted.owns(id),"unknown_widget_ui_allocation");host.deleteAppWidgetId(id);
            }
            new android.util.AtomicFile(path).delete();new java.io.File(path.getPath()+".new").delete();
            report.putBoolean("interrupted_fixture_recovered",true);
        }
        int[] before=host.getAppWidgetIds();
        check(before.length==0,"previous_widget_ui_ids_present");
        int allocated=0;Activity owned=null;
        try {
            android.appwidget.AppWidgetManager manager=android.appwidget.AppWidgetManager.getInstance(context);
            android.appwidget.AppWidgetProviderInfo selected=null;
            for(android.appwidget.AppWidgetProviderInfo provider:manager.getInstalledProviders()) {
                // Clock content has no account data; no configuration or other
                // application's UI is launched by this test.
                if(provider.provider.getPackageName().equals("com.android.deskclock")&&provider.configure==null
                        &&(provider.widgetCategory&android.appwidget.AppWidgetProviderInfo.WIDGET_CATEGORY_HOME_SCREEN)!=0) {
                    selected=provider;break;
                }
            }
            check(selected!=null,"unconfigured_native_clock_provider_unavailable");
            allocated=host.allocateAppWidgetId();check(allocated>0,"widget_ui_allocation_failed");
            android.app.UiAutomation automation=getUiAutomation(android.app.UiAutomation.FLAG_DONT_SUPPRESS_ACCESSIBILITY_SERVICES);
            try {
                automation.adoptShellPermissionIdentity("android.permission.BIND_APPWIDGET");
                check(manager.bindAppWidgetIdIfAllowed(allocated,selected.getProfile(),selected.provider,null),"temporary_widget_bind_failed");
            } finally {automation.dropShellPermissionIdentity();}
            check(manager.getAppWidgetInfo(allocated)!=null,"bound_widget_info_missing");
            long user=context.getSystemService(android.os.UserManager.class).getSerialNumberForUser(selected.getProfile());
            java.util.ArrayList<WidgetPlacements.Entry> entries=new java.util.ArrayList<>();
            float density=context.getResources().getDisplayMetrics().density;
            entries.add(new WidgetPlacements.Entry(allocated,selected.provider.flattenToString(),user,
                    Math.max(300,Math.round(selected.minWidth/density)+32),Math.max(180,Math.round(selected.minHeight/density)+32)));
            new WidgetPlacements(path).write(new WidgetPlacements.State(entries,null,""));
            java.util.List<android.content.pm.LauncherActivityInfo> clocks=context.getSystemService(LauncherApps.class)
                    .getActivityList("com.android.deskclock",android.os.Process.myUserHandle());
            check(!clocks.isEmpty(),"native_clock_launchable_missing");
            new android.util.AtomicFile(placementPath).delete();
            new LauncherPlacements(placementPath).favorite("android:"+user+":"+clocks.get(0).getComponentName().flattenToString(),true);
            Intent launch=new Intent(Intent.ACTION_MAIN).setComponent(new ComponentName(context.getPackageName(),context.getPackageName()+".MakepadApp"))
                    .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK).putExtra("--remote",true).putExtra("octosense.widget_test",true).putExtra("octosense.placement_test",true);
            context.startActivity(launch);
            long launchDeadline=SystemClock.elapsedRealtime()+15_000;
            while(ValidationRemote.ownedActivity()==null&&SystemClock.elapsedRealtime()<launchDeadline) SystemClock.sleep(50);
            owned=ValidationRemote.ownedActivity();check(owned!=null,"owned_validation_activity_missing");
            report.putInt("widget_id",allocated);report.putString("provider",selected.provider.flattenToString());
            report.putString("binding_authority","temporary_UiAutomation_BIND_APPWIDGET_dropped_before_activity_launch");
            long deadline=SystemClock.elapsedRealtime()+180_000;
            while(!owned.isFinishing()&&!owned.isDestroyed()&&SystemClock.elapsedRealtime()<deadline) SystemClock.sleep(100);
            check(owned.isFinishing()||owned.isDestroyed(),"owned_remote_not_closed_before_deadline");
            report.putBoolean("real_provider_bound_and_released",true);
            report.putBoolean("consent_ui_tested",false);
        } finally {
            if(owned!=null&&!owned.isFinishing()&&!owned.isDestroyed()) {Activity activity=owned;runOnMainSync(activity::finishAndRemoveTask);}
            if(allocated>0) host.deleteAppWidgetId(allocated);
            new android.util.AtomicFile(path).delete();new java.io.File(path.getPath()+".new").delete();
            new android.util.AtomicFile(placementPath).delete();new java.io.File(placementPath.getPath()+".new").delete();
            int[] after=host.getAppWidgetIds();java.util.Arrays.sort(before);java.util.Arrays.sort(after);
            check(java.util.Arrays.equals(before,after),"widget_ui_host_changed_after_cleanup");
        }
    }
    private void cleanupShortcuts(Context context,Bundle report,String owner) {
        if(owner==null||!owner.matches("[a-f0-9-]{36}")) throw new IllegalArgumentException("Invalid shortcut fixture owner");
        LauncherApps launcher=context.getSystemService(LauncherApps.class);
        LauncherApps.ShortcutQuery query=new LauncherApps.ShortcutQuery().setPackage(Protocol.BRIDGE_PACKAGE)
                .setQueryFlags(LauncherApps.ShortcutQuery.FLAG_MATCH_PINNED);
        java.util.List<android.content.pm.ShortcutInfo> current=launcher.getShortcuts(query,android.os.Process.myUserHandle());
        java.util.ArrayList<String> retained=new java.util.ArrayList<>();
        String prefix="octosense-validation-pin-"+owner+"-";
        if(current!=null) for(android.content.pm.ShortcutInfo shortcut:current)
            if(!shortcut.getId().startsWith(prefix)) retained.add(shortcut.getId());
        launcher.pinShortcuts(Protocol.BRIDGE_PACKAGE,retained,android.os.Process.myUserHandle());
        current=launcher.getShortcuts(query,android.os.Process.myUserHandle());
        java.util.ArrayList<String> actual=new java.util.ArrayList<>();
        if(current!=null) for(android.content.pm.ShortcutInfo shortcut:current) actual.add(shortcut.getId());
        java.util.Collections.sort(retained);java.util.Collections.sort(actual);
        check(retained.equals(actual),"shortcut_cleanup_changed_unowned_pins");
        report.putBoolean("owned_shortcuts_unpinned",true);
    }
    private void launcherUi(Context context,Bundle report,boolean recovering) throws Exception {
        java.io.File path=new java.io.File(context.getCacheDir(),"home-geometry-placements.json");
        android.content.ComponentName component=new ComponentName(Protocol.BRIDGE_PACKAGE,Protocol.BRIDGE_PACKAGE+".validation.LauncherFixtureActivity");
        long user=context.getSystemService(android.os.UserManager.class).getSerialNumberForUser(android.os.Process.myUserHandle());
        String identity="android:"+user+":"+component.flattenToString();
        dev.makepad.android.MakepadActivity owned=null;
        LauncherUiFixture.start();
        LauncherUiFixture.allowShortcutCleanup(arguments.getString("shortcut_owner"));
        try {
            check(context.getSystemService(LauncherApps.class).isActivityEnabled(component,android.os.Process.myUserHandle()),"launcher_fixture_activity_missing");
            if(recovering) {
                check(path.exists(),"launcher_recovery_journal_missing");
                LauncherPlacements saved=new LauncherPlacements(path);
                check(saved.isFavorite(identity)&&saved.isDocked(identity),"launcher_recovery_placements_missing");
            } else {
                new android.util.AtomicFile(path).delete();new LauncherPlacements(path).favorite(identity,true);
            }
            context.startActivity(new Intent(Intent.ACTION_MAIN).setComponent(new ComponentName(context.getPackageName(),context.getPackageName()+".MakepadApp"))
                    .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK).putExtra("--remote",true).putExtra("octosense.placement_test",true));
            long deadline=SystemClock.elapsedRealtime()+180_000;
            while(!LauncherUiFixture.finished()&&SystemClock.elapsedRealtime()<deadline) {
                dev.makepad.android.MakepadActivity current=ValidationRemote.ownedActivity();if(current!=null) owned=current;
                String cleanupOwner=LauncherUiFixture.takeShortcutCleanup();
                if(cleanupOwner!=null) {cleanupShortcuts(context,report,cleanupOwner);report.putBoolean("live_shortcut_cleanup",true);}
                SystemClock.sleep(100);
            }
            check(LauncherUiFixture.finished(),"launcher_fixture_not_closed_before_deadline");
            check(owned!=null,"launcher_owned_activity_missing");
            report.putString("fixture_identity",identity);report.putBoolean("production_placements_modified",false);
            report.putBoolean("recovered_existing_journal",recovering);
        } finally {
            if(owned!=null&&!owned.isFinishing()&&!owned.isDestroyed()) {Activity activity=owned;runOnMainSync(activity::finishAndRemoveTask);}
            new android.util.AtomicFile(path).delete();new java.io.File(path.getPath()+".new").delete();LauncherUiFixture.detach();
            check(!path.exists(),"launcher_fixture_journal_not_removed");
        }
    }
    private void replyUi(Context context,Bundle report) throws Exception {
        dev.makepad.android.MakepadActivity owned=null;
        try {
            context.startActivity(new Intent(Intent.ACTION_MAIN).setComponent(new ComponentName(context.getPackageName(),context.getPackageName()+".MakepadApp"))
                    .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK).putExtra("--remote",true));
            long deadline=SystemClock.elapsedRealtime()+15_000;
            while(ValidationRemote.ownedActivity()==null&&SystemClock.elapsedRealtime()<deadline) SystemClock.sleep(50);
            owned=ValidationRemote.ownedActivity();check(owned!=null,"owned_reply_activity_missing");
            dev.makepad.android.MakepadActivity activity=owned;runOnMainSync(() -> ReplyUiFixture.attach(activity));
            deadline=SystemClock.elapsedRealtime()+180_000;
            while(!owned.isFinishing()&&!owned.isDestroyed()&&SystemClock.elapsedRealtime()<deadline) SystemClock.sleep(100);
            check(owned.isFinishing()||owned.isDestroyed(),"reply_remote_not_closed_before_deadline");
            report.putBoolean("notification_access_granted",false);report.putBoolean("external_message_sent",false);
        } finally {
            runOnMainSync(ReplyUiFixture::detach);
            if(owned!=null&&!owned.isFinishing()&&!owned.isDestroyed()) {Activity activity=owned;runOnMainSync(activity::finishAndRemoveTask);}
        }
    }
    private void notificationInput(Context context,Bundle report) throws Exception {
        android.app.RemoteInput free=new android.app.RemoteInput.Builder("message").setLabel("Reply").build();
        android.app.RemoteInput choice=new android.app.RemoteInput.Builder("choice").setAllowFreeFormInput(false).setChoices(new String[]{"yes","no"}).build();
        android.app.RemoteInput[] inputs={free,choice};
        ArrayBlockingQueue<Intent> received=new ArrayBlockingQueue<>(8);
        android.content.BroadcastReceiver receiver=new android.content.BroadcastReceiver() {
            public void onReceive(Context c,Intent intent) {received.offer(intent);}
        };
        String action=context.getPackageName()+".VALIDATE_REPLY."+session;
        if(android.os.Build.VERSION.SDK_INT>=33) context.registerReceiver(receiver,new android.content.IntentFilter(action),Context.RECEIVER_NOT_EXPORTED);
        else context.registerReceiver(receiver,new android.content.IntentFilter(action));
        android.app.PendingIntent target=android.app.PendingIntent.getBroadcast(context,0,new Intent(action).setPackage(context.getPackageName()),android.app.PendingIntent.FLAG_MUTABLE|android.app.PendingIntent.FLAG_CANCEL_CURRENT);
        android.app.PendingIntent immutable=android.app.PendingIntent.getBroadcast(context,1,new Intent(action).setPackage(context.getPackageName()),android.app.PendingIntent.FLAG_IMMUTABLE|android.app.PendingIntent.FLAG_CANCEL_CURRENT);
        try {
            check(dev.makepad.octosense.contracts.NotificationActionSender.acceptsReply(inputs),"freeform_not_detected");
            check(!dev.makepad.octosense.contracts.NotificationActionSender.acceptsReply(new android.app.RemoteInput[]{choice}),"choice_action_marked_freeform");
            String text="Reply validation: 你好 👋\nsecond line";
            dev.makepad.octosense.contracts.NotificationActionSender.send(context,target,inputs,text);
            Intent result=received.poll(5,TimeUnit.SECONDS);check(result!=null,"native_reply_not_delivered");
            Bundle data=android.app.RemoteInput.getResultsFromIntent(result);
            check(data!=null&&text.contentEquals(data.getCharSequence("message")),"unicode_reply_changed");
            check(!data.containsKey("choice"),"freeform_overwrote_choice_only_input");
            if(android.os.Build.VERSION.SDK_INT>=28) check(android.app.RemoteInput.getResultsSource(result)==android.app.RemoteInput.SOURCE_FREE_FORM_INPUT,"reply_source_missing");
            for(String invalid:new String[]{null," \n",new String(new char[2001]).replace('\0','x')}) {
                try {dev.makepad.octosense.contracts.NotificationActionSender.send(context,target,inputs,invalid);throw new AssertionError("invalid_reply_dispatched");}
                catch(IllegalArgumentException expected) {checks++;}
            }
            try {dev.makepad.octosense.contracts.NotificationActionSender.send(context,target,new android.app.RemoteInput[]{choice},"text");throw new AssertionError("choice_only_reply_dispatched");}
            catch(IllegalArgumentException expected) {checks++;}
            if(android.os.Build.VERSION.SDK_INT>=31) {
                try {dev.makepad.octosense.contracts.NotificationActionSender.send(context,immutable,inputs,"text");throw new AssertionError("immutable_reply_dispatched");}
                catch(IllegalArgumentException expected) {checks++;}
            }
            check(received.poll(250,TimeUnit.MILLISECONDS)==null,"invalid_input_had_side_effect");
            dev.makepad.octosense.contracts.NotificationActionSender.send(context,target,null,null);
            result=received.poll(5,TimeUnit.SECONDS);check(result!=null&&android.app.RemoteInput.getResultsFromIntent(result)==null,"ordinary_action_changed");
            target.cancel();
            try {dev.makepad.octosense.contracts.NotificationActionSender.send(context,target,inputs,"text");throw new AssertionError("cancelled_action_dispatched");}
            catch(android.app.PendingIntent.CanceledException expected) {checks++;}
            report.putBoolean("native_remote_input_and_pending_intent_verified",true);
            report.putBoolean("notification_listener_lifecycle_tested",false);
        } finally {target.cancel();immutable.cancel();context.unregisterReceiver(receiver);}
    }
    private Bundle sampleHomeLayout() {
        Bundle value=new Bundle();value.putInt("version",1);value.putString("epoch",session);value.putInt("display_id",0);value.putInt("rotation",0);
        value.putIntArray("viewport",new int[]{0,0,1080,2280});value.putIntArray("insets",new int[]{0,80,0,80});
        Bundle icon=new Bundle();icon.putString("component","com.android.deskclock/.DeskClock");icon.putLong("user",0);icon.putFloatArray("bounds",new float[]{100,200,240,340});
        java.util.ArrayList<Bundle> icons=new java.util.ArrayList<>();icons.add(icon);value.putParcelableArrayList("icons",icons);return value;
    }
    private void rejectsHomeLayout(Bundle value,String reason) {
        try {HomeLayout.decode(1,value);throw new AssertionError(reason);}
        catch(IllegalArgumentException|ClassCastException expected) {checks++;}
    }
    private void homeLayout(Bundle report) {
        Bundle value=sampleHomeLayout();HomeLayout model=HomeLayout.decode(7,value);
        check(model.revision==7&&model.iconCount()==1&&model.displayId==0&&model.rotation==0,"valid_home_geometry_rejected");
        check(model.transitionId==0,"legacy_layout_assigned_transition");
        ComponentName clock=new ComponentName("com.android.deskclock","com.android.deskclock.DeskClock");
        float[] target=model.boundsFor(clock,0);
        check(target!=null&&target[0]==100&&target[3]==340,"matching_target_not_found");
        target[0]=-1;
        check(model.boundsFor(clock,0)[0]==100,"selected_target_can_mutate_snapshot");
        check(model.boundsFor(clock,10)==null,"target_selected_from_wrong_profile");
        check(model.boundsFor(new ComponentName("com.android.deskclock","com.android.deskclock.Other"),0)==null,"target_selected_from_wrong_component");
        value.getIntArray("viewport")[2]=1;
        check(model.bundle().getIntArray("viewport")[2]==1080,"input_can_mutate_cached_viewport");
        Bundle output=model.bundle();output.getIntArray("viewport")[2]=2;
        check(model.bundle().getIntArray("viewport")[2]==1080,"output_can_mutate_cached_viewport");
        java.util.ArrayList<Bundle> icons=output.getParcelableArrayList("icons");icons.get(0).getFloatArray("bounds")[0]=-500;
        icons=model.bundle().getParcelableArrayList("icons");check(icons.get(0).getFloatArray("bounds")[0]==100,"output_can_mutate_cached_icon");
        value=sampleHomeLayout();value.putInt("version",2);rejectsHomeLayout(value,"unknown_geometry_version_accepted");
        value=sampleHomeLayout();value.putLong("transition_id",123);
        HomeLayout transition=HomeLayout.decode(8,value);
        check(transition.transitionId==123&&transition.bundle().getLong("transition_id")==123,"transition_identity_not_retained");
        value.putLong("transition_id",124);check(transition.transitionId==123,"input_can_mutate_transition_identity");
        value=sampleHomeLayout();value.putLong("transition_id",-1);rejectsHomeLayout(value,"negative_transition_identity_accepted");
        value=sampleHomeLayout();value.putString("transition_id","123");rejectsHomeLayout(value,"wrong_transition_identity_type_accepted");
        value=sampleHomeLayout();value.putInt("rotation",4);rejectsHomeLayout(value,"invalid_rotation_accepted");
        value=sampleHomeLayout();value.putIntArray("insets",new int[]{1080,0,1,0});rejectsHomeLayout(value,"covered_viewport_accepted");
        value=sampleHomeLayout();value.putIntArray("viewport",new int[]{0,0,0,2280});rejectsHomeLayout(value,"empty_viewport_accepted");
        value=sampleHomeLayout();icons=value.getParcelableArrayList("icons");icons.get(0).putFloatArray("bounds",new float[]{Float.NaN,10,20,30});rejectsHomeLayout(value,"nan_geometry_accepted");
        value=sampleHomeLayout();icons=value.getParcelableArrayList("icons");icons.get(0).putFloatArray("bounds",new float[]{10,10,1081,30});rejectsHomeLayout(value,"offscreen_icon_accepted");
        value=sampleHomeLayout();icons=value.getParcelableArrayList("icons");icons.get(0).putFloatArray("bounds",new float[]{10,10,5,30});rejectsHomeLayout(value,"reversed_icon_accepted");
        value=sampleHomeLayout();icons=value.getParcelableArrayList("icons");icons.add(new Bundle(icons.get(0)));rejectsHomeLayout(value,"duplicate_profile_target_accepted");
        value=sampleHomeLayout();icons=value.getParcelableArrayList("icons");Bundle other=new Bundle(icons.get(0));other.putLong("user",10);icons.add(other);
        check(HomeLayout.decode(1,value).iconCount()==2,"distinct_profile_targets_collapsed");
        value=sampleHomeLayout();icons=value.getParcelableArrayList("icons");icons.get(0).putLong("user",-1);rejectsHomeLayout(value,"invalid_user_serial_accepted");
        value=sampleHomeLayout();icons=value.getParcelableArrayList("icons");icons.get(0).putString("component","not-a-component");rejectsHomeLayout(value,"invalid_component_accepted");
        value=sampleHomeLayout();icons=value.getParcelableArrayList("icons");while(icons.size()<=HomeLayout.MAX_ICONS) icons.add(new Bundle(icons.get(0)));rejectsHomeLayout(value,"oversize_layout_accepted");
        try {HomeLayout.decode(0,sampleHomeLayout());throw new AssertionError("zero_revision_accepted");} catch(IllegalArgumentException expected) {checks++;}
        Bundle event=new Bundle();event.putInt("version",1);event.putString("session",session);event.putString("epoch",session);
        event.putLong("event_revision",1);event.putLong("user",0);event.putInt("phase",0);event.putInt("display_id",0);
        event.putInt("rotation",0);event.putFloat("progress",0);event.putString("component","com.android.deskclock/.DeskClock");
        dev.makepad.octosense.contracts.HomeTransitionEvent start=dev.makepad.octosense.contracts.HomeTransitionEvent.decode(10,-1,event);
        check(start.id==10&&start.layoutRevision== -1&&start.component.equals(clock),"transition_start_identity_changed");
        event.putFloat("progress",0.5f);event.putInt("phase",1);event.putLong("event_revision",2);
        check(dev.makepad.octosense.contracts.HomeTransitionEvent.decode(10,8,event).progress==0.5f,"transition_progress_rejected");
        check(start.progress==0&&start.eventRevision==1,"transition_event_input_mutated_snapshot");
        for(String key:new String[]{"phase","display_id","rotation"}) {
            Bundle invalid=new Bundle(event);invalid.putInt(key,-1);rejectsTransition(invalid,"invalid_transition_"+key+"_accepted");
        }
        Bundle invalid=new Bundle(event);invalid.putFloat("progress",Float.NaN);rejectsTransition(invalid,"nan_transition_progress_accepted");
        invalid=new Bundle(event);invalid.putFloat("progress",1.1f);rejectsTransition(invalid,"outside_transition_progress_accepted");
        invalid=new Bundle(event);invalid.putInt("phase",0);rejectsTransition(invalid,"nonzero_start_progress_accepted");
        invalid=new Bundle(event);invalid.putInt("phase",2);rejectsTransition(invalid,"unfinished_terminal_progress_accepted");
        invalid=new Bundle(event);invalid.putLong("event_revision",0);rejectsTransition(invalid,"unordered_transition_accepted");
        invalid=new Bundle(event);invalid.putString("component","invalid");rejectsTransition(invalid,"invalid_transition_component_accepted");
        invalid=new Bundle(event);invalid.putLong("user",-1);rejectsTransition(invalid,"invalid_transition_profile_accepted");
        event.putInt("phase",2);event.putFloat("progress",1);
        check(dev.makepad.octosense.contracts.HomeTransitionEvent.decode(10,8,event).phase==2,"transition_finish_rejected");
        event.putInt("phase",3);event.putFloat("progress",0.4f);
        check(dev.makepad.octosense.contracts.HomeTransitionEvent.decode(10,8,event).phase==3,"transition_cancel_rejected");
        report.putBoolean("bounded_profile_geometry_and_defensive_copy_verified",true);
    }
    private void rejectsTransition(Bundle event,String reason) {
        try {dev.makepad.octosense.contracts.HomeTransitionEvent.decode(10,8,event);throw new AssertionError(reason);}
        catch(IllegalArgumentException|ClassCastException expected) {checks++;}
    }
    private void homeTransport(Context context,Bundle report) throws Exception {
        CountDownLatch attached=new CountDownLatch(1);
        dev.makepad.octosense.contracts.IHomeIntegration[] endpoint=new dev.makepad.octosense.contracts.IHomeIntegration[1];
        ArrayBlockingQueue<Bundle> states=new ArrayBlockingQueue<>(32);
        java.util.concurrent.atomic.AtomicBoolean dropped=new java.util.concurrent.atomic.AtomicBoolean();
        ServiceConnection connection=new ServiceConnection() {
            public void onServiceConnected(ComponentName name,IBinder binder) {endpoint[0]=dev.makepad.octosense.contracts.IHomeIntegration.Stub.asInterface(binder);attached.countDown();}
            public void onServiceDisconnected(ComponentName name) {endpoint[0]=null;}
        };
        boolean bound=context.bindService(new Intent().setComponent(new ComponentName(Protocol.QUICKSTEP_PACKAGE,
                Protocol.QUICKSTEP_PACKAGE+".HomeIntegrationService")),connection,Context.BIND_AUTO_CREATE);
        try {
            check(bound&&attached.await(10,TimeUnit.SECONDS),"home_transport_bind_failed");
            dev.makepad.octosense.contracts.IHomeIntegration service=endpoint[0];
            Bundle info=service.getProtocolInfo();check(info.getInt("major")==Protocol.MAJOR,"home_transport_protocol_mismatch");
            check(!info.getBoolean("controllers_implemented")&&!info.getBoolean("transitions_validated"),"unimplemented_transitions_advertised");
            dev.makepad.octosense.contracts.IHomeIntegrationCallback callback=new dev.makepad.octosense.contracts.IHomeIntegrationCallback.Stub() {
                public void onTransition(long id,long revision,Bundle event) {dropped.set(true);}
                public void onExtensionState(Bundle state) {if(!states.offer(state)) dropped.set(true);}
            };
            long[] observed={-1};String e=info.getString("epoch");
            service.subscribe(session,callback);homeState(states,observed,e,"subscribed",false,-1);
            service.publishHomeLayout(session,1,sampleHomeLayout());homeState(states,observed,e,"layout_cached",false,1);
            service.setHomeReady(session,1,true);homeState(states,observed,e,"home_ready",true,1);
            service.publishHomeLayout(session,1,sampleHomeLayout());homeState(states,observed,e,"stale_layout_revision",true,1);
            service.setHomeReady(session,2,true);homeState(states,observed,e,"readiness_revision_mismatch",false,1);
            Bundle invalid=sampleHomeLayout();invalid.putInt("rotation",6);
            service.publishHomeLayout(session,2,invalid);homeState(states,observed,e,"invalid_layout",false,1);
            service.setHomeReady(session,1,true);homeState(states,observed,e,"readiness_revision_mismatch",false,1);
            service.publishHomeLayout(session,3,sampleHomeLayout());homeState(states,observed,e,"layout_cached",false,3);
            service.setHomeReady(session,3,true);homeState(states,observed,e,"home_ready",true,3);
            service.setHomeReady(session,3,false);homeState(states,observed,e,"home_not_ready",false,3);
            service.setHomeReady(session,3,true);homeState(states,observed,e,"readiness_revision_mismatch",false,3);
            Bundle replacement=sampleHomeLayout();replacement.putString("epoch",UUID.randomUUID().toString());
            service.publishHomeLayout(session,4,replacement);homeState(states,observed,e,"layout_epoch_changed_resubscribe",false,3);
            service.subscribe(session,callback);homeState(states,observed,e,"subscribed",false,-1);
            service.publishHomeLayout(session,1,replacement);homeState(states,observed,e,"layout_cached",false,1);
            service.setHomeReady(session,1,true);homeState(states,observed,e,"home_ready",true,1);
            ComponentName alias=new ComponentName(context.getPackageName(),"dev.makepad.octosense.validation.PackageChangeAlias");
            android.content.pm.PackageManager packages=context.getPackageManager();
            int original=packages.getComponentEnabledSetting(alias);
            check(original==android.content.pm.PackageManager.COMPONENT_ENABLED_STATE_DEFAULT,"package_fixture_override_exists");
            try {
                packages.setComponentEnabledSetting(alias,android.content.pm.PackageManager.COMPONENT_ENABLED_STATE_ENABLED,
                        android.content.pm.PackageManager.DONT_KILL_APP);
                homeState(states,observed,e,"layout_invalidated",false,1);
                service.setHomeReady(session,1,true);homeState(states,observed,e,"readiness_revision_mismatch",false,1);
                service.publishHomeLayout(session,2,replacement);homeState(states,observed,e,"layout_cached",false,2);
                service.setHomeReady(session,2,true);homeState(states,observed,e,"home_ready",true,2);
            } finally {
                packages.setComponentEnabledSetting(alias,original,android.content.pm.PackageManager.DONT_KILL_APP);
            }
            homeState(states,observed,e,"layout_invalidated",false,2);
            service.setHomeReady(session,2,true);homeState(states,observed,e,"readiness_revision_mismatch",false,2);
            service.publishHomeLayout(session,3,replacement);homeState(states,observed,e,"layout_cached",false,3);
            service.setHomeReady(session,3,true);homeState(states,observed,e,"home_ready",true,3);
            check(packages.getComponentEnabledSetting(alias)==original,"package_fixture_not_restored");
            service.setHomeReady(UUID.randomUUID().toString(),3,false);homeState(states,observed,e,"layout_invalidated",false,3);
            service.setHomeReady(session,3,true);homeState(states,observed,e,"readiness_revision_mismatch",false,3);
            service.publishHomeLayout(session,4,replacement);homeState(states,observed,e,"layout_cached",false,4);
            service.setHomeReady(session,4,true);homeState(states,observed,e,"home_ready",true,4);
            check(!dropped.get(),"callback_overflow_or_unimplemented_transition_event");
            service.unsubscribe(session);
            report.putBoolean("native_home_uid_layout_ordering_and_invalidation_verified",true);
        } finally {if(bound) context.unbindService(connection);}
    }
    private void homeState(ArrayBlockingQueue<Bundle> states,long[] observed,String epoch,String reason,boolean ready,long revision) throws Exception {
        Bundle value=states.poll(10,TimeUnit.SECONDS);check(value!=null,"missing_home_state_"+reason);
        check(session.equals(value.getString("session"))&&epoch.equals(value.getString("epoch")),"uncorrelated_home_state");
        check(value.getLong("event_revision")>observed[0],"home_callback_revision_regressed");observed[0]=value.getLong("event_revision");
        check(reason.equals(value.getString("reason")),"unexpected_home_state_"+value.getString("reason"));
        check(value.getBoolean("home_ready")==ready&&value.getLong("layout_revision")==revision,"stale_home_ready_or_revision");
    }
    private void notificationIdentity(Context context,Bundle report) throws Exception {
        java.io.File directory=new java.io.File(context.getCacheDir(),"notification-identity-validation-"+session);
        dev.makepad.octosense.NotificationAppIdentity identity=new dev.makepad.octosense.NotificationAppIdentity(context,directory);
        try {
            Bundle state=new Bundle(),entry=new Bundle();
            entry.putString("package",Protocol.BRIDGE_PACKAGE);entry.putString("title","Fixture title");
            entry.putString("app_label","Untrusted label");entry.putString("app_icon","/untrusted/fixture.png");
            java.util.ArrayList<Bundle> entries=new java.util.ArrayList<>();entries.add(entry);state.putParcelableArrayList("notifications",entries);
            Bundle first=identity.decorate(state).<Bundle>getParcelableArrayList("notifications").get(0);
            String expected=context.getPackageManager().getApplicationLabel(context.getPackageManager().getApplicationInfo(Protocol.BRIDGE_PACKAGE,0)).toString();
            check(first.getString("app_label").equals(expected),"notification_app_label_not_from_package_manager");
            check(entry.getString("app_label").equals("Untrusted label")&&entry.getString("app_icon").equals("/untrusted/fixture.png"),"presentation_mutated_bridge_input");
            check(first.getString("title").equals("Fixture title"),"presentation_changed_notification_content");
            java.io.File icon=new java.io.File(first.getString("app_icon"));
            check(icon.isFile()&&icon.getParentFile().equals(directory),"notification_icon_not_in_owned_cache");
            android.graphics.Bitmap bitmap=android.graphics.BitmapFactory.decodeFile(icon.getAbsolutePath());
            check(bitmap!=null&&bitmap.getWidth()>=48&&bitmap.getWidth()<=192&&bitmap.getWidth()==bitmap.getHeight(),"notification_icon_not_bounded_png");
            bitmap.recycle();long modified=icon.lastModified();
            Bundle second=identity.decorate(state).<Bundle>getParcelableArrayList("notifications").get(0);
            check(first.getString("app_icon").equals(second.getString("app_icon"))&&icon.lastModified()==modified,"unchanged_notification_icon_reencoded");
            identity.invalidate();second=identity.decorate(state).<Bundle>getParcelableArrayList("notifications").get(0);
            check(first.getString("app_icon").equals(second.getString("app_icon")),"same_package_revision_changed_icon_path");
            entry.putString("package","dev.makepad.octosense.fixture.missing");
            Bundle missing=identity.decorate(state).<Bundle>getParcelableArrayList("notifications").get(0);
            check(missing.getString("app_label").equals(entry.getString("package"))&&missing.getString("app_icon").isEmpty(),"missing_package_did_not_fall_back");
            check(!icon.exists(),"unused_notification_icon_not_pruned");
            entries.clear();check(identity.decorate(state).<Bundle>getParcelableArrayList("notifications").isEmpty(),"removed_notifications_retained");
            org.json.JSONArray notices=new org.json.JSONArray();String text=new String(new char[20000]).replace("\0","\uD83D\uDE42");
            for(int index=0;index<8;index++) notices.put(new org.json.JSONObject().put("id",index).put("text",text));
            org.json.JSONObject event=new org.json.JSONObject().put("epoch","fixture").put("revision",1)
                .put("state",new org.json.JSONObject().put("volume",0.5).put("notifications",notices));
            dev.makepad.octosense.NotificationAppIdentity.fitSnapshot(event);
            check(event.toString().getBytes(java.nio.charset.StandardCharsets.UTF_8).length<=Protocol.MAX_PACKET_BYTES,"enriched_snapshot_exceeds_jni_budget");
            check(event.getJSONObject("state").getBoolean("notifications_truncated")&&notices.length()>0&&notices.length()<8,"snapshot_truncation_not_explicit");
            check(notices.getJSONObject(notices.length()-1).getInt("id")==7&&event.getJSONObject("state").getDouble("volume")==0.5,"snapshot_dropped_recent_notice_or_device_state");
            report.putString("resolved_app_label",expected);report.putBoolean("notification_listener_access_granted",false);
        } finally {
            java.io.File[] files=directory.listFiles();if(files!=null) for(java.io.File file:files) file.delete();
            directory.delete();check(!directory.exists(),"notification_identity_fixture_cache_not_removed");
        }
    }
    private void placements(Context context,Bundle report) throws Exception {
        java.io.File directory=new java.io.File(context.getCacheDir(),"placement-validation-"+session);
        if(!directory.mkdirs()) throw new AssertionError("placement_test_directory_failed");
        java.io.File path=new java.io.File(directory,"placements.json");
        try {
            String app="android:0:dev.makepad.octosense/.MakepadApp";
            String profile="android:10:dev.makepad.octosense/.MakepadApp";
            String shortcut="android-shortcut:0:dev.makepad.octosense:test:with:colon";
            LauncherPlacements store=new LauncherPlacements(path);
            check(store.snapshot().getJSONArray("dock").length()==4,"default_dock_wrong");
            store.favorite(app,true);store.favorite(profile,true);store.favorite(shortcut,true);store.favorite(app,true);
            store=new LauncherPlacements(path);
            check(store.snapshot().getJSONArray("favorites").length()==3,"favorites_not_persisted_or_deduplicated");
            check(store.isFavorite(app)&&store.isFavorite(profile)&&store.isFavorite(shortcut),"profile_or_shortcut_identity_lost");
            store.dock(app,0);store=new LauncherPlacements(path);
            check(store.snapshot().getJSONArray("dock").getString(0).equals(app),"dock_not_persisted");
            store.dock(app,3);store=new LauncherPlacements(path);
            check(store.snapshot().getJSONArray("dock").getString(0).isEmpty(),"old_dock_slot_not_cleared");
            check(store.snapshot().getJSONArray("dock").getString(3).equals(app),"dock_move_not_persisted");
            store.undock(app);store.favorite(profile,false);store=new LauncherPlacements(path);
            check(store.snapshot().getJSONArray("dock").getString(3).isEmpty(),"removed_dock_slot_not_cleared");
            check(!store.isFavorite(profile)&&store.isFavorite(app),"removal_crossed_profile_identity");
            try {store.favorite("apps.browser",true);throw new AssertionError("invalid_placement_accepted");}
            catch(IllegalArgumentException expected) {checks++;}
            check(new LauncherPlacements(path).snapshot().getJSONArray("favorites").length()==2,"invalid_request_changed_placements");
            LauncherPlacements otherWriter=new LauncherPlacements(path);
            store.favorite(profile,true);otherWriter.dock(app,1);
            LauncherPlacements concurrent=new LauncherPlacements(path);
            check(concurrent.isFavorite(profile)&&concurrent.snapshot().getJSONArray("dock").getString(1).equals(app),"stale_dock_writer_lost_new_pin");
            otherWriter.favorite(profile,false);store.dock(app,2);
            concurrent=new LauncherPlacements(path);
            check(!concurrent.isFavorite(profile)&&concurrent.snapshot().getJSONArray("dock").getString(2).equals(app),"stale_writer_revived_removed_pin");
            store.undock(app);store=new LauncherPlacements(path);
            check(store.isFavorite("finance"),"hosted_default_home_missing");
            store.favorite("finance",false);store=new LauncherPlacements(path);
            check(!store.isFavorite("finance")&&store.isFavorite("robrix"),"hosted_removal_not_persisted_or_crossed_identity");
            store.dock("finance",0);store.dock("finance",3);store=new LauncherPlacements(path);
            check(store.snapshot().getJSONArray("dock").getString(0).isEmpty()&&store.snapshot().getJSONArray("dock").getString(3).equals("finance"),"hosted_dock_move_not_persisted");
            check(!store.isFavorite("finance"),"docking_revived_hidden_hosted");
            store.undock("finance");store.favorite("finance",true);
            check(store.isFavorite("finance")&&!store.isDocked("finance"),"hosted_home_restore_failed");
            store.dock("photos",2);store.dock("photos",0);store=new LauncherPlacements(path);
            check(store.snapshot().getJSONArray("dock").getString(0).equals("photos")&&store.snapshot().getJSONArray("dock").getString(2).isEmpty(),"default_hosted_move_duplicated_icon");
            store.undock("photos");store.dock("robrix",1);store=new LauncherPlacements(path);
            check(!store.isDocked("photos"),"later_dock_edit_revived_removed_icon");
            otherWriter=new LauncherPlacements(path);store.favorite("finance",false);otherWriter.favorite(shortcut,true);
            check(!new LauncherPlacements(path).isFavorite("finance"),"stale_pin_writer_lost_hosted_removal");
            org.json.JSONObject legacy=new org.json.JSONObject().put("version",1)
                .put("favorites",new org.json.JSONArray().put(app).put(shortcut))
                .put("dock",new org.json.JSONArray().put(app).put("files").put("photos").put("terminal"));
            byte[] original=legacy.toString().getBytes(java.nio.charset.StandardCharsets.UTF_8);
            try(java.io.FileOutputStream output=new java.io.FileOutputStream(path)) {output.write(original);}
            store=new LauncherPlacements(path);
            check(java.util.Arrays.equals(original,new android.util.AtomicFile(path).readFully()),"legacy_read_rewrote_file");
            check(store.isFavorite(app)&&store.isFavorite(shortcut)&&store.isFavorite("finance")&&store.isDocked(app),"legacy_migration_lost_placements");
            store.favorite("finance",false);store=new LauncherPlacements(path);
            check(store.snapshot().getInt("version")==2&&!store.isFavorite("finance")&&store.isFavorite(shortcut)&&store.isDocked(app),"migration_edit_lost_placements");
            byte[] future=store.snapshot().put("version",3).toString().getBytes(java.nio.charset.StandardCharsets.UTF_8);
            try(java.io.FileOutputStream output=new java.io.FileOutputStream(path)) {output.write(future);}
            try {new LauncherPlacements(path);throw new AssertionError("future_schema_accepted");}
            catch(java.io.IOException expected) {checks++;}
            check(java.util.Arrays.equals(future,new android.util.AtomicFile(path).readFully()),"future_schema_overwritten");
            try {store.favorite("robrix",false);throw new AssertionError("stale_writer_overwrote_future_schema");}
            catch(java.io.IOException expected) {checks++;}
            check(java.util.Arrays.equals(future,new android.util.AtomicFile(path).readFully()),"future_schema_overwritten_by_stale_writer");
            report.putBoolean("placements_persisted_profile_scoped_and_future_schema_preserved",true);
        } finally {
            new android.util.AtomicFile(path).delete();
            new java.io.File(path.getPath()+".new").delete();directory.delete();
        }
    }
    @Override public void onStart() {
        Bundle report=new Bundle();AudioManager audio=null;int originalVolume=-1;
        try {
            Context context=getTargetContext();
            check(Protocol.HOME_PACKAGE.equals(context.getPackageName()),"wrong_target_package");
            report.putInt("caller_uid",android.os.Process.myUid());
            if("notification_flow".equals(arguments.getString("mode"))) {
                NotificationFlowFixture.run(this,report);report.putString("result","pass");return;
            }
            if("notification_roundtrip".equals(arguments.getString("mode"))) {
                new NotificationRoundtrip(this).run(context,report);
                report.putString("result","pass");return;
            }
            if("notification_ui".equals(arguments.getString("mode"))) {
                notificationIdentity(context,report);NotificationUiFixture.start();
                try {launcherUi(context,report,false);} finally {runOnMainSync(NotificationUiFixture::finish);}
                java.io.File directory=new java.io.File(context.getCacheDir(),"notification-validation-icons");
                java.io.File[] files=directory.listFiles();
                check(files==null||files.length==0,"notification_ui_cache_not_cleared");directory.delete();
                report.putInt("checks",checks);report.putString("result","pass");return;
            }
            if("shortcut_cleanup".equals(arguments.getString("mode"))) {
                cleanupShortcuts(context,report,arguments.getString("shortcut_owner"));
                report.putInt("checks",checks);report.putString("result","pass");return;
            }
            if("shortcut_ui".equals(arguments.getString("mode"))) {
                String owner=arguments.getString("shortcut_owner");
                check(owner!=null&&owner.matches("[a-f0-9-]{36}"),"shortcut_fixture_owner_missing");
                placements(context,report);
                try {launcherUi(context,report,false);} finally {cleanupShortcuts(context,report,owner);}
                report.putInt("checks",checks);report.putString("result","pass");return;
            }
            if("launcher_ui".equals(arguments.getString("mode"))||"launcher_resume".equals(arguments.getString("mode"))) {
                launcherUi(context,report,"launcher_resume".equals(arguments.getString("mode")));
                report.putInt("checks",checks);report.putString("result","pass");return;
            }
            if("reply_ui".equals(arguments.getString("mode"))) {
                replyUi(context,report);report.putInt("checks",checks);report.putString("result","pass");return;
            }
            if("notification_input".equals(arguments.getString("mode"))) {
                notificationInput(context,report);report.putInt("checks",checks);report.putString("result","pass");return;
            }
            if("home_transport".equals(arguments.getString("mode"))) {
                homeTransport(context,report);report.putInt("checks",checks);report.putString("result","pass");return;
            }
            if("home_layout".equals(arguments.getString("mode"))) {
                homeLayout(report);report.putInt("checks",checks);report.putString("result","pass");return;
            }
            if("widget_ui".equals(arguments.getString("mode"))) {
                widgetUi(context,report);report.putInt("checks",checks);report.putString("result","pass");return;
            }
            if("widgets".equals(arguments.getString("mode"))) {
                widgets(context,report);report.putInt("checks",checks);report.putString("result","pass");return;
            }
            if("placements".equals(arguments.getString("mode"))) {
                placements(context,report);report.putInt("checks",checks);report.putString("result","pass");return;
            }
            LauncherApps launcher=context.getSystemService(LauncherApps.class);int launchables=0;
            for(android.os.UserHandle profile:launcher.getProfiles()) launchables+=launcher.getActivityList(null,profile).size();
            check(launchables>0,"launcher_catalog_empty");report.putInt("launchables",launchables);
            bound=context.bindService(new Intent().setComponent(new ComponentName(Protocol.BRIDGE_PACKAGE,
                    Protocol.BRIDGE_PACKAGE+".SystemBridgeService")),connection,Context.BIND_AUTO_CREATE);
            check(bound&&connected.await(10,TimeUnit.SECONDS)&&bridge!=null,"bridge_bind_timeout");
            check(bridge.getProtocolInfo().getInt("major")==Protocol.MAJOR,"protocol_major_mismatch");
            bridge.subscribe(session,callback);Bundle initial=snapshot();
            Bundle capabilities=initial.getBundle("capabilities");check(capabilities!=null,"capabilities_missing");
            report.putString("root_state",capabilities.getString("root_state"));
            report.putBoolean("notifications_accessible",accessible(capabilities,"notifications"));
            report.putString("epoch",epoch);report.putLong("initial_revision",revision);

            // Invalid oneway input must produce a correlated terminal result,
            // rather than an exception logged only in the helper process.
            bridge.setVolume(session,1,Float.NaN);completion(1,Protocol.INVALID_ARGUMENT,true);
            bridge.setInterruptionFilter(session,2,5);completion(2,Protocol.INVALID_ARGUMENT,true);
            bridge.dismissNotification(session,3,"");completion(3,Protocol.INVALID_ARGUMENT,true);
            if(!accessible(capabilities,"wifi")) {
                bridge.setWifiEnabled(session,4,true);completion(4,Protocol.PREREQUISITE_MISSING,true);
                report.putBoolean("root_unavailable_explicit",true);
            }
            if(!accessible(capabilities,"brightness")) {
                bridge.setBrightness(session,5,0.5f,false);completion(5,Protocol.PREREQUISITE_MISSING,true);
                report.putBoolean("write_settings_missing_explicit",true);
            }
            if(!accessible(capabilities,"rotation")) {
                bridge.setRotationLocked(session,6,true);completion(6,Protocol.PREREQUISITE_MISSING,true);
            }
            if(!accessible(capabilities,"dnd")) {
                bridge.setInterruptionFilter(session,7,1);completion(7,Protocol.PREREQUISITE_MISSING,true);
                report.putBoolean("dnd_access_missing_explicit",true);
            }
            if(!accessible(capabilities,"bluetooth")) {
                bridge.setBluetoothEnabled(session,8,true);completion(8,Protocol.PREREQUISITE_MISSING,true);
            }
            if(!accessible(capabilities,"battery_saver")) {
                bridge.setBatterySaver(session,9,false);completion(9,Protocol.PREREQUISITE_MISSING,true);
            }
            if(!accessible(capabilities,"notifications")) {
                bridge.dismissAllNotifications(session,12);completion(12,Protocol.PREREQUISITE_MISSING,true);
                report.putBoolean("notification_access_missing_explicit",true);
            }
            if("controls".equals(arguments.getString("mode"))) {
                audio=context.getSystemService(AudioManager.class);
                originalVolume=audio.getStreamVolume(AudioManager.STREAM_MUSIC);
                int maximum=audio.getStreamMaxVolume(AudioManager.STREAM_MUSIC);
                check(maximum>0,"media_volume_unavailable");
                int target=originalVolume>0?originalVolume-1:Math.min(1,maximum);
                bridge.setVolume(session,20,target/(float)maximum);completion(20,Protocol.COMPLETED,true);
                check(audio.getStreamVolume(AudioManager.STREAM_MUSIC)==target,"volume_not_applied");
                snapshots.clear();bridge.requestSnapshot(session);Bundle observed=snapshot();
                check(Math.abs(observed.getFloat("volume")-target/(float)maximum)<0.001,"volume_snapshot_wrong");
                bridge.setVolume(session,20,originalVolume/(float)maximum);completion(20,Protocol.COMPLETED,false);
                check(audio.getStreamVolume(AudioManager.STREAM_MUSIC)==target,"duplicate_command_reapplied");
                bridge.setVolume(session,19,originalVolume/(float)maximum);completion(19,Protocol.EXPIRED_HANDLE,false);
                check(audio.getStreamVolume(AudioManager.STREAM_MUSIC)==target,"old_command_dispatched");
                bridge.setVolume(session,21,originalVolume/(float)maximum);completion(21,Protocol.COMPLETED,true);
                check(audio.getStreamVolume(AudioManager.STREAM_MUSIC)==originalVolume,"volume_not_restored");
                report.putBoolean("volume_applied_observed_deduplicated_restored",true);
            }
            report.putInt("checks",checks);report.putString("result","pass");
        } catch(Throwable e) {report.putString("result","fail");report.putString("failure",e.toString());}
        finally {
            if(audio!=null&&originalVolume>=0) audio.setStreamVolume(AudioManager.STREAM_MUSIC,originalVolume,0);
            if(bridge!=null) try {bridge.unsubscribe(session);} catch(Exception ignored) {}
            if(bound) getTargetContext().unbindService(connection);
            finish("pass".equals(report.getString("result"))?Activity.RESULT_OK:Activity.RESULT_CANCELED,report);
        }
    }
}
