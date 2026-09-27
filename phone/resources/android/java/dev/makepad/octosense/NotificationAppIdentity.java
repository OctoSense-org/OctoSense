package dev.makepad.octosense;

import android.content.Context;
import android.content.pm.PackageInfo;
import android.content.pm.PackageManager;
import android.graphics.Bitmap;
import android.graphics.Canvas;
import android.graphics.drawable.Drawable;
import android.os.Bundle;
import android.util.AtomicFile;
import dev.makepad.octosense.contracts.Protocol;
import java.io.File;
import java.io.FileOutputStream;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;

/** Worker-owned presentation metadata. The authenticated bridge already limits
 * notifications to this Android user. Labels/icons come from PackageManager,
 * never from a notification's self-declared app label or arbitrary file URI. */
public final class NotificationAppIdentity {
    private final Context context;
    private final File directory;
    private final HashMap<String,String[]> cached=new HashMap<>();
    public NotificationAppIdentity(Context context,File directory) {this.context=context;this.directory=directory;}
    public void invalidate() {cached.clear();}
    /** JNI counts UTF-8 bytes, while the bridge budget counts Parcel bytes.
     * Keep recent notifications and explicitly report bounded-snapshot loss. */
    public static org.json.JSONObject fitSnapshot(org.json.JSONObject event) throws org.json.JSONException {
        org.json.JSONObject state=event.getJSONObject("state");
        org.json.JSONArray notices=state.getJSONArray("notifications");
        while(event.toString().getBytes(StandardCharsets.UTF_8).length>Protocol.MAX_PACKET_BYTES && notices.length()>0) {
            notices.remove(0);state.put("notifications_truncated",true);
        }
        if(event.toString().getBytes(StandardCharsets.UTF_8).length>Protocol.MAX_PACKET_BYTES)
            throw new org.json.JSONException("Snapshot metadata exceeds JNI limit");
        return event;
    }

    public Bundle decorate(Bundle original) {
        Bundle state=new Bundle(original);
        ArrayList<Bundle> incoming=original.getParcelableArrayList("notifications");
        ArrayList<Bundle> outgoing=new ArrayList<>();
        HashSet<String> packages=new HashSet<>(),files=new HashSet<>();
        if(incoming!=null) for(Bundle entry:incoming) {
            if(outgoing.size()>=Protocol.MAX_NOTIFICATIONS) break;
            String packageName=entry.getString("package","");
            if(packageName.isEmpty()||packageName.length()>255) continue;
            String[] identity=cached.get(packageName);
            if(identity==null) {identity=resolve(packageName);cached.put(packageName,identity);}
            Bundle model=new Bundle(entry);
            model.putString("app_label",identity[0]);model.putString("app_icon",identity[1]);
            outgoing.add(model);packages.add(packageName);
            if(!identity[1].isEmpty()) files.add(identity[1]);
        }
        cached.keySet().retainAll(packages);
        File[] old=directory.listFiles();
        if(old!=null) for(File file:old) if(!files.contains(file.getAbsolutePath())) file.delete();
        state.putParcelableArrayList("notifications",outgoing);
        return state;
    }
    private String[] resolve(String packageName) {
        String label=packageName,path="";
        try {
            PackageManager manager=context.getPackageManager();
            PackageInfo info=manager.getPackageInfo(packageName,0);
            if(info.applicationInfo==null) return new String[]{label,path};
            CharSequence name=manager.getApplicationLabel(info.applicationInfo);
            if(name!=null&&!name.toString().trim().isEmpty()) {
                label=name.toString().trim();
                // Bound presentation strings without splitting a UTF-16 pair.
                if(label.codePointCount(0,label.length())>160) label=label.substring(0,label.offsetByCodePoints(0,160));
            }
            int density=context.getResources().getDisplayMetrics().densityDpi;
            String key=packageName+":"+info.applicationInfo.uid+":"+info.lastUpdateTime+":"+density+":"+
                    context.getResources().getConfiguration().getLocales().toLanguageTags();
            byte[] digest=MessageDigest.getInstance("SHA-256").digest(key.getBytes(StandardCharsets.UTF_8));
            StringBuilder hash=new StringBuilder();for(byte value:digest) hash.append(String.format("%02x",value&255));
            File target=new File(directory,hash+".png");
            if(!target.isFile()) {
                if(!directory.isDirectory()&&!directory.mkdirs()) return new String[]{label,path};
                Drawable icon=manager.getUserBadgedIcon(manager.getApplicationIcon(info.applicationInfo),android.os.Process.myUserHandle());
                int size=Math.min(192,Math.max(48,Math.round(48*context.getResources().getDisplayMetrics().density)));
                Bitmap bitmap=Bitmap.createBitmap(size,size,Bitmap.Config.ARGB_8888);
                AtomicFile file=new AtomicFile(target);FileOutputStream output=null;
                try {
                    icon.setBounds(0,0,size,size);icon.draw(new Canvas(bitmap));
                    output=file.startWrite();
                    if(!bitmap.compress(Bitmap.CompressFormat.PNG,100,output)) throw new java.io.IOException("Icon encoding failed");
                    file.finishWrite(output);output=null;
                } finally {bitmap.recycle();if(output!=null) file.failWrite(output);}
            }
            path=target.getAbsolutePath();
        } catch(Exception ignored) {
            // A removed/hidden package or failed icon cache must not hide the
            // notification. Keep its verified package identity as the fallback.
        }
        return new String[]{label,path};
    }
}
