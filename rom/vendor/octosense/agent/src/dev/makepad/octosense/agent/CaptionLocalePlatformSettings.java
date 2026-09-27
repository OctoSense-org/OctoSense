package dev.makepad.octosense.agent;

import android.Manifest;
import android.app.ActivityManager;
import android.app.KeyguardManager;
import android.content.Context;
import android.content.pm.PackageManager;
import android.os.Bundle;
import android.os.SystemClock;
import android.os.UserHandle;
import android.os.UserManager;
import android.provider.Settings;
import android.view.accessibility.CaptioningManager;
import com.android.internal.app.LocalePicker;
import dev.makepad.octosense.controls.CaptionLocaleSettings;
import java.security.SecureRandom;
import java.util.ArrayList;
import java.util.List;
import java.util.Objects;
import org.json.JSONArray;
import org.json.JSONObject;

/** Native caption language catalog and finite owner-user mutation. No caption enable writes. */
final class CaptionLocalePlatformSettings {
    private final Context context;private final CaptionLocaleSettings.Backend backend;private final SecureRandom random=new SecureRandom();
    CaptionLocalePlatformSettings(Context context){this.context=context;backend=new CaptionLocaleSettings.Backend(new CaptionLocaleSettings.Store(){
        @Override public CaptionLocaleSettings.State read(){
            CaptioningManager manager=context.getSystemService(CaptioningManager.class);if(manager==null)return null;
            String before=Settings.Secure.getString(context.getContentResolver(),Settings.Secure.ACCESSIBILITY_CAPTIONING_LOCALE);
            List<LocalePicker.LocaleInfo> nativeEntries=LocalePicker.getAllAssetLocales(context,false);List<CaptionLocaleSettings.LocaleEntry> entries=new ArrayList<>();
            if(nativeEntries==null||nativeEntries.size()>CaptionLocaleSettings.MAX_LOCALES)return null;
            for(LocalePicker.LocaleInfo info:nativeEntries)entries.add(new CaptionLocaleSettings.LocaleEntry(info.getLocale().toString(),info.toString()));
            String service=manager.getRawLocale(),after=Settings.Secure.getString(context.getContentResolver(),Settings.Secure.ACCESSIBILITY_CAPTIONING_LOCALE);
            return Objects.equals(before,after)?new CaptionLocaleSettings.State(after,service,entries):null;
        }
        @Override public boolean write(String value){return Settings.Secure.putString(context.getContentResolver(),Settings.Secure.ACCESSIBILITY_CAPTIONING_LOCALE,value);}
    },this::access,SystemClock::elapsedRealtime,this::nonce);}
    private String nonce(){byte[] bytes=new byte[32];random.nextBytes(bytes);StringBuilder out=new StringBuilder(64);for(byte value:bytes)out.append(Character.forDigit((value>>>4)&15,16)).append(Character.forDigit(value&15,16));return out.toString();}
    private CaptionLocaleSettings.Access access(){
        UserManager users=context.getSystemService(UserManager.class);KeyguardManager lock=context.getSystemService(KeyguardManager.class);
        if(UserHandle.myUserId()!=0||ActivityManager.getCurrentUser()!=0||users==null||!users.isUserUnlocked()||lock==null||lock.isKeyguardLocked())return CaptionLocaleSettings.Access.RESTRICTED;
        return context.checkSelfPermission(Manifest.permission.WRITE_SECURE_SETTINGS)==PackageManager.PERMISSION_GRANTED?CaptionLocaleSettings.Access.WRITABLE:CaptionLocaleSettings.Access.READ_ONLY;
    }
    Bundle snapshot(long request,String key,String query,int offset)throws Exception{
        if(request<=0)throw new IllegalArgumentException("Invalid caption language request");
        CaptionLocaleSettings.Snapshot snapshot=backend.snapshot(key,query,offset);
        JSONObject json=new JSONObject().put("schema",1).put("page_size",CaptionLocaleSettings.PAGE_SIZE).put("request_id",request).put("status",snapshot.reason).put("key",snapshot.key).put("current",snapshot.current).put("system_default",snapshot.systemDefault).put("custom",snapshot.custom).put("query",snapshot.query).put("offset",snapshot.offset).put("total",snapshot.total);
        JSONArray rows=new JSONArray();for(CaptionLocaleSettings.Row row:snapshot.rows)rows.put(new JSONObject().put("choice",row.choice).put("label",row.label).put("locale",row.locale).put("selected",row.selected));json.put("rows",rows);
        Bundle out=new Bundle();out.putBoolean("ok",true);out.putString("json",json.toString());return out;
    }
    String select(String key,String choice){return backend.select(key,choice);}
    void invalidate(){backend.invalidate();}
}
