import dev.makepad.octosense.AppsSettingsContract;

/** Script/native boundaries cannot select arbitrary intents, users or malformed package pages. */
public final class AppsSettingsContractTest {
    private static void rejects(Runnable action) {
        try {action.run();} catch(IllegalArgumentException expected) {return;}
        throw new AssertionError("Invalid Apps request accepted");
    }
    public static void main(String[] args) {
        if(AppsSettingsContract.Action.parse("language")!=AppsSettingsContract.Action.LANGUAGE)throw new AssertionError();
        for(String action:new String[]{"android.intent.action.DELETE","force_stop","grant_permission","launch:com.example.app","",null})
            rejects(() -> AppsSettingsContract.Action.parse(action));
        for(String name:new String[]{"","single","com.example/.Activity","com.example;am start","com..example","com.1example","com.example\n","com.example$other",null})
            rejects(() -> AppsSettingsContract.packageName(name));
        AppsSettingsContract.packageName("android");AppsSettingsContract.packageName("com.example.valid_app");
        for(Object offset:new Object[]{"20",20.0,-20,1,100020L,Long.MAX_VALUE,true,null})
            rejects(() -> AppsSettingsContract.offset(offset));
        AppsSettingsContract.offset(0);AppsSettingsContract.offset(20L);
        rejects(() -> AppsSettingsContract.query("a".repeat(129)));
        rejects(() -> AppsSettingsContract.query("mail\nother"));
        rejects(() -> AppsSettingsContract.query(null));
        AppsSettingsContract.query("邮".repeat(128));AppsSettingsContract.query("\uD83D\uDC8C".repeat(128));
        for(String generation:new String[]{"a","A".repeat(64),"g".repeat(64),"a".repeat(63)})
            rejects(() -> AppsSettingsContract.generation(generation));
        AppsSettingsContract.generation(null);AppsSettingsContract.generation("a".repeat(64));
        for(String name:new String[]{"android","com.android.settings","dev.makepad.octosense","dev.makepad.octosense.bridge"})
            if(!AppsSettingsContract.protectedPackage(name)) throw new AssertionError("Core package not protected");
    }
}
