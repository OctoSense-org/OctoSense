package dev.makepad.octosense;

import android.content.ComponentName;
import android.util.AtomicFile;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.HashSet;
import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

/** Home-owned placement state. Construct, read and write only on a worker.
 * Unavailable packages/profiles retain their identities until the user removes
 * them. A corrupt or future-version file is never silently overwritten. */
public final class LauncherPlacements {
    private static final String[] DEFAULT_DOCK={"browser","files","photos","terminal"};
    private static final int MAX_BYTES=192*1024;
    // Both Home and the native pin Activity mutate this journal from workers.
    // Reload under one process-wide IO lock to avoid stale writers losing edits.
    // No caller may construct or mutate a placement store on the UI thread.
    private static final Object IO_LOCK=new Object();
    private final AtomicFile file;
    private ArrayList<String> favorites=new ArrayList<>();
    private ArrayList<String> dock=new ArrayList<>();
    private ArrayList<String> hiddenHosted=new ArrayList<>();

    public LauncherPlacements(File path) throws IOException,JSONException {
        file=new AtomicFile(path);
        synchronized(IO_LOCK) {reload();}
    }
    private void reload() throws IOException,JSONException {
        File path=file.getBaseFile();
        byte[] bytes;
        try(FileInputStream input=file.openRead()) {
            bytes=readBytes(input);
        } catch(java.io.FileNotFoundException e) {
            if(!path.exists() && !new File(path.getPath()+".bak").exists()) {
                favorites=new ArrayList<>();hiddenHosted=new ArrayList<>();dock=new ArrayList<>();for(String id:DEFAULT_DOCK) dock.add(id);return;
            }
            throw e;
        }
        JSONObject stored=new JSONObject(new String(bytes,StandardCharsets.UTF_8));
        int version=stored.getInt("version");
        if(version!=1 && version!=2) throw new IOException("Unsupported placement version");
        favorites=read(stored.getJSONArray("favorites"),128,false);
        dock=read(stored.getJSONArray("dock"),4,true);
        if(dock.size()!=4) throw new IOException("Invalid dock size");
        hiddenHosted=version==1?new ArrayList<>():read(stored.getJSONArray("hidden_hosted"),128,true);
        for(String id:hiddenHosted) if(!isHosted(id)) throw new IOException("Invalid hidden hosted identity");
        // A v1 file is read without rewriting it. Its original dock grammar
        // remains strict; v2 adds hosted IDs and deliberately empty slots.
        if(version==1) for(String id:dock) if(id.isEmpty() || (isHosted(id)&&!java.util.Arrays.asList(DEFAULT_DOCK).contains(id)))
            throw new IOException("Invalid v1 dock identity");
    }
    private static byte[] readBytes(FileInputStream input) throws IOException {
        java.io.ByteArrayOutputStream output=new java.io.ByteArrayOutputStream();
        byte[] buffer=new byte[4096];int count;
        while((count=input.read(buffer))!=-1) {
            if(output.size()+count>MAX_BYTES) throw new IOException("Placement file exceeds limit");
            output.write(buffer,0,count);
        }
        return output.toByteArray();
    }
    private static ArrayList<String> read(JSONArray values,int limit,boolean hosted) throws JSONException {
        if(values.length()>limit) throw new IllegalArgumentException("Too many placements");
        ArrayList<String> result=new ArrayList<>();HashSet<String> seen=new HashSet<>();
        for(int index=0;index<values.length();index++) {
            String id=values.getString(index);
            if(!(hosted&&id.isEmpty())) requireIdentity(id,hosted);
            if(!id.isEmpty()&&!seen.add(id)) throw new IllegalArgumentException("Duplicate placement");
            result.add(id);
        }
        return result;
    }
    private static void requireIdentity(String id,boolean hosted) {
        if(id==null || id.length()>1024) throw new IllegalArgumentException("Invalid placement identity");
        if(hosted && isHosted(id)) return;
        String prefix=id.startsWith("android:")?"android:":"android-shortcut:";
        if(!id.startsWith(prefix)) throw new IllegalArgumentException("Invalid placement identity");
        String value=id.substring(prefix.length());int separator=value.indexOf(':');
        if(separator<=0) throw new IllegalArgumentException("Missing profile identity");
        long serial=Long.parseLong(value.substring(0,separator));
        if(serial<0) throw new IllegalArgumentException("Invalid profile identity");
        String target=value.substring(separator+1);
        if(prefix.equals("android:")) {
            if(ComponentName.unflattenFromString(target)==null) throw new IllegalArgumentException("Invalid app component");
        } else {
            int split=target.indexOf(':');
            if(split<=0 || split==target.length()-1) throw new IllegalArgumentException("Invalid shortcut identity");
        }
    }
    public static boolean isHosted(String id) {
        return id!=null && id.matches("[a-z][a-z0-9_-]{0,127}");
    }
    private static JSONObject model(ArrayList<String> favorites,ArrayList<String> dock,ArrayList<String> hidden) throws JSONException {
        return new JSONObject().put("version",2).put("favorites",new JSONArray(favorites)).put("dock",new JSONArray(dock)).put("hidden_hosted",new JSONArray(hidden));
    }
    public JSONObject snapshot() throws JSONException {return model(favorites,dock,hiddenHosted);}
    public boolean isFavorite(String id) {return isHosted(id)?!hiddenHosted.contains(id):favorites.contains(id);}
    public boolean isDocked(String id) {return dock.contains(id);}
    public boolean isPlaced(String id) {return isFavorite(id)||isDocked(id);}
    public void favorite(String id,boolean pinned) throws IOException,JSONException {
        synchronized(IO_LOCK) {
        reload();
        requireIdentity(id,true);ArrayList<String> next=new ArrayList<>(favorites),hidden=new ArrayList<>(hiddenHosted);
        if(isHosted(id)) {
            if(pinned) hidden.remove(id);
            else if(!hidden.contains(id)) {
                if(hidden.size()>=128) throw new IllegalArgumentException("Hidden hosted limit reached");
                hidden.add(id);
            }
        } else if(pinned && !next.contains(id)) {
            if(next.size()>=128) throw new IllegalArgumentException("Favorite limit reached");
            next.add(id);
        } else if(!pinned) next.remove(id);
        save(next,new ArrayList<>(dock),hidden);
        }
    }
    public void dock(String id,int slot) throws IOException,JSONException {
        synchronized(IO_LOCK) {
        reload();
        requireIdentity(id,true);
        if(slot<0 || slot>3) throw new IllegalArgumentException("Invalid dock slot");
        ArrayList<String> next=new ArrayList<>(dock);
        removeFromDock(next,id);
        next.set(slot,id);save(new ArrayList<>(favorites),next,new ArrayList<>(hiddenHosted));
        }
    }
    public void undock(String id) throws IOException,JSONException {
        synchronized(IO_LOCK) {
        reload();
        requireIdentity(id,true);ArrayList<String> next=new ArrayList<>(dock);
        removeFromDock(next,id);
        save(new ArrayList<>(favorites),next,new ArrayList<>(hiddenHosted));
        }
    }
    private static void removeFromDock(ArrayList<String> next,String id) {
        for(int index=0;index<4;index++) if(next.get(index).equals(id)) next.set(index,"");
    }
    private void save(ArrayList<String> nextFavorites,ArrayList<String> nextDock,ArrayList<String> nextHidden) throws IOException,JSONException {
        if(nextFavorites.equals(favorites) && nextDock.equals(dock) && nextHidden.equals(hiddenHosted)) return;
        byte[] bytes=model(nextFavorites,nextDock,nextHidden).toString().getBytes(StandardCharsets.UTF_8);
        if(bytes.length>MAX_BYTES) throw new IllegalArgumentException("Placements exceed storage limit");
        FileOutputStream stream=null;
        try {
            stream=file.startWrite();stream.write(bytes);file.finishWrite(stream);stream=null;
            try(FileInputStream input=file.openRead()) {
                if(!java.util.Arrays.equals(bytes,readBytes(input))) throw new IOException("Placement write not retained");
            }
            favorites=nextFavorites;dock=nextDock;hiddenHosted=nextHidden;
        } catch(IOException|RuntimeException e) {if(stream!=null) file.failWrite(stream);throw e;}
    }
}
