package android.os;
public final class UserManager {
    public boolean admin=true,unlocked=true,restricted;
    public boolean isAdminUser(){return admin;}public boolean isUserUnlocked(){return unlocked;}
    public boolean hasUserRestriction(String key){return restricted;}
}
