package dev.makepad.octosense;

import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.nio.file.Files;
import java.nio.file.LinkOption;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;
import java.security.MessageDigest;
import java.util.Arrays;

/** Local updater policy, independent of Android so rejection paths run on the JVM. */
final class HomeUpdatePolicy {
    static final String PACKAGE="dev.makepad.octosense";
    // Public standalone release identity, not a secret. ROM updates use their own service.
    static final String RELEASE_SIGNER="6bf3927755714edb325ea7fbdc7c7e8780f4a103589d3ee5926b982773f11e34";
    static final long MAX_BYTES=512L*1024*1024;

    static final class Package {
        final String name;
        final long version;
        final int minSdk;
        final String[] signers;
        Package(String name,long version,int minSdk,String[] signers) {
            this.name=name;this.version=version;this.minSdk=minSdk;
            this.signers=signers==null?new String[0]:signers.clone();
            Arrays.sort(this.signers);
        }
    }

    static boolean standalone(Package installed,boolean systemPackage) {
        return !systemPackage&&PACKAGE.equals(installed.name)&&installed.signers.length==1
            &&RELEASE_SIGNER.equals(installed.signers[0]);
    }

    static void validatePackage(Package installed,Package candidate,int sdk) throws IOException {
        if(candidate==null||!installed.name.equals(candidate.name)||!PACKAGE.equals(candidate.name))
            throw new IOException("package_mismatch");
        if(candidate.version<=installed.version)throw new IOException("not_newer");
        if(candidate.minSdk<=0||candidate.minSdk>sdk)throw new IOException("android_version_unsupported");
        if(installed.signers.length==0||candidate.signers.length==0||!Arrays.equals(installed.signers,candidate.signers))
            throw new IOException("signature_mismatch");
    }

    static String digest(byte[] value) throws Exception {
        return hex(MessageDigest.getInstance("SHA-256").digest(value));
    }

    static String hex(byte[] value) {
        StringBuilder out=new StringBuilder(value.length*2);
        for(byte b:value)out.append(Character.forDigit((b>>>4)&15,16)).append(Character.forDigit(b&15,16));
        return out.toString();
    }

    /** Resolve only a regular APK below our private cache; reject every symlink component.
     * Android itself can alias /data/user/0 to /data/data, so first canonicalize the
     * OS-provided cache parent, then enforce the caller's path below that trusted root. */
    static File checkedFile(File cacheRoot,String supplied) throws IOException {
        Path root=cacheRoot.toPath().toAbsolutePath().normalize();
        if(Files.isSymbolicLink(root)||!Files.isDirectory(root,LinkOption.NOFOLLOW_LINKS))
            throw new IOException("invalid_cache");
        root=cacheRoot.getCanonicalFile().toPath();
        Path candidate=new File(supplied).toPath();
        if(!candidate.isAbsolute()||!candidate.equals(candidate.normalize())||!candidate.startsWith(root)
                ||candidate.equals(root)||!candidate.getFileName().toString().endsWith(".apk"))
            throw new IOException("invalid_path");
        Path cursor=root;
        for(Path component:root.relativize(candidate)) {
            cursor=cursor.resolve(component);
            if(Files.isSymbolicLink(cursor))throw new IOException("invalid_path");
        }
        if(!Files.isRegularFile(candidate,LinkOption.NOFOLLOW_LINKS))throw new IOException("invalid_path");
        long size=Files.size(candidate);
        if(size<=0||size>MAX_BYTES)throw new IOException("invalid_size");
        return candidate.toFile();
    }

    /** Hash exactly the bytes staged for PackageInstaller as well as the initial file.
     * Any mutation between metadata inspection and copying abandons the session. */
    static long copyVerified(File source,OutputStream sink,String expected) throws Exception {
        return copyVerified(source,sink,expected,() -> false);
    }
    static long copyVerified(File source,OutputStream sink,String expected,java.util.function.BooleanSupplier cancelled) throws Exception {
        if(expected==null||!expected.matches("[0-9a-f]{64}"))throw new IOException("invalid_digest");
        MessageDigest digest=MessageDigest.getInstance("SHA-256");
        long total=0;
        try(InputStream in=Files.newInputStream(source.toPath(),StandardOpenOption.READ,LinkOption.NOFOLLOW_LINKS)) {
            byte[] buffer=new byte[64*1024];
            int count;
            while((count=in.read(buffer))!=-1) {
                if(Thread.currentThread().isInterrupted()||cancelled.getAsBoolean())throw new IOException("cancelled");
                total+=count;
                if(total>MAX_BYTES)throw new IOException("invalid_size");
                digest.update(buffer,0,count);
                if(sink!=null)sink.write(buffer,0,count);
            }
        }
        if(total<=0)throw new IOException("invalid_size");
        if(!hex(digest.digest()).equals(expected))throw new IOException("digest_mismatch");
        return total;
    }
}
