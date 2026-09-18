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
    /** The person's own order of the home page's icons (hosted and Android ids alike); ids missing here keep the default order after the listed ones. */
    private ArrayList<String> order=new ArrayList<>();
    /** The person's app pairs as {name, apps:[a,b]} once edited (null: the shell's seeds), and the tiles taken off the page. */
    private JSONArray pairs=null;
    private ArrayList<String> hiddenTiles=new ArrayList<>();
    /** The portrait grid's columns, 4 or 5 (0: the shell's default). */
    private int columns=0;

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
        order=stored.has("order")?read(stored.getJSONArray("order"),256,true):new ArrayList<>();
        order.removeIf(String::isEmpty);
        pairs=stored.has("pairs")?stored.getJSONArray("pairs"):null;
        hiddenTiles=stored.has("hidden_tiles")?read(stored.getJSONArray("hidden_tiles"),16,true):new ArrayList<>();
        hiddenTiles.removeIf(String::isEmpty);
        columns=stored.optInt("columns",0);
        if(columns!=0 && (columns<4 || columns>5)) columns=0;
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
    private JSONObject model(ArrayList<String> favorites,ArrayList<String> dock,ArrayList<String> hidden,ArrayList<String> order) throws JSONException {
        JSONObject model=new JSONObject().put("version",2).put("favorites",new JSONArray(favorites)).put("dock",new JSONArray(dock)).put("hidden_hosted",new JSONArray(hidden)).put("order",new JSONArray(order))
                .put("hidden_tiles",new JSONArray(hiddenTiles)).put("columns",columns);
        if(pairs!=null) model.put("pairs",pairs);
        return model;
    }
    public JSONObject snapshot() throws JSONException {return model(favorites,dock,hiddenHosted,order);}
    /** The whole list of pairs after an edit: names of at most 32 characters, two distinct hosted apps each, at most 16 pairs. */
    public void setPairs(JSONArray next) throws IOException,JSONException {
        synchronized(IO_LOCK) {
        reload();
        if(next.length()>16) throw new IllegalArgumentException("Too many pairs");
        for(int index=0;index<next.length();index++) {
            JSONObject pair=next.getJSONObject(index);
            String name=pair.getString("name");JSONArray apps=pair.getJSONArray("apps");
            if(name.isEmpty() || name.length()>32 || apps.length()<2 || apps.length()>8) throw new IllegalArgumentException("Invalid pair");
            HashSet<String> members=new HashSet<>();
            for(int a=0;a<apps.length();a++) if(!isHosted(apps.getString(a)) || !members.add(apps.getString(a))) throw new IllegalArgumentException("Invalid pair app");
        }
        pairs=next;
        persist();
        }
    }
    /** A tile taken off the home page (or put back); `null` puts every tile back. */
    public void hideTile(String app,boolean hidden) throws IOException,JSONException {
        synchronized(IO_LOCK) {
        reload();
        if(app==null) hiddenTiles.clear();
        else {
            if(!isHosted(app)) throw new IllegalArgumentException("Invalid tile");
            hiddenTiles.remove(app);
            if(hidden) {if(hiddenTiles.size()>=16) throw new IllegalArgumentException("Too many hidden tiles");hiddenTiles.add(app);}
        }
        persist();
        }
    }
    public void setColumns(int next) throws IOException,JSONException {
        synchronized(IO_LOCK) {
        reload();
        if(next!=0 && (next<4 || next>5)) throw new IllegalArgumentException("Invalid grid");
        columns=next;
        persist();
        }
    }
    private void persist() throws IOException,JSONException {
        write(model(favorites,dock,hiddenHosted,order).toString().getBytes(StandardCharsets.UTF_8));
    }
    /** The whole home order after a drag: every id checked, no duplicates, at most 256. */
    public void reorder(ArrayList<String> next) throws IOException,JSONException {
        synchronized(IO_LOCK) {
        reload();
        if(next.size()>256) throw new IllegalArgumentException("Too many placements");
        HashSet<String> seen=new HashSet<>();
        for(String id:next) {requireIdentity(id,true);if(!seen.add(id)) throw new IllegalArgumentException("Duplicate placement");}
        save(new ArrayList<>(favorites),new ArrayList<>(dock),new ArrayList<>(hiddenHosted),next);
        }
    }
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
        save(nextFavorites,nextDock,nextHidden,new ArrayList<>(order));
    }
    private void save(ArrayList<String> nextFavorites,ArrayList<String> nextDock,ArrayList<String> nextHidden,ArrayList<String> nextOrder) throws IOException,JSONException {
        if(nextFavorites.equals(favorites) && nextDock.equals(dock) && nextHidden.equals(hiddenHosted) && nextOrder.equals(order)) return;
        write(model(nextFavorites,nextDock,nextHidden,nextOrder).toString().getBytes(StandardCharsets.UTF_8));
        favorites=nextFavorites;dock=nextDock;hiddenHosted=nextHidden;order=nextOrder;
    }
    private void write(byte[] bytes) throws IOException {
        if(bytes.length>MAX_BYTES) throw new IllegalArgumentException("Placements exceed storage limit");
        FileOutputStream stream=null;
        try {
            stream=file.startWrite();stream.write(bytes);file.finishWrite(stream);stream=null;
            try(FileInputStream input=file.openRead()) {
                if(!java.util.Arrays.equals(bytes,readBytes(input))) throw new IOException("Placement write not retained");
            }
        } catch(IOException|RuntimeException e) {if(stream!=null) file.failWrite(stream);throw e;}
    }
}
