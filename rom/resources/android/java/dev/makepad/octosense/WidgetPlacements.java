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

/** Worker-owned journal. Persist pending consent/configuration before starting
 * external activities; IDs not in a successfully read journal can be reclaimed. */
public final class WidgetPlacements {
    public static final int MAX_WIDGETS=16;
    private static final int MAX_BYTES=64*1024;
    public static final class Entry {
        public final int id,width,height;
        public final String provider;
        public final long user;
        public Entry(int id,String provider,long user,int width,int height) {
            if(id<=0 || provider==null || provider.length()>512 || ComponentName.unflattenFromString(provider)==null
                    || user<0 || width<1 || height<1 || width>4096 || height>4096)
                throw new IllegalArgumentException("Invalid widget placement");
            this.id=id;this.provider=provider;this.user=user;this.width=width;this.height=height;
        }
        Entry(JSONObject value) throws JSONException {
            this(value.getInt("id"),value.getString("provider"),value.getLong("user"),value.getInt("width"),value.getInt("height"));
        }
        JSONObject json() throws JSONException {
            return new JSONObject().put("id",id).put("provider",provider).put("user",user).put("width",width).put("height",height);
        }
    }
    public static final class State {
        public final ArrayList<Entry> entries;
        public final Entry pending;
        public final String stage;
        public State(ArrayList<Entry> entries,Entry pending,String stage) {
            if(entries.size()>MAX_WIDGETS || (pending!=null && entries.size()>=MAX_WIDGETS)) throw new IllegalArgumentException("Widget limit reached");
            HashSet<Integer> ids=new HashSet<>();
            for(Entry entry:entries) if(!ids.add(entry.id)) throw new IllegalArgumentException("Duplicate widget ID");
            if(pending!=null && (!ids.add(pending.id) || !(stage.equals("bind")||stage.equals("configure")))) throw new IllegalArgumentException("Invalid pending widget");
            if(pending==null && !stage.isEmpty()) throw new IllegalArgumentException("Unexpected pending stage");
            this.entries=new ArrayList<>(entries);this.pending=pending;this.stage=stage;
        }
        public boolean owns(int id) {
            if(pending!=null && pending.id==id) return true;
            for(Entry entry:entries) if(entry.id==id) return true;
            return false;
        }
        public JSONObject json() throws JSONException {
            JSONArray rows=new JSONArray();for(Entry entry:entries) rows.put(entry.json());
            return new JSONObject().put("version",1).put("entries",rows).put("pending",pending==null?JSONObject.NULL:pending.json()).put("stage",stage);
        }
    }
    private final AtomicFile file;
    public WidgetPlacements(File path) {file=new AtomicFile(path);}
    public State read() throws IOException,JSONException {
        byte[] bytes;
        try(FileInputStream input=file.openRead()) {bytes=bytes(input);}
        catch(java.io.FileNotFoundException e) {
            if(!file.getBaseFile().exists()&&!new File(file.getBaseFile().getPath()+".bak").exists()) return new State(new ArrayList<>(),null,"");
            throw e;
        }
        JSONObject root=new JSONObject(new String(bytes,StandardCharsets.UTF_8));
        if(root.getInt("version")!=1) throw new IOException("Unsupported widget placement version");
        JSONArray rows=root.getJSONArray("entries");
        if(rows.length()>MAX_WIDGETS) throw new IOException("Too many widgets");
        ArrayList<Entry> entries=new ArrayList<>();for(int index=0;index<rows.length();index++) entries.add(new Entry(rows.getJSONObject(index)));
        return new State(entries,root.isNull("pending")?null:new Entry(root.getJSONObject("pending")),root.getString("stage"));
    }
    public void write(State state) throws IOException,JSONException {
        byte[] bytes=state.json().toString().getBytes(StandardCharsets.UTF_8);
        if(bytes.length>MAX_BYTES) throw new IOException("Widget placements exceed limit");
        FileOutputStream stream=null;
        try {
            stream=file.startWrite();stream.write(bytes);file.finishWrite(stream);stream=null;
            try(FileInputStream input=file.openRead()) {
                if(!java.util.Arrays.equals(bytes,bytes(input))) throw new IOException("Widget placements were not retained");
            }
        } catch(IOException|RuntimeException e) {if(stream!=null) file.failWrite(stream);throw e;}
    }
    private static byte[] bytes(FileInputStream input) throws IOException {
        java.io.ByteArrayOutputStream output=new java.io.ByteArrayOutputStream();byte[] buffer=new byte[4096];int count;
        while((count=input.read(buffer))!=-1) {
            if(output.size()+count>MAX_BYTES) throw new IOException("Widget placements exceed limit");
            output.write(buffer,0,count);
        }
        return output.toByteArray();
    }
}
