package com.android.permissioncontroller.octosense;

import android.app.AlertDialog;
import android.app.Dialog;
import android.content.DialogInterface;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.os.SystemClock;
import android.widget.Toast;
import android.widget.LinearLayout;
import android.widget.ProgressBar;
import android.widget.TextView;
import androidx.fragment.app.DialogFragment;
import androidx.fragment.app.Fragment;
import androidx.fragment.app.FragmentActivity;
import androidx.lifecycle.ViewModel;
import androidx.lifecycle.ViewModelProvider;
import com.android.permissioncontroller.permission.ui.model.AppPermissionViewModel;
import com.android.permissioncontroller.permission.ui.model.AppPermissionViewModel.ChangeRequest;
import com.android.permissioncontroller.permission.ui.v33.AdvancedConfirmDialogArgs;
import dev.makepad.octosense.permissions.PermissionsSettingsContract.Choice;
import static com.android.permissioncontroller.PermissionControllerStatsLog.*;

/** An immutable observed choice reaches the native model; required warnings stay platform-owned. */
public final class OctoSensePermissionOperationActivity extends FragmentActivity {
    public static final class OperationState extends ViewModel {
        String ticketId;PermissionSettingsBackend.Ticket ticket;
        boolean redeemed,dispatched,warning,approved,completed;
        ChangeRequest warningRequest;int messageId,button;boolean oneTime;
    }
    private final Handler main=new Handler(Looper.getMainLooper());
    private OperationState state;private NativePermissionModel.Session session;private PermissionSettingsBackend backend;
    private TextView progressText;
    @Override public void onCreate(Bundle saved){
        super.onCreate(saved);getWindow().setHideOverlayWindows(true);
        backend=PermissionSettingsBackend.get(this);state=new ViewModelProvider(this).get(OperationState.class);
        if(state.ticket==null){state.ticketId=getIntent().getStringExtra("ticket");state.ticket=backend.peek(state.ticketId);}
        if(state.ticket==null||state.completed||!backend.owner()){finish();return;}
        LinearLayout progress=new LinearLayout(this);progress.setOrientation(LinearLayout.VERTICAL);
        int padding=(int)(24*getResources().getDisplayMetrics().density);progress.setPadding(padding,padding,padding,padding);
        progress.addView(new ProgressBar(this));progressText=new TextView(this);progressText.setText("Checking permission…");progress.addView(progressText);setContentView(progress);
        if(saved==null)getSupportFragmentManager().beginTransaction().add(android.R.id.content,new OperationFragment(),"operation").commit();
        main.postDelayed(this::unavailable,Math.max(1,state.ticket.expires-SystemClock.elapsedRealtime()));
    }
    private void observe(){
        if(session!=null||isFinishing()||state.completed)return;
        session=backend.source.observe(state.ticket.pkg,state.ticket.group,this::observed);
        session.start();
        main.postDelayed(()->{if(!isFinishing()&&!state.warning&&!state.completed)unavailable();},10_000);
    }
    private void observed(NativePermissionModel.State current){
        if(isFinishing()||state.completed)return;
        if(current==null||!backend.owner()){unavailable();return;}
        if(!state.dispatched){
            PermissionSettingsBackend.Ticket fresh=backend.redeem(state.ticketId,current);
            if(fresh==null){unavailable();return;}state.ticket=fresh;state.redeemed=true;
            Fragment fragment=getSupportFragmentManager().findFragmentByTag("operation");
            if(!(fragment instanceof OperationFragment)||!fragment.isAdded()){unavailable();return;}
            state.dispatched=true;
            progressText.setText("Applying permission…");
            try{execute(session.model,(OperationFragment)fragment,state.ticket.choice);}catch(Exception failure){unavailable();}
            // Permission updates can be synchronous; reread through the native observation.
            main.post(this::observeCurrent);
        }else if(state.warning&&!state.approved){
            if(!backend.valid(state.ticket,current)){unavailable();return;}showWarning();
        }else if(backend.applied(state.ticket,current)){
            state.completed=true;finish();
        }
    }
    private static void execute(AppPermissionViewModel model,OperationFragment fragment,Choice choice){
        ChangeRequest request;boolean oneTime=false;int button;
        switch(choice){
            case ALLOW:request=ChangeRequest.GRANT_FOREGROUND;button=APP_PERMISSION_FRAGMENT_ACTION_REPORTED__BUTTON_PRESSED__ALLOW;break;
            case ALLOW_ALWAYS:request=ChangeRequest.GRANT_BOTH;button=APP_PERMISSION_FRAGMENT_ACTION_REPORTED__BUTTON_PRESSED__ALLOW_ALWAYS;break;
            case ALLOW_FOREGROUND:request=ChangeRequest.GRANT_FOREGROUND_ONLY;button=APP_PERMISSION_FRAGMENT_ACTION_REPORTED__BUTTON_PRESSED__ALLOW_FOREGROUND;break;
            case ASK:request=ChangeRequest.REVOKE_BOTH;oneTime=true;button=APP_PERMISSION_FRAGMENT_ACTION_REPORTED__BUTTON_PRESSED__ASK_EVERY_TIME;break;
            case DENY:request=ChangeRequest.REVOKE_BOTH;button=APP_PERMISSION_FRAGMENT_ACTION_REPORTED__BUTTON_PRESSED__DENY;break;
            case DENY_FOREGROUND:request=ChangeRequest.REVOKE_FOREGROUND;button=APP_PERMISSION_FRAGMENT_ACTION_REPORTED__BUTTON_PRESSED__DENY_FOREGROUND;break;
            case PRECISE:request=ChangeRequest.GRANT_FINE_LOCATION;button=APP_PERMISSION_FRAGMENT_ACTION_REPORTED__BUTTON_PRESSED__GRANT_FINE_LOCATION;break;
            case APPROXIMATE:request=ChangeRequest.REVOKE_FINE_LOCATION;button=APP_PERMISSION_FRAGMENT_ACTION_REPORTED__BUTTON_PRESSED__REVOKE_FINE_LOCATION;break;
            default:throw new IllegalArgumentException("Observed one-time access is not a command");
        }
        model.requestChange(oneTime,fragment,fragment,request,button);
    }
    private void warning(ChangeRequest request,int messageId,int button,boolean oneTime){
        if(state.warning||state.approved||state.completed||isFinishing()){unavailable();return;}
        // Media/all-files changes are outside this common-group adapter.
        if(request!=ChangeRequest.REVOKE_BOTH&&request!=ChangeRequest.REVOKE_FOREGROUND&&request!=ChangeRequest.GRANT_FOREGROUND_ONLY){unavailable();return;}
        state.warning=true;state.warningRequest=request;state.messageId=messageId;state.button=button;state.oneTime=oneTime;showWarning();
    }
    private void showWarning(){
        if(isFinishing()||!state.warning||state.approved||getSupportFragmentManager().findFragmentByTag("warning")!=null||getSupportFragmentManager().isStateSaved())return;
        new WarningDialog().show(getSupportFragmentManager(),"warning");
    }
    private void approveWarning(){
        if(state==null||!state.warning||state.approved||state.completed||session==null||!backend.valid(state.ticket,session.current())){unavailable();return;}
        state.approved=true;
        try{session.model.onDenyAnyWay(state.warningRequest,state.button,state.oneTime);}
        catch(Exception failure){unavailable();return;}
        main.post(this::observeCurrent);
        main.postDelayed(()->{if(!state.completed&&!isFinishing())unavailable();},10_000);
    }
    private int nativeString(String name){return getResources().getIdentifier(name,"string",getPackageName());}
    private void observeCurrent(){if(session!=null&&!isFinishing()){NativePermissionModel.State current=session.current();if(current!=null)observed(current);}}
    private void unavailable(){if(isFinishing())return;Toast.makeText(this,"Permissions changed or access is restricted. Review the current choices.",Toast.LENGTH_LONG).show();finish();}
    private void cancel(){if(state!=null)backend.cancel(state.ticketId);finish();}
    @Override public void onBackPressed(){cancel();}
    @Override public void onResume(){super.onResume();if(backend!=null&&!backend.owner())cancel();}
    @Override public void onDestroy(){main.removeCallbacksAndMessages(null);if(session!=null)session.close();if(isFinishing()&&state!=null)backend.cancel(state.ticketId);super.onDestroy();}
    public static final class OperationFragment extends Fragment implements AppPermissionViewModel.ConfirmDialogShowingFragment {
        @Override public void onResume(){super.onResume();OctoSensePermissionOperationActivity activity=(OctoSensePermissionOperationActivity)getActivity();if(activity!=null)activity.observe();}
        @Override public void showConfirmDialog(ChangeRequest request,int messageId,int button,boolean oneTime){OctoSensePermissionOperationActivity activity=(OctoSensePermissionOperationActivity)getActivity();if(activity!=null)activity.warning(request,messageId,button,oneTime);}
        @Override public void showAdvancedConfirmDialog(AdvancedConfirmDialogArgs args){OctoSensePermissionOperationActivity activity=(OctoSensePermissionOperationActivity)getActivity();if(activity!=null)activity.unavailable();}
    }
    public static final class WarningDialog extends DialogFragment {
        @Override public Dialog onCreateDialog(Bundle saved){
            OctoSensePermissionOperationActivity activity=(OctoSensePermissionOperationActivity)requireActivity();
            int deny=activity.nativeString("grant_dialog_button_deny_anyway"),cancel=activity.nativeString("cancel");
            if(deny==0||cancel==0||activity.state==null||activity.state.messageId==0){activity.unavailable();return new Dialog(activity);}
            AlertDialog dialog=new AlertDialog.Builder(activity).setMessage(activity.state.messageId)
                    .setNegativeButton(cancel,(value,which)->activity.cancel())
                    .setPositiveButton(deny,(value,which)->activity.approveWarning()).create();
            dialog.setCanceledOnTouchOutside(true);return dialog;
        }
        @Override public void onCancel(DialogInterface dialog){super.onCancel(dialog);OctoSensePermissionOperationActivity activity=(OctoSensePermissionOperationActivity)getActivity();if(activity!=null)activity.cancel();}
    }
}
