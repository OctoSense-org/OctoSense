package dev.makepad.octosense.settingsboundary;
import android.app.Activity;
import android.content.*;
import android.os.*;
import android.util.Log;
import java.util.ArrayList;

/** Disposable caller: observations and invalid-key denial probes, no production identities. */
public final class BoundaryActivity extends Activity {
    private final Handler handler=new Handler(Looper.getMainLooper());
    private final ArrayList<ServiceConnection> connections=new ArrayList<>();
    private int done;
    private int expected=17;
    private boolean failed;
    private void result(String label,boolean denied) {
        failed|=!denied;Log.i("OctoSenseBoundary",label+" denied="+denied);
        if(++done==expected) {Log.i("OctoSenseBoundary",failed?"BOUNDARY_FAIL":"BOUNDARY_PASS");handler.removeCallbacksAndMessages(null);finish();}
    }
    @Override public void onCreate(Bundle state) {
        super.onCreate(state);
        String roles=getIntent().getStringExtra("roles");
        String permissions=getIntent().getStringExtra("permissions");
        String dnd=getIntent().getStringExtra("dnd");
        String appPolicy=getIntent().getStringExtra("app_policy");
        String captions=getIntent().getStringExtra("captions");boolean captionCustom="custom".equals(captions)||"all".equals(captions),captionLanguage="language".equals(captions)||"all".equals(captions);
        boolean systemLanguages=getIntent().getBooleanExtra("system_languages",false);
        boolean network="network".equals(appPolicy)||"all".equals(appPolicy),battery="battery".equals(appPolicy)||"all".equals(appPolicy),storage="storage".equals(appPolicy)||"all".equals(appPolicy),language="language".equals(appPolicy)||"all".equals(appPolicy);
        boolean roleController="controller".equals(roles)||"all".equals(roles),roleAgent="all".equals(roles);
        boolean permissionController="controller".equals(permissions)||"all".equals(permissions),permissionAgent="all".equals(permissions);
        boolean dndBroker="broker".equals(dnd)||"all".equals(dnd),dndAgent="all".equals(dnd);
        expected=19+(roleController?3:0)+(roleAgent?2:0)+(permissionController?3:0)+(permissionAgent?2:0)+(dndBroker?5:0)+(dndAgent?5:0)+(network?4:0)+(battery?4:0)+(storage?4:0)+(language?4:0);
        expected+=(captionCustom?3:0)+(captionLanguage?2:0);
        expected+=systemLanguages?4:0;
        handler.postDelayed(()->{Log.e("OctoSenseBoundary","BOUNDARY_FAIL timeout completed="+done);finish();},10000);
        try {startActivity(new Intent().setComponent(new ComponentName("dev.makepad.octosense.settingsbroker","dev.makepad.octosense.settingsbroker.OctoSenseRemoveAccountActivity")));result("unexported_confirmation",false);}
        catch(SecurityException denied) {result("unexported_confirmation",true);}
        if(captionCustom)for(int i=0;i<3;i++)probe("caption_custom_"+new String[]{"snapshot","set","close"}[i],"dev.makepad.octosense.agent","dev.makepad.octosense.agent.AgentPlatformService","dev.makepad.octosense.agent.IAgentPlatform",i==0,77+i);
        if(captionLanguage)for(int i=0;i<2;i++)probe("caption_language_"+(i==0?"snapshot":"set"),"dev.makepad.octosense.agent","dev.makepad.octosense.agent.AgentPlatformService","dev.makepad.octosense.agent.IAgentPlatform",i==0,80+i);
        if(systemLanguages)for(int i=0;i<2;i++){
            probe("agent_system_languages_"+(i==0?"snapshot":"apply"),"dev.makepad.octosense.agent","dev.makepad.octosense.agent.AgentPlatformService","dev.makepad.octosense.agent.IAgentPlatform",i==0,82+i);
            probe("broker_system_languages_"+(i==0?"snapshot":"apply"),"dev.makepad.octosense.settingsbroker","dev.makepad.octosense.settingsbroker.OctoSenseSystemLanguageService","dev.makepad.octosense.settingsbroker.ISystemLanguageSettings",i==0,i);
        }
        if(roleController){
            try {startActivity(new Intent().setComponent(new ComponentName("com.android.permissioncontroller","com.android.permissioncontroller.octosense.OctoSenseRoleConfirmationActivity")));result("unexported_role_confirmation",false);}
            catch(SecurityException denied){result("unexported_role_confirmation",true);}
            probe("controller_roles","com.android.permissioncontroller","com.android.permissioncontroller.octosense.OctoSenseRolesService","com.android.permissioncontroller.octosense.IRoleSettings",true,0);
            probe("controller_roles_confirm","com.android.permissioncontroller","com.android.permissioncontroller.octosense.OctoSenseRolesService","com.android.permissioncontroller.octosense.IRoleSettings",false,1);
        }
        if(roleAgent){
            probe("roles","dev.makepad.octosense.agent","dev.makepad.octosense.agent.AgentPlatformService","dev.makepad.octosense.agent.IAgentPlatform",true,60);
            probe("roles_confirm","dev.makepad.octosense.agent","dev.makepad.octosense.agent.AgentPlatformService","dev.makepad.octosense.agent.IAgentPlatform",false,61);
        }
        if(permissionController){
            try {startActivity(new Intent().setComponent(new ComponentName("com.android.permissioncontroller","com.android.permissioncontroller.octosense.OctoSensePermissionOperationActivity")));result("unexported_permission_operation",false);}
            catch(SecurityException denied){result("unexported_permission_operation",true);}
            probe("controller_permissions","com.android.permissioncontroller","com.android.permissioncontroller.octosense.OctoSensePermissionsService","com.android.permissioncontroller.octosense.IPermissionSettings",true,0);
            probe("controller_permission_choice","com.android.permissioncontroller","com.android.permissioncontroller.octosense.OctoSensePermissionsService","com.android.permissioncontroller.octosense.IPermissionSettings",false,1);
        }
        if(permissionAgent){
            probe("permissions","dev.makepad.octosense.agent","dev.makepad.octosense.agent.AgentPlatformService","dev.makepad.octosense.agent.IAgentPlatform",true,62);
            probe("permission_choice","dev.makepad.octosense.agent","dev.makepad.octosense.agent.AgentPlatformService","dev.makepad.octosense.agent.IAgentPlatform",false,63);
        }
        for(int i=0;i<2;i++){
            if(network){
                probe("agent_app_network_"+(i==0?"snapshot":"set"),"dev.makepad.octosense.agent","dev.makepad.octosense.agent.AgentPlatformService","dev.makepad.octosense.agent.IAgentPlatform",i==0,69+i);
                probe("broker_app_network_"+(i==0?"snapshot":"set"),"dev.makepad.octosense.settingsbroker","dev.makepad.octosense.settingsbroker.OctoSenseAppNetworkService","dev.makepad.octosense.settingsbroker.IAppNetworkSettings",i==0,i);
            }
            if(battery){
                probe("agent_app_battery_"+(i==0?"snapshot":"set"),"dev.makepad.octosense.agent","dev.makepad.octosense.agent.AgentPlatformService","dev.makepad.octosense.agent.IAgentPlatform",i==0,71+i);
                probe("broker_app_battery_"+(i==0?"snapshot":"set"),"dev.makepad.octosense.settingsbroker","dev.makepad.octosense.settingsbroker.OctoSenseAppBatteryService","dev.makepad.octosense.settingsbroker.IAppBatterySettings",i==0,i);
            }
            if(language){
                probe("agent_app_language_"+(i==0?"snapshot":"set"),"dev.makepad.octosense.agent","dev.makepad.octosense.agent.AgentPlatformService","dev.makepad.octosense.agent.IAgentPlatform",i==0,75+i);
                probe("broker_app_language_"+(i==0?"snapshot":"set"),"dev.makepad.octosense.settingsbroker","dev.makepad.octosense.settingsbroker.OctoSenseAppLanguageService","dev.makepad.octosense.settingsbroker.IAppLanguageSettings",i==0,i);
            }
            if(storage){
                probe("agent_app_storage_"+(i==0?"snapshot":"set"),"dev.makepad.octosense.agent","dev.makepad.octosense.agent.AgentPlatformService","dev.makepad.octosense.agent.IAgentPlatform",i==0,73+i);
                probe("broker_app_storage_"+(i==0?"snapshot":"set"),"dev.makepad.octosense.settingsbroker","dev.makepad.octosense.settingsbroker.OctoSenseAppStorageService","dev.makepad.octosense.settingsbroker.IAppStorageSettings",i==0,i);
            }
        }
        String[] dndMethods={"snapshot","policy","schedule","enabled","delete"};
        for(int i=0;i<dndMethods.length;i++){
            if(dndBroker)probe("broker_dnd_"+dndMethods[i],"dev.makepad.octosense.settingsbroker","dev.makepad.octosense.settingsbroker.OctoSenseZenSettingsService","dev.makepad.octosense.settingsbroker.IZenSettings",i==0,2+i);
            if(dndAgent)probe("agent_dnd_"+dndMethods[i],"dev.makepad.octosense.agent","dev.makepad.octosense.agent.AgentPlatformService","dev.makepad.octosense.agent.IAgentPlatform",i==0,64+i);
        }
        probe("helper","dev.makepad.octosense.agent","dev.makepad.octosense.agent.AgentPlatformService","dev.makepad.octosense.agent.IAgentPlatform",false);
        probe("sounds","dev.makepad.octosense.agent","dev.makepad.octosense.agent.AgentPlatformService","dev.makepad.octosense.agent.IAgentPlatform",true,52);
        probe("sound_stop","dev.makepad.octosense.agent","dev.makepad.octosense.agent.AgentPlatformService","dev.makepad.octosense.agent.IAgentPlatform",false,55);
        probe("display","dev.makepad.octosense.agent","dev.makepad.octosense.agent.AgentPlatformService","dev.makepad.octosense.agent.IAgentPlatform",true,56);
        probe("display_write","dev.makepad.octosense.agent","dev.makepad.octosense.agent.AgentPlatformService","dev.makepad.octosense.agent.IAgentPlatform",false,57);
        probe("text_interaction","dev.makepad.octosense.agent","dev.makepad.octosense.agent.AgentPlatformService","dev.makepad.octosense.agent.IAgentPlatform",true,27);
        probe("text_interaction_write","dev.makepad.octosense.agent","dev.makepad.octosense.agent.AgentPlatformService","dev.makepad.octosense.agent.IAgentPlatform",false,28);
        probe("controls","dev.makepad.octosense.agent","dev.makepad.octosense.agent.AgentPlatformService","dev.makepad.octosense.agent.IAgentPlatform",true,27);
        probe("controls_write","dev.makepad.octosense.agent","dev.makepad.octosense.agent.AgentPlatformService","dev.makepad.octosense.agent.IAgentPlatform",false,28);
        probe("app_notifications","dev.makepad.octosense.agent","dev.makepad.octosense.agent.AgentPlatformService","dev.makepad.octosense.agent.IAgentPlatform",true,58);
        probe("app_notifications_write","dev.makepad.octosense.agent","dev.makepad.octosense.agent.AgentPlatformService","dev.makepad.octosense.agent.IAgentPlatform",false,59);
        probe("broker_notifications","dev.makepad.octosense.settingsbroker","dev.makepad.octosense.settingsbroker.OctoSenseAppNotificationsService","dev.makepad.octosense.settingsbroker.IAppNotifications",true,0);
        probe("broker_notifications_write","dev.makepad.octosense.settingsbroker","dev.makepad.octosense.settingsbroker.OctoSenseAppNotificationsService","dev.makepad.octosense.settingsbroker.IAppNotifications",false,1);
        probe("history","dev.makepad.octosense.agent","dev.makepad.octosense.agent.AgentPlatformService","dev.makepad.octosense.agent.IAgentPlatform",true,51);
        probe("time","dev.makepad.octosense.agent","dev.makepad.octosense.agent.AgentPlatformService","dev.makepad.octosense.agent.IAgentPlatform",true,49);
        probe("accounts","dev.makepad.octosense.settingsbroker","dev.makepad.octosense.settingsbroker.OctoSenseAccountSettingsService","dev.makepad.octosense.settingsbroker.IAccountSettings",true);
        probe("sensors","com.android.systemui","com.android.systemui.octosense.OctoSenseSensorSettingsService","com.android.systemui.octosense.ISensorSettings",false);
        probe("dnd","dev.makepad.octosense.settingsbroker","dev.makepad.octosense.settingsbroker.OctoSenseZenSettingsService","dev.makepad.octosense.settingsbroker.IZenSettings",false);
    }
    private void probe(String label,String pkg,String cls,String descriptor,boolean requestId) {probe(label,pkg,cls,descriptor,requestId,0);}
    private void probe(String label,String pkg,String cls,String descriptor,boolean requestId,int transaction) {
        ServiceConnection connection=new ServiceConnection() {
            @Override public void onServiceConnected(ComponentName name,IBinder binder) {
                Parcel data=Parcel.obtain(),reply=Parcel.obtain();boolean denied=false;
                try {data.writeInterfaceToken(descriptor);if(requestId)data.writeLong(1);if(transaction==51){data.writeString(null);data.writeInt(0);}if(transaction==52){data.writeString("ringtone");data.writeString(null);data.writeInt(0);}
                    if(transaction==57){data.writeString("invalid");data.writeString("invalid");data.writeString("invalid");}
                    if(transaction==27)data.writeString(label.equals("text_interaction")?"accessibility_text_interaction":"sound_feedback");
                    if(transaction>=77&&transaction<=79){data.writeStrongBinder(new Binder());data.writeString("a".repeat(64));data.writeLong(1);if(transaction==78){data.writeString("caption_foreground_color");data.writeString("caption_swatch:48");}}
                    if(transaction==80){data.writeString("");data.writeString("");data.writeInt(0);}
                    if(transaction==81){data.writeString("a".repeat(64));data.writeString("b".repeat(64));}
                    if(label.contains("_system_languages_")){
                        if(label.endsWith("snapshot")){data.writeString("");data.writeString("");data.writeString("");data.writeInt(0);}
                        else{data.writeString("a".repeat(64));data.writeStringArray(new String[]{"b".repeat(64)});}
                    }
                    if(transaction==28){boolean text=label.equals("text_interaction_write");data.writeString(text?"accessibility_text_interaction":"sound_feedback");data.writeString(text?"high_contrast_text":"vibration_enabled");data.writeString(text?"on":"invalid");}
                    if(transaction==58||label.equals("broker_notifications")){data.writeString(getPackageName());data.writeInt(0);data.writeString(null);}
                    if(transaction==59||label.equals("broker_notifications_write")){
                        data.writeString(getPackageName());data.writeString("invalid");data.writeString("invalid");
                        data.writeString("app_enabled");data.writeString("invalid");
                    }
                    if(transaction==60||label.equals("controller_roles")){data.writeString("browser");data.writeInt(0);data.writeString(null);}
                    if(transaction==61||label.equals("controller_roles_confirm")){data.writeString("browser");data.writeString("invalid");data.writeString("invalid");}
                    if(transaction==62||label.equals("controller_permissions")){data.writeString(getPackageName());data.writeString(null);data.writeInt(0);data.writeString(null);}
                    if(transaction==63||label.equals("controller_permission_choice")){data.writeString(getPackageName());data.writeString("camera");data.writeString("invalid");data.writeString("invalid");}
                    if(label.startsWith("agent_app_language_")||label.startsWith("broker_app_language_")){
                        data.writeString(getPackageName());
                        if(label.endsWith("snapshot")){data.writeString("");data.writeString("");data.writeString("");data.writeInt(0);}
                        else {data.writeString("0".repeat(64));data.writeString("1".repeat(64));}
                    }
                    if(label.startsWith("agent_app_network_")||label.startsWith("broker_app_network_")||label.startsWith("agent_app_battery_")||label.startsWith("broker_app_battery_")||label.startsWith("agent_app_storage_")||label.startsWith("broker_app_storage_")){
                        data.writeString(getPackageName());if(label.endsWith("set")){
                            data.writeString("invalid");data.writeString(label.contains("network")?"background":label.contains("storage")?"clear_cache":"optimized");
                            if(label.contains("network"))data.writeInt(0);
                        }
                    }
                    if(label.startsWith("broker_dnd_")||label.startsWith("agent_dnd_")){
                        if(label.endsWith("snapshot")){data.writeInt(0);data.writeString(null);}
                        else{
                            data.writeString("invalid");
                            if(label.endsWith("policy")){data.writeString("alarms");data.writeString("off");}
                            else if(label.endsWith("schedule")){
                                data.writeString(null);data.writeString("Untrusted caller must not create this");data.writeIntArray(new int[]{2});
                                data.writeInt(600);data.writeInt(660);data.writeInt(0);data.writeInt(0);
                            }else {data.writeString("invalid");if(label.endsWith("enabled"))data.writeInt(0);}
                        }
                    }
                    if(!binder.transact(IBinder.FIRST_CALL_TRANSACTION+transaction,data,reply,0))throw new IllegalStateException("Missing transaction");
                    reply.readException();
                }catch(SecurityException expected){denied=true;}
                catch(Exception error){Log.e("OctoSenseBoundary",label+" unexpected="+error.getClass().getSimpleName());}
                finally {data.recycle();reply.recycle();}
                result(label,denied);
            }
            @Override public void onServiceDisconnected(ComponentName name) {}
        };
        try {if(bindService(new Intent().setComponent(new ComponentName(pkg,cls)),connection,BIND_AUTO_CREATE))connections.add(connection);else result(label,false);}
        catch(SecurityException denied) {result(label,true);}
    }
    @Override public void onDestroy() {for(ServiceConnection connection:connections)unbindService(connection);super.onDestroy();}
}
