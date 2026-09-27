package dev.makepad.octosense.settingsbroker;

import android.app.AlertDialog;
import android.content.Intent;
import android.os.Bundle;
import android.view.WindowManager;
import android.view.inputmethod.InputMethodInfo;
import android.view.inputmethod.InputMethodManager;
import android.widget.LinearLayout;
import android.widget.TextView;
import androidx.fragment.app.FragmentActivity;
import androidx.preference.PreferenceFragmentCompat;
import androidx.preference.PreferenceScreen;
import com.android.settingslib.inputmethod.InputMethodAndSubtypeUtilCompat;
import com.android.settingslib.inputmethod.InputMethodPreference;
import com.android.settingslib.inputmethod.InputMethodSettingValuesWrapper;
import dev.makepad.octosense.keyboards.KeyboardContract;
import dev.makepad.octosense.keyboards.KeyboardPolicy;
import java.lang.reflect.Field;

/** Immutable reviewed entry only. Native SettingsLib owns warning text/order and linked writes. */
public final class OctoSenseKeyboardActivity extends FragmentActivity {
    private KeyboardSession session;
    private String ticket;
    private boolean invoked,claimed;
    private NativePreferences preferences;
    @Override public void onCreate(Bundle saved){
        super.onCreate(saved);getWindow().addSystemFlags(WindowManager.LayoutParams.SYSTEM_FLAG_HIDE_NON_SYSTEM_OVERLAY_WINDOWS);
        session=KeyboardSession.get(this);
        try{ticket=getIntent().getStringExtra("ticket");KeyboardContract.key(ticket);}catch(Exception invalid){finish();return;}
        // Handled configuration changes retain the existing native dialog. Process restoration
        // never replays a consumed operation or reauthorizes a previous warning.
        if(saved!=null){session.policy.cancel(ticket);message("Return to Settings and review the current keyboard state.");return;}
        KeyboardPolicy.Review review=session.policy.checkedReview(ticket);if(review==null||!session.platform.writable()){message("Keyboard choices changed. Return to Settings and refresh.");return;}
        LinearLayout content=new LinearLayout(this);content.setOrientation(LinearLayout.VERTICAL);content.setPadding(32,32,32,32);content.setFilterTouchesWhenObscured(true);
        TextView heading=new TextView(this);heading.setText("Keyboard settings");heading.setTextSize(22);content.addView(heading);
        TextView detail=new TextView(this);detail.setText("Android is opening the selected keyboard control.");content.addView(detail);setContentView(content);
        if(review.action==KeyboardPolicy.Action.ENABLE||review.action==KeyboardPolicy.Action.DISABLE){
            preferences=new NativePreferences();getSupportFragmentManager().beginTransaction().add(preferences,"native-keyboards").commitNow();
        }
    }
    @Override public void onWindowFocusChanged(boolean focused){super.onWindowFocusChanged(focused);if(focused&&!invoked&&!isFinishing()&&ticket!=null){invoked=true;getWindow().getDecorView().post(this::begin);}}
    private void begin(){
        if(isFinishing()||!hasWindowFocus()||!session.platform.writable()){finish();return;}
        KeyboardPolicy.Review review=session.policy.checkedReview(ticket);if(review==null){message("Keyboard choices changed. Return to Settings and refresh.");return;}
        try{
            if(review.action==KeyboardPolicy.Action.ENABLE||review.action==KeyboardPolicy.Action.DISABLE){
                InputMethodPreference preference=preferences.findPreference(review.method.id);
                if(preference==null||preference.isChecked()!=review.method.enabled){message("Keyboard choices changed. Return to Settings and refresh.");return;}
                preference.onPreferenceChange(preference,review.action==KeyboardPolicy.Action.ENABLE);
                if(!claimed&&!isFinishing())observeNativeDialogExit(preference);
            }else{
                KeyboardPolicy.Claim claim=session.policy.claimAfterNativeConsent(ticket);claimed=true;if(claim==null){message("Keyboard choices changed. Return to Settings and refresh.");return;}
                if(claim.action==KeyboardPolicy.Action.PICK_DEFAULT){getSystemService(InputMethodManager.class).showInputMethodPickerFromSystem(false,getDisplay().getDisplayId());finish();return;}
                InputMethodInfo imi=session.platform.method(claim.method.id);if(imi==null){message("This keyboard is unavailable.");return;}
                Intent intent=claim.action==KeyboardPolicy.Action.SETTINGS?session.platform.provider(imi):session.platform.subtypes(imi);
                if(intent==null){message("This keyboard control is unavailable.");return;}
                startActivity(intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK|Intent.FLAG_ACTIVITY_MULTIPLE_TASK|Intent.FLAG_ACTIVITY_EXCLUDE_FROM_RECENTS));finish();
            }
        }catch(Exception|LinkageError unavailable){message("Android could not open this keyboard control. Check its current state in Settings.");}
    }
    private void nativeSaved(InputMethodPreference selected){
        if(claimed||isFinishing())return;KeyboardPolicy.Review review=session.policy.review(ticket);
        if(review==null){message("Keyboard choices changed. Return to Settings and refresh.");return;}
        boolean desired=review.action==KeyboardPolicy.Action.ENABLE;
        // The native security dialog calls its save listener with false on Cancel too.
        if(selected.isChecked()!=desired){session.policy.cancel(ticket);finish();return;}
        if(!session.platform.writable()){message("Keyboard changes are unavailable while locked or restricted.");return;}
        KeyboardPolicy.Claim claim=session.policy.claimAfterNativeConsent(ticket);claimed=true;
        if(claim==null){message("Keyboard choices changed. Return to Settings and refresh.");return;}
        try{
            InputMethodAndSubtypeUtilCompat.saveInputMethodSubtypeListForUser(preferences,getContentResolver(),session.platform.methods(),session.platform.hardwareKeyboard(),0);
            InputMethodSettingValuesWrapper.getInstance(this).refreshAllInputMethodAndSubtypes();
            if(session.policy.observeCompletion(claim)==KeyboardPolicy.Result.APPLIED){setResult(RESULT_OK);finish();}
            else message("The keyboard change could not be confirmed. Return to Settings to check its current state.");
        }catch(Exception|LinkageError unavailable){message("The keyboard change could not be confirmed. Return to Settings to check its current state.");}
    }
    private void observeNativeDialogExit(InputMethodPreference preference)throws Exception{
        // This exact linked SettingsLib field is used only to retire this host after native
        // Back/Cancel. Direct Boot's native dialog has no save/cancel callback. A dismiss
        // never grants authority; only nativeSaved with the intended checked state can do so.
        Field field=InputMethodPreference.class.getDeclaredField("mDialog");field.setAccessible(true);
        AlertDialog dialog=(AlertDialog)field.get(preference);if(dialog==null||!dialog.isShowing()){if(!claimed)finish();return;}
        dialog.setOnDismissListener(dismissed->{
            if(isFinishing()||claimed)return;
            try{AlertDialog current=(AlertDialog)field.get(preference);if(current!=null&&current!=dialog&&current.isShowing())observeNativeDialogExit(preference);else finish();}
            catch(Exception unknown){finish();}
        });
    }
    private void message(String text){
        if(isFinishing())return;invoked=true;if(ticket!=null)session.policy.cancel(ticket);
        new AlertDialog.Builder(this).setTitle("Keyboard settings").setMessage(text).setPositiveButton("Return to Settings",(d,w)->finish()).setOnCancelListener(d->finish()).show();
    }
    @Override public void onSaveInstanceState(Bundle out){out.putBoolean("invoked",invoked);super.onSaveInstanceState(out);}
    @Override protected void onDestroy(){if(isFinishing()&&session!=null&&ticket!=null)session.policy.cancel(ticket);super.onDestroy();}
    public static final class NativePreferences extends PreferenceFragmentCompat {
        @Override public void onCreatePreferences(Bundle saved,String root){
            OctoSenseKeyboardActivity activity=(OctoSenseKeyboardActivity)requireActivity();KeyboardSession session=activity.session;
            PreferenceScreen screen=getPreferenceManager().createPreferenceScreen(requireContext());setPreferenceScreen(screen);
            try{
                KeyboardPolicy.State state=session.platform.read();if(!"available".equals(state.availability))return;
                for(KeyboardPolicy.Method row:state.methods){InputMethodInfo imi=session.platform.method(row.id);if(imi==null)continue;
                    InputMethodPreference pref=new InputMethodPreference(requireContext(),imi,row.enabled||row.canEnable,activity::nativeSaved,0);
                    pref.setChecked(row.enabled);screen.addPreference(pref);
                }
            }catch(Exception|LinkageError unavailable){screen.removeAll();}
        }
    }
}
