package dev.makepad.octosense.settingsbroker;

import android.accounts.Account;
import android.accounts.AccountManager;
import android.app.Activity;
import android.app.ActivityManager;
import android.app.AlertDialog;
import android.os.Bundle;
import android.os.UserHandle;
import android.view.WindowManager;
import dev.makepad.octosense.accounts.AccountsSettingsBackend;
import dev.makepad.octosense.accounts.AccountsSettingsContract;

/** Unexported, one-shot platform confirmation. Provider auth never crosses into scripts. */
public final class OctoSenseRemoveAccountActivity extends Activity {
    private boolean pending;
    private String key;
    private String incarnation;
    private AccountsSettingsBackend backend;
    private AlertDialog progress;
    private boolean currentUser() {return ActivityManager.getCurrentUser()==UserHandle.myUserId()&&backend.unlocked();}
    @Override public void onCreate(Bundle saved) {
        super.onCreate(saved);
        getWindow().addSystemFlags(WindowManager.LayoutParams.SYSTEM_FLAG_HIDE_NON_SYSTEM_OVERLAY_WINDOWS);
        backend=new AccountsSettingsBackend(this);
        try {key=AccountsSettingsContract.key(getIntent().getStringExtra("account_key"));incarnation=AccountsSettingsContract.key(getIntent().getStringExtra("account_incarnation"));}catch(IllegalArgumentException invalid) {finish();return;}
        if(!currentUser()) {finish();return;}
        if(saved!=null&&saved.getBoolean("pending")) {message("Return to Settings to check whether this account was removed.");return;}
        Account account=backend.resolveConfirmationAccount(key);if(!sameIncarnation(account)||!backend.canRemove(account)) {message("This account is unavailable or removal is restricted.");return;}
        new AlertDialog.Builder(this).setTitle("Remove account?")
                .setMessage(display(account.name)+" ("+display(account.type)+")\n\nThis removes the account and its synced information from this Android user. It does not delete the account at its provider.")
                .setNegativeButton(android.R.string.cancel,(dialog,which)->finish())
                .setPositiveButton("Remove account",(dialog,which)->remove())
                .setOnCancelListener(dialog->finish()).show();
    }
    private boolean sameIncarnation(Account account) {
        return account!=null&&account.getAccessId()!=null&&incarnation.equals(AccountsSettingsContract.fingerprint("account-incarnation",UserHandle.myUserId(),key,account.getAccessId()));
    }
    private void remove() {
        if(pending||!currentUser()) {finish();return;}
        Account account=backend.resolveConfirmationAccount(key);if(!sameIncarnation(account)||!backend.canRemove(account)) {message("This account is unavailable or removal is restricted.");return;}
        pending=true;
        progress=new AlertDialog.Builder(this).setTitle("Removing account…")
                .setMessage("Waiting for the account provider. You can return to Settings and check its current state.")
                .setPositiveButton("Return to Settings",(dialog,which)->finish()).setOnCancelListener(dialog->finish()).show();
        try {
            AccountManager.get(this).removeAccount(account,this,future->{
                if(isFinishing()||isDestroyed()) return;
                boolean removed=false;
                try {removed=future.getResult().getBoolean(AccountManager.KEY_BOOLEAN_RESULT,false);}catch(Exception unavailable) {}
                pending=false;if(progress!=null) progress.dismiss();
                if(removed) {setResult(RESULT_OK);finish();}else {message("Account removal was cancelled or could not be completed. Check Settings for its current state.");}
            },null);
        }catch(SecurityException unavailable) {pending=false;if(progress!=null) progress.dismiss();message("Android did not allow account removal.");}
    }
    private static String display(String value) {
        StringBuilder out=new StringBuilder();value.codePoints().limit(512).forEach(codepoint->out.appendCodePoint(Character.isISOControl(codepoint)?' ':codepoint));return out.toString();
    }
    private void message(String message) {
        new AlertDialog.Builder(this).setTitle("Account removal").setMessage(message)
                .setPositiveButton(android.R.string.ok,(dialog,which)->finish()).setOnCancelListener(dialog->finish()).show();
    }
    @Override public void onSaveInstanceState(Bundle out) {out.putBoolean("pending",pending);super.onSaveInstanceState(out);}
}
