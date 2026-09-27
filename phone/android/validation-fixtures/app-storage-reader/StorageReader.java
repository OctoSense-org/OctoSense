package dev.makepad.octosense.storagereaderfixture;
/** Read-only ordered receiver: unlike instrumentation, it never force-stops the grantee. */
public final class StorageReader extends android.content.BroadcastReceiver {
 @Override public void onReceive(android.content.Context context,android.content.Intent intent){
  if(!"dev.makepad.octosense.storagefixture.COMMAND".equals(intent.getAction())||!"state".equals(intent.getStringExtra("operation"))){setResultCode(0);return;}
  android.net.Uri uri=android.net.Uri.parse("content://dev.makepad.octosense.storagefixture.probe/data");
  boolean granted=context.checkUriPermission(uri,android.os.Process.myPid(),android.os.Process.myUid(),android.content.Intent.FLAG_GRANT_READ_URI_PERMISSION)==android.content.pm.PackageManager.PERMISSION_GRANTED;
  String json="{\"uri_granted\":"+granted+"}";
  setResultCode(1);setResultData(android.util.Base64.encodeToString(json.getBytes(java.nio.charset.StandardCharsets.UTF_8),android.util.Base64.NO_WRAP));
 }
}
