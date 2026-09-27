package dev.makepad.octosense.storagefixture;
public final class StorageFixtureProvider extends android.content.ContentProvider {
 @Override public boolean onCreate(){return true;}
 @Override public android.os.ParcelFileDescriptor openFile(android.net.Uri uri,String mode)throws java.io.FileNotFoundException{if(!"r".equals(mode)||!"/data".equals(uri.getPath()))throw new java.io.FileNotFoundException();return android.os.ParcelFileDescriptor.open(new java.io.File(getContext().getFilesDir(),"durable.bin"),android.os.ParcelFileDescriptor.MODE_READ_ONLY);}
 @Override public String getType(android.net.Uri uri){return "application/octet-stream";}
 @Override public android.database.Cursor query(android.net.Uri u,String[] p,String s,String[] a,String sort){return null;}
 @Override public android.net.Uri insert(android.net.Uri u,android.content.ContentValues v){throw new UnsupportedOperationException();}
 @Override public int delete(android.net.Uri u,String s,String[] a){throw new UnsupportedOperationException();}
 @Override public int update(android.net.Uri u,android.content.ContentValues v,String s,String[] a){throw new UnsupportedOperationException();}
}
