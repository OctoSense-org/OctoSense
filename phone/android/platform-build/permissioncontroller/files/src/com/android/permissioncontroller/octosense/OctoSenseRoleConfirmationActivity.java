package com.android.permissioncontroller.octosense;

import android.content.DialogInterface;
import android.content.Intent;
import android.os.Bundle;
import android.os.UserHandle;
import android.widget.Toast;
import androidx.fragment.app.Fragment;
import androidx.fragment.app.FragmentActivity;
import androidx.lifecycle.ViewModel;
import androidx.lifecycle.ViewModelProvider;
import com.android.permissioncontroller.role.ui.DefaultAppConfirmationDialogFragment;
import com.android.permissioncontroller.role.ui.DefaultAppViewModel;
import com.android.permissioncontroller.role.ui.ManageRoleHolderStateLiveData;
import com.android.permissioncontroller.role.utils.RoleUiBehaviorUtils;

/** Unexported; only an immutable one-shot ticket reaches this native confirmation UI. */
public final class OctoSenseRoleConfirmationActivity extends FragmentActivity {
    public static final class ReviewState extends ViewModel {
        RoleSettingsBackend.Reviewed reviewed;String ticket;boolean approved,completed;
    }
    private ReviewState state;private DefaultAppViewModel model;
    @Override public void onCreate(Bundle saved){
        super.onCreate(saved);getWindow().setHideOverlayWindows(true);
        state=new ViewModelProvider(this).get(ReviewState.class);
        RoleSettingsBackend backend=RoleSettingsBackend.get(this);
        if(!backend.owner()){finish();return;}
        if(state.reviewed==null){
            state.ticket=getIntent().getStringExtra("ticket");
            try{state.reviewed=backend.review(state.ticket,false);}catch(Exception unavailable){state.reviewed=null;}
            if(state.reviewed==null){finish();return;}
        }
        model=new ViewModelProvider(this,new DefaultAppViewModel.Factory(state.reviewed.role,UserHandle.SYSTEM,getApplication())).get(DefaultAppViewModel.class);
        model.getManageRoleHolderStateLiveData().observe(this,result->{
            if(result==null||!state.approved||state.completed)return;
            if(result==ManageRoleHolderStateLiveData.STATE_SUCCESS){
                state.completed=true;
                boolean applied;try{applied=backend.isApplied(state.reviewed);}catch(Exception unavailable){applied=false;}
                if(!applied){unavailable();return;}
                // This is the native default-picker's post-selection behavior.
                if(state.reviewed.packageName!=null)state.reviewed.role.onHolderSelectedAsUser(state.reviewed.packageName,UserHandle.SYSTEM,this);
                finish();
            }else if(result==ManageRoleHolderStateLiveData.STATE_FAILURE){state.completed=true;unavailable();}
        });
        if(saved==null)getSupportFragmentManager().beginTransaction().add(android.R.id.content,new ReviewFragment()).commit();
    }
    private CharSequence message(){
        RoleSettingsBackend.Reviewed reviewed=state.reviewed;
        if(reviewed.packageName!=null){CharSequence warning=RoleUiBehaviorUtils.getConfirmationMessage(reviewed.role,reviewed.packageName,this);if(warning!=null)return warning;
            if(reviewed.role.getRequestTitleResource()!=0)return getString(reviewed.role.getRequestTitleResource(),reviewed.label);}
        String selected=reviewed.label;
        if(reviewed.packageName==null){int none=getResources().getIdentifier("default_app_none","string",getPackageName());selected=none==0?"None":getString(none);}
        return getString(reviewed.role.getLabelResource())+"\n"+selected;
    }
    private void approve(){
        if(state.approved||state.completed)return;
        RoleSettingsBackend.Reviewed fresh;
        try{fresh=RoleSettingsBackend.get(this).review(state.ticket,true);}catch(Exception unavailable){fresh=null;}
        if(fresh==null){unavailable();return;}
        state.reviewed=fresh;state.approved=true;
        if(fresh.packageName==null)model.setNoneDefaultApp();else model.setDefaultApp(fresh.packageName);
    }
    private void unavailable(){Toast.makeText(this,"Default app changed or access is restricted. Review the current choices.",Toast.LENGTH_LONG).show();finish();}
    private void dismissed(){if(!isChangingConfigurations()&&state!=null&&!state.approved){RoleSettingsBackend.get(this).cancel(state.ticket);finish();}}
    @Override public void onBackPressed(){if(state==null||!state.approved||state.completed){dismissed();super.onBackPressed();}}
    @Override public void onResume(){super.onResume();if(!RoleSettingsBackend.get(this).owner()){if(state!=null)RoleSettingsBackend.get(this).cancel(state.ticket);finish();}}
    @Override public void onDestroy(){if(isFinishing()&&state!=null&&!state.approved)RoleSettingsBackend.get(this).cancel(state.ticket);super.onDestroy();}
    public static final class ReviewFragment extends Fragment implements DefaultAppConfirmationDialogFragment.Listener {
        @Override public void onResume(){super.onResume();OctoSenseRoleConfirmationActivity activity=(OctoSenseRoleConfirmationActivity)getActivity();
            if(activity==null||activity.isFinishing()||activity.state==null||activity.state.reviewed==null||activity.state.approved||!getChildFragmentManager().getFragments().isEmpty())return;
            Confirmation dialog=new Confirmation();Bundle args=new Bundle();args.putString(Intent.EXTRA_PACKAGE_NAME,activity.state.reviewed.packageName);args.putCharSequence(Intent.EXTRA_TEXT,activity.message());dialog.setArguments(args);dialog.show(getChildFragmentManager(),"octosense_role_confirmation");}
        @Override public void setDefaultApp(String ignored){OctoSenseRoleConfirmationActivity activity=(OctoSenseRoleConfirmationActivity)getActivity();if(activity!=null)activity.approve();}
    }
    public static final class Confirmation extends DefaultAppConfirmationDialogFragment {
        @Override public void onDismiss(DialogInterface dialog){super.onDismiss(dialog);OctoSenseRoleConfirmationActivity activity=(OctoSenseRoleConfirmationActivity)getActivity();if(activity!=null)activity.dismissed();}
    }
}
