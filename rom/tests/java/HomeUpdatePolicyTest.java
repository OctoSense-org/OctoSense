package dev.makepad.octosense;

import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Arrays;

public final class HomeUpdatePolicyTest {
    interface Checked {void run() throws Exception;}
    private static void check(boolean value,String message) {if(!value)throw new AssertionError(message);}
    private static void rejects(String reason,Checked action) throws Exception {
        try {action.run();throw new AssertionError("accepted "+reason);}catch(IOException denied){check(reason.equals(denied.getMessage()),denied.toString());}
    }
    private static HomeUpdatePolicy.Package pkg(String name,long version,int sdk,String...signers) {
        return new HomeUpdatePolicy.Package(name,version,sdk,signers);
    }
    private static HomeUpdatePolicy.Package installed() {return pkg(HomeUpdatePolicy.PACKAGE,10,33,HomeUpdatePolicy.RELEASE_SIGNER);}
    public static void main(String[] args) throws Exception {
        Path root=Files.createTempDirectory("home-updates-test").toRealPath();
        try {
            switch(args[0]) {
            case "newer":
                HomeUpdatePolicy.validatePackage(installed(),pkg(HomeUpdatePolicy.PACKAGE,11,33,HomeUpdatePolicy.RELEASE_SIGNER),35);break;
            case "identity":
                check(HomeUpdatePolicy.standalone(installed(),false),"public install rejected");
                check(!HomeUpdatePolicy.standalone(installed(),true),"system app accepted");
                check(!HomeUpdatePolicy.standalone(pkg(HomeUpdatePolicy.PACKAGE,10,33,"other"),false),"ROM signer accepted");
                check(!HomeUpdatePolicy.standalone(pkg("test.isolated",10,33,HomeUpdatePolicy.RELEASE_SIGNER),false),"isolated app accepted");
                check(!HomeUpdatePolicy.standalone(pkg(HomeUpdatePolicy.PACKAGE,10,33),false),"unsigned accepted");break;
            case "package":
                rejects("package_mismatch",() -> HomeUpdatePolicy.validatePackage(installed(),pkg("dev.makepad.octosense.bridge",11,33,HomeUpdatePolicy.RELEASE_SIGNER),35));
                rejects("package_mismatch",() -> HomeUpdatePolicy.validatePackage(installed(),null,35));break;
            case "version":
                for(long version:new long[]{-1,0,9,10})rejects("not_newer",() -> HomeUpdatePolicy.validatePackage(installed(),pkg(HomeUpdatePolicy.PACKAGE,version,33,HomeUpdatePolicy.RELEASE_SIGNER),35));break;
            case "sdk":
                for(int sdk:new int[]{-1,0,36})rejects("android_version_unsupported",() -> HomeUpdatePolicy.validatePackage(installed(),pkg(HomeUpdatePolicy.PACKAGE,11,sdk,HomeUpdatePolicy.RELEASE_SIGNER),35));break;
            case "signature":
                rejects("signature_mismatch",() -> HomeUpdatePolicy.validatePackage(installed(),pkg(HomeUpdatePolicy.PACKAGE,11,33,"other"),35));
                rejects("signature_mismatch",() -> HomeUpdatePolicy.validatePackage(installed(),pkg(HomeUpdatePolicy.PACKAGE,11,33),35));
                rejects("signature_mismatch",() -> HomeUpdatePolicy.validatePackage(installed(),pkg(HomeUpdatePolicy.PACKAGE,11,33,HomeUpdatePolicy.RELEASE_SIGNER,"other"),35));
                HomeUpdatePolicy.validatePackage(pkg(HomeUpdatePolicy.PACKAGE,10,33,"a","b"),pkg(HomeUpdatePolicy.PACKAGE,11,33,"b","a"),35);break;
            case "nested": {
                Path apk=Files.createDirectories(root.resolve("version/platform")).resolve("home.apk");Files.write(apk,new byte[]{1});
                check(HomeUpdatePolicy.checkedFile(root.toFile(),apk.toString()).equals(apk.toFile()),"valid nested file rejected");break;
            }
            case "escape": {
                Path apk=root.resolve("home.apk");Files.write(apk,new byte[]{1});
                rejects("invalid_path",() -> HomeUpdatePolicy.checkedFile(root.toFile(),"home.apk"));
                rejects("invalid_path",() -> HomeUpdatePolicy.checkedFile(root.toFile(),root.resolve("../home.apk").toString()));
                rejects("invalid_path",() -> HomeUpdatePolicy.checkedFile(root.toFile(),root.resolve("child/../home.apk").toString()));
                rejects("invalid_path",() -> HomeUpdatePolicy.checkedFile(root.toFile(),root+"-sibling/home.apk"));
                rejects("invalid_path",() -> HomeUpdatePolicy.checkedFile(root.toFile(),root.toString()));
                rejects("invalid_path",() -> HomeUpdatePolicy.checkedFile(root.toFile(),root.resolve("home.zip").toString()));break;
            }
            case "symlink": {
                Path real=Files.createDirectories(root.resolve("real"));Path apk=real.resolve("home.apk");Files.write(apk,new byte[]{1});
                Path link=root.resolve("linked.apk");Files.createSymbolicLink(link,apk);
                rejects("invalid_path",() -> HomeUpdatePolicy.checkedFile(root.toFile(),link.toString()));
                Path dir=root.resolve("link-dir");Files.createSymbolicLink(dir,real);
                rejects("invalid_path",() -> HomeUpdatePolicy.checkedFile(root.toFile(),dir.resolve("home.apk").toString()));
                rejects("invalid_cache",() -> HomeUpdatePolicy.checkedFile(dir.toFile(),apk.toString()));break;
            }
            case "size": {
                Path apk=root.resolve("home.apk");Files.write(apk,new byte[0]);
                rejects("invalid_size",() -> HomeUpdatePolicy.checkedFile(root.toFile(),apk.toString()));
                try(java.io.RandomAccessFile sparse=new java.io.RandomAccessFile(apk.toFile(),"rw")){sparse.setLength(HomeUpdatePolicy.MAX_BYTES+1);}
                rejects("invalid_size",() -> HomeUpdatePolicy.checkedFile(root.toFile(),apk.toString()));break;
            }
            case "hash": {
                byte[] bytes="exact staged bytes".getBytes(java.nio.charset.StandardCharsets.UTF_8);
                Path apk=root.resolve("home.apk");Files.write(apk,bytes);String hash=HomeUpdatePolicy.digest(bytes);
                ByteArrayOutputStream sink=new ByteArrayOutputStream();
                check(HomeUpdatePolicy.copyVerified(apk.toFile(),sink,hash)==bytes.length,"size mismatch");
                check(Arrays.equals(bytes,sink.toByteArray()),"different bytes copied");
                rejects("invalid_digest",() -> HomeUpdatePolicy.copyVerified(apk.toFile(),null,"x"));
                rejects("invalid_digest",() -> HomeUpdatePolicy.copyVerified(apk.toFile(),null,hash.toUpperCase(java.util.Locale.ROOT)));
                Files.write(apk,new byte[]{1,2,3});
                rejects("digest_mismatch",() -> HomeUpdatePolicy.copyVerified(apk.toFile(),sink,hash));break;
            }
            case "cancelled_during_copy": {
                byte[] bytes=new byte[128*1024];Path apk=root.resolve("home.apk");Files.write(apk,bytes);
                java.util.concurrent.atomic.AtomicBoolean cancelled=new java.util.concurrent.atomic.AtomicBoolean();
                ByteArrayOutputStream sink=new ByteArrayOutputStream() {
                    @Override public void write(byte[] value,int start,int count) {super.write(value,start,count);cancelled.set(true);}
                };
                rejects("cancelled",() -> HomeUpdatePolicy.copyVerified(apk.toFile(),sink,HomeUpdatePolicy.digest(bytes),cancelled::get));
                check(sink.size()==64*1024,"cancelled copy wrote another chunk");break;
            }
            case "interrupted": {
                Path apk=root.resolve("home.apk");Files.write(apk,new byte[]{1});
                Thread.currentThread().interrupt();
                try {rejects("cancelled",() -> HomeUpdatePolicy.copyVerified(apk.toFile(),null,HomeUpdatePolicy.digest(new byte[]{1})));}
                finally {Thread.interrupted();}break;
            }
            default:throw new AssertionError("unknown case");
            }
        } finally {
            try(java.util.stream.Stream<Path> paths=Files.walk(root)) {
                paths.sorted(java.util.Comparator.reverseOrder()).forEach(path -> {try{Files.delete(path);}catch(IOException e){throw new RuntimeException(e);}});
            }
        }
    }
}
