package dev.makepad.octosense.storagefixture;
import android.app.*;import android.app.job.*;import android.content.*;import android.content.pm.PackageManager;import android.net.Uri;import android.os.*;import java.io.*;import org.json.JSONObject;
/** Synthetic owned files only; public readback after native Settings operations. */
public final class StorageFixture extends BroadcastReceiver {
 private File file(File dir,String name){if(dir==null)throw new IllegalStateException("Fixture storage is unavailable");return new File(dir,name);}
 private void write(File target,int size)throws Exception{target.getParentFile().mkdirs();try(FileOutputStream out=new FileOutputStream(target)){byte[] block=new byte[4096];for(int n=0;n<size;n+=block.length)out.write(block,0,Math.min(block.length,size-n));}}
 @Override public void onReceive(Context c,Intent intent){try{if(!"dev.makepad.octosense.storagefixture.COMMAND".equals(intent.getAction()))throw new IllegalArgumentException("Unknown fixture action");String operation=intent.getStringExtra("operation");String pkg=c.getPackageName();if(!pkg.equals("dev.makepad.octosense.storagefixture")&&!pkg.equals("dev.makepad.octosense.storagespacefixture"))throw new SecurityException("Not a storage fixture");
 File data=file(c.getFilesDir(),"durable.bin"),cache=file(c.getCacheDir(),"temporary.bin"),code=file(c.getCodeCacheDir(),"compiled.bin"),external=file(c.getExternalFilesDir(null),"durable.bin"),externalCache=file(c.getExternalCacheDir(),"temporary.bin");
 if(operation.equals("seed")){write(data,1048576);write(cache,2097152);write(code,524288);write(external,1048576);write(externalCache,1048576);
 c.getSystemService(NotificationManager.class).createNotificationChannel(new NotificationChannel("storage_probe","Synthetic storage probe",NotificationManager.IMPORTANCE_LOW));
 c.getSystemService(JobScheduler.class).schedule(new JobInfo.Builder(981,new ComponentName(pkg,"dev.makepad.octosense.storagefixture.StorageFixtureJob")).setMinimumLatency(86400000).build());
 PendingIntent alarm=PendingIntent.getBroadcast(c,982,new Intent(c,StorageFixtureAlarm.class),PendingIntent.FLAG_UPDATE_CURRENT|PendingIntent.FLAG_IMMUTABLE);c.getSystemService(AlarmManager.class).set(AlarmManager.ELAPSED_REALTIME,SystemClock.elapsedRealtime()+86400000,alarm);
 if(pkg.equals("dev.makepad.octosense.storagefixture"))c.grantUriPermission("dev.makepad.octosense.storagereaderfixture",Uri.parse("content://dev.makepad.octosense.storagefixture.probe/data"),Intent.FLAG_GRANT_READ_URI_PERMISSION);
 }else if(operation.equals("grow_cache")){write(cache,3145728);}else if(!operation.equals("state"))throw new IllegalArgumentException("Unknown fixture operation");
 JSONObject state=new JSONObject().put("data",data.isFile()?data.length():0).put("cache",cache.isFile()?cache.length():0).put("code_cache",code.isFile()?code.length():0).put("external_data",external.isFile()?external.length():0).put("external_cache",externalCache.isFile()?externalCache.length():0)
 .put("camera",c.checkSelfPermission("android.permission.CAMERA")==PackageManager.PERMISSION_GRANTED).put("channel",c.getSystemService(NotificationManager.class).getNotificationChannel("storage_probe")!=null)
 .put("job",c.getSystemService(JobScheduler.class).getPendingJob(981)!=null)
 .put("alarm_token",PendingIntent.getBroadcast(c,982,new Intent(c,StorageFixtureAlarm.class),PendingIntent.FLAG_NO_CREATE|PendingIntent.FLAG_IMMUTABLE)!=null);
 setResultCode(1);setResultData(android.util.Base64.encodeToString(state.toString().getBytes(java.nio.charset.StandardCharsets.UTF_8),android.util.Base64.NO_WRAP));
 }catch(Exception error){setResultCode(0);setResultData(error.getClass().getSimpleName());}}
}
